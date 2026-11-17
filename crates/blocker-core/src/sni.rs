//! Destination hostname sniffing from the first outbound bytes of a TCP flow.
//!
//! The content filter peeks at the start of a flow to learn the hostname when
//! the OS did not provide one (Chrome's built-in resolver, DoH, ...). Two
//! protocols are recognised:
//!
//! - TLS ClientHello (any port): the `server_name` extension. Hellos that span
//!   several TLS records (post-quantum key shares make them exceed 1500 bytes)
//!   are reassembled.
//! - HTTP/1.x requests: the `Host:` header (or the request-target for `CONNECT`).
//!
//! All parsing is bounds-checked and never panics on malformed input.

/// Max bytes a caller should ever peek (16 KiB + 5 header): a ClientHello with
/// post-quantum key shares can exceed 1500 bytes and span multiple TLS records.
pub const MAX_PEEK: usize = 16389;

/// Max size of an HTTP request head we are willing to buffer.
const MAX_HTTP_HEAD: usize = 8192;

/// Max plaintext TLS record payload (RFC 8446 allows a little more with
/// ciphertext expansion, but handshake records are never encrypted here).
const MAX_RECORD_LEN: usize = 16384;

/// Result of inspecting the first bytes of a stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Peek {
    /// Hostname found (lowercased, trailing dot stripped; raw — caller normalizes).
    Host(String),
    /// Looks like the right protocol but the buffer is truncated; ask for more bytes (up to `want` total).
    NeedMore { want: usize },
    /// Recognized protocol but no hostname present (e.g. ClientHello without SNI, HTTP/1.0 without Host).
    NoHost,
    /// Not TLS ClientHello nor HTTP/1.x request.
    Unknown,
}

/// Inspects the first outbound bytes of a TCP stream: TLS ClientHello (any port) or HTTP/1.x request.
pub fn peek_host(buf: &[u8]) -> Peek {
    match buf.first() {
        None => Peek::NeedMore { want: 5 },
        Some(0x16) => parse_tls_sni(buf),
        Some(_) => parse_http_host(buf),
    }
}

// ---------------------------------------------------------------------------
// TLS
// ---------------------------------------------------------------------------

/// Bounds-checked cursor over a byte slice.
struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }

    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        if self.remaining() < n {
            return None;
        }
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Some(s)
    }

    fn u8(&mut self) -> Option<usize> {
        self.take(1).map(|b| b[0] as usize)
    }

    fn u16(&mut self) -> Option<usize> {
        self.take(2).map(|b| ((b[0] as usize) << 8) | b[1] as usize)
    }

    /// Reads a length prefix of `len_bytes` (1 or 2) and returns that many bytes.
    fn vec8(&mut self) -> Option<&'a [u8]> {
        let n = self.u8()?;
        self.take(n)
    }

    fn vec16(&mut self) -> Option<&'a [u8]> {
        let n = self.u16()?;
        self.take(n)
    }
}

/// Validates a TLS record header prefix (as many of the 5 bytes as are present).
fn record_header_prefix_ok(hdr: &[u8]) -> bool {
    if let Some(&t) = hdr.first() {
        if t != 0x16 {
            return false;
        }
    }
    if let Some(&major) = hdr.get(1) {
        if major != 0x03 {
            return false;
        }
    }
    if let Some(&minor) = hdr.get(2) {
        if minor > 0x04 {
            return false;
        }
    }
    true
}

fn cap_want(want: usize, have: usize) -> Peek {
    let want = want.min(MAX_PEEK);
    if want > have {
        Peek::NeedMore { want }
    } else {
        // Already at the cap and still incomplete: give up.
        Peek::Unknown
    }
}

/// TLS only: parse the server_name extension from a ClientHello.
///
/// Reassembles handshake fragments across multiple TLS records. With ECH the
/// outer SNI (the public name) is returned as-is; see [`peek_tls`].
pub fn parse_tls_sni(buf: &[u8]) -> Peek {
    peek_tls(buf).0
}

