//! The guard: things that are never blocked, so the blocker cannot break
//! VPN / zero-trust tools (Tailscale, Twingate), Apple's own networking, or
//! itself.
//!
//! The guard is consulted on every flow, before any rule. It combines a
//! built-in set with the user's `allow_*` settings and caller-supplied extra
//! domains (such as the blocklist server's host).

use std::collections::HashSet;
use std::net::IpAddr;

use ipnet::IpNet;

use crate::config::Settings;
use crate::domain;

/// Built-in protected domains (suffix match: the domain and everything under it).
pub const PROTECTED_DOMAINS: &[&str] = &[
    // Tailscale: control plane, login, DERP relays, downloads; ts.net is MagicDNS.
    "tailscale.com",
    "tailscale.io",
    "tailscale.net",
    "ts.net",
    // Twingate: controller, client login and relays.
    "twingate.com",
    "twingate.net",
    // Apple: captive portal detection, certificate revocation/validation.
    "captive.apple.com",
    "ocsp.apple.com",
    "ocsp2.apple.com",
    "valid.apple.com",
    "crl.apple.com",
];

/// Built-in protected CIDRs: CGNAT (Tailscale/Twingate), Tailscale ULA, loopback,
/// private, link-local, multicast and unspecified/broadcast addresses.
pub const PROTECTED_CIDRS: &[&str] = &[
    "100.64.0.0/10",
    "fd7a:115c:a1e0::/48",
    "127.0.0.0/8",
    "::1/128",
    "10.0.0.0/8",
    "172.16.0.0/12",
    "192.168.0.0/16",
    "169.254.0.0/16",
    "fe80::/10",
    "fc00::/7",
    "224.0.0.0/4",
    "ff00::/8",
    "0.0.0.0/32",
    "::/128",
    "255.255.255.255/32",
];

/// Built-in protected app signing identifiers / bundle id prefixes. An entry
/// ending in `.` is a prefix; any other entry must match exactly.
pub const PROTECTED_APPS: &[&str] = &[
    "io.tailscale.",
    "com.tailscale.",
    "tailscaled",
    "com.twingate.",
    "com.briandevins.blocker",
    "com.briandevins.blocker.",
    "com.apple.mDNSResponder",
    "com.apple.networkd",
    "com.apple.nesessionmanager",
    "com.apple.nehelper",
    "com.apple.trustd",
    "com.apple.apsd",
];

/// Immutable set of never-blocked domains, networks and apps.
#[derive(Debug, Clone)]
pub struct Guard {
    /// Normalized protected domains, for O(labels) suffix lookups.
    domain_set: HashSet<String>,
    /// Same domains in insertion order, for [`Guard::conflict`].
    domain_list: Vec<String>,
    nets: Vec<IpNet>,
    apps: Vec<String>,
}

impl Guard {
    /// Builds a guard from the built-ins, `settings.allow_*` and `extra_domains`
    /// (e.g. the blocklist server's own host so we can always update).
    ///
    /// Invalid user entries are skipped and returned as warnings.
    pub fn new(settings: &Settings, extra_domains: &[String]) -> (Guard, Vec<String>) {
        let mut warnings = Vec::new();
        let mut g = Guard {
            domain_set: HashSet::new(),
            domain_list: Vec::new(),
            nets: Vec::new(),
            apps: Vec::new(),
        };

        for d in PROTECTED_DOMAINS {
            g.add_domain(d);
        }
        for d in settings.allow_domains.iter().chain(extra_domains) {
            if !g.add_domain(d) {
                warnings.push(format!("ignoring invalid allowed domain {d:?}"));
            }
        }

        for c in PROTECTED_CIDRS {
            if let Ok(n) = c.parse::<IpNet>() {
                g.nets.push(n);
            }
        }
        for c in &settings.allow_cidrs {
            match parse_net(c) {
                Some(n) => g.nets.push(n),
                None => warnings.push(format!("ignoring invalid allowed CIDR {c:?}")),
            }
        }

        g.apps.extend(PROTECTED_APPS.iter().map(|s| s.to_string()));
        for a in &settings.allow_apps {
            let a = a.trim();
            if a.is_empty() || a.chars().any(char::is_whitespace) {
                warnings.push(format!("ignoring invalid allowed app {a:?}"));
            } else {
                g.apps.push(a.to_string());
            }
        }

        (g, warnings)
    }

