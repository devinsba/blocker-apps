//! Compact, memory-mappable set of blocked domains backed by an FST.
//!
//! Keys are domains with their labels reversed and joined by `\0`
//! (`ads.example.com` -> `com\0example\0ads`). Because `\0` sorts before every
//! character allowed in a label, a domain's descendants immediately follow it
//! in key order, which lets [`DomainSet::build`] drop entries already covered
//! by a parent in a single pass.
//!
//! A lookup checks each suffix of the host (`com`, `com\0example`,
//! `com\0example\0ads`), so the cost is proportional to the number of labels
//! in the hostname, independent of how many millions of domains the set holds.

use std::path::Path;

use anyhow::{Context, Result};
use fst::{IntoStreamer, Set, Streamer};

const SEP: u8 = 0;

/// Owned bytes or a read-only memory map of a compiled set.
enum Backing {
    Owned(Vec<u8>),
    Mapped(memmap2::Mmap),
}

impl AsRef<[u8]> for Backing {
    fn as_ref(&self) -> &[u8] {
        match self {
            Backing::Owned(v) => v,
            Backing::Mapped(m) => m,
        }
    }
}

pub struct DomainSet {
    set: Set<Backing>,
}

impl std::fmt::Debug for DomainSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DomainSet")
            .field("len", &self.len())
            .finish()
    }
}

impl DomainSet {
    /// Compiles normalized domains (see [`crate::domain::normalize`]) into FST
    /// bytes. Duplicates and domains covered by a listed parent are dropped.
    /// Memory use is proportional to the input; the server does this, not clients.
    pub fn build<I, S>(domains: I) -> Result<Vec<u8>>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut keys: Vec<Vec<u8>> = domains.into_iter().map(|d| encode(d.as_ref())).collect();
        keys.sort_unstable();
        keys.dedup();

        let mut builder = fst::SetBuilder::memory();
        let mut last: Option<&[u8]> = None;
        for key in &keys {
            if let Some(parent) = last {
                if key.len() > parent.len() && key.starts_with(parent) && key[parent.len()] == SEP {
                    continue;
                }
            }
            builder.insert(key).context("inserting into domain set")?;
            last = Some(key);
        }
        Ok(builder.into_inner()?)
    }

    /// Builds an in-memory set; convenient for the handful of per-rule domains.
    pub fn from_domains<I, S>(domains: I) -> Result<Self>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        Self::from_bytes(Self::build(domains)?)
    }

    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self> {
        let set = Set::new(Backing::Owned(bytes)).context("invalid domain set")?;
        Ok(Self { set })
    }

    /// Memory-maps a compiled set from disk.
    pub fn open(path: &Path) -> Result<Self> {
        let file =
            std::fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
        // SAFETY: list files are replaced by atomic rename, never modified in place,
        // so the mapped bytes cannot change underneath us.
        let map = unsafe { memmap2::Mmap::map(&file) }
            .with_context(|| format!("mapping {}", path.display()))?;
        let set = Set::new(Backing::Mapped(map))
            .with_context(|| format!("invalid domain set {}", path.display()))?;
        Ok(Self { set })
    }

    pub fn len(&self) -> usize {
        self.set.len()
    }

    pub fn is_empty(&self) -> bool {
        self.set.is_empty()
    }

    /// Returns the listed domain that covers `host` (the host itself or one of
    /// its parents). `host` must be normalized.
    pub fn find(&self, host: &str) -> Option<String> {
        let mut key = Vec::with_capacity(host.len());
        for label in host.rsplit('.') {
            if !key.is_empty() {
                key.push(SEP);
            }
            key.extend_from_slice(label.as_bytes());
            if self.set.contains(&key) {
                return Some(decode(&key));
            }
        }
        None
    }

    pub fn contains(&self, host: &str) -> bool {
        self.find(host).is_some()
    }

    /// All domains in the set, in key order. Intended for small sets and debugging.
    pub fn domains(&self) -> Vec<String> {
        let mut out = Vec::with_capacity(self.len());
        let mut stream = self.set.stream();
        while let Some(key) = stream.next() {
            out.push(decode(key));
        }
        out
    }

    /// Domains in this set whose reversed form starts with `prefix`'s; used by
    /// `blocker check --explain` style tooling.
    pub fn under(&self, domain: &str) -> Vec<String> {
        let start = encode(domain);
        let mut out = Vec::new();
        let mut stream = self.set.range().ge(&start).into_stream();
        while let Some(key) = stream.next() {
            if key != start.as_slice()
                && !(key.starts_with(&start) && key.get(start.len()) == Some(&SEP))
            {
                break;
            }
            out.push(decode(key));
        }
        out
    }
}

