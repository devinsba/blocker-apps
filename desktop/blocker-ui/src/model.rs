//! What the menu-bar app shows, computed off the UI thread from the config
//! directory and `blocker status --json`. Pure data: the tray and the window
//! both render from a [`Snapshot`] and an [`Activity`].

use blocker_core::status::{ExtensionState, SystemStatus};
use blocker_core::{Store, WeekTime};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot {
    pub rules: Vec<RuleRow>,
    pub server: Option<String>,
    pub serial: u64,
    /// Unix seconds of the last list check; 0 = never.
    pub checked_at: u64,
    pub categories: Vec<CategoryRow>,
    pub warnings: Vec<String>,
    /// The config couldn't be loaded; blocking keeps the previous rules.
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuleRow {
    pub name: String,
    pub enabled: bool,
    pub locked: bool,
    pub active: bool,
    pub schedule: String,
    pub domains: usize,
    pub categories: Vec<String>,
    pub apps: usize,
}

impl RuleRow {
    /// A locked rule can't be switched off while it's active.
    pub fn can_toggle(&self) -> bool {
        !(self.locked && self.active)
    }

    /// "3 domains · social, ads · 2 apps"
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if self.domains > 0 {
            parts.push(plural(self.domains, "domain"));
        }
        if !self.categories.is_empty() {
            parts.push(self.categories.join(", "));
        }
        if self.apps > 0 {
            parts.push(plural(self.apps, "app"));
        }
        parts.join(" · ")
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CategoryRow {
    pub name: String,
    /// `None` until downloaded.
    pub domains: Option<usize>,
}

/// The one-word state the tray icon shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Activity {
    /// At least one rule is active and nothing is known to be wrong.
    Blocking { rules: Vec<String> },
    /// No rule is active right now.
    Idle,
    /// Something needs the user: a broken config, or rules that should be
    /// blocking while the filter is off.
    Attention(String),
}

impl Activity {
    pub fn headline(&self) -> String {
        match self {
            Activity::Blocking { rules } => format!("Blocking · {}", rules.join(", ")),
            Activity::Idle => "Not blocking right now".into(),
            Activity::Attention(why) => format!("Needs attention · {why}"),
        }
    }
}

impl Snapshot {
    /// Reads the config, list state and installed categories. Never fails:
    /// problems land in `error` / `warnings` so the UI can show them.
    pub fn load(store: &Store, at: WeekTime) -> Self {
        let config = match store.load_config() {
            Ok(c) => c,
            Err(e) => {
                return Self {
                    error: Some(format!("{e:#}")),
                    ..Self::default()
                }
            }
        };
        let rules = config
            .rules
            .iter()
            .map(|r| RuleRow {
                name: r.name.clone(),
                enabled: r.enabled,
                locked: r.locked,
                active: r.enabled && r.schedule.is_active_at(at.day, at.minute),
                schedule: r.schedule.describe(),
                domains: r.domains.len(),
                categories: r.categories.clone(),
                apps: r.bundle_ids.len() + r.process_names.len(),
            })
            .collect();
        let state = store.load_state().unwrap_or_default();
        let (sets, mut warnings) = store.load_categories(&config);
        let categories = config
            .wanted_categories()
            .into_iter()
            .map(|name| CategoryRow {
                domains: sets.get(&name).map(|s| s.len()),
                name,
            })
            .collect();
        if let Err(e) = store.load_engine().map(|(_, w)| warnings.extend(w)) {
            warnings.push(format!("{e:#}"));
        }
        warnings.sort();
        warnings.dedup();
        Self {
            rules,
            server: config.server.map(|s| s.url),
            serial: state.serial,
            checked_at: state.checked_at,
            categories,
            warnings,
            error: None,
        }
    }

    pub fn active_rules(&self) -> Vec<String> {
        self.rules
            .iter()
            .filter(|r| r.active)
            .map(|r| r.name.clone())
            .collect()
    }

    pub fn activity(&self, system: Option<&SystemStatus>) -> Activity {
        if let Some(e) = &self.error {
            return Activity::Attention(format!("config error: {e}"));
        }
        let rules = self.active_rules();
        if rules.is_empty() {
            return Activity::Idle;
        }
        // Only flag the filter when the system positively says it's off; an
        // unknown answer (e.g. a development build) isn't evidence of a problem.
        if let Some(sys) = system {
            match sys.filter.extension {
                ExtensionState::NotInstalled => {
                    return Activity::Attention("filter not installed".into())
                }
                ExtensionState::AwaitingApproval => {
                    return Activity::Attention("approve the filter in System Settings".into())
                }
                ExtensionState::Installed if sys.filter.enabled == Some(false) => {
                    return Activity::Attention("filter turned off".into())
                }
                _ => {}
            }
        }
        Activity::Blocking { rules }
    }
}

fn plural(n: usize, what: &str) -> String {
    format!("{n} {what}{}", if n == 1 { "" } else { "s" })
}

/// "never", "just now", "5 min ago", "3 h ago", "2 d ago".
pub fn ago(unix: u64, now: u64) -> String {
    if unix == 0 {
        return "never".into();
    }
    let s = now.saturating_sub(unix);
    match s {
        0..60 => "just now".into(),
        60..3600 => format!("{} min ago", s / 60),
        3600..86400 => format!("{} h ago", s / 3600),
        _ => format!("{} d ago", s / 86400),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blocker_core::config::{Config, Rule};
    use blocker_core::schedule::{parse_window, Day, Schedule};
    use blocker_core::status::{DaemonInfo, FilterInfo};

    fn rule(name: &str, schedule: Schedule) -> Rule {
        Rule {
            name: name.into(),
            enabled: true,
            locked: false,
            domains: vec!["reddit.com".into()],
            categories: vec!["social".into()],
            bundle_ids: vec!["com.valvesoftware.steam".into()],
            process_names: vec![],
            schedule,
        }
    }

    fn store_with(rules: Vec<Rule>) -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        store
            .save_config(&Config {
                rules,
                ..Config::default()
            })
            .unwrap();
        (dir, store)
    }

    const MON_10: WeekTime = WeekTime {
        day: Day::Mon,
        minute: 10 * 60,
    };
    const SAT_10: WeekTime = WeekTime {
        day: Day::Sat,
        minute: 10 * 60,
    };

    fn work() -> Schedule {
        Schedule::Windows {
            windows: vec![parse_window("mon-fri 09:00-17:00").unwrap()],
        }
    }

    fn system(ext: ExtensionState, enabled: Option<bool>) -> SystemStatus {
        SystemStatus {
            daemon: DaemonInfo::default(),
            filter: FilterInfo {
                extension: ext,
                enabled,
                ..FilterInfo::default()
            },
        }
    }

    #[test]
    fn active_rules_follow_the_schedule() {
        let (_d, store) = store_with(vec![rule("work", work()), rule("always", Schedule::Always)]);
        let mon = Snapshot::load(&store, MON_10);
        assert_eq!(mon.active_rules(), vec!["work", "always"]);
        assert_eq!(
            mon.activity(None),
            Activity::Blocking {
                rules: vec!["work".into(), "always".into()]
            }
        );
        let sat = Snapshot::load(&store, SAT_10);
        assert_eq!(sat.active_rules(), vec!["always"]);
        assert_eq!(sat.rules[0].summary(), "1 domain · social · 1 app");
        assert!(
            sat.warnings.iter().any(|w| w.contains("social")),
            "{:?}",
            sat.warnings
        );
        assert_eq!(
            sat.categories,
            vec![CategoryRow {
                name: "social".into(),
                domains: None
            }]
        );
    }

    #[test]
    fn idle_when_nothing_is_active() {
        let mut off = rule("off", Schedule::Always);
        off.enabled = false;
        let (_d, store) = store_with(vec![rule("work", work()), off]);
        let snap = Snapshot::load(&store, SAT_10);
        assert_eq!(
            snap.activity(Some(&system(ExtensionState::NotInstalled, None))),
            Activity::Idle
        );
    }

    #[test]
    fn attention_only_when_the_filter_is_known_to_be_off() {
        let (_d, store) = store_with(vec![rule("always", Schedule::Always)]);
        let snap = Snapshot::load(&store, MON_10);
        let blocking = Activity::Blocking {
            rules: vec!["always".into()],
        };
        assert_eq!(
            snap.activity(Some(&system(ExtensionState::Unknown, None))),
            blocking
        );
        assert_eq!(
            snap.activity(Some(&system(ExtensionState::Installed, Some(true)))),
            blocking
        );
        assert!(matches!(
            snap.activity(Some(&system(ExtensionState::Installed, Some(false)))),
            Activity::Attention(_)
        ));
        assert!(matches!(
            snap.activity(Some(&system(ExtensionState::AwaitingApproval, None))),
            Activity::Attention(_)
        ));
        assert!(matches!(
            snap.activity(Some(&system(ExtensionState::NotInstalled, None))),
            Activity::Attention(_)
        ));
    }

    #[test]
    fn broken_config_needs_attention() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("config.toml"), "[[rule]]\nname = 3\n").unwrap();
        let snap = Snapshot::load(&Store::new(dir.path()), MON_10);
        assert!(snap.error.is_some());
        assert!(
            matches!(snap.activity(None), Activity::Attention(w) if w.starts_with("config error"))
        );
    }

    #[test]
    fn locked_active_rules_cannot_be_toggled() {
        let mut r = rule("focus", work());
        r.locked = true;
        let (_d, store) = store_with(vec![r]);
        assert!(!Snapshot::load(&store, MON_10).rules[0].can_toggle());
        assert!(Snapshot::load(&store, SAT_10).rules[0].can_toggle());
    }

    #[test]
    fn relative_times() {
        assert_eq!(ago(0, 100), "never");
        assert_eq!(ago(100, 130), "just now");
        assert_eq!(ago(100, 100 + 5 * 60), "5 min ago");
        assert_eq!(ago(100, 100 + 3 * 3600), "3 h ago");
        assert_eq!(ago(100, 100 + 2 * 86400), "2 d ago");
    }
}
