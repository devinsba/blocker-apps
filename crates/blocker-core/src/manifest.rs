//! Signed blocklist manifest: the protocol between the list server and clients.
//!
//! # Protocol
//!
//! The server serves:
//!
//! * `GET {base}/v1/manifest.json` - a [`SignedManifest`] JSON document.
//! * `GET {base}/v1/{entry.file}` - the compiled FST for each category in the
//!   manifest (`entry.file` is relative, content-addressed, e.g.
//!   `categories/social-0123456789abcdef.fst`).
//!
//! The manifest is signed with Ed25519. The exact signed bytes travel inside
//! the envelope as the string [`SignedManifest::payload`], so verification
//! never depends on JSON re-serialization. Clients pin the server public key
//! (`ServerConfig.public_key`, base64) and:
//!
//! 1. verify the signature over `payload` bytes,
//! 2. parse the payload as a [`Manifest`] and check `format`,
//! 3. validate every entry (names, paths, hashes),
//! 4. reject rollbacks (`serial < last seen serial`); equal serials are fine,
//! 5. download each changed file and check its size and SHA-256
//!    ([`verify_file`]) before using it.
//!
//! `expires_at` is informational only and is deliberately not enforced:
//! clients keep using their last good lists when the server is unreachable.

use anyhow::{anyhow, bail, ensure, Context, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Manifest location relative to the server base URL.
pub const MANIFEST_PATH: &str = "v1/manifest.json";
/// The only manifest `format` this build understands.
pub const FORMAT_VERSION: u32 = 1;

/// The unsigned manifest contents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// Must equal [`FORMAT_VERSION`].
    pub format: u32,
    /// Monotonically increasing; clients reject rollbacks (`serial < last seen`).
    pub serial: u64,
    /// RFC 3339 generation time (informational).
    pub generated_at: String,
    /// RFC 3339 expiry (informational only, never enforced).
    pub expires_at: Option<String>,
    /// Available categories.
    pub categories: Vec<CategoryEntry>,
}

/// One downloadable category list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CategoryEntry {
    /// Category name, `[a-z0-9_-]{1,64}`.
    pub name: String,
    /// Human readable description.
    pub description: String,
    /// Relative path under `v1/`, content-addressed:
    /// `categories/{name}-{sha256[..16]}.fst`.
    pub file: String,
    /// Lowercase hex SHA-256 of the file.
    pub sha256: String,
    /// File size in bytes.
    pub size: u64,
    /// Number of domains in the list.
    pub domains: u64,
}

/// Wire format: the exact signed bytes are carried as a string so
/// verification never depends on JSON re-serialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedManifest {
    /// JSON text of a [`Manifest`]; these exact bytes are signed.
    pub payload: String,
    /// Base64 Ed25519 signature over the payload bytes.
    pub signature: String,
    /// First 8 bytes of SHA-256 of the public key, hex.
    pub key_id: String,
}

impl Manifest {
    /// Looks up a category by name.
    pub fn category(&self, name: &str) -> Option<&CategoryEntry> {
        self.categories.iter().find(|c| c.name == name)
    }
}

impl CategoryEntry {
    /// The canonical content-addressed file path for a category.
    pub fn content_addressed_file(name: &str, sha256: &str) -> String {
        let short = &sha256[..sha256.len().min(16)];
        format!("categories/{name}-{short}.fst")
    }
}

/// Generates a new keypair, returned as `(secret_b64, public_b64)`.
/// The secret is the 32-byte seed.
pub fn generate_keypair() -> (String, String) {
    let sk = SigningKey::generate(&mut rand::rngs::OsRng);
    (
        B64.encode(sk.to_bytes()),
        B64.encode(sk.verifying_key().to_bytes()),
    )
}

fn signing_key(secret_b64: &str) -> Result<SigningKey> {
    let bytes = B64
        .decode(secret_b64.trim())
        .context("secret key is not valid base64")?;
    let seed: [u8; 32] = bytes
        .try_into()
        .map_err(|_| anyhow!("secret key must be 32 bytes"))?;
    Ok(SigningKey::from_bytes(&seed))
}

fn verifying_key(public_b64: &str) -> Result<VerifyingKey> {
    let bytes = B64
        .decode(public_b64.trim())
        .context("public key is not valid base64")?;
    let arr: [u8; 32] = bytes
        .try_into()
        .map_err(|_| anyhow!("public key must be 32 bytes"))?;
    VerifyingKey::from_bytes(&arr).context("invalid public key")
}