/// Like [`parse_tls_sni`], also reporting whether the hello offers Encrypted
/// Client Hello (extension 0xfe0d). With ECH the real hostname is encrypted
/// and the returned SNI is only the provider's public name.
pub fn peek_tls(buf: &[u8]) -> (Peek, bool) {
    match reassemble_hello(buf) {
        Ok(body) => parse_client_hello(&body),
        Err(peek) => (peek, false),
    }
}

/// Collects the ClientHello body (after the 4-byte handshake header) from one
/// or more TLS records, or says why it can't yet.
fn reassemble_hello(buf: &[u8]) -> Result<Vec<u8>, Peek> {
    let mut hs: Vec<u8> = Vec::new();
    let mut pos = 0usize;

    loop {
        // Is the reassembled handshake message complete yet?
        let mut hs_total: Option<usize> = None;
        if let Some(&t) = hs.first() {
            if t != 0x01 {
                return Err(Peek::Unknown);
            }
        }
        if hs.len() >= 4 {
            let len = ((hs[1] as usize) << 16) | ((hs[2] as usize) << 8) | hs[3] as usize;
            let total = 4 + len;
            if total > MAX_PEEK {
                return Err(Peek::Unknown);
            }
            if hs.len() >= total {
                return Ok(hs[4..total].to_vec());
            }
            hs_total = Some(total);
        }
        let hs_missing = hs_total.map(|t| t - hs.len());

        // Need another record header.
        let rest = &buf[pos..];
        if rest.len() < 5 {
            if !record_header_prefix_ok(rest) {
                return Err(Peek::Unknown);
            }
            let body = hs_missing.unwrap_or_else(|| 4usize.saturating_sub(hs.len()).max(1));
            return Err(cap_want(pos + 5 + body, buf.len()));
        }
        if !record_header_prefix_ok(&rest[..5]) {
            return Err(Peek::Unknown);
        }
        let rec_len = ((rest[3] as usize) << 8) | rest[4] as usize;
        if rec_len == 0 || rec_len > MAX_RECORD_LEN {
            return Err(Peek::Unknown);
        }
        let avail = rec_len.min(rest.len() - 5);
        hs.extend_from_slice(&rest[5..5 + avail]);
        pos += 5 + avail;

        if avail < rec_len {
            // Truncated mid-record. Re-check completeness with what we have.
            if hs.len() >= 4 {
                let len = ((hs[1] as usize) << 16) | ((hs[2] as usize) << 8) | hs[3] as usize;
                if hs[0] != 0x01 || 4 + len > MAX_PEEK {
                    return Err(Peek::Unknown);
                }
                if hs.len() >= 4 + len {
                    return Ok(hs[4..4 + len].to_vec());
                }
                let still = 4 + len - hs.len();
                return Err(cap_want(pos + (rec_len - avail).min(still), buf.len()));
            }
            if hs.first().is_some_and(|&t| t != 0x01) {
                return Err(Peek::Unknown);
            }
            return Err(cap_want(pos + (rec_len - avail), buf.len()));
        }
    }
}

/// Parses a complete ClientHello body (after the 4-byte handshake header).
/// The bool reports an Encrypted Client Hello extension.
fn parse_client_hello(body: &[u8]) -> (Peek, bool) {
    fn inner(body: &[u8]) -> Option<(Peek, bool)> {
        let mut r = Reader::new(body);
        r.take(2)?; // legacy_version
        r.take(32)?; // random
        r.vec8()?; // session_id
        r.vec16()?; // cipher_suites
        r.vec8()?; // compression_methods
        if r.remaining() == 0 {
            return Some((Peek::NoHost, false)); // TLS 1.2 hello without extensions
        }
        let mut exts = Reader::new(r.vec16()?);
        let mut host = Peek::NoHost;
        let mut ech = false;
        while exts.remaining() > 0 {
            let ty = exts.u16()?;
            let data = exts.vec16()?;
            match ty {
                0x0000 => host = parse_server_name_ext(data)?,
                ECH_EXTENSION => ech = true,
                _ => {}
            }
        }
        Some((host, ech))
    }
    inner(body).unwrap_or((Peek::Unknown, false))
}

/// `encrypted_client_hello` extension type.
const ECH_EXTENSION: usize = 0xfe0d;