    fn add_domain(&mut self, d: &str) -> bool {
        match domain::normalize(d) {
            Some(n) => {
                if self.domain_set.insert(n.clone()) {
                    self.domain_list.push(n);
                }
                true
            }
            None => false,
        }
    }

    /// True if `host` (already normalized) is equal to or under any protected domain.
    pub fn protects_host(&self, host: &str) -> bool {
        self.covering(host).is_some()
    }

    /// Finds the protected domain that equals or is a parent of `host`.
    fn covering<'a>(&'a self, host: &str) -> Option<&'a String> {
        let mut s = host.trim_end_matches('.');
        loop {
            if let Some(d) = self.domain_set.get(s) {
                return Some(d);
            }
            match s.split_once('.') {
                Some((_, rest)) => s = rest,
                None => return None,
            }
        }
    }

    /// True if `ip` lies in any protected network.
    pub fn protects_ip(&self, ip: IpAddr) -> bool {
        // Treat IPv4-mapped IPv6 (::ffff:a.b.c.d) as the embedded IPv4 address.
        let ip = match ip {
            IpAddr::V6(v6) => v6.to_ipv4_mapped().map(IpAddr::V4).unwrap_or(ip),
            v4 => v4,
        };
        self.nets.iter().any(|n| n.contains(&ip))
    }

    /// True if `app_id` (a signing identifier or bundle id) is protected.
    /// Case-sensitive, as macOS signing ids are.
    pub fn protects_app(&self, app_id: &str) -> bool {
        self.apps
            .iter()
            .any(|p| app_id == p || (p.ends_with('.') && app_id.starts_with(p.as_str())))
    }

    /// Validates a user rule domain before it is saved. Blocking `rule_domain`
    /// blocks its whole subtree, so this returns the protected domain it would
    /// catch: either `rule_domain` is under a protected domain, or a protected
    /// domain is under `rule_domain` (rule `net` catches `tailscale.net`).
    pub fn conflict(&self, rule_domain: &str) -> Option<String> {
        let rule = domain::normalize(rule_domain)?;
        if let Some(d) = self.covering(&rule) {
            return Some(d.clone());
        }
        self.domain_list
            .iter()
            .find(|p| domain::is_subdomain_of(p, &rule))
            .cloned()
    }
}

