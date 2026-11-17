//! Blocklist updater: keeps the installed category lists in sync with the
//! central server. See [`blocker_core::manifest`] for the protocol.
//!
//! One call to [`update`] performs a full check:
//!
//! 1. fetch `{url}/v1/manifest.json` (conditional on the stored `ETag` unless
//!    forced, or unless a wanted category is not installed yet),
//! 2. verify the signature with the pinned key and reject rollbacks,
//! 3. download every wanted category whose SHA-256 differs from the installed
//!    one, verify size and hash, and install it atomically,
//! 4. delete lists no longer wanted and persist the new [`ListState`].
//!
//! A failed category download never removes a previously installed list. If
//! the manifest itself cannot be fetched or verified nothing is modified.
//!
//! Only `https` URLs are accepted, except loopback `http` for local testing.
//! Redirects are not followed, so a server cannot downgrade the transport.

use std::io::Read;
use std::time::Duration;

use anyhow::{anyhow, bail, ensure, Context, Result};
use blocker_core::config::ServerConfig;
use blocker_core::manifest::{self, SignedManifest, MANIFEST_PATH};
use blocker_core::store::{ListState, Store};
use blocker_core::Config;

/// Maximum size of a manifest response.
const MAX_MANIFEST_BYTES: u64 = 4 * 1024 * 1024;
/// Maximum size of a category file response.
const MAX_CATEGORY_BYTES: u64 = 512 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(30);

/// Outcome of one [`update`] run.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct UpdateReport {
    /// Serial of the manifest now in effect.
    pub serial: u64,
    /// The server answered `304 Not Modified` (local files were still reconciled).
    pub not_modified: bool,
    /// Categories downloaded and installed.
    pub installed: Vec<String>,
    /// Wanted categories that were already up to date.
    pub unchanged: Vec<String>,
    /// Categories deleted because no rule wants them any more.
    pub removed: Vec<String>,
    /// Wanted categories the server does not publish.
    pub missing: Vec<String>,
    /// `(category, error)` for per-category failures; the old file is kept.
    pub failed: Vec<(String, String)>,
}

/// True if a check is due: never checked, or `checked_at + refresh_minutes`
/// has passed. A `checked_at` in the future (clock change) counts as due.
pub fn due(state: &ListState, server: &ServerConfig, now_unix: u64) -> bool {
    if state.checked_at == 0 || state.checked_at > now_unix {
        return true;
    }
    state
        .checked_at
        .saturating_add(server.refresh_minutes.saturating_mul(60))
        <= now_unix
}

/// Synchronizes the installed lists with the server configured in `config`.
///
/// `force` ignores the stored `ETag` and always fetches the full manifest.
/// Per-category problems are reported in [`UpdateReport::failed`]; an `Err`
/// means the manifest could not be fetched or verified and nothing changed.
pub fn update(store: &Store, config: &Config, force: bool) -> Result<UpdateReport> {
    let server = config
        .server
        .as_ref()
        .ok_or_else(|| anyhow!("no [server] configured"))?;
    check_url(&server.url)?;
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(TIMEOUT)
        .timeout_read(TIMEOUT)
        .timeout_write(TIMEOUT)
        .user_agent(concat!("blocker/", env!("CARGO_PKG_VERSION")))
        .redirects(0)
        .build();
    let now = chrono::Utc::now().timestamp().max(0) as u64;
    update_with(store, config, server, force, now, &|url, etag, max| {
        http_get(&agent, url, etag, max)
    })
}

enum Fetched {
    NotModified,
    Body {
        bytes: Vec<u8>,
        etag: Option<String>,
    },
}

type Fetch<'a> = &'a dyn Fn(&str, Option<&str>, u64) -> Result<Fetched>;