/// Parses the `server_name` extension body. `None` means malformed.
fn parse_server_name_ext(data: &[u8]) -> Option<Peek> {
    let mut list = Reader::new(Reader::new(data).vec16()?);
    while list.remaining() > 0 {
        let name_type = list.u8()?;
        let name = list.vec16()?;
        if name_type == 0 {
            return Some(match normalize_host(name) {
                Some(h) => Peek::Host(h),
                None => Peek::NoHost,
            });
        }
    }
    Some(Peek::NoHost)
}

/// Lowercases and strips a trailing dot; rejects empty or non-printable names.
fn normalize_host(raw: &[u8]) -> Option<String> {
    if raw.is_empty() || !raw.iter().all(|&b| (0x21..=0x7e).contains(&b)) {
        return None;
    }
    let mut s = String::from_utf8(raw.to_vec()).ok()?.to_ascii_lowercase();
    while s.ends_with('.') {
        s.pop();
    }
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

// ---------------------------------------------------------------------------
// HTTP
// ---------------------------------------------------------------------------

const METHODS: [&str; 8] = [
    "GET ", "POST ", "PUT ", "HEAD ", "DELETE ", "OPTIONS ", "PATCH ", "CONNECT ",
];

/// Strips a `:port` suffix and IPv6 brackets from a `host[:port]` string.
fn strip_port(hp: &str) -> &str {
    if let Some(rest) = hp.strip_prefix('[') {
        return rest.split(']').next().unwrap_or(rest);
    }
    match hp.rsplit_once(':') {
        Some((h, p)) if !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()) => h,
        _ => hp,
    }
}

fn host_from_hostport(hp: &[u8]) -> Peek {
    let Ok(s) = std::str::from_utf8(hp) else {
        return Peek::NoHost;
    };
    match normalize_host(strip_port(s.trim_matches(|c| c == ' ' || c == '\t')).as_bytes()) {
        Some(h) => Peek::Host(h),
        None => Peek::NoHost,
    }
}

fn find_crlf(buf: &[u8]) -> Option<usize> {
    buf.windows(2).position(|w| w == b"\r\n")
}