fn encode(domain: &str) -> Vec<u8> {
    let mut key = Vec::with_capacity(domain.len());
    for label in domain.rsplit('.') {
        if !key.is_empty() {
            key.push(SEP);
        }
        key.extend_from_slice(label.as_bytes());
    }
    key
}

fn decode(key: &[u8]) -> String {
    let labels: Vec<&str> = key
        .split(|&b| b == SEP)
        .map(|l| std::str::from_utf8(l).unwrap_or(""))
        .collect();
    labels.into_iter().rev().collect::<Vec<_>>().join(".")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_domain_and_subdomains_only() {
        let set = DomainSet::from_domains(["example.com", "ads.tracker.net"]).unwrap();
        assert_eq!(set.find("example.com").as_deref(), Some("example.com"));
        assert_eq!(set.find("a.b.example.com").as_deref(), Some("example.com"));
        assert_eq!(
            set.find("ads.tracker.net").as_deref(),
            Some("ads.tracker.net")
        );
        assert!(!set.contains("tracker.net"));
        assert!(!set.contains("notexample.com"));
        assert!(!set.contains("example.com.evil.org"));
        assert!(!set.contains("com"));
    }

    #[test]
    fn prunes_duplicates_and_covered_children() {
        let set = DomainSet::from_domains([
            "a.example.com",
            "example.com",
            "example-foo.com",
            "b.example.com",
            "example.com",
            "x.example-foo.com",
        ])
        .unwrap();
        // "com\0example" sorts before "com\0example-foo".
        assert_eq!(set.domains(), vec!["example.com", "example-foo.com"]);
    }

    #[test]
    fn sibling_with_hyphen_does_not_break_pruning() {
        // "example-foo" sorts between "example" and "example\0..." without the \0 separator trick.
        let set =
            DomainSet::from_domains(["example.com", "example-foo.com", "z.example.com"]).unwrap();
        assert_eq!(set.len(), 2);
        assert!(set.contains("z.example.com"));
    }

    #[test]
    fn under_lists_descendants() {
        let set = DomainSet::from_domains([
            "a.example.com",
            "b.example.com",
            "example-x.com",
            "c.other.com",
        ])
        .unwrap();
        assert_eq!(
            set.under("example.com"),
            vec!["a.example.com", "b.example.com"]
        );
    }

    #[test]
    fn round_trips_through_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("set.fst");
        std::fs::write(&path, DomainSet::build(["example.com"]).unwrap()).unwrap();
        let set = DomainSet::open(&path).unwrap();
        assert!(set.contains("www.example.com"));
        assert!(DomainSet::from_bytes(b"garbage".to_vec()).is_err());
    }

    #[test]
    fn scales_to_a_million_domains() {
        let domains: Vec<String> = (0..1_000_000)
            .map(|i| format!("host{i}.site{}.example{}.com", i % 997, i % 13))
            .collect();
        let bytes = DomainSet::build(&domains).unwrap();
        let set = DomainSet::from_bytes(bytes).unwrap();
        assert_eq!(set.len(), 1_000_000);
        assert!(set.contains(&format!("www.{}", domains[123_456])));
        assert!(set.contains(&domains[999_999]));
        assert!(!set.contains("host1.site1.example2.com"));
    }
}
