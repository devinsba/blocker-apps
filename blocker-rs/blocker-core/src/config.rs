//! On-disk configuration (`config.toml`).
//!
//! ```toml
//! [settings]
//! block_quic = true
//!
//! [server]
//! url = "https://lists.example.com"
//! public_key = "<base64 ed25519 public key>"
//!
//! [[rule]]
//! name = "focus"
//! domains = ["youtube.com", "reddit.com"]
//! categories = ["social"]
//! bundle_ids = ["com.valvesoftware.steam"]
//! process_names = ["Discord"]
//! schedule = { type = "windows", windows = [{ days = ["mon","tue","wed","thu","fri"], start = "09:00", end = "17:00" }] }
//! locked = true
//! ```

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::schedule::Schedule;

/// Root-owned directory the CLI writes and the system extension reads.
pub const SYSTEM_DIR: &str = "/Library/Application Support/Blocker";

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Config {
    #[serde(default)]
    pub settings: Settings,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server: Option<ServerConfig>,
    #[serde(default, rename = "rule", skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<Rule>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// Drop UDP/443 (QUIC) to unknown hosts so browsers fall back to TCP,
    /// where the TLS SNI is visible. Protected apps are never affected.
    pub block_quic: bool,
    /// Drop TLS connections using Encrypted Client Hello while a domain rule
    /// is active. ECH hides the real hostname behind the provider's public
    /// name (e.g. `cloudflare-ech.com`), so without this ECH sites slip past
    /// domain rules; with it they fail to load during active rules. Prefer
    /// turning ECH off in browsers by policy (`EncryptedClientHelloEnabled`).
    pub block_ech: bool,
    /// Domains that are never blocked, in addition to the built-in
    /// Tailscale/Twingate set. Suffix match.
    pub allow_domains: Vec<String>,
    /// CIDRs that are never blocked, in addition to the built-in set.
    pub allow_cidrs: Vec<String>,
    /// Signing identifiers / bundle id prefixes whose traffic is never filtered.
    pub allow_apps: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            block_quic: true,
            block_ech: false,
            allow_domains: Vec::new(),
            allow_cidrs: Vec::new(),
            allow_apps: Vec::new(),
        }
    }
}

/// The central blocklist server. See [`crate::manifest`] for the protocol.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ServerConfig {
    /// Base URL; the client fetches `{url}/v1/manifest.json`.
    pub url: String,
    /// Base64 Ed25519 public key the manifest must be signed with.
    pub public_key: String,
    /// How often to check for a new manifest, in minutes.
    #[serde(default = "default_refresh_minutes")]
    pub refresh_minutes: u64,
}

fn default_refresh_minutes() -> u64 {
    60
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Rule {
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// A locked rule cannot be removed, disabled or edited while active.
    #[serde(default)]
    pub locked: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub domains: Vec<String>,
    /// Server categories (e.g. "ads", "social") whose domains this rule blocks.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bundle_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub process_names: Vec<String>,
    #[serde(default)]
    pub schedule: Schedule,
}

fn default_true() -> bool {
    true
}

impl Config {
    pub fn system_dir() -> PathBuf {
        PathBuf::from(SYSTEM_DIR)
    }

    pub fn default_path() -> PathBuf {
        Self::system_dir().join("config.toml")
    }

    /// Loads `path`, returning an empty config if it does not exist.
    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) => {
                toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
        }
    }

    /// Writes atomically (temp file + rename) so readers never see a partial file.
    pub fn save(&self, path: &Path) -> Result<()> {
        let text = toml::to_string_pretty(self)?;
        write_atomic(path, text.as_bytes())
    }

    pub fn rule(&self, name: &str) -> Option<&Rule> {
        self.rules.iter().find(|r| r.name == name)
    }

    /// Directory holding downloaded, verified category sets.
    pub fn lists_dir(dir: &Path) -> PathBuf {
        dir.join("lists")
    }

    /// Every category referenced by an enabled rule, sorted and deduplicated.
    pub fn wanted_categories(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .rules
            .iter()
            .filter(|r| r.enabled)
            .flat_map(|r| r.categories.iter().cloned())
            .collect();
        v.sort();
        v.dedup();
        v
    }
}

/// Atomically replaces `path` with `bytes`, creating parent directories.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes).with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("renaming into {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_documented_example() {
        let text = r#"
[settings]
block_quic = true

[server]
url = "https://lists.example.com"
public_key = "AAAA"

[[rule]]
name = "focus"
domains = ["youtube.com", "reddit.com"]
categories = ["social", "ads"]
bundle_ids = ["com.valvesoftware.steam"]
process_names = ["Discord"]
schedule = { type = "windows", windows = [{ days = ["mon","tue","wed","thu","fri"], start = "09:00", end = "17:00" }] }
locked = true
"#;
        let cfg: Config = toml::from_str(text).unwrap();
        assert_eq!(cfg.rules.len(), 1);
        assert!(cfg.rules[0].enabled);
        assert!(cfg.rules[0].locked);
        assert_eq!(cfg.server.as_ref().unwrap().refresh_minutes, 60);
        assert_eq!(cfg.wanted_categories(), vec!["ads", "social"]);
        let round: Config = toml::from_str(&toml::to_string_pretty(&cfg).unwrap()).unwrap();
        assert_eq!(round, cfg);
    }

    #[test]
    fn missing_file_is_empty_config() {
        let cfg = Config::load(Path::new("/nonexistent/blocker/config.toml")).unwrap();
        assert!(cfg.rules.is_empty());
    }
}