/// HTTP/1.x only: method line + `Host:` header (case-insensitive, strip :port).
///
/// For `CONNECT` the request-target host is used instead.
pub fn parse_http_host(buf: &[u8]) -> Peek {
    let need_more = || {
        if buf.len() >= MAX_HTTP_HEAD {
            Peek::NoHost
        } else {
            Peek::NeedMore {
                want: MAX_HTTP_HEAD,
            }
        }
    };

    // Method.
    let method = METHODS.iter().find(|m| buf.starts_with(m.as_bytes()));
    let Some(method) = method else {
        let could_be = METHODS.iter().any(|m| m.as_bytes().starts_with(buf));
        return if could_be {
            Peek::NeedMore {
                want: MAX_HTTP_HEAD,
            }
        } else {
            Peek::Unknown
        };
    };

    // Request line.
    let Some(line_end) = find_crlf(buf) else {
        return if buf.len() >= MAX_HTTP_HEAD {
            Peek::Unknown
        } else {
            Peek::NeedMore {
                want: MAX_HTTP_HEAD,
            }
        };
    };
    let line = &buf[method.len()..line_end];
    let Some(sp) = line.iter().position(|&b| b == b' ') else {
        return Peek::Unknown;
    };
    let (target, version) = (&line[..sp], &line[sp + 1..]);
    if !version.starts_with(b"HTTP/1.") || target.is_empty() {
        return Peek::Unknown;
    }
    if *method == "CONNECT " {
        return host_from_hostport(target);
    }

    // Header lines; only complete (CRLF-terminated) lines are considered.
    let mut pos = line_end + 2;
    loop {
        let Some(off) = find_crlf(&buf[pos..]) else {
            return need_more();
        };
        let l = &buf[pos..pos + off];
        pos += off + 2;
        if l.is_empty() {
            return Peek::NoHost; // end of headers, no Host seen
        }
        if let Some(colon) = l.iter().position(|&b| b == b':') {
            if l[..colon].eq_ignore_ascii_case(b"host") {
                return host_from_hostport(&l[colon + 1..]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ext(ty: u16, data: &[u8]) -> Vec<u8> {
        let mut v = ty.to_be_bytes().to_vec();
        v.extend((data.len() as u16).to_be_bytes());
        v.extend(data);
        v
    }

    /// Builds a ClientHello handshake message (with 4-byte header).
    fn hello_handshake(sni: Option<&str>, pad_extension_len: usize) -> Vec<u8> {
        let mut body = vec![0x03, 0x03];
        body.extend([0xab; 32]); // random
        body.push(32);
        body.extend([0xcd; 32]); // session id
        body.extend([0x00, 0x04, 0x13, 0x01, 0x13, 0x02]); // cipher suites
        body.extend([0x01, 0x00]); // compression

        let mut exts = Vec::new();
        if let Some(name) = sni {
            let mut sn = Vec::new();
            sn.push(0u8);
            sn.extend((name.len() as u16).to_be_bytes());
            sn.extend(name.as_bytes());
            let mut data = (sn.len() as u16).to_be_bytes().to_vec();
            data.extend(sn);
            exts.extend(ext(0x0000, &data));
        }
        exts.extend(ext(0x000a, &[0x00, 0x02, 0x00, 0x1d])); // supported_groups
        if pad_extension_len > 0 {
            exts.extend(ext(0x0015, &vec![0u8; pad_extension_len]));
        }
        body.extend((exts.len() as u16).to_be_bytes());
        body.extend(exts);

        let mut hs = vec![0x01];
        hs.extend(&(body.len() as u32).to_be_bytes()[1..]);
        hs.extend(body);
        hs
    }

    fn record(payload: &[u8]) -> Vec<u8> {
        let mut r = vec![0x16, 0x03, 0x01];
        r.extend((payload.len() as u16).to_be_bytes());
        r.extend(payload);
        r
    }

    /// Builds a ClientHello, optionally split across two TLS records at `split`.
    fn client_hello_split(sni: Option<&str>, pad: usize, split: Option<usize>) -> Vec<u8> {
        let hs = hello_handshake(sni, pad);
        match split {
            None => record(&hs),
            Some(at) => {
                let mut out = record(&hs[..at]);
                out.extend(record(&hs[at..]));
                out
            }
        }
    }

    fn client_hello(sni: Option<&str>, pad_extension_len: usize) -> Vec<u8> {
        client_hello_split(sni, pad_extension_len, None)
    }

    #[test]
    fn finds_sni() {
        let b = client_hello(Some("www.Example.COM."), 0);
        assert_eq!(parse_tls_sni(&b), Peek::Host("www.example.com".into()));
        assert_eq!(peek_host(&b), Peek::Host("www.example.com".into()));
    }

    #[test]
    fn real_captured_style_hello() {
        // Hand-assembled ClientHello for "example.com" (TLS 1.3 style).
        let hex = "1603010034010000300303\
            0000000000000000000000000000000000000000000000000000000000000000\
            00\
            0002130100\
            0005\
            0000000100";
        // Deliberately minimal / odd: must not panic, whatever it returns.
        let bytes: Vec<u8> = (0..hex.len() / 2)
            .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap())
            .collect();
        let _ = peek_host(&bytes);

        let hello = client_hello(Some("example.com"), 0);
        assert_eq!(&hello[..3], &[0x16, 0x03, 0x01]);
        assert_eq!(parse_tls_sni(&hello), Peek::Host("example.com".into()));
    }

    #[test]
    fn detects_encrypted_client_hello() {
        let mut hs = hello_handshake(Some("cloudflare-ech.com"), 0);
        assert_eq!(
            peek_tls(&record(&hs)),
            (Peek::Host("cloudflare-ech.com".into()), false)
        );
        // Append an ECH extension: bump the extensions length and the handshake length.
        let ext_len_pos = hs.len() - hello_ext_block_len(&hs) - 2;
        let ech = ext(0xfe0d, &[0u8; 8]);
        let new_ext_len = hello_ext_block_len(&hs) + ech.len();
        hs[ext_len_pos..ext_len_pos + 2].copy_from_slice(&(new_ext_len as u16).to_be_bytes());
        hs.extend_from_slice(&ech);
        let body_len = hs.len() - 4;
        hs[1..4].copy_from_slice(&(body_len as u32).to_be_bytes()[1..]);
        assert_eq!(
            peek_tls(&record(&hs)),
            (Peek::Host("cloudflare-ech.com".into()), true)
        );
    }

    /// Length of the extensions block, read back from a hello built by `hello_handshake`.
    fn hello_ext_block_len(hs: &[u8]) -> usize {
        let mut r = Reader::new(&hs[4..]);
        r.take(34).unwrap();
        r.vec8().unwrap();
        r.vec16().unwrap();
        r.vec8().unwrap();
        r.u16().unwrap()
    }

    #[test]
    fn no_sni() {
        let b = client_hello(None, 0);
        assert_eq!(parse_tls_sni(&b), Peek::NoHost);
    }

    #[test]
    fn truncated_every_prefix() {
        for split in [None, Some(30)] {
            let b = client_hello_split(Some("example.com"), 100, split);
            for n in 0..b.len() {
                match peek_host(&b[..n]) {
                    Peek::NeedMore { want } => {
                        assert!(want > n, "want {want} <= have {n}");
                        assert!(want <= b.len(), "want {want} beyond full {}", b.len());
                    }
                    Peek::Host(h) => assert_eq!(h, "example.com", "prefix {n}"),
                    other => panic!("prefix {n}: unexpected {other:?}"),
                }
            }
            assert_eq!(peek_host(&b), Peek::Host("example.com".into()));
        }
    }

    #[test]
    fn need_more_want_is_exact_for_single_record() {
        let b = client_hello(Some("example.com"), 0);
        assert_eq!(parse_tls_sni(&b[..10]), Peek::NeedMore { want: b.len() });
    }

    #[test]
    fn multi_record_hello() {
        let b = client_hello_split(Some("big.example.org"), 3000, Some(1200));
        assert!(b.len() > 3000);
        assert_eq!(parse_tls_sni(&b), Peek::Host("big.example.org".into()));
        // Split inside the 4-byte handshake header too.
        for at in [1, 2, 3, 4, 5] {
            let b = client_hello_split(Some("a.test"), 0, Some(at));
            assert_eq!(parse_tls_sni(&b), Peek::Host("a.test".into()), "split {at}");
        }
    }

    #[test]
    fn large_single_record() {
        let b = client_hello(Some("x.example"), 15000);
        assert_eq!(parse_tls_sni(&b), Peek::Host("x.example".into()));
    }

    #[test]
    fn malformed_lengths() {
        let good = client_hello(Some("example.com"), 0);

        // Bad record type / version.
        let mut b = good.clone();
        b[0] = 0x17;
        assert_eq!(parse_tls_sni(&b), Peek::Unknown);
        let mut b = good.clone();
        b[1] = 0x02;
        assert_eq!(parse_tls_sni(&b), Peek::Unknown);
        let mut b = good.clone();
        b[2] = 0x09;
        assert_eq!(parse_tls_sni(&b), Peek::Unknown);

        // Oversized record length.
        let mut b = good.clone();
        b[3] = 0xff;
        b[4] = 0xff;
        assert_eq!(parse_tls_sni(&b), Peek::Unknown);

        // Zero-length record.
        let mut b = good.clone();
        b[3] = 0;
        b[4] = 0;
        assert_eq!(parse_tls_sni(&b), Peek::Unknown);

        // Not a ClientHello.
        let mut b = good.clone();
        b[5] = 0x02;
        assert_eq!(parse_tls_sni(&b), Peek::Unknown);

        // Handshake length absurdly large.
        let mut b = good.clone();
        b[6] = 0xff;
        assert_eq!(parse_tls_sni(&b), Peek::Unknown);

        // Handshake length shorter than contents: inner parse hits end.
        let mut b = good.clone();
        b[8] = 20;
        assert_eq!(parse_tls_sni(&b), Peek::Unknown);

        // Corrupt every single byte to every value: never panics.
        for i in 0..good.len() {
            for v in [0x00u8, 0x01, 0x7f, 0xff] {
                let mut b = good.clone();
                b[i] = v;
                let _ = peek_host(&b);
            }
        }
    }

    #[test]
    fn invalid_hostname_bytes() {
        let b = client_hello(Some("bad host"), 0);
        assert_eq!(parse_tls_sni(&b), Peek::NoHost);
        let b = client_hello(Some(""), 0);
        assert_eq!(parse_tls_sni(&b), Peek::NoHost);
    }

    fn xorshift(state: &mut u64) -> u64 {
        let mut x = *state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        *state = x;
        x
    }

    #[test]
    fn fuzz_no_panic() {
        let mut s = 0x9e3779b97f4a7c15u64;
        let seed = client_hello(Some("example.com"), 50);
        for i in 0..10_000 {
            let len = (xorshift(&mut s) % 300) as usize;
            let mut b: Vec<u8> = (0..len).map(|_| xorshift(&mut s) as u8).collect();
            match i % 3 {
                0 => {
                    if !b.is_empty() {
                        b[0] = 0x16;
                    }
                    if b.len() > 2 {
                        b[1] = 3;
                        b[2] = 1;
                    }
                }
                1 => {
                    // Mutate a valid hello.
                    b = seed.clone();
                    for _ in 0..3 {
                        let idx = (xorshift(&mut s) as usize) % b.len();
                        b[idx] = xorshift(&mut s) as u8;
                    }
                    let cut = (xorshift(&mut s) as usize) % (b.len() + 1);
                    b.truncate(cut);
                }
                _ => {}
            }
            let _ = peek_host(&b);
            let _ = parse_http_host(&b);
            let _ = parse_tls_sni(&b);
        }
    }

    #[test]
    fn http_get_with_port() {
        let b = b"GET /index.html HTTP/1.1\r\nUser-Agent: x\r\nHost: Example.com:8080\r\nAccept: */*\r\n\r\n";
        assert_eq!(parse_http_host(b), Peek::Host("example.com".into()));
        assert_eq!(peek_host(b), Peek::Host("example.com".into()));
    }

    #[test]
    fn http_lowercase_host_header() {
        let b = b"POST /x HTTP/1.1\r\nhost: foo.bar\r\n\r\n";
        assert_eq!(parse_http_host(b), Peek::Host("foo.bar".into()));
        let b = b"GET / HTTP/1.1\r\nHOST:   foo.bar.  \r\n\r\n";
        assert_eq!(parse_http_host(b), Peek::Host("foo.bar".into()));
    }

    #[test]
    fn http_connect() {
        let b = b"CONNECT Example.org:443 HTTP/1.1\r\nProxy-Connection: keep-alive\r\n\r\n";
        assert_eq!(parse_http_host(b), Peek::Host("example.org".into()));
        let b = b"CONNECT [2001:db8::1]:443 HTTP/1.1\r\n\r\n";
        assert_eq!(parse_http_host(b), Peek::Host("2001:db8::1".into()));
    }

    #[test]
    fn http_no_host_and_truncated() {
        let b = b"GET / HTTP/1.0\r\nAccept: */*\r\n\r\n";
        assert_eq!(parse_http_host(b), Peek::NoHost);

        let full = b"GET / HTTP/1.1\r\nAccept: */*\r\nHost: a.b\r\n\r\n";
        for n in 0..full.len() {
            match parse_http_host(&full[..n]) {
                Peek::NeedMore { want } => assert!(want > n),
                Peek::Host(h) => assert_eq!(h, "a.b"),
                other => panic!("prefix {n}: {other:?}"),
            }
        }
        // Headers never terminate and exceed the cap.
        let mut big = b"GET / HTTP/1.1\r\n".to_vec();
        while big.len() < MAX_HTTP_HEAD + 10 {
            big.extend(b"X-Pad: aaaaaaaaaaaaaaaa\r\n");
        }
        assert_eq!(parse_http_host(&big), Peek::NoHost);
    }

    #[test]
    fn garbage_is_unknown() {
        assert_eq!(peek_host(b"\x00\x01\x02\x03\x04\x05"), Peek::Unknown);
        assert_eq!(peek_host(b"SSH-2.0-OpenSSH_9.0\r\n"), Peek::Unknown);
        assert_eq!(peek_host(b"GARBAGE / HTTP/1.1\r\n\r\n"), Peek::Unknown);
        assert_eq!(parse_http_host(b"GET / FTP/1.1\r\n\r\n"), Peek::Unknown);
        assert_eq!(peek_host(b""), Peek::NeedMore { want: 5 });
    }
}
