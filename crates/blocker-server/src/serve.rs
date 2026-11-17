//! Minimal static file server for the public directory.
//!
//! Serves only `GET`/`HEAD` for `/v1/manifest.json` and
//! `/v1/categories/<name>-<sha16>.fst`; everything else is a 404. The output
//! directory is plain static files, so a CDN or bucket works just as well.

use std::path::Path;
use std::sync::Arc;
use std::thread;

use anyhow::{anyhow, Result};
use blocker_core::manifest::sha256_hex;
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

const WORKERS: usize = 8;

/// What a request path resolves to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `/v1/manifest.json`.
    Manifest,
    /// A content-addressed category file.
    Category,
}

/// Validates a request target (query string ignored) and returns the path
/// relative to the public dir, e.g. `v1/manifest.json`. Equivalent to
/// `^/v1/(manifest\.json|categories/[a-z0-9_-]+-[0-9a-f]{16}\.fst)$`.
pub fn validate_path(target: &str) -> Option<(Kind, &str)> {
    let path = target.split(['?', '#']).next()?;
    if path.contains("..") || path.contains('\\') || path.contains('%') || path.contains('\0') {
        return None;
    }
    let rest = path.strip_prefix("/v1/")?;
    let rel = &path[1..];
    if rest == "manifest.json" {
        return Some((Kind::Manifest, rel));
    }
    let stem = rest.strip_prefix("categories/")?.strip_suffix(".fst")?;
    if stem.len() < 18 {
        return None;
    }
    let (name, hash) = stem.split_at(stem.len() - 16);
    let name = name.strip_suffix('-')?;
    let name_ok = !name.is_empty()
        && name
            .bytes()
            .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-'));
    let hash_ok = hash.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
    (name_ok && hash_ok).then_some((Kind::Category, rel))
}

/// A running server (for tests and the CLI).
pub struct Running {
    server: Arc<Server>,
    workers: Vec<thread::JoinHandle<()>>,
}

impl Running {
    /// The bound address.
    pub fn addr(&self) -> std::net::SocketAddr {
        self.server.server_addr().to_ip().expect("ip listener")
    }
    /// Stops accepting requests and joins the workers (used by tests).
    #[allow(dead_code)]
    pub fn shutdown(self) {
        // `unblock` wakes a single `recv`, so wake once per worker.
        for _ in &self.workers {
            self.server.unblock();
        }
        for w in self.workers {
            let _ = w.join();
        }
    }
    /// Blocks until the workers exit (normally never).
    pub fn wait(self) {
        for w in self.workers {
            let _ = w.join();
        }
    }
}

/// Binds `addr` and starts worker threads serving `dir`.
pub fn start(dir: &Path, addr: &str) -> Result<Running> {
    let server = Arc::new(Server::http(addr).map_err(|e| anyhow!("binding {addr}: {e}"))?);
    let dir = Arc::new(dir.to_path_buf());
    let workers = (0..WORKERS)
        .map(|_| {
            let (server, dir) = (server.clone(), dir.clone());
            thread::spawn(move || {
                while let Ok(req) = server.recv() {
                    handle(req, &dir);
                }
            })
        })
        .collect();
    Ok(Running { server, workers })
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("valid header")
}