fn key_id_of(key: &VerifyingKey) -> String {
    hex::encode(&Sha256::digest(key.to_bytes())[..8])
}

/// Derives the base64 public key from a base64 secret seed.
pub fn public_key_from_secret(secret_b64: &str) -> Result<String> {
    Ok(B64.encode(signing_key(secret_b64)?.verifying_key().to_bytes()))
}

/// Serializes and signs a manifest.
pub fn sign(manifest: &Manifest, secret_b64: &str) -> Result<SignedManifest> {
    let sk = signing_key(secret_b64)?;
    let payload = serde_json::to_string(manifest).context("serializing manifest")?;
    let sig = sk.sign(payload.as_bytes());
    Ok(SignedManifest {
        payload,
        signature: B64.encode(sig.to_bytes()),
        key_id: key_id_of(&sk.verifying_key()),
    })
}

fn valid_name(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

fn valid_sha256(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn validate_file_path(file: &str) -> Result<()> {
    ensure!(!file.is_empty(), "empty file path");
    ensure!(
        !file.starts_with('/'),
        "file path must be relative: {file:?}"
    );
    ensure!(
        !file.contains('\\'),
        "file path contains backslash: {file:?}"
    );
    ensure!(!file.contains('\0'), "file path contains NUL");
    for part in file.split('/') {
        ensure!(
            !part.is_empty() && part != "." && part != "..",
            "invalid file path component in {file:?}"
        );
    }
    Ok(())
}

fn validate_entry(e: &CategoryEntry) -> Result<()> {
    ensure!(valid_name(&e.name), "invalid category name {:?}", e.name);
    validate_file_path(&e.file).with_context(|| format!("category {}", e.name))?;
    ensure!(
        valid_sha256(&e.sha256),
        "category {}: sha256 must be 64 lowercase hex chars",
        e.name
    );
    Ok(())
}

/// Verifies the signature with `public_b64`, parses the payload, checks
/// `format == FORMAT_VERSION`, validates every entry and rejects rollbacks
/// (`serial < min_serial`; equal is accepted).
pub fn verify(signed: &SignedManifest, public_b64: &str, min_serial: u64) -> Result<Manifest> {
    let key = verifying_key(public_b64)?;
    ensure!(
        signed.key_id == key_id_of(&key),
        "manifest key_id {} does not match pinned key",
        signed.key_id
    );
    let sig_bytes = B64
        .decode(signed.signature.trim())
        .context("signature is not valid base64")?;
    let sig = Signature::from_slice(&sig_bytes).context("malformed signature")?;
    key.verify(signed.payload.as_bytes(), &sig)
        .map_err(|_| anyhow!("manifest signature verification failed"))?;

    let manifest: Manifest =
        serde_json::from_str(&signed.payload).context("parsing manifest payload")?;
    if manifest.format != FORMAT_VERSION {
        bail!(
            "unsupported manifest format {} (expected {FORMAT_VERSION})",
            manifest.format
        );
    }
    let mut seen = std::collections::HashSet::new();
    for e in &manifest.categories {
        validate_entry(e)?;
        ensure!(
            seen.insert(e.name.as_str()),
            "duplicate category {}",
            e.name
        );
    }
    ensure!(
        manifest.serial >= min_serial,
        "manifest rollback: serial {} < last seen {}",
        manifest.serial,
        min_serial
    );
    Ok(manifest)
}

/// Lowercase hex SHA-256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// Verifies that `bytes` match `entry.size` and `entry.sha256`.
pub fn verify_file(entry: &CategoryEntry, bytes: &[u8]) -> Result<()> {
    ensure!(
        bytes.len() as u64 == entry.size,
        "category {}: size mismatch (got {}, expected {})",
        entry.name,
        bytes.len(),
        entry.size
    );
    let got = sha256_hex(bytes);
    ensure!(
        got == entry.sha256,
        "category {}: sha256 mismatch (got {got}, expected {})",
        entry.name,
        entry.sha256
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, data: &[u8]) -> CategoryEntry {
        let sha = sha256_hex(data);
        CategoryEntry {
            name: name.into(),
            description: "d".into(),
            file: CategoryEntry::content_addressed_file(name, &sha),
            sha256: sha,
            size: data.len() as u64,
            domains: 3,
        }
    }

    fn manifest(serial: u64) -> Manifest {
        Manifest {
            format: FORMAT_VERSION,
            serial,
            generated_at: "2026-01-01T00:00:00Z".into(),
            expires_at: None,
            categories: vec![entry("social", b"abc"), entry("news-1", b"defg")],
        }
    }

    fn signed(m: &Manifest) -> (SignedManifest, String, String) {
        let (sec, publ) = generate_keypair();
        (sign(m, &sec).unwrap(), sec, publ)
    }

    #[test]
    fn round_trip() {
        let m = manifest(5);
        let (s, sec, publ) = signed(&m);
        assert_eq!(public_key_from_secret(&sec).unwrap(), publ);
        let got = verify(&s, &publ, 0).unwrap();
        assert_eq!(got, m);
        assert!(got.category("social").is_some());
        assert!(got.category("nope").is_none());
    }

    #[test]
    fn tampered_payload_fails() {
        let (mut s, _, publ) = signed(&manifest(1));
        s.payload = s.payload.replace("\"serial\":1", "\"serial\":9");
        assert!(verify(&s, &publ, 0).is_err());
    }

    #[test]
    fn wrong_key_fails() {
        let (s, _, _) = signed(&manifest(1));
        let (_, other) = generate_keypair();
        assert!(verify(&s, &other, 0).is_err());
    }

    #[test]
    fn rollback_rejected_equal_accepted() {
        let (s, _, publ) = signed(&manifest(5));
        assert!(verify(&s, &publ, 6).is_err());
        assert!(verify(&s, &publ, 5).is_ok());
        assert!(verify(&s, &publ, 4).is_ok());
    }

    #[test]
    fn bad_format_version() {
        let mut m = manifest(1);
        m.format = 2;
        let (s, _, publ) = signed(&m);
        assert!(verify(&s, &publ, 0).is_err());
    }

    fn rejects(mutate: impl FnOnce(&mut Manifest)) {
        let mut m = manifest(1);
        mutate(&mut m);
        let (s, _, publ) = signed(&m);
        assert!(verify(&s, &publ, 0).is_err());
    }

    #[test]
    fn path_traversal_rejected() {
        rejects(|m| m.categories[0].name = "../x".into());
        rejects(|m| m.categories[0].name = "Upper".into());
        rejects(|m| m.categories[0].name = String::new());
        rejects(|m| m.categories[0].name = "a".repeat(65));
        rejects(|m| m.categories[0].file = "../x".into());
        rejects(|m| m.categories[0].file = "categories/../../x".into());
        rejects(|m| m.categories[0].file = "/etc/passwd".into());
        rejects(|m| m.categories[0].file = "a\\b".into());
        rejects(|m| m.categories[0].file = "a//b".into());
    }

    #[test]
    fn bad_sha_rejected() {
        rejects(|m| m.categories[0].sha256 = "abc".into());
        rejects(|m| m.categories[0].sha256 = "A".repeat(64));
        rejects(|m| m.categories[0].sha256 = "g".repeat(64));
    }

    #[test]
    fn duplicate_names_rejected() {
        rejects(|m| m.categories[1].name = "social".into());
    }

    #[test]
    fn verify_file_checks() {
        let e = entry("social", b"abc");
        assert!(verify_file(&e, b"abc").is_ok());
        assert!(verify_file(&e, b"abcd").is_err());
        assert!(verify_file(&e, b"abd").is_err());
        assert!(e.file.starts_with("categories/social-"));
        assert!(e.file.ends_with(".fst"));
    }

    #[test]
    fn json_envelope_round_trip() {
        let (s, _, publ) = signed(&manifest(2));
        let text = serde_json::to_string_pretty(&s).unwrap();
        let back: SignedManifest = serde_json::from_str(&text).unwrap();
        assert_eq!(verify(&back, &publ, 0).unwrap().serial, 2);
    }

    #[test]
    fn key_id_deterministic() {
        let (sec, publ) = generate_keypair();
        let a = sign(&manifest(1), &sec).unwrap();
        let b = sign(&manifest(2), &sec).unwrap();
        assert_eq!(a.key_id, b.key_id);
        assert_eq!(a.key_id.len(), 16);
        assert!(verify(&a, &publ, 0).is_ok());
    }
}
