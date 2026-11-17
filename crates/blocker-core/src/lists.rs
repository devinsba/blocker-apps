//! Streaming parser for public blocklists.
//!
//! Understands, per line and in any mixture:
//!
//! - hosts files: `0.0.0.0 ads.example.com tracker.example.net # note`
//! - adblock network rules: `||example.com^` (optionally `$important`, `$all`,
//!   `$document`, `$doc`, `$popup`)
//! - dnsmasq: `address=/example.com/0.0.0.0`, `server=/example.com/`
//! - plain domain lists: `example.com`, `*.example.com`
//!
//! Comments (`#`, `!`), blank lines, `[Adblock Plus 2.0]` headers, exception
//! rules (`@@`), cosmetic rules, regex rules, path-bearing rules and
//! single-label results (bare TLDs) are never emitted as domains.

use std::io::{self, BufRead};
use std::net::IpAddr;

use crate::domain::normalize;

/// Counters describing one parse run.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ParseStats {
    /// Every line read from the input, including blank and comment lines.
    pub lines: u64,
    /// Domains passed to the callback.
    pub domains: u64,
    /// Candidate entries that were recognised as rules but rejected
    /// (unsupported syntax, invalid domain, single label, ...).
    pub skipped: u64,
}

/// Hosts-file names that are local plumbing, never block targets.
const LOCAL_NAMES: &[&str] = &[
    "localhost",
    "localhost.localdomain",
    "local",
    "broadcasthost",
    "ip6-localhost",
    "ip6-loopback",
    "ip6-localnet",
    "ip6-mcastprefix",
    "ip6-allnodes",
    "ip6-allrouters",
    "ip6-allhosts",
    "0.0.0.0",
];

/// Adblock `$options` that do not narrow a rule to a subset of traffic.
const OK_OPTIONS: &[&str] = &["important", "all", "document", "doc", "popup"];

/// Streams `reader` line by line, calling `on_domain` with each normalized
/// domain, and returns the counters. Never loads the whole input into memory;
/// invalid UTF-8 is replaced lossily.
pub fn parse_reader<R: BufRead>(
    mut reader: R,
    mut on_domain: impl FnMut(String),
) -> io::Result<ParseStats> {
    let mut stats = ParseStats::default();
    let mut buf: Vec<u8> = Vec::with_capacity(256);
    loop {
        buf.clear();
        if reader.read_until(b'\n', &mut buf)? == 0 {
            break;
        }
        stats.lines += 1;
        let line = String::from_utf8_lossy(&buf);
        parse_line(&line, &mut stats, &mut on_domain);
    }
    Ok(stats)
}

/// Convenience wrapper around [`parse_reader`] for tests and small inputs.
pub fn parse_str(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    // Reading from a byte slice cannot fail.
    let _ = parse_reader(text.as_bytes(), |d| out.push(d));
    out
}

fn emit(candidate: &str, stats: &mut ParseStats, on_domain: &mut impl FnMut(String)) {
    match normalize(candidate) {
        Some(d) if d.contains('.') => {
            stats.domains += 1;
            on_domain(d);
        }
        _ => stats.skipped += 1,
    }
}

fn parse_line(raw: &str, stats: &mut ParseStats, on_domain: &mut impl FnMut(String)) {
    let line = raw.trim();
    if line.is_empty() || line.starts_with('#') || line.starts_with('!') {
        return;
    }
    if line.starts_with('[') && line.ends_with(']') {
        return;
    }

    // Adblock exceptions, cosmetic/scriptlet rules and regex rules.
    if line.starts_with("@@")
        || line.starts_with('/')
        || ["##", "#@#", "#?#", "#$#"].iter().any(|m| line.contains(m))
    {
        stats.skipped += 1;
        return;
    }

    if let Some(rule) = line.strip_prefix("||") {
        parse_adblock(rule, stats, on_domain);
        return;
    }

    // Strip an inline `#` comment for hosts / dnsmasq / plain lines.
    let line = match line.find('#') {
        Some(i) => line[..i].trim_end(),
        None => line,
    };
    if line.is_empty() {
        return;
    }

    if line.starts_with("address=/") || line.starts_with("server=/") {
        let parts: Vec<&str> = line.split('/').collect();
        let domains = &parts[1..parts.len().saturating_sub(1)];
        if domains.iter().all(|d| d.is_empty()) {
            stats.skipped += 1;
        }
        for d in domains.iter().filter(|d| !d.is_empty()) {
            emit(d, stats, on_domain);
        }
        return;
    }

    let mut tokens = line.split_whitespace();
    let first = match tokens.next() {
        Some(t) => t,
        None => return,
    };

    if first.parse::<IpAddr>().is_ok() {
        let mut any = false;
        for name in tokens {
            any = true;
            if LOCAL_NAMES.iter().any(|l| l.eq_ignore_ascii_case(name)) {
                continue;
            }
            emit(name, stats, on_domain);
        }
        if !any {
            stats.skipped += 1;
        }
        return;
    }

    // Plain list: exactly one token, free of rule syntax.
    if tokens.next().is_some() || first.contains(['$', '^', '|', '@', '=', ',', ' ']) {
        stats.skipped += 1;
        return;
    }
    emit(first, stats, on_domain);
}

