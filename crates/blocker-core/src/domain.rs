//! Domain-name normalization and suffix helpers.
//!
//! All other modules compare domains in the normalized form produced by
//! [`normalize`]: bare, lowercase, ASCII (punycode), no trailing dot.

/// Normalizes user/list input to a bare lowercase ASCII (punycode) domain, or
/// `None` if it isn't a valid domain.
///
/// Accepts and strips: surrounding whitespace, a URL scheme (`https://`),
/// userinfo (`user@`), path/query/fragment, port (`:443`), a leading `*.` or
/// `.` wildcard marker, and a trailing dot. Converts IDN (unicode) to
/// punycode. Rejects: empty input, IP literals (v4, v6, bracketed v6), empty
/// labels, labels over 63 chars, names over 253 chars, characters other than
/// `[a-z0-9-_]` in a label, and labels starting or ending with `-`.
/// Single-label names (e.g. `localhost`, `com`) are accepted; callers decide
/// whether that is meaningful. A purely numeric final label is rejected, as it
/// would be an IPv4 address (or look like one).
pub fn normalize(input: &str) -> Option<String> {
    let mut s = input.trim();
    if let Some(i) = s.find("://") {
        s = &s[i + 3..];
    }
    if let Some(i) = s.find(['/', '?', '#']) {
        s = &s[..i];
    }
    if let Some(i) = s.rfind('@') {
        s = &s[i + 1..];
    }
    if s.starts_with('[') {
        return None;
    }
    if let Some(i) = s.find(':') {
        let port = &s[i + 1..];
        // Bare IPv6 (more colons) and non-numeric ports are rejected here.
        if !port.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        s = &s[..i];
    }
    if let Some(rest) = s.strip_prefix("*.") {
        s = rest;
    } else if let Some(rest) = s.strip_prefix('.') {
        s = rest;
    }
    if let Some(rest) = s.strip_suffix('.') {
        s = rest;
    }
    if s.is_empty() {
        return None;
    }

    let out = if s.is_ascii() {
        if s.bytes().any(|b| b.is_ascii_uppercase()) {
            s.to_ascii_lowercase()
        } else {
            s.to_owned()
        }
    } else {
        idna::domain_to_ascii(s).ok()?
    };
    valid(&out).then_some(out)
}

