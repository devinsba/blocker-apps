//! Turns a [`Config`] plus downloaded category sets into per-flow verdicts.
//!
//! The content filter calls [`Engine::decide`] once when a flow opens and,
//! if that returns [`Verdict::Peek`], again with the hostname parsed from the
//! first outbound bytes (TLS SNI or HTTP `Host`). The engine never sees
//! packets and never touches routing or DNS, which is what lets it sit
//! alongside VPN tunnels like Tailscale and Twingate.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;

use anyhow::Result;

use crate::config::{Config, Rule};
use crate::domain;
use crate::domainset::DomainSet;
use crate::guard::Guard;
use crate::schedule::{Day, Schedule};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Proto {
    Tcp,
    Udp,
    Other,
}

/// What the OS tells us about a new flow. Any field may be missing.
#[derive(Debug, Clone, Copy)]
pub struct FlowInfo<'a> {
    /// Hostname the app connected by, when the OS knows it.
    pub hostname: Option<&'a str>,
    pub remote_ip: Option<IpAddr>,
    pub remote_port: u16,
    pub proto: Proto,
    /// Code-signing identifier of the source app (usually its bundle id).
    pub app_id: Option<&'a str>,
    /// The peeked ClientHello offered Encrypted Client Hello, so the hostname
    /// seen is only the provider's public name.
    pub ech: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Allow,
    Block(Reason),
    /// Hostname unknown: inspect the first outbound bytes and decide again.
    Peek,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reason {
    pub rule: String,
    /// The domain, category entry, app id or "quic" that triggered the block.
    pub matched: String,
}

impl std::fmt::Display for Reason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "rule {:?} matched {}", self.rule, self.matched)
    }
}

/// Strips the Team ID prefix macOS puts on third-party signing identifiers
/// (`EQHXZ8M8AV.com.google.Chrome` -> `com.google.Chrome`) so rules and the
/// guard can match plain bundle ids.
pub fn normalize_app_id(id: &str) -> &str {
    match id.split_once('.') {
        Some((team, rest))
            if team.len() == 10
                && !rest.is_empty()
                && team
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit()) =>
        {
            rest
        }
        _ => id,
    }
}

/// A point in the local week at minute resolution, as schedules see it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WeekTime {
    pub day: Day,
    pub minute: u16,
}

impl WeekTime {
    pub fn now() -> Self {
        use chrono::{Datelike, Timelike};
        let now = chrono::Local::now();
        Self {
            day: now.weekday().into(),
            minute: (now.hour() * 60 + now.minute()) as u16,
        }
    }
}

struct CompiledRule {
    name: String,
    schedule: Schedule,
    domains: DomainSet,
    categories: Vec<(String, Arc<DomainSet>)>,
    bundle_ids: Vec<String>,
    process_names: Vec<String>,
}

impl CompiledRule {
    fn blocks_hosts(&self) -> bool {
        !self.domains.is_empty() || !self.categories.is_empty()
    }

    fn match_host(&self, host: &str) -> Option<String> {
        if let Some(d) = self.domains.find(host) {
            return Some(d);
        }
        self.categories
            .iter()
            .find_map(|(name, set)| set.find(host).map(|d| format!("{d} (category {name})")))
    }
}

pub struct Engine {
    rules: Vec<CompiledRule>,
    guard: Guard,
    block_quic: bool,
    block_ech: bool,
}

/// Apps and process names the app agent should keep closed right now.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct BlockedApps {
    pub bundle_ids: Vec<String>,
    pub process_names: Vec<String>,
}