fn handle(req: Request, dir: &Path) {
    let head = *req.method() == Method::Head;
    if !matches!(req.method(), Method::Get | Method::Head) {
        let _ = req.respond(Response::empty(405).with_header(header("Allow", "GET, HEAD")));
        return;
    }
    let Some((kind, rel)) = validate_path(req.url()) else {
        let _ = req.respond(Response::empty(404));
        return;
    };
    let Ok(bytes) = std::fs::read(dir.join(rel)) else {
        let _ = req.respond(Response::empty(404));
        return;
    };
    let etag = format!("\"{}\"", sha256_hex(&bytes));
    let (ctype, cache) = match kind {
        Kind::Manifest => ("application/json", "no-cache"),
        Kind::Category => (
            "application/octet-stream",
            "public, max-age=31536000, immutable",
        ),
    };
    let inm = req
        .headers()
        .iter()
        .find(|h| h.field.equiv("If-None-Match"))
        .map(|h| h.value.as_str().to_string());
    let not_modified = inm.is_some_and(|v| {
        v.split(',').any(|t| {
            let t = t.trim();
            t == "*" || t.strip_prefix("W/").unwrap_or(t) == etag
        })
    });

    let base = |r: Response<std::io::Cursor<Vec<u8>>>| {
        r.with_header(header("Content-Type", ctype))
            .with_header(header("Cache-Control", cache))
            .with_header(header("ETag", &etag))
    };
    let resp = if not_modified {
        base(Response::new(
            StatusCode(304),
            vec![],
            std::io::Cursor::new(vec![]),
            Some(0),
            None,
        ))
    } else if head {
        base(Response::new(
            StatusCode(200),
            vec![header("Content-Length", &bytes.len().to_string())],
            std::io::Cursor::new(vec![]),
            Some(0),
            None,
        ))
    } else {
        base(Response::from_data(bytes))
    };
    let _ = req.respond(resp);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_known_shapes() {
        let ok = [
            "/v1/manifest.json",
            "/v1/manifest.json?x=1",
            "/v1/categories/ads-0123456789abcdef.fst",
            "/v1/categories/my_cat-2-0123456789abcdef.fst",
        ];
        for p in ok {
            assert!(validate_path(p).is_some(), "{p}");
        }
        assert_eq!(
            validate_path("/v1/manifest.json").unwrap().1,
            "v1/manifest.json"
        );
        let bad = [
            "/",
            "/manifest.json",
            "/v1/",
            "/v1/../etc/passwd",
            "/v1/categories/../manifest.json",
            "/v1/categories/ads-0123456789abcdef.fst/..",
            "/v1/categories\\ads-0123456789abcdef.fst",
            "/v1/categories/%2e%2e/x",
            "/v1/categories/ads-0123456789ABCDEF.fst",
            "/v1/categories/ads-0123456789abcde.fst",
            "/v1/categories/-0123456789abcdef.fst",
            "/v1/categories/Ads-0123456789abcdef.fst",
            "/v1/categories/ads-0123456789abcdef.txt",
            "/v1/categories/sub/ads-0123456789abcdef.fst",
            "/v1/manifest.json/",
            "/v2/manifest.json",
        ];
        for p in bad {
            assert!(validate_path(p).is_none(), "{p}");
        }
    }

    #[test]
    fn serves_manifest_with_etag_and_rejects_traversal() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("v1/categories")).unwrap();
        std::fs::write(dir.path().join("v1/manifest.json"), b"{\"a\":1}").unwrap();
        std::fs::write(dir.path().join("secret.txt"), b"nope").unwrap();
        let srv = start(dir.path(), "127.0.0.1:0").unwrap();
        let base = format!("http://{}", srv.addr());

        let r = ureq::get(&format!("{base}/v1/manifest.json"))
            .call()
            .unwrap();
        assert_eq!(r.status(), 200);
        assert_eq!(r.header("Content-Type"), Some("application/json"));
        assert_eq!(r.header("Cache-Control"), Some("no-cache"));
        let etag = r.header("ETag").unwrap().to_string();
        assert_eq!(etag, format!("\"{}\"", sha256_hex(b"{\"a\":1}")));
        assert_eq!(r.into_string().unwrap(), "{\"a\":1}");

        let r = ureq::get(&format!("{base}/v1/manifest.json"))
            .set("If-None-Match", &etag)
            .call()
            .unwrap();
        assert_eq!(r.status(), 304);

        for p in [
            "/v1/../secret.txt",
            "/secret.txt",
            "/v1/categories/x-0123456789abcdef.fst",
        ] {
            match ureq::get(&format!("{base}{p}")).call() {
                Err(ureq::Error::Status(404, _)) => {}
                other => panic!("{p}: expected 404, got {other:?}"),
            }
        }
        match ureq::post(&format!("{base}/v1/manifest.json")).send_string("x") {
            Err(ureq::Error::Status(405, _)) => {}
            other => panic!("expected 405, got {other:?}"),
        }
        srv.shutdown();
    }
}