fn update_with(
    store: &Store,
    config: &Config,
    server: &ServerConfig,
    force: bool,
    now: u64,
    fetch: Fetch,
) -> Result<UpdateReport> {
    let mut state = store.load_state()?;
    let wanted = config.wanted_categories();

    // A wanted category that is not installed needs the full manifest even if
    // the server's ETag is unchanged.
    let incomplete = wanted
        .iter()
        .any(|n| !state.installed.contains_key(n) || !store.category_path(n).exists());
    let etag = if force || incomplete {
        None
    } else {
        state.etag.as_deref()
    };

    let url = join_url(&server.url, MANIFEST_PATH);
    let fetched =
        fetch(&url, etag, MAX_MANIFEST_BYTES).with_context(|| format!("fetching {url}"))?;

    let mut report = UpdateReport::default();
    let (manifest, new_etag) = match fetched {
        Fetched::NotModified => {
            ensure!(etag.is_some(), "unexpected 304 Not Modified");
            report.not_modified = true;
            report.serial = state.serial;
            report.unchanged = wanted.clone();
            remove_unwanted(store, &mut state, &wanted, &mut report);
            state.checked_at = now;
            store.save_state(&state)?;
            return Ok(report);
        }
        Fetched::Body { bytes, etag } => {
            let signed: SignedManifest =
                serde_json::from_slice(&bytes).context("parsing manifest envelope")?;
            let m = manifest::verify(&signed, &server.public_key, state.serial)?;
            (m, etag)
        }
    };
    report.serial = manifest.serial;

    for name in &wanted {
        let Some(entry) = manifest.category(name) else {
            report.missing.push(name.clone());
            continue;
        };
        let path = store.category_path(name);
        if state.installed.get(name) == Some(&entry.sha256) && path.exists() {
            report.unchanged.push(name.clone());
            continue;
        }
        let url = join_url(&server.url, &format!("v1/{}", entry.file));
        let result = fetch(&url, None, MAX_CATEGORY_BYTES)
            .with_context(|| format!("fetching {url}"))
            .and_then(|f| match f {
                Fetched::Body { bytes, .. } => Ok(bytes),
                Fetched::NotModified => bail!("unexpected 304 Not Modified"),
            })
            .and_then(|bytes| {
                manifest::verify_file(entry, &bytes)?;
                store.install_category(name, &bytes)
            });
        match result {
            Ok(()) => {
                state.installed.insert(name.clone(), entry.sha256.clone());
                report.installed.push(name.clone());
            }
            Err(e) => {
                log::warn!("category {name}: {e:#}");
                report.failed.push((name.clone(), format!("{e:#}")));
            }
        }
    }

    remove_unwanted(store, &mut state, &wanted, &mut report);

    state.serial = manifest.serial;
    // Without a complete result the next check must not short-circuit on 304.
    state.etag = if report.failed.is_empty() {
        new_etag
    } else {
        None
    };
    state.checked_at = now;
    store.save_state(&state)?;
    Ok(report)
}

/// Deletes installed lists no rule wants any more.
fn remove_unwanted(
    store: &Store,
    state: &mut ListState,
    wanted: &[String],
    report: &mut UpdateReport,
) {
    let mut stale: Vec<String> = state
        .installed
        .keys()
        .filter(|n| !wanted.contains(n))
        .cloned()
        .collect();
    stale.sort();
    for name in stale {
        match std::fs::remove_file(store.category_path(&name)) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                report.failed.push((name, format!("removing list: {e}")));
                continue;
            }
        }
        state.installed.remove(&name);
        report.removed.push(name);
    }
}

fn http_get(agent: &ureq::Agent, url: &str, etag: Option<&str>, max: u64) -> Result<Fetched> {
    check_url(url)?;
    let mut req = agent.get(url);
    if let Some(etag) = etag {
        req = req.set("If-None-Match", etag);
    }
    let resp = match req.call() {
        Ok(r) => r,
        Err(ureq::Error::Status(code, _)) => bail!("HTTP {code}"),
        Err(e) => return Err(e.into()),
    };
    match resp.status() {
        304 => return Ok(Fetched::NotModified),
        200 => {}
        other => bail!("unexpected HTTP status {other}"),
    }
    let etag = resp.header("ETag").map(str::to_owned);
    let mut bytes = Vec::new();
    resp.into_reader()
        .take(max + 1)
        .read_to_end(&mut bytes)
        .context("reading response body")?;
    ensure!(
        bytes.len() as u64 <= max,
        "response larger than {max} bytes"
    );
    Ok(Fetched::Body { bytes, etag })
}