impl Engine {
    /// Compiles `config`. `categories` holds the verified sets downloaded from
    /// the list server; rules naming a missing category still enforce their
    /// other entries, and the gap is reported in the returned warnings.
    pub fn new(
        config: &Config,
        categories: &HashMap<String, Arc<DomainSet>>,
    ) -> Result<(Self, Vec<String>)> {
        let extra: Vec<String> = config
            .server
            .as_ref()
            .and_then(|s| server_host(&s.url))
            .into_iter()
            .collect();
        let (guard, mut warnings) = Guard::new(&config.settings, &extra);

        let mut rules = Vec::new();
        for rule in config.rules.iter().filter(|r| r.enabled) {
            rules.push(compile_rule(rule, categories, &guard, &mut warnings)?);
        }
        Ok((
            Self {
                rules,
                guard,
                block_quic: config.settings.block_quic,
                block_ech: config.settings.block_ech,
            },
            warnings,
        ))
    }

    fn active(&self, at: WeekTime) -> impl Iterator<Item = &CompiledRule> {
        self.rules
            .iter()
            .filter(move |r| r.schedule.is_active_at(at.day, at.minute))
    }

    pub fn active_rules(&self, at: WeekTime) -> Vec<&str> {
        self.active(at).map(|r| r.name.as_str()).collect()
    }

    /// True if no rule could block anything at `at`; lets the filter skip work.
    pub fn is_idle(&self, at: WeekTime) -> bool {
        self.active(at).next().is_none()
    }

    /// Decides a flow. `peeked` carries the hostname found in the flow's first
    /// bytes on the second call (`Some(None)` if inspection found nothing).
    pub fn decide(&self, flow: &FlowInfo, peeked: Option<Option<&str>>, at: WeekTime) -> Verdict {
        if flow.app_id.is_some_and(|id| self.guard.protects_app(id)) {
            return Verdict::Allow;
        }
        if flow.remote_ip.is_some_and(|ip| self.guard.protects_ip(ip)) {
            return Verdict::Allow;
        }

        let active: Vec<&CompiledRule> = self.active(at).collect();
        if active.is_empty() {
            return Verdict::Allow;
        }

        if let Some(app) = flow.app_id {
            for rule in &active {
                if rule.bundle_ids.iter().any(|b| b == app) {
                    return block(rule, app);
                }
            }
        }

        let host = peeked
            .flatten()
            .or(flow.hostname)
            .and_then(domain::normalize);
        if let Some(host) = host {
            if let Some(v) = self.check_host_in(&active, &host) {
                return v;
            }
            if flow.ech && self.block_ech {
                if let Some(rule) = active.iter().find(|r| r.blocks_hosts()) {
                    return block(rule, "encrypted client hello");
                }
            }
            return Verdict::Allow;
        }

        let blocks_hosts = active.iter().find(|r| r.blocks_hosts());
        match (blocks_hosts, flow.proto) {
            (None, _) => Verdict::Allow,
            (Some(_), Proto::Tcp) if peeked.is_none() => Verdict::Peek,
            (Some(rule), Proto::Udp) if self.block_quic && flow.remote_port == 443 => {
                block(rule, "quic")
            }
            _ => Verdict::Allow,
        }
    }

    /// Would `host` be blocked at `at`? Used by the CLI's `check` command.
    pub fn check_host(&self, host: &str, at: WeekTime) -> Verdict {
        let Some(host) = domain::normalize(host) else {
            return Verdict::Allow;
        };
        let active: Vec<&CompiledRule> = self.active(at).collect();
        self.check_host_in(&active, &host).unwrap_or(Verdict::Allow)
    }

    fn check_host_in(&self, active: &[&CompiledRule], host: &str) -> Option<Verdict> {
        if self.guard.protects_host(host) {
            return Some(Verdict::Allow);
        }
        active
            .iter()
            .find_map(|rule| rule.match_host(host).map(|m| block(rule, &m)))
    }

    pub fn blocked_apps(&self, at: WeekTime) -> BlockedApps {
        let mut out = BlockedApps::default();
        for rule in self.active(at) {
            out.bundle_ids.extend(rule.bundle_ids.iter().cloned());
            out.process_names.extend(rule.process_names.iter().cloned());
        }
        out.bundle_ids.sort();
        out.bundle_ids.dedup();
        out.process_names.sort();
        out.process_names.dedup();
        out
    }

