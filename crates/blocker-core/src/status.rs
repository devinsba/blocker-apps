//! Machine-readable system status, printed by `blocker status --json` and read
//! by the menu-bar app. Only the signed `blocker` executable can query the
//! content filter, so the UI asks it rather than calling NetworkExtension.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SystemStatus {
    pub daemon: DaemonInfo,
    pub filter: FilterInfo,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DaemonInfo {
    /// The LaunchDaemon is loaded.
    pub installed: bool,
    pub running: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionState {
    Installed,
    AwaitingApproval,
    NotInstalled,
    /// The system couldn't be asked (e.g. not run from Blocker.app).
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilterInfo {
    pub extension: ExtensionState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// `None` when the filter preferences couldn't be read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub configured: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl SystemStatus {
    /// True when the network filter is known to be running.
    pub fn filtering(&self) -> bool {
        self.filter.extension == ExtensionState::Installed && self.filter.enabled == Some(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_round_trip_and_defaults() {
        let s = SystemStatus {
            daemon: DaemonInfo {
                installed: true,
                running: true,
                pid: Some(42),
            },
            filter: FilterInfo {
                extension: ExtensionState::Installed,
                version: Some("0.1.0".into()),
                configured: Some(true),
                enabled: Some(true),
                error: None,
            },
        };
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains(r#""extension":"installed""#), "{json}");
        assert_eq!(serde_json::from_str::<SystemStatus>(&json).unwrap(), s);
        assert!(s.filtering());

        let sparse: SystemStatus = serde_json::from_str(
            r#"{"daemon":{"installed":false,"running":false},"filter":{"extension":"unknown"}}"#,
        )
        .unwrap();
        assert!(!sparse.filtering());
        assert_eq!(sparse.filter.enabled, None);
    }
}