/// Validates an already-ASCII-lowercased candidate.
fn valid(d: &str) -> bool {
    if d.is_empty() || d.len() > 253 {
        return false;
    }
    let mut last = "";
    for label in d.split('.') {
        let b = label.as_bytes();
        if b.is_empty() || b.len() > 63 || b[0] == b'-' || b[b.len() - 1] == b'-' {
            return false;
        }
        if !b
            .iter()
            .all(|&c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-' || c == b'_')
        {
            return false;
        }
        last = label;
    }
    // Numeric TLD => IPv4 literal (or something that would parse like one).
    !last.bytes().all(|c| c.is_ascii_digit())
}

/// `"ads.example.com"` -> `"com.example.ads"` (labels reversed, `.` joined).
/// Input is assumed normalized.
pub fn reversed(domain: &str) -> String {
    let mut out = String::with_capacity(domain.len());
    for (i, label) in domain.rsplit('.').enumerate() {
        if i > 0 {
            out.push('.');
        }
        out.push_str(label);
    }
    out
}

/// Yields the domain and each parent: `"a.b.c"` -> `["a.b.c", "b.c", "c"]`.
/// Does not allocate; yields subslices of `domain`.
pub fn suffixes(domain: &str) -> impl Iterator<Item = &str> {
    std::iter::successors(Some(domain).filter(|d| !d.is_empty()), |d| {
        d.find('.').map(|i| &d[i + 1..]).filter(|r| !r.is_empty())
    })
}

/// True if `domain` == `parent` or `domain` ends with `"." + parent`. Both are
/// assumed normalized.
pub fn is_subdomain_of(domain: &str, parent: &str) -> bool {
    if domain.len() < parent.len() || !domain.ends_with(parent) {
        return false;
    }
    let cut = domain.len() - parent.len();
    cut == 0 || domain.as_bytes()[cut - 1] == b'.'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(s: &str) -> Option<String> {
        normalize(s)
    }

    #[test]
    fn plain_and_case() {
        assert_eq!(n("example.com").as_deref(), Some("example.com"));
        assert_eq!(n("Example.COM").as_deref(), Some("example.com"));
        assert_eq!(n("  example.com\n").as_deref(), Some("example.com"));
    }

    #[test]
    fn strips_url_parts() {
        assert_eq!(
            n("HTTPS://User@WWW.Example.COM:8080/path?q#f").as_deref(),
            Some("www.example.com")
        );
        assert_eq!(n("https://example.com").as_deref(), Some("example.com"));
        assert_eq!(n("example.com:443").as_deref(), Some("example.com"));
        assert_eq!(n("example.com/a/b").as_deref(), Some("example.com"));
        assert_eq!(n("example.com?x=1").as_deref(), Some("example.com"));
        assert_eq!(n("example.com#frag").as_deref(), Some("example.com"));
        assert_eq!(n("user:pw@example.com").as_deref(), Some("example.com"));
        assert_eq!(n("example.com:").as_deref(), Some("example.com"));
    }

    #[test]
    fn wildcard_and_trailing_dot() {
        assert_eq!(n("*.example.com").as_deref(), Some("example.com"));
        assert_eq!(n(".example.com").as_deref(), Some("example.com"));
        assert_eq!(n("example.com.").as_deref(), Some("example.com"));
        assert_eq!(n("*.example.com.").as_deref(), Some("example.com"));
        assert_eq!(n("*.*.example.com"), None);
        assert_eq!(n("example.com.."), None);
    }

    #[test]
    fn idn() {
        assert_eq!(n("bücher.de").as_deref(), Some("xn--bcher-kva.de"));
        assert_eq!(n("BÜCHER.de").as_deref(), Some("xn--bcher-kva.de"));
        assert_eq!(
            n("https://bücher.de/x").as_deref(),
            Some("xn--bcher-kva.de")
        );
        assert_eq!(n("xn--bcher-kva.de").as_deref(), Some("xn--bcher-kva.de"));
    }

    #[test]
    fn rejects_empty() {
        assert_eq!(n(""), None);
        assert_eq!(n("   "), None);
        assert_eq!(n("."), None);
        assert_eq!(n("*."), None);
        assert_eq!(n("https://"), None);
        assert_eq!(n("/path"), None);
    }

    #[test]
    fn rejects_ips() {
        assert_eq!(n("1.2.3.4"), None);
        assert_eq!(n("1.2.3.4:80"), None);
        assert_eq!(n("http://127.0.0.1/"), None);
        assert_eq!(n("[::1]"), None);
        assert_eq!(n("[::1]:8080"), None);
        assert_eq!(n("::1"), None);
        assert_eq!(n("2001:db8::1"), None);
        assert_eq!(n("123"), None);
        // A numeric label that is not the TLD is fine.
        assert_eq!(n("1.example.com").as_deref(), Some("1.example.com"));
    }

    #[test]
    fn rejects_bad_labels() {
        assert_eq!(n("a..b"), None);
        assert_eq!(n("-a.com"), None);
        assert_eq!(n("a-.com"), None);
        assert_eq!(n("a.-b.com"), None);
        assert_eq!(n("a b.com"), None);
        assert_eq!(n("a!.com"), None);
        assert_eq!(n("ex*mple.com"), None);
        assert_eq!(n("example.com:abc"), None);
    }

    #[test]
    fn allows_underscore_and_hyphen() {
        assert_eq!(
            n("_dmarc.example.com").as_deref(),
            Some("_dmarc.example.com")
        );
        assert_eq!(n("a-b.example.com").as_deref(), Some("a-b.example.com"));
        assert_eq!(n("xn--bcher-kva.de").as_deref(), Some("xn--bcher-kva.de"));
    }

    #[test]
    fn length_limits() {
        let l63 = "a".repeat(63);
        let l64 = "a".repeat(64);
        assert_eq!(n(&format!("{l63}.com")), Some(format!("{l63}.com")));
        assert_eq!(n(&format!("{l64}.com")), None);

        // 253 total is allowed, 254 is not.
        let label = "a".repeat(61); // 61 + 1 dot = 62 per segment
        let ok = format!(
            "{label}.{label}.{label}.{label}.{}",
            "b".repeat(253 - 4 * 62 + 1 - 1)
        );
        assert_eq!(ok.len(), 253);
        assert_eq!(n(&ok).as_deref(), Some(ok.as_str()));
        let bad = format!("{ok}c");
        assert_eq!(bad.len(), 254);
        assert_eq!(n(&bad), None);
    }

    #[test]
    fn single_label_accepted() {
        assert_eq!(n("localhost").as_deref(), Some("localhost"));
        assert_eq!(n("COM").as_deref(), Some("com"));
    }

    #[test]
    fn reversed_works() {
        assert_eq!(reversed("ads.example.com"), "com.example.ads");
        assert_eq!(reversed("com"), "com");
        assert_eq!(reversed("a.b"), "b.a");
        assert_eq!(reversed(""), "");
    }

    #[test]
    fn suffixes_works() {
        assert_eq!(suffixes("a.b.c").collect::<Vec<_>>(), ["a.b.c", "b.c", "c"]);
        assert_eq!(suffixes("com").collect::<Vec<_>>(), ["com"]);
        assert_eq!(suffixes("").count(), 0);
    }

    #[test]
    fn subdomain_checks() {
        assert!(is_subdomain_of("example.com", "example.com"));
        assert!(is_subdomain_of("www.example.com", "example.com"));
        assert!(is_subdomain_of("a.b.example.com", "example.com"));
        assert!(!is_subdomain_of("notexample.com", "example.com"));
        assert!(!is_subdomain_of("example.com", "www.example.com"));
        assert!(!is_subdomain_of("example.org", "example.com"));
        assert!(!is_subdomain_of("com", "example.com"));
    }
}