    pub fn guard(&self) -> &Guard {
        &self.guard
    }
}

fn block(rule: &CompiledRule, matched: &str) -> Verdict {
    Verdict::Block(Reason {
        rule: rule.name.clone(),
        matched: matched.to_string(),
    })
}

fn compile_rule(
    rule: &Rule,
    categories: &HashMap<String, Arc<DomainSet>>,
    guard: &Guard,
    warnings: &mut Vec<String>,
) -> Result<CompiledRule> {
    let mut domains = Vec::new();
    for raw in &rule.domains {
        match domain::normalize(raw) {
            Some(d) if !d.contains('.') => warnings.push(format!(
                "rule {:?}: refusing to block whole TLD {d:?}",
                rule.name
            )),
            Some(d) => match guard.conflict(&d) {
                Some(p) => warnings.push(format!(
                    "rule {:?}: {d:?} would block protected {p:?}; skipped",
                    rule.name
                )),
                None => domains.push(d),
            },
            None => warnings.push(format!("rule {:?}: invalid domain {raw:?}", rule.name)),
        }
    }

    let mut cats = Vec::new();
    for name in &rule.categories {
        match categories.get(name) {
            Some(set) => cats.push((name.clone(), Arc::clone(set))),
            None => warnings.push(format!(
                "rule {:?}: category {name:?} not downloaded yet",
                rule.name
            )),
        }
    }

    Ok(CompiledRule {
        name: rule.name.clone(),
        schedule: rule.schedule.clone(),
        domains: DomainSet::from_domains(&domains)?,
        categories: cats,
        bundle_ids: rule.bundle_ids.clone(),
        process_names: rule.process_names.clone(),
    })
}

