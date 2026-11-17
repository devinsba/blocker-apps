//! On-disk layout shared by the CLI (writer) and the content filter (reader).
//!
//! ```text
//! /Library/Application Support/Blocker/
//!   config.toml            rules and settings
//!   lists/
//!     state.json           serial + hashes of the last verified manifest
//!     <category>.fst       verified category sets, replaced by atomic rename
//! ```

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::config::{write_atomic, Config};
use crate::domainset::DomainSet;
use crate::engine::Engine;

pub struct Store {
    dir: PathBuf,
}

/// What the updater last installed, so restarts don't re-download and a
/// replayed older manifest is rejected.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ListState {
    pub serial: u64,
    /// ETag of the last manifest response, for conditional requests.
    #[serde(default)]
    pub etag: Option<String>,
    /// category name -> sha256 of the installed file.
    #[serde(default)]
    pub installed: HashMap<String, String>,
    /// Unix seconds of the last successful check.
    #[serde(default)]
    pub checked_at: u64,
}

impl Store {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn system() -> Self {
        Self::new(Config::system_dir())
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn config_path(&self) -> PathBuf {
        self.dir.join("config.toml")
    }

    pub fn lists_dir(&self) -> PathBuf {
        Config::lists_dir(&self.dir)
    }

    pub fn category_path(&self, name: &str) -> PathBuf {
        self.lists_dir().join(format!("{name}.fst"))
    }

    fn state_path(&self) -> PathBuf {
        self.lists_dir().join("state.json")
    }

    pub fn load_config(&self) -> Result<Config> {
        Config::load(&self.config_path())
    }

    pub fn save_config(&self, config: &Config) -> Result<()> {
        config.save(&self.config_path())
    }

    pub fn load_state(&self) -> Result<ListState> {
        match std::fs::read(self.state_path()) {
            Ok(bytes) => serde_json::from_slice(&bytes).context("parsing list state"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(ListState::default()),
            Err(e) => Err(e).context("reading list state"),
        }
    }

    pub fn save_state(&self, state: &ListState) -> Result<()> {
        write_atomic(&self.state_path(), &serde_json::to_vec_pretty(state)?)
    }

    /// Installs an already-verified category file.
    pub fn install_category(&self, name: &str, bytes: &[u8]) -> Result<()> {
        DomainSet::from_bytes(bytes.to_vec())
            .with_context(|| format!("category {name} is not a valid set"))?;
        write_atomic(&self.category_path(name), bytes)
    }

    /// Memory-maps every category the config uses that is present on disk.
    pub fn load_categories(
        &self,
        config: &Config,
    ) -> (HashMap<String, Arc<DomainSet>>, Vec<String>) {
        let mut sets = HashMap::new();
        let mut warnings = Vec::new();
        for name in config.wanted_categories() {
            let path = self.category_path(&name);
            if !path.exists() {
                continue; // reported by Engine::new as "not downloaded yet"
            }
            match DomainSet::open(&path) {
                Ok(set) => {
                    sets.insert(name, Arc::new(set));
                }
                Err(e) => warnings.push(format!("category {name}: {e:#}")),
            }
        }
        (sets, warnings)
    }

    /// Loads config and categories and compiles an engine.
    pub fn load_engine(&self) -> Result<(Engine, Vec<String>)> {
        let config = self.load_config()?;
        let (sets, mut warnings) = self.load_categories(&config);
        let (engine, more) = Engine::new(&config, &sets)?;
        warnings.extend(more);
        Ok((engine, warnings))
    }

    /// Changes whenever the config or any installed list changes. Cheap enough
    /// to poll every few seconds.
    pub fn fingerprint(&self) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        mtime(&self.config_path()).hash(&mut h);
        if let Ok(entries) = std::fs::read_dir(self.lists_dir()) {
            let mut files: Vec<(PathBuf, Option<SystemTime>)> = entries
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "fst"))
                .map(|p| {
                    let m = mtime(&p);
                    (p, m)
                })
                .collect();
            files.sort();
            files.hash(&mut h);
        }
        h.finish()
    }
}

fn mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Rule;
    use crate::engine::{Verdict, WeekTime};
    use crate::schedule::{Day, Schedule};

    #[test]
    fn loads_engine_from_config_and_installed_categories() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        let config = Config {
            rules: vec![Rule {
                name: "r".into(),
                enabled: true,
                locked: false,
                domains: vec![],
                categories: vec!["social".into(), "missing".into()],
                bundle_ids: vec![],
                process_names: vec![],
                schedule: Schedule::Always,
            }],
            ..Config::default()
        };
        store.save_config(&config).unwrap();
        let before = store.fingerprint();
        store
            .install_category("social", &DomainSet::build(["reddit.com"]).unwrap())
            .unwrap();
        assert_ne!(before, store.fingerprint());
        assert!(store.install_category("bad", b"nope").is_err());

        let (engine, warnings) = store.load_engine().unwrap();
        assert!(warnings.iter().any(|w| w.contains("missing")));
        let at = WeekTime {
            day: Day::Mon,
            minute: 0,
        };
        assert!(matches!(
            engine.check_host("www.reddit.com", at),
            Verdict::Block(_)
        ));

        let state = ListState {
            serial: 7,
            ..Default::default()
        };
        store.save_state(&state).unwrap();
        assert_eq!(store.load_state().unwrap(), state);
    }
}