fn parse_net(s: &str) -> Option<IpNet> {
    let s = s.trim();
    s.parse::<IpNet>()
        .ok()
        .or_else(|| s.parse::<IpAddr>().ok().map(IpNet::from))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guard() -> Guard {
        Guard::new(&Settings::default(), &[]).0
    }

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn builtin_domains() {
        let g = guard();
        for d in PROTECTED_DOMAINS {
            assert!(g.protects_host(d), "{d}");
            assert!(g.protects_host(&format!("a.b.{d}")), "{d}");
        }
        assert!(g.protects_host("login.tailscale.com"));
        assert!(g.protects_host("my-machine.tail1234.ts.net"));
        assert!(g.protects_host("controlplane.tailscale.com"));
        assert!(g.protects_host("acme.twingate.com"));
        assert!(!g.protects_host("nottailscale.com"));
        assert!(!g.protects_host("tailscale.com.evil.org"));
        assert!(!g.protects_host("apple.com"));
        assert!(!g.protects_host("net"));
        assert!(!g.protects_host("youtube.com"));
        assert!(g.protects_host("tailscale.com."));
    }

    #[test]
    fn builtin_ips() {
        let g = guard();
        for s in [
            "100.100.100.100",
            "100.96.0.1",
            "100.64.0.1",
            "100.127.255.254",
            "fd7a:115c:a1e0::1",
            "fd7a:115c:a1e0:ab12:4843:cd96:6262:1234",
            "127.0.0.1",
            "::1",
            "10.1.2.3",
            "172.20.0.1",
            "192.168.1.1",
            "169.254.1.1",
            "fe80::1",
            "fd00::1",
            "224.0.0.251",
            "ff02::fb",
            "0.0.0.0",
            "::",
            "255.255.255.255",
            "::ffff:192.168.1.1",
        ] {
            assert!(g.protects_ip(ip(s)), "{s}");
        }
        for s in [
            "8.8.8.8",
            "100.63.255.255",
            "100.128.0.0",
            "172.32.0.1",
            "2001:4860:4860::8888",
            "2606:4700::1111",
        ] {
            assert!(!g.protects_ip(ip(s)), "{s}");
        }
    }

    #[test]
    fn builtin_apps() {
        let g = guard();
        assert!(g.protects_app("io.tailscale.ipn.macsys.network-extension"));
        assert!(g.protects_app("com.tailscale.ipn.macos"));
        assert!(g.protects_app("tailscaled"));
        assert!(g.protects_app("com.twingate.macos"));
        assert!(g.protects_app("com.briandevins.blocker"));
        assert!(g.protects_app("com.briandevins.blocker.filter"));
        assert!(g.protects_app("com.apple.mDNSResponder"));
        assert!(g.protects_app("com.apple.trustd"));
        assert!(!g.protects_app("io.tailscalex"));
        assert!(!g.protects_app("io.tailscale"));
        assert!(!g.protects_app("tailscaled2"));
        assert!(!g.protects_app("com.apple.mDNSResponderHelper"));
        assert!(!g.protects_app("com.apple.Safari"));
        assert!(!g.protects_app("IO.TAILSCALE.app"));
    }

    #[test]
    fn user_allow_entries() {
        let s = Settings {
            allow_domains: vec!["Corp.Example.COM".into()],
            allow_cidrs: vec!["203.0.113.0/24".into(), "198.51.100.7".into()],
            allow_apps: vec!["com.corp.vpn.".into(), "corpd".into()],
            ..Settings::default()
        };
        let (g, w) = Guard::new(&s, &["lists.example.org".to_string()]);
        assert!(w.is_empty(), "{w:?}");
        assert!(g.protects_host("vpn.corp.example.com"));
        assert!(g.protects_host("lists.example.org"));
        assert!(!g.protects_host("example.com"));
        assert!(g.protects_ip(ip("203.0.113.9")));
        assert!(g.protects_ip(ip("198.51.100.7")));
        assert!(!g.protects_ip(ip("198.51.100.8")));
        assert!(g.protects_app("com.corp.vpn.helper"));
        assert!(g.protects_app("corpd"));
        assert!(!g.protects_app("corpd2"));
    }

    #[test]
    fn invalid_user_entries_warn() {
        let s = Settings {
            allow_domains: vec!["bad domain!".into(), "ok.example.com".into()],
            allow_cidrs: vec!["not-a-cidr".into(), "10.0.0.0/99".into()],
            allow_apps: vec!["".into(), "has space".into()],
            ..Settings::default()
        };
        let (g, w) = Guard::new(&s, &["".to_string()]);
        assert_eq!(w.len(), 6, "{w:?}");
        assert!(g.protects_host("ok.example.com"));
        assert!(g.protects_host("tailscale.com"));
    }

    #[test]
    fn conflict_cases() {
        let g = guard();
        assert_eq!(
            g.conflict("tailscale.com").as_deref(),
            Some("tailscale.com")
        );
        assert_eq!(
            g.conflict("login.tailscale.com").as_deref(),
            Some("tailscale.com")
        );
        assert_eq!(g.conflict("x.y.ts.net").as_deref(), Some("ts.net"));
        assert_eq!(g.conflict("net").as_deref(), Some("tailscale.net"));
        assert_eq!(g.conflict("com").as_deref(), Some("tailscale.com"));
        assert_eq!(
            g.conflict("apple.com").as_deref(),
            Some("captive.apple.com")
        );
        assert_eq!(
            g.conflict("Captive.Apple.com").as_deref(),
            Some("captive.apple.com")
        );
        assert_eq!(g.conflict("youtube.com"), None);
        assert_eq!(g.conflict("nottailscale.com"), None);
        assert_eq!(g.conflict("not a domain!"), None);
    }
}