/// Joins a base URL and a relative path with exactly one `/`.
fn join_url(base: &str, path: &str) -> String {
    format!(
        "{}/{}",
        base.trim_end_matches('/'),
        path.trim_start_matches('/')
    )
}

/// Accepts `https://` URLs and `http://` only for loopback hosts.
fn check_url(url: &str) -> Result<()> {
    let lower = url.to_ascii_lowercase();
    let (secure, rest) = if let Some(r) = lower.strip_prefix("https://") {
        (true, r)
    } else if let Some(r) = lower.strip_prefix("http://") {
        (false, r)
    } else {
        bail!("server URL must start with https://: {url}");
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    ensure!(!authority.is_empty(), "server URL has no host: {url}");
    ensure!(
        !authority.contains('@'),
        "server URL must not contain credentials: {url}"
    );
    if secure {
        return Ok(());
    }
    let host = match authority.strip_prefix('[') {
        Some(v6) => v6
            .split(']')
            .next()
            .map(|h| format!("[{h}]"))
            .unwrap_or_default(),
        None => authority.split(':').next().unwrap_or("").to_string(),
    };
    ensure!(
        matches!(host.as_str(), "127.0.0.1" | "localhost" | "[::1]"),
        "plain http is only allowed for localhost; use https: {url}"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use blocker_core::config::Rule;
    use blocker_core::manifest::{
        generate_keypair, sha256_hex, sign, CategoryEntry, Manifest, FORMAT_VERSION,
    };
    use blocker_core::DomainSet;
    use std::collections::HashMap;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    type Routes = HashMap<String, (Vec<u8>, Option<String>)>;

    struct TestServer {
        base: String,
        routes: Arc<Mutex<Routes>>,
        hits: Arc<Mutex<Vec<String>>>,
    }

    impl TestServer {
        fn start() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
            let routes: Arc<Mutex<Routes>> = Arc::default();
            let hits: Arc<Mutex<Vec<String>>> = Arc::default();
            let (r, h) = (routes.clone(), hits.clone());
            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    let Ok(mut stream) = stream else { continue };
                    let mut reader = BufReader::new(stream.try_clone().unwrap());
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
                    let mut inm = None;
                    loop {
                        let mut hl = String::new();
                        reader.read_line(&mut hl).unwrap();
                        if hl.trim().is_empty() {
                            break;
                        }
                        if let Some((k, v)) = hl.split_once(':') {
                            if k.eq_ignore_ascii_case("if-none-match") {
                                inm = Some(v.trim().to_string());
                            }
                        }
                    }
                    h.lock().unwrap().push(path.clone());
                    let route = r.lock().unwrap().get(&path).cloned();
                    let resp = match route {
                        None => b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
                        Some((_, Some(etag))) if inm.as_deref() == Some(etag.as_str()) => format!(
                            "HTTP/1.1 304 Not Modified\r\nETag: {etag}\r\nConnection: close\r\n\r\n"
                        )
                        .into_bytes(),
                        Some((body, etag)) => {
                            let mut v = format!(
                                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n",
                                body.len()
                            );
                            if let Some(e) = etag {
                                v.push_str(&format!("ETag: {e}\r\n"));
                            }
                            v.push_str("\r\n");
                            let mut out = v.into_bytes();
                            out.extend_from_slice(&body);
                            out
                        }
                    };
                    let _ = stream.write_all(&resp);
                }
            });
            Self { base, routes, hits }
        }

        fn set(&self, path: &str, body: Vec<u8>, etag: Option<String>) {
            self.routes
                .lock()
                .unwrap()
                .insert(path.to_string(), (body, etag));
        }

        fn hits(&self) -> Vec<String> {
            self.hits.lock().unwrap().clone()
        }

        fn clear_hits(&self) {
            self.hits.lock().unwrap().clear();
        }
    }

    struct Fixture {
        server: TestServer,
        secret: String,
        public: String,
        store: Store,
        _dir: tempfile::TempDir,
    }

    fn set_bytes(domains: &[&str]) -> Vec<u8> {
        DomainSet::build(domains.iter().copied()).unwrap()
    }

    impl Fixture {
        fn new() -> Self {
            let (secret, public) = generate_keypair();
            let dir = tempfile::tempdir().unwrap();
            Self {
                server: TestServer::start(),
                secret,
                public,
                store: Store::new(dir.path()),
                _dir: dir,
            }
        }

        fn publish_with(&self, secret: &str, serial: u64, cats: &[(&str, &[u8])]) {
            let categories = cats
                .iter()
                .map(|(name, data)| {
                    let sha = sha256_hex(data);
                    let entry = CategoryEntry {
                        name: name.to_string(),
                        description: String::new(),
                        file: CategoryEntry::content_addressed_file(name, &sha),
                        sha256: sha,
                        size: data.len() as u64,
                        domains: 1,
                    };
                    self.server
                        .set(&format!("/v1/{}", entry.file), data.to_vec(), None);
                    entry
                })
                .collect();
            let m = Manifest {
                format: FORMAT_VERSION,
                serial,
                generated_at: "2026-01-01T00:00:00Z".into(),
                expires_at: None,
                categories,
            };
            let body = serde_json::to_vec(&sign(&m, secret).unwrap()).unwrap();
            self.server
                .set("/v1/manifest.json", body, Some(format!("\"m{serial}\"")));
        }

        fn publish(&self, serial: u64, cats: &[(&str, &[u8])]) {
            self.publish_with(&self.secret, serial, cats);
        }

        fn config(&self, cats: &[&str]) -> Config {
            Config {
                server: Some(ServerConfig {
                    url: format!("{}/", self.server.base),
                    public_key: self.public.clone(),
                    refresh_minutes: 60,
                }),
                rules: vec![Rule {
                    name: "r".into(),
                    enabled: true,
                    locked: false,
                    domains: vec![],
                    categories: cats.iter().map(|s| s.to_string()).collect(),
                    bundle_ids: vec![],
                    process_names: vec![],
                    schedule: Default::default(),
                }],
                ..Config::default()
            }
        }

        fn file_hits(&self) -> usize {
            self.server
                .hits()
                .iter()
                .filter(|p| p.contains("/categories/"))
                .count()
        }
    }

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn first_install_then_not_modified() {
        let f = Fixture::new();
        let social = set_bytes(&["reddit.com"]);
        f.publish(1, &[("social", &social)]);
        let cfg = f.config(&["social"]);

        let r = update(&f.store, &cfg, false).unwrap();
        assert_eq!(r.serial, 1);
        assert!(!r.not_modified);
        assert_eq!(r.installed, s(&["social"]));
        assert_eq!(
            std::fs::read(f.store.category_path("social")).unwrap(),
            social
        );
        let st = f.store.load_state().unwrap();
        assert_eq!(st.serial, 1);
        assert_eq!(st.etag.as_deref(), Some("\"m1\""));
        assert!(st.checked_at > 0);

        f.server.clear_hits();
        let r = update(&f.store, &cfg, false).unwrap();
        assert!(r.not_modified);
        assert!(r.installed.is_empty());
        assert_eq!(r.serial, 1);
        assert_eq!(f.server.hits(), vec!["/v1/manifest.json"]);

        // force bypasses the ETag but downloads nothing (hash unchanged).
        f.server.clear_hits();
        let r = update(&f.store, &cfg, true).unwrap();
        assert!(!r.not_modified);
        assert_eq!(r.unchanged, s(&["social"]));
        assert_eq!(f.file_hits(), 0);
    }

    #[test]
    fn changed_category_only_downloaded() {
        let f = Fixture::new();
        let social = set_bytes(&["reddit.com"]);
        let ads = set_bytes(&["ads.example.com"]);
        f.publish(1, &[("social", &social), ("ads", &ads)]);
        let cfg = f.config(&["social", "ads"]);
        assert_eq!(update(&f.store, &cfg, false).unwrap().installed.len(), 2);

        let social2 = set_bytes(&["reddit.com", "x.com"]);
        f.publish(2, &[("social", &social2), ("ads", &ads)]);
        f.server.clear_hits();
        let r = update(&f.store, &cfg, false).unwrap();
        assert_eq!(r.serial, 2);
        assert_eq!(r.installed, s(&["social"]));
        assert_eq!(r.unchanged, s(&["ads"]));
        assert_eq!(f.file_hits(), 1);
        assert_eq!(
            std::fs::read(f.store.category_path("social")).unwrap(),
            social2
        );
    }

    #[test]
    fn rollback_rejected() {
        let f = Fixture::new();
        let social = set_bytes(&["reddit.com"]);
        f.publish(5, &[("social", &social)]);
        let cfg = f.config(&["social"]);
        update(&f.store, &cfg, false).unwrap();

        let older = set_bytes(&["old.com"]);
        f.publish(4, &[("social", &older)]);
        let err = update(&f.store, &cfg, true).unwrap_err();
        assert!(format!("{err:#}").contains("rollback"), "{err:#}");
        assert_eq!(f.store.load_state().unwrap().serial, 5);
        assert_eq!(
            std::fs::read(f.store.category_path("social")).unwrap(),
            social
        );
    }

    #[test]
    fn bad_signature_rejected_nothing_changed() {
        let f = Fixture::new();
        let social = set_bytes(&["reddit.com"]);
        let (other_secret, _) = generate_keypair();
        f.publish_with(&other_secret, 1, &[("social", &social)]);
        let cfg = f.config(&["social"]);
        assert!(update(&f.store, &cfg, false).is_err());
        assert_eq!(f.store.load_state().unwrap(), ListState::default());
        assert!(!f.store.category_path("social").exists());
        assert_eq!(f.file_hits(), 0);
    }

    #[test]
    fn corrupted_category_fails_and_keeps_previous() {
        let f = Fixture::new();
        let v1 = set_bytes(&["reddit.com"]);
        f.publish(1, &[("social", &v1)]);
        let cfg = f.config(&["social"]);
        update(&f.store, &cfg, false).unwrap();
        let old_sha = sha256_hex(&v1);

        let v2 = set_bytes(&["reddit.com", "x.com"]);
        f.publish(2, &[("social", &v2)]);
        let file = CategoryEntry::content_addressed_file("social", &sha256_hex(&v2));
        let mut bad = v2.clone();
        let last = bad.len() - 1;
        bad[last] ^= 0xff;
        f.server.set(&format!("/v1/{file}"), bad, None);

        let r = update(&f.store, &cfg, false).unwrap();
        assert!(r.installed.is_empty());
        assert_eq!(r.failed.len(), 1);
        assert_eq!(r.failed[0].0, "social");
        assert_eq!(std::fs::read(f.store.category_path("social")).unwrap(), v1);
        let st = f.store.load_state().unwrap();
        assert_eq!(st.installed["social"], old_sha);
        // No ETag stored, so the next run retries instead of getting a 304.
        assert_eq!(st.etag, None);
    }

    #[test]
    fn removed_category_deleted_even_on_304() {
        let f = Fixture::new();
        let social = set_bytes(&["reddit.com"]);
        let ads = set_bytes(&["ads.example.com"]);
        f.publish(1, &[("social", &social), ("ads", &ads)]);
        update(&f.store, &f.config(&["social", "ads"]), false).unwrap();
        assert!(f.store.category_path("ads").exists());

        let r = update(&f.store, &f.config(&["social"]), false).unwrap();
        assert!(r.not_modified);
        assert_eq!(r.removed, s(&["ads"]));
        assert!(!f.store.category_path("ads").exists());
        assert!(f.store.category_path("social").exists());
        assert!(!f.store.load_state().unwrap().installed.contains_key("ads"));
    }

    #[test]
    fn newly_wanted_category_ignores_etag() {
        let f = Fixture::new();
        let social = set_bytes(&["reddit.com"]);
        let ads = set_bytes(&["ads.example.com"]);
        f.publish(1, &[("social", &social), ("ads", &ads)]);
        update(&f.store, &f.config(&["social"]), false).unwrap();
        let r = update(&f.store, &f.config(&["social", "ads"]), false).unwrap();
        assert!(!r.not_modified);
        assert_eq!(r.installed, s(&["ads"]));
        assert_eq!(r.unchanged, s(&["social"]));
    }

    #[test]
    fn wanted_but_absent_is_missing() {
        let f = Fixture::new();
        let social = set_bytes(&["reddit.com"]);
        f.publish(1, &[("social", &social)]);
        let r = update(&f.store, &f.config(&["social", "nope"]), false).unwrap();
        assert_eq!(r.installed, s(&["social"]));
        assert_eq!(r.missing, s(&["nope"]));
        assert!(r.failed.is_empty());
    }

    #[test]
    fn manifest_http_error_is_err() {
        let f = Fixture::new();
        let err = update(&f.store, &f.config(&["social"]), false).unwrap_err();
        assert!(format!("{err:#}").contains("404"), "{err:#}");
        assert_eq!(f.store.load_state().unwrap(), ListState::default());
    }

    #[test]
    fn url_policy() {
        assert!(check_url("https://lists.example.com").is_ok());
        assert!(check_url("HTTPS://lists.example.com/base/").is_ok());
        assert!(check_url("http://127.0.0.1:8080").is_ok());
        assert!(check_url("http://localhost").is_ok());
        assert!(check_url("http://localhost:3000/x").is_ok());
        assert!(check_url("http://[::1]:80/").is_ok());
        assert!(check_url("http://example.com").is_err());
        assert!(check_url("http://localhost.evil.com").is_err());
        assert!(check_url("http://localhost@evil.com/").is_err());
        assert!(check_url("ftp://example.com").is_err());
        assert!(check_url("example.com").is_err());
        assert!(check_url("https://").is_err());

        let dir = tempfile::tempdir().unwrap();
        let cfg = Config {
            server: Some(ServerConfig {
                url: "http://example.com".into(),
                public_key: "x".into(),
                refresh_minutes: 60,
            }),
            ..Config::default()
        };
        assert!(update(&Store::new(dir.path()), &cfg, false).is_err());
        assert!(update(&Store::new(dir.path()), &Config::default(), false).is_err());
    }

    #[test]
    fn join_handles_slashes() {
        assert_eq!(join_url("https://a.b", "v1/x"), "https://a.b/v1/x");
        assert_eq!(join_url("https://a.b/", "v1/x"), "https://a.b/v1/x");
        assert_eq!(
            join_url("https://a.b/base//", "/v1/x"),
            "https://a.b/base/v1/x"
        );
    }

    #[test]
    fn due_logic() {
        let server = ServerConfig {
            url: "https://x".into(),
            public_key: String::new(),
            refresh_minutes: 10,
        };
        let mut st = ListState::default();
        assert!(due(&st, &server, 1_000_000));
        st.checked_at = 1_000_000;
        assert!(!due(&st, &server, 1_000_000));
        assert!(!due(&st, &server, 1_000_599));
        assert!(due(&st, &server, 1_000_600));
        assert!(due(&st, &server, 999_999)); // clock went backwards
    }
}
