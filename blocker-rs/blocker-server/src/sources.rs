//! `sources.toml`: which upstream lists make up each served category.
//!
//! ```toml
//! [[category]]
//! name = "ads"
//! description = "Advertising and tracking"
//! sources = ["https://big.oisd.nl/domainswild2", "file:lists/ads-extra.txt"]
//! allow = ["example-cdn.com"]
//! ```
//!
//! `file:` paths are relative to the directory containing `sources.toml`.
//! Plain `http://` is only accepted for localhost; everything else must be
//! `https://`.

use std::collections::HashSet;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, bail, ensure, Context, Result};
use blocker_core::domain;
use serde::Deserialize;
use url::Url;

/// Maximum accepted size of one downloaded source.
pub const MAX_BODY_BYTES: u64 = 512 * 1024 * 1024;
const MAX_REDIRECTS: usize = 5;
const USER_AGENT: &str = concat!("blocker-server/", env!("CARGO_PKG_VERSION"));

/// Raw TOML document.
#[derive(Debug, Deserialize)]
struct RawFile {
    #[serde(default)]
    category: Vec<RawCategory>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCategory {
    name: String,
    #[serde(default)]
    description: String,
    sources: Vec<String>,
    #[serde(default)]
    allow: Vec<String>,
}

/// Where a list comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// Remote list fetched over HTTP(S).
    Http(Url),
    /// Local file (already resolved against the `sources.toml` directory).
    File(PathBuf),
}

impl std::fmt::Display for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Source::Http(u) => write!(f, "{u}"),
            Source::File(p) => write!(f, "file:{}", p.display()),
        }
    }
}

/// A validated category definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Category {
    /// `[a-z0-9_-]{1,64}`.
    pub name: String,
    /// Human readable description.
    pub description: String,
    /// Upstream lists, all of which must be fetched for the category to publish.
    pub sources: Vec<Source>,
    /// Normalized domains removed from the category (exact or subdomain match).
    pub allow: Vec<String>,
}

impl Category {
    /// True when `domain` equals or is a subdomain of an `allow` entry.
    pub fn is_allowed(&self, domain: &str) -> bool {
        self.allow.iter().any(|a| {
            domain == a
                || (domain.len() > a.len()
                    && domain.ends_with(a.as_str())
                    && domain.as_bytes()[domain.len() - a.len() - 1] == b'.')
        })
    }
}

/// Loads and validates a `sources.toml` file.
pub fn load(path: &Path) -> Result<Vec<Category>> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let base = path.parent().unwrap_or_else(|| Path::new("."));
    parse(&text, base).with_context(|| format!("in {}", path.display()))
}

/// Parses `sources.toml` text; `file:` sources resolve against `base_dir`.
pub fn parse(text: &str, base_dir: &Path) -> Result<Vec<Category>> {
    let raw: RawFile = toml::from_str(text).context("invalid sources.toml")?;
    ensure!(!raw.category.is_empty(), "no [[category]] entries");
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for c in raw.category {
        ensure!(valid_name(&c.name), "invalid category name {:?}", c.name);
        ensure!(
            seen.insert(c.name.clone()),
            "duplicate category {:?}",
            c.name
        );
        ensure!(
            !c.sources.is_empty(),
            "category {:?} has no sources",
            c.name
        );
        let sources = c
            .sources
            .iter()
            .map(|s| parse_source(s, base_dir))
            .collect::<Result<Vec<_>>>()
            .with_context(|| format!("category {:?}", c.name))?;
        let allow = c
            .allow
            .iter()
            .map(|a| {
                domain::normalize(a)
                    .ok_or_else(|| anyhow!("category {:?}: bad allow entry {a:?}", c.name))
            })
            .collect::<Result<Vec<_>>>()?;
        out.push(Category {
            name: c.name,
            description: c.description,
            sources,
            allow,
        });
    }
    Ok(out)
}

fn valid_name(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && name
            .bytes()
            .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-'))
}

fn parse_source(s: &str, base_dir: &Path) -> Result<Source> {
    if let Some(p) = s.strip_prefix("file:") {
        ensure!(!p.is_empty(), "empty file: source");
        let p = Path::new(p);
        return Ok(Source::File(if p.is_absolute() {
            p.to_path_buf()
        } else {
            base_dir.join(p)
        }));
    }
    let url = Url::parse(s).with_context(|| format!("bad source {s:?}"))?;
    check_url(&url)?;
    Ok(Source::Http(url))
}