/// Host part of the list server URL, so it is always reachable for updates.
fn server_host(url: &str) -> Option<String> {
    domain::normalize(url)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ServerConfig, Settings};
    use crate::schedule::{HhMm, TimeWindow};

    const MON_NOON: WeekTime = WeekTime {
        day: Day::Mon,
        minute: 12 * 60,
    };
    const SAT_NOON: WeekTime = WeekTime {
        day: Day::Sat,
        minute: 12 * 60,
    };

    fn rule(name: &str) -> Rule {
        Rule {
            name: name.into(),
            enabled: true,
            locked: false,
            domains: vec![],
            categories: vec![],
            bundle_ids: vec![],
            process_names: vec![],
            schedule: Schedule::Always,
        }
    }

    fn flow<'a>(
        hostname: Option<&'a str>,
        proto: Proto,
        port: u16,
        app: Option<&'a str>,
    ) -> FlowInfo<'a> {
        FlowInfo {
            hostname,
            remote_ip: Some("93.184.216.34".parse().unwrap()),
            remote_port: port,
            proto,
            app_id: app,
            ech: false,
        }
    }

    fn engine(rules: Vec<Rule>, cats: &[(&str, &[&str])]) -> Engine {
        let config = Config {
            settings: Settings::default(),
            server: Some(ServerConfig {
                url: "https://lists.example.org".into(),
                public_key: String::new(),
                refresh_minutes: 60,
            }),
            rules,
        };
        let categories = cats
            .iter()
            .map(|(n, d)| {
                (
                    n.to_string(),
                    Arc::new(DomainSet::from_domains(d.iter()).unwrap()),
                )
            })
            .collect();
        Engine::new(&config, &categories).unwrap().0
    }

    fn is_block(v: &Verdict) -> bool {
        matches!(v, Verdict::Block(_))
    }

    #[test]
    fn blocks_rule_domains_and_categories_by_hostname() {
        let mut r = rule("focus");
        r.domains = vec!["https://YouTube.com/watch?v=1".into()];
        r.categories = vec!["social".into()];
        let e = engine(vec![r], &[("social", &["reddit.com"])]);
        assert!(is_block(&e.decide(
            &flow(Some("m.youtube.com"), Proto::Tcp, 443, None),
            None,
            MON_NOON
        )));
        assert!(!is_block(&e.decide(
            &flow(Some("notyoutube.com"), Proto::Tcp, 443, None),
            None,
            MON_NOON
        )));
        assert!(is_block(&e.decide(
            &flow(Some("old.reddit.com"), Proto::Tcp, 443, None),
            None,
            MON_NOON
        )));
        assert_eq!(
            e.decide(
                &flow(Some("example.com"), Proto::Tcp, 443, None),
                None,
                MON_NOON
            ),
            Verdict::Allow
        );
    }

    #[test]
    fn unknown_host_peeks_then_uses_sni() {
        let mut r = rule("focus");
        r.domains = vec!["reddit.com".into()];
        let e = engine(vec![r], &[]);
        let f = flow(None, Proto::Tcp, 443, Some("com.google.Chrome"));
        assert_eq!(e.decide(&f, None, MON_NOON), Verdict::Peek);
        assert!(is_block(&e.decide(
            &f,
            Some(Some("www.reddit.com")),
            MON_NOON
        )));
        assert_eq!(
            e.decide(&f, Some(Some("example.com")), MON_NOON),
            Verdict::Allow
        );
        assert_eq!(e.decide(&f, Some(None), MON_NOON), Verdict::Allow);
    }

    #[test]
    fn quic_to_unknown_host_is_dropped_only_while_blocking() {
        let mut r = rule("focus");
        r.domains = vec!["reddit.com".into()];
        r.schedule = Schedule::Windows {
            windows: vec![TimeWindow {
                days: vec![Day::Mon],
                start: HhMm(9 * 60),
                end: HhMm(17 * 60),
            }],
        };
        let e = engine(vec![r], &[]);
        let f = flow(None, Proto::Udp, 443, None);
        assert!(is_block(&e.decide(&f, None, MON_NOON)));
        assert_eq!(e.decide(&f, None, SAT_NOON), Verdict::Allow);
        assert_eq!(
            e.decide(&flow(None, Proto::Udp, 53, None), None, MON_NOON),
            Verdict::Allow
        );
    }

    #[test]
    fn ech_is_blocked_only_in_strict_mode_and_only_while_active() {
        let mut r = rule("focus");
        r.domains = vec!["reddit.com".into()];
        r.schedule = Schedule::Windows {
            windows: vec![TimeWindow {
                days: vec![Day::Mon],
                start: HhMm(9 * 60),
                end: HhMm(17 * 60),
            }],
        };
        let mut f = flow(None, Proto::Tcp, 443, None);
        f.ech = true;
        let lenient = engine(vec![r.clone()], &[]);
        assert_eq!(
            lenient.decide(&f, Some(Some("cloudflare-ech.com")), MON_NOON),
            Verdict::Allow
        );

        let config = Config {
            settings: Settings {
                block_ech: true,
                ..Settings::default()
            },
            rules: vec![r],
            ..Config::default()
        };
        let strict = Engine::new(&config, &HashMap::new()).unwrap().0;
        assert!(is_block(&strict.decide(
            &f,
            Some(Some("cloudflare-ech.com")),
            MON_NOON
        )));
        assert_eq!(
            strict.decide(&f, Some(Some("cloudflare-ech.com")), SAT_NOON),
            Verdict::Allow
        );
        f.ech = false;
        assert_eq!(
            strict.decide(&f, Some(Some("example.com")), MON_NOON),
            Verdict::Allow
        );
    }

    #[test]
    fn app_rules_block_network_for_that_app() {
        let mut r = rule("games");
        r.bundle_ids = vec!["com.valvesoftware.steam".into()];
        let e = engine(vec![r], &[]);
        assert!(is_block(&e.decide(
            &flow(
                Some("example.com"),
                Proto::Tcp,
                443,
                Some("com.valvesoftware.steam")
            ),
            None,
            MON_NOON
        )));
        assert_eq!(
            e.decide(
                &flow(None, Proto::Tcp, 443, Some("com.apple.Safari")),
                None,
                MON_NOON
            ),
            Verdict::Allow
        );
        assert_eq!(
            e.blocked_apps(MON_NOON).bundle_ids,
            vec!["com.valvesoftware.steam"]
        );
    }

    #[test]
    fn vpn_tools_and_their_addresses_are_never_blocked() {
        let mut r = rule("everything");
        r.domains = vec!["example.com".into()];
        r.categories = vec!["all".into()];
        r.bundle_ids = vec!["io.tailscale.ipn.macsys".into()];
        let e = engine(
            vec![r],
            &[("all", &["tailscale.com", "twingate.com", "example.net"])],
        );
        // Tailscale's own app, even if a rule names it.
        assert_eq!(
            e.decide(
                &flow(
                    Some("example.com"),
                    Proto::Tcp,
                    443,
                    Some("io.tailscale.ipn.macsys")
                ),
                None,
                MON_NOON
            ),
            Verdict::Allow
        );
        // Control planes listed in a category.
        assert_eq!(
            e.decide(
                &flow(Some("controlplane.tailscale.com"), Proto::Tcp, 443, None),
                None,
                MON_NOON
            ),
            Verdict::Allow
        );
        assert_eq!(
            e.decide(
                &flow(Some("acme.twingate.com"), Proto::Tcp, 443, None),
                None,
                MON_NOON
            ),
            Verdict::Allow
        );
        // Tailnet / Twingate CGNAT addresses, even for blocked names and QUIC.
        let mut f = flow(Some("example.com"), Proto::Udp, 443, None);
        f.remote_ip = Some("100.101.102.103".parse().unwrap());
        assert_eq!(e.decide(&f, None, MON_NOON), Verdict::Allow);
        // The list server stays reachable.
        assert_eq!(e.check_host("lists.example.org", MON_NOON), Verdict::Allow);
    }

    #[test]
    fn rejects_rules_that_would_cover_protected_domains() {
        let config = Config {
            rules: vec![Rule {
                domains: vec!["ts.net".into(), "com".into(), "ok.com".into()],
                ..rule("bad")
            }],
            ..Config::default()
        };
        let (e, warnings) = Engine::new(&config, &HashMap::new()).unwrap();
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert!(is_block(&e.check_host("ok.com", MON_NOON)));
    }

    #[test]
    fn missing_category_warns_but_rule_still_applies() {
        let mut r = rule("focus");
        r.domains = vec!["reddit.com".into()];
        r.categories = vec!["gambling".into()];
        let config = Config {
            rules: vec![r],
            ..Config::default()
        };
        let (e, warnings) = Engine::new(&config, &HashMap::new()).unwrap();
        assert!(warnings.iter().any(|w| w.contains("gambling")));
        assert!(is_block(&e.check_host("reddit.com", MON_NOON)));
    }

    #[test]
    fn strips_team_id_prefix_only() {
        assert_eq!(
            normalize_app_id("EQHXZ8M8AV.com.google.Chrome"),
            "com.google.Chrome"
        );
        assert_eq!(normalize_app_id("com.apple.Safari"), "com.apple.Safari");
        assert_eq!(
            normalize_app_id("W5364U7YZB.io.tailscale.ipn.macsys.network-extension"),
            "io.tailscale.ipn.macsys.network-extension"
        );
        assert_eq!(normalize_app_id("lowercase1.com.x"), "lowercase1.com.x");
        assert_eq!(normalize_app_id("ABCDEFGHIJ."), "ABCDEFGHIJ.");
    }

    #[test]
    fn disabled_and_inactive_rules_do_nothing() {
        let mut r = rule("off");
        r.domains = vec!["reddit.com".into()];
        r.enabled = false;
        let e = engine(vec![r], &[]);
        assert!(e.is_idle(MON_NOON));
        assert_eq!(
            e.decide(&flow(None, Proto::Tcp, 443, None), None, MON_NOON),
            Verdict::Allow
        );
    }
}