/// `rule` is the text after the leading `||`.
fn parse_adblock(rule: &str, stats: &mut ParseStats, on_domain: &mut impl FnMut(String)) {
    let (body, options) = match rule.split_once('$') {
        Some((b, o)) => (b, Some(o)),
        None => (rule, None),
    };
    if let Some(opts) = options {
        let ok = opts
            .split(',')
            .all(|o| OK_OPTIONS.contains(&o.trim().to_ascii_lowercase().as_str()));
        if !ok {
            stats.skipped += 1;
            return;
        }
    }
    let domain = body.strip_suffix('^').unwrap_or(body);
    if domain.is_empty() || domain.contains(['/', '*', '^', '|', ':', '?', ' ', '\t']) {
        stats.skipped += 1;
        return;
    }
    emit(domain, stats, on_domain);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn stats_of(text: &str) -> (Vec<String>, ParseStats) {
        let mut out = Vec::new();
        let s = parse_reader(Cursor::new(text.as_bytes()), |d| out.push(d)).unwrap();
        (out, s)
    }

    #[test]
    fn hosts_format() {
        let text = "\
# Title: test
127.0.0.1 localhost
127.0.0.1 localhost.localdomain
255.255.255.255 broadcasthost
::1 localhost ip6-localhost ip6-loopback
fe80::1%lo0 localhost
0.0.0.0 0.0.0.0
0.0.0.0 ads.example.com
:: tracker.example.net
";
        assert_eq!(
            parse_str(text),
            vec!["ads.example.com", "tracker.example.net"]
        );
    }

    #[test]
    fn hosts_multiple_names_and_inline_comment() {
        let (d, s) = stats_of("0.0.0.0 a.example.com B.Example.com # trailing\n");
        assert_eq!(d, vec!["a.example.com", "b.example.com"]);
        assert_eq!(
            s,
            ParseStats {
                lines: 1,
                domains: 2,
                skipped: 0
            }
        );
    }

    #[test]
    fn adblock_network_rules() {
        let text = "\
[Adblock Plus 2.0]
! Title: x
||example.com^
||Sub.Example.org^$important
||all.example.com^$all
||doc.example.com^$document,popup
||plain.example.com
";
        assert_eq!(
            parse_str(text),
            vec![
                "example.com",
                "sub.example.org",
                "all.example.com",
                "doc.example.com",
                "plain.example.com"
            ]
        );
    }

    #[test]
    fn adblock_skipped_rules() {
        let text = "\
@@||allowed.example.com^
example.com##.banner
example.com#@#.banner
example.com#?#div
example.com#$#abort-on-property-read foo
||example.com/ads^
||ex*mple.com^
||tp.example.com^$third-party
||img.example.com^$image,script
/ads\\d+/
";
        let (d, s) = stats_of(text);
        assert!(d.is_empty(), "{d:?}");
        assert_eq!(s.lines, 10);
        assert_eq!(s.skipped, 10);
        assert_eq!(s.domains, 0);
    }

    #[test]
    fn dnsmasq_format() {
        let text = "\
address=/ads.example.com/0.0.0.0
address=/v6.example.com/::
server=/dns.example.net/
server=/x.example.net/1.1.1.1
";
        assert_eq!(
            parse_str(text),
            vec![
                "ads.example.com",
                "v6.example.com",
                "dns.example.net",
                "x.example.net"
            ]
        );
    }

    #[test]
    fn plain_format() {
        let text = "example.com\n*.wild.example.org\n  Spaced.Example.NET  \nfoo.com # note\n";
        assert_eq!(
            parse_str(text),
            vec![
                "example.com",
                "wild.example.org",
                "spaced.example.net",
                "foo.com"
            ]
        );
    }

    #[test]
    fn single_label_skipped() {
        let (d, s) = stats_of("com\n0.0.0.0 org\n||net^\naddress=/io/0.0.0.0\n");
        assert!(d.is_empty());
        assert_eq!(s.skipped, 4);
    }

    #[test]
    fn garbage_is_skipped() {
        let (d, s) = stats_of("not a valid line\nfoo$bar\n0.0.0.0\n");
        assert!(d.is_empty());
        assert_eq!(
            s,
            ParseStats {
                lines: 3,
                domains: 0,
                skipped: 3
            }
        );
    }

    #[test]
    fn comments_and_blanks_ignored() {
        let (d, s) = stats_of("\n   \n# c\n  ! c\n#\n[Adblock Plus 2.0]\n");
        assert!(d.is_empty());
        assert_eq!(
            s,
            ParseStats {
                lines: 6,
                domains: 0,
                skipped: 0
            }
        );
    }

    #[test]
    fn crlf_and_no_trailing_newline() {
        let (d, s) = stats_of("0.0.0.0 a.example.com\r\n||b.example.com^\r\nc.example.com");
        assert_eq!(d, vec!["a.example.com", "b.example.com", "c.example.com"]);
        assert_eq!(
            s,
            ParseStats {
                lines: 3,
                domains: 3,
                skipped: 0
            }
        );
    }

    #[test]
    fn mixed_input_stats() {
        let text = "\
# hosts
0.0.0.0 h1.example.com h2.example.com
||a1.example.com^
@@||ok.example.com^
address=/d1.example.com/0.0.0.0
p1.example.com
tld
";
        let (d, s) = stats_of(text);
        assert_eq!(
            d,
            vec![
                "h1.example.com",
                "h2.example.com",
                "a1.example.com",
                "d1.example.com",
                "p1.example.com"
            ]
        );
        assert_eq!(
            s,
            ParseStats {
                lines: 7,
                domains: 5,
                skipped: 2
            }
        );
    }

    #[test]
    fn invalid_utf8_does_not_error() {
        let bytes: &[u8] = b"0.0.0.0 ok.example.com\n0.0.0.0 \xff\xfe.example.com\n";
        let mut out = Vec::new();
        let s = parse_reader(bytes, |d| out.push(d)).unwrap();
        assert_eq!(s.lines, 2);
        assert!(out.contains(&"ok.example.com".to_string()));
    }

    #[test]
    fn streams_many_lines() {
        stream_lines(100_000);
    }

    /// Run with `cargo test --release -- --ignored` for the full-size check.
    #[test]
    #[ignore]
    fn streams_a_million_lines() {
        stream_lines(1_000_000);
    }

    fn stream_lines(n: u32) {
        use std::io::{BufReader, Read};

        // Generates lines lazily so the input itself is never held in memory.
        struct Gen {
            i: u32,
            n: u32,
            pending: Vec<u8>,
            pos: usize,
        }
        impl Read for Gen {
            fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
                if self.pos >= self.pending.len() {
                    if self.i >= self.n {
                        return Ok(0);
                    }
                    let i = self.i;
                    self.i += 1;
                    self.pending = match i % 4 {
                        0 => format!("0.0.0.0 host{i}.example.com\n"),
                        1 => format!("||ad{i}.example.net^\n"),
                        2 => format!("address=/dns{i}.example.org/0.0.0.0\n"),
                        _ => format!("plain{i}.example.io\n"),
                    }
                    .into_bytes();
                    self.pos = 0;
                }
                let n = (self.pending.len() - self.pos).min(out.len());
                out[..n].copy_from_slice(&self.pending[self.pos..self.pos + n]);
                self.pos += n;
                Ok(n)
            }
        }

        let reader = BufReader::new(Gen {
            i: 0,
            n,
            pending: Vec::new(),
            pos: 0,
        });
        let mut count = 0u64;
        let start = std::time::Instant::now();
        let s = parse_reader(reader, |_| count += 1).unwrap();
        let elapsed = start.elapsed();
        assert_eq!(s.lines, n as u64);
        assert_eq!(s.domains, n as u64);
        assert_eq!(s.skipped, 0);
        assert_eq!(count, n as u64);
        eprintln!("parsed {n} lines in {elapsed:?}");
    }
}