fn check_url(url: &Url) -> Result<()> {
    match url.scheme() {
        "https" => Ok(()),
        "http" => {
            let local = matches!(
                url.host_str(),
                Some("localhost") | Some("127.0.0.1") | Some("[::1]") | Some("::1")
            );
            ensure!(local, "plain http is only allowed for localhost: {url}");
            Ok(())
        }
        other => bail!("unsupported source scheme {other:?}"),
    }
}

/// Opens a source for streaming. HTTP bodies are capped at [`MAX_BODY_BYTES`];
/// exceeding the cap surfaces as a read error so the category fails.
pub fn open(source: &Source) -> Result<Box<dyn BufRead>> {
    match source {
        Source::File(p) => {
            let f = std::fs::File::open(p).with_context(|| format!("opening {}", p.display()))?;
            Ok(Box::new(BufReader::with_capacity(1 << 16, f)))
        }
        Source::Http(url) => {
            let agent = ureq::AgentBuilder::new()
                .user_agent(USER_AGENT)
                .redirects(0)
                .timeout_connect(Duration::from_secs(30))
                .timeout_read(Duration::from_secs(60))
                .timeout(Duration::from_secs(30 * 60))
                .build();
            let mut url = url.clone();
            for _ in 0..=MAX_REDIRECTS {
                let resp = agent
                    .get(url.as_str())
                    .call()
                    .map_err(|e| anyhow!("GET {url}: {e}"))?;
                if (300..400).contains(&resp.status()) {
                    let loc = resp
                        .header("location")
                        .ok_or_else(|| anyhow!("GET {url}: redirect without Location"))?;
                    url = url
                        .join(loc)
                        .with_context(|| format!("bad redirect {loc:?}"))?;
                    check_url(&url).context("redirect target rejected")?;
                    continue;
                }
                ensure!(resp.status() == 200, "GET {url}: HTTP {}", resp.status());
                return Ok(Box::new(BufReader::with_capacity(
                    1 << 16,
                    CappedReader {
                        inner: resp.into_reader(),
                        left: MAX_BODY_BYTES,
                    },
                )));
            }
            bail!("too many redirects fetching {url}")
        }
    }
}

/// Reader that errors (rather than silently truncating) past a byte limit.
struct CappedReader<R> {
    inner: R,
    left: u64,
}

impl<R: Read> Read for CappedReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        if n as u64 > self.left {
            return Err(std::io::Error::other("source exceeds maximum size"));
        }
        self.left -= n as u64;
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "/etc/blocker";

    #[test]
    fn parses_valid_file() {
        let cats = parse(
            r#"
[[category]]
name = "ads"
description = "Advertising"
sources = ["https://big.oisd.nl/domainswild2", "file:lists/x.txt", "http://localhost:9/a"]
allow = ["Example-CDN.com"]

[[category]]
name = "social"
sources = ["file:/abs/s.txt"]
"#,
            Path::new(BASE),
        )
        .unwrap();
        assert_eq!(cats.len(), 2);
        assert_eq!(cats[0].allow, vec!["example-cdn.com"]);
        assert_eq!(
            cats[0].sources[1],
            Source::File(PathBuf::from("/etc/blocker/lists/x.txt"))
        );
        assert_eq!(
            cats[1].sources[0],
            Source::File(PathBuf::from("/abs/s.txt"))
        );
        assert_eq!(cats[1].description, "");
    }

    #[test]
    fn rejects_bad_input() {
        let p = Path::new(BASE);
        let bad = [
            "",
            "[[category]]\nname=\"Ads\"\nsources=[\"file:a\"]",
            "[[category]]\nname=\"a\"\nsources=[]",
            "[[category]]\nname=\"a\"\nsources=[\"http://evil.example/x\"]",
            "[[category]]\nname=\"a\"\nsources=[\"ftp://x/y\"]",
            "[[category]]\nname=\"a\"\nsources=[\"file:a\"]\n[[category]]\nname=\"a\"\nsources=[\"file:b\"]",
            "[[category]]\nname=\"a\"\nsources=[\"file:a\"]\nbogus=1",
        ];
        for b in bad {
            assert!(parse(b, p).is_err(), "should reject: {b}");
        }
    }

    #[test]
    fn allow_matches_exact_and_subdomains_only() {
        let c = Category {
            name: "a".into(),
            description: String::new(),
            sources: vec![],
            allow: vec!["example.com".into()],
        };
        assert!(c.is_allowed("example.com"));
        assert!(c.is_allowed("cdn.example.com"));
        assert!(!c.is_allowed("badexample.com"));
        assert!(!c.is_allowed("example.org"));
    }
}
