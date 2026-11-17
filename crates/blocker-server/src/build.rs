//! The `build` pipeline: download sources, compile FSTs, sign the manifest.
//!
//! Output layout under `<out>/v1/`:
//!
//! ```text
//! manifest.json                      signed envelope (written atomically)
//! categories/<name>-<sha16>.fst      content-addressed category lists
//! ```
//!
//! Safety rule: a category is only republished when every source fetched
//! cleanly, the result is non-empty and it did not shrink by more than half
//! (unless `allow_shrink`). Otherwise the previous manifest entry is kept
//! unchanged, so a broken upstream never degrades what clients enforce.

use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use blocker_core::domainset::DomainSet;
use blocker_core::lists;
use blocker_core::manifest::{
    self, CategoryEntry, Manifest, SignedManifest, FORMAT_VERSION, MANIFEST_PATH,
};

use crate::sources::{self, Category};

/// Manifest lifetime (informational for clients).
const EXPIRY_DAYS: i64 = 7;

/// Inputs to [`run`].
#[derive(Debug, Clone)]
pub struct BuildOptions {
    /// `sources.toml`.
    pub sources: PathBuf,
    /// Public directory; files go under `<out>/v1/`.
    pub out: PathBuf,
    /// File holding the base64 secret seed.
    pub key: PathBuf,
    /// Publish categories even if they shrank by more than 50%.
    pub allow_shrink: bool,
}

/// Outcome for one category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// A new list was published.
    Updated,
    /// The freshly built list is identical to the previous one.
    Unchanged,
    /// The build failed or was refused; the previous entry stays published.
    KeptPrevious,
    /// The build failed and there is no previous entry; category omitted.
    Failed,
}

impl Status {
    fn label(self) -> &'static str {
        match self {
            Status::Updated => "updated",
            Status::Unchanged => "unchanged",
            Status::KeptPrevious => "kept-previous",
            Status::Failed => "failed",
        }
    }
    fn ok(self) -> bool {
        matches!(self, Status::Updated | Status::Unchanged)
    }
}

/// Per-category result row.
#[derive(Debug, Clone)]
pub struct Row {
    /// Category name.
    pub name: String,
    /// What happened.
    pub status: Status,
    /// The entry that is published for this category, if any.
    pub entry: Option<CategoryEntry>,
}

/// Result of a successful build.
#[derive(Debug)]
pub struct Report {
    /// The manifest that was published.
    pub manifest: Manifest,
    /// One row per category in `sources.toml`.
    pub rows: Vec<Row>,
}

impl Report {
    /// Prints the summary table to stdout.
    pub fn print(&self) {
        println!(
            "serial {}  generated {}",
            self.manifest.serial, self.manifest.generated_at
        );
        println!(
            "{:<20} {:>10} {:>12} {:<10} STATUS",
            "CATEGORY", "DOMAINS", "SIZE", "SHA"
        );
        for r in &self.rows {
            match &r.entry {
                Some(e) => println!(
                    "{:<20} {:>10} {:>12} {:<10} {}",
                    r.name,
                    e.domains,
                    e.size,
                    &e.sha256[..e.sha256.len().min(8)],
                    r.status.label()
                ),
                None => println!(
                    "{:<20} {:>10} {:>12} {:<10} {}",
                    r.name,
                    "-",
                    "-",
                    "-",
                    r.status.label()
                ),
            }
        }
    }
}

/// Runs a full build. Errors only if every category failed (nothing is
/// published then) or on I/O/signing failures.
pub fn run(opts: &BuildOptions) -> Result<Report> {
    let categories = sources::load(&opts.sources)?;
    let secret = fs::read_to_string(&opts.key)
        .with_context(|| format!("reading key {}", opts.key.display()))?;
    let secret = secret.trim();
    let public = manifest::public_key_from_secret(secret)?;

    let v1 = opts.out.join("v1");
    let cat_dir = v1.join("categories");
    fs::create_dir_all(&cat_dir).with_context(|| format!("creating {}", cat_dir.display()))?;

    let previous = load_previous(&opts.out, &public);
    let mut rows = Vec::new();
    let mut entries = Vec::new();

    for cat in &categories {
        let prev = previous.as_ref().and_then(|m| m.category(&cat.name));
        let (status, entry) = match build_category(cat, prev, &cat_dir, opts.allow_shrink) {
            Ok(entry) => {
                let same = prev.is_some_and(|p| p.sha256 == entry.sha256);
                (
                    if same {
                        Status::Unchanged
                    } else {
                        Status::Updated
                    },
                    Some(entry),
                )
            }
            Err(e) => match prev {
                Some(p) => {
                    eprintln!(
                        "warning: category {}: {e:#}; keeping previous entry",
                        cat.name
                    );
                    (Status::KeptPrevious, Some(p.clone()))
                }
                None => {
                    eprintln!(
                        "warning: category {}: {e:#}; omitting (no previous entry)",
                        cat.name
                    );
                    (Status::Failed, None)
                }
            },
        };
        if let Some(e) = &entry {
            entries.push(e.clone());
        }
        rows.push(Row {
            name: cat.name.clone(),
            status,
            entry,
        });
    }

    if !rows.iter().any(|r| r.status.ok()) {
        bail!("every category failed; nothing published");
    }

    let now = chrono::Utc::now();
    let serial = (now.timestamp().max(0) as u64).max(previous.as_ref().map_or(0, |m| m.serial + 1));
    let manifest = Manifest {
        format: FORMAT_VERSION,
        serial,
        generated_at: now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        expires_at: Some(
            (now + chrono::Duration::days(EXPIRY_DAYS))
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        ),
        categories: entries,
    };
    let signed = manifest::sign(&manifest, secret)?;
    write_atomic(
        &opts.out.join(MANIFEST_PATH),
        serde_json::to_string(&signed)?.as_bytes(),
    )?;

    garbage_collect(&cat_dir, &manifest, previous.as_ref())?;
    Ok(Report { manifest, rows })
}

/// Reads and verifies the previously published manifest, if any.
fn load_previous(out: &Path, public: &str) -> Option<Manifest> {
    let path = out.join(MANIFEST_PATH);
    let bytes = fs::read(&path).ok()?;
    let verified = serde_json::from_slice::<SignedManifest>(&bytes)
        .map_err(anyhow::Error::from)
        .and_then(|s| manifest::verify(&s, public, 0));
    match verified {
        Ok(m) => Some(m),
        Err(e) => {
            eprintln!(
                "warning: ignoring previous manifest {}: {e:#}",
                path.display()
            );
            None
        }
    }
}

/// Builds one category and writes its file. Any error means "do not publish".
fn build_category(
    cat: &Category,
    prev: Option<&CategoryEntry>,
    cat_dir: &Path,
    allow_shrink: bool,
) -> Result<CategoryEntry> {
    let mut domains: Vec<String> = Vec::new();
    for src in &cat.sources {
        let reader = sources::open(src).with_context(|| format!("source {src}"))?;
        let before = domains.len();
        let stats = lists::parse_reader(reader, |d| {
            if !cat.is_allowed(&d) {
                domains.push(d);
            }
        })
        .with_context(|| format!("reading source {src}"))?;
        eprintln!(
            "  {}: {src}: {} lines, {} domains kept ({} skipped lines)",
            cat.name,
            stats.lines,
            domains.len() - before,
            stats.skipped
        );
    }
    let bytes = DomainSet::build(&domains).context("building domain set")?;
    drop(domains);
    let count = DomainSet::from_bytes(bytes.clone())?.len() as u64;
    if count == 0 {
        bail!("category produced 0 domains");
    }
    if let Some(p) = prev {
        if !allow_shrink && count * 2 < p.domains {
            bail!(
                "category shrank from {} to {count} domains (>50%); use --allow-shrink to override",
                p.domains
            );
        }
    }
    let sha = manifest::sha256_hex(&bytes);
    let file = CategoryEntry::content_addressed_file(&cat.name, &sha);
    let path = cat_dir.join(file.trim_start_matches("categories/"));
    let up_to_date = fs::read(&path)
        .map(|b| manifest::sha256_hex(&b) == sha)
        .unwrap_or(false);
    if !up_to_date {
        write_atomic(&path, &bytes)?;
    }
    Ok(CategoryEntry {
        name: cat.name.clone(),
        description: cat.description.clone(),
        file,
        sha256: sha,
        size: bytes.len() as u64,
        domains: count,
    })
}

/// Writes `data` to a temp file beside `path`, then renames over it.
fn write_atomic(path: &Path, data: &[u8]) -> Result<()> {
    let dir = path.parent().context("path has no parent")?;
    let tmp = dir.join(format!(
        ".{}.tmp{}",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("file"),
        std::process::id()
    ));
    let write = || -> std::io::Result<()> {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
        fs::rename(&tmp, path)
    };
    write().map_err(|e| {
        let _ = fs::remove_file(&tmp);
        anyhow::Error::new(e).context(format!("writing {}", path.display()))
    })
}

/// Deletes `categories/*.fst` referenced by neither manifest.
fn garbage_collect(cat_dir: &Path, new: &Manifest, prev: Option<&Manifest>) -> Result<()> {
    let keep: HashSet<&str> = new
        .categories
        .iter()
        .chain(prev.iter().flat_map(|m| m.categories.iter()))
        .map(|e| e.file.trim_start_matches("categories/"))
        .collect();
    for entry in fs::read_dir(cat_dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if name.ends_with(".fst") && !keep.contains(name) {
            eprintln!("  gc: removing {name}");
            fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use blocker_core::manifest::{generate_keypair, verify};

    struct Env {
        dir: tempfile::TempDir,
        public: String,
        opts: BuildOptions,
    }

    fn setup(sources_toml: &str) -> Env {
        let dir = tempfile::tempdir().unwrap();
        let (secret, public) = generate_keypair();
        fs::write(dir.path().join("key"), &secret).unwrap();
        fs::write(dir.path().join("sources.toml"), sources_toml).unwrap();
        let opts = BuildOptions {
            sources: dir.path().join("sources.toml"),
            out: dir.path().join("public"),
            key: dir.path().join("key"),
            allow_shrink: false,
        };
        Env { dir, public, opts }
    }

    fn manifest_of(env: &Env) -> Manifest {
        let s: SignedManifest =
            serde_json::from_slice(&fs::read(env.opts.out.join(MANIFEST_PATH)).unwrap()).unwrap();
        verify(&s, &env.public, 0).unwrap()
    }

    fn write(env: &Env, name: &str, text: &str) {
        fs::write(env.dir.path().join(name), text).unwrap();
    }

    const TOML: &str = r#"
[[category]]
name = "ads"
description = "Ads"
sources = ["file:a.txt", "file:b.txt"]
allow = ["ok.example.com"]

[[category]]
name = "social"
sources = ["file:s.txt"]
"#;

    fn populate(env: &Env) {
        write(
            env,
            "a.txt",
            "0.0.0.0 ads1.com\n0.0.0.0 ads2.com\n0.0.0.0 ok.example.com\n",
        );
        write(env, "b.txt", "||ads3.com^\nads4.com\n");
        write(env, "s.txt", "social.com\n");
    }

    #[test]
    fn builds_signed_manifest_and_lists() {
        let env = setup(TOML);
        populate(&env);
        let report = run(&env.opts).unwrap();
        assert!(report.rows.iter().all(|r| r.status == Status::Updated));
        let m = manifest_of(&env);
        assert_eq!(m.categories.len(), 2);
        let ads = m.category("ads").unwrap();
        assert_eq!(ads.domains, 4);
        let bytes = fs::read(env.opts.out.join("v1").join(&ads.file)).unwrap();
        manifest::verify_file(ads, &bytes).unwrap();
        let set = DomainSet::from_bytes(bytes).unwrap();
        assert!(set.contains("ads1.com") && set.contains("x.ads3.com"));
        assert!(!set.contains("ok.example.com"), "allow list applied");
        assert!(m.expires_at.is_some());

        // Rebuild with identical input: unchanged, serial strictly increases.
        let again = run(&env.opts).unwrap();
        assert!(again.rows.iter().all(|r| r.status == Status::Unchanged));
        assert!(again.manifest.serial > m.serial);
    }

    #[test]
    fn missing_source_keeps_previous_entry() {
        let env = setup(TOML);
        populate(&env);
        let first = run(&env.opts).unwrap();
        fs::remove_file(env.dir.path().join("b.txt")).unwrap();
        let second = run(&env.opts).unwrap();
        let ads = second.rows.iter().find(|r| r.name == "ads").unwrap();
        assert_eq!(ads.status, Status::KeptPrevious);
        assert_eq!(
            ads.entry.as_ref(),
            first.manifest.category("ads"),
            "previous entry unchanged"
        );
        assert_eq!(second.rows[1].status, Status::Unchanged);
    }

    #[test]
    fn all_failed_errors_and_new_failure_is_omitted() {
        let env = setup(TOML);
        // Nothing exists: every category fails, nothing published.
        assert!(run(&env.opts).is_err());
        assert!(!env.opts.out.join(MANIFEST_PATH).exists());
        // Only one source exists: the other category is omitted.
        write(&env, "s.txt", "social.com\n");
        let r = run(&env.opts).unwrap();
        assert_eq!(r.rows[0].status, Status::Failed);
        assert_eq!(manifest_of(&env).categories.len(), 1);
    }

    #[test]
    fn shrink_over_half_is_refused_unless_allowed() {
        let env = setup(TOML);
        populate(&env);
        run(&env.opts).unwrap();
        write(&env, "a.txt", "ads1.com\n");
        write(&env, "b.txt", "");
        let r = run(&env.opts).unwrap();
        assert_eq!(r.rows[0].status, Status::KeptPrevious);
        assert_eq!(r.rows[0].entry.as_ref().unwrap().domains, 4);

        let mut opts = env.opts.clone();
        opts.allow_shrink = true;
        let r = run(&opts).unwrap();
        assert_eq!(r.rows[0].status, Status::Updated);
        assert_eq!(r.rows[0].entry.as_ref().unwrap().domains, 1);
    }

    #[test]
    fn gc_keeps_previous_manifest_files_only() {
        let env = setup(TOML);
        populate(&env);
        let gen1 = run(&env.opts).unwrap();
        let f1 = gen1.manifest.category("social").unwrap().file.clone();

        write(&env, "s.txt", "social.com\nsocial2.com\n");
        let gen2 = run(&env.opts).unwrap();
        let f2 = gen2.manifest.category("social").unwrap().file.clone();
        assert_ne!(f1, f2);
        let v1 = env.opts.out.join("v1");
        assert!(v1.join(&f1).exists(), "previous generation kept");
        assert!(v1.join(&f2).exists());

        write(&env, "s.txt", "social.com\nsocial2.com\nsocial3.com\n");
        let gen3 = run(&env.opts).unwrap();
        let f3 = gen3.manifest.category("social").unwrap().file.clone();
        assert!(!v1.join(&f1).exists(), "two generations old is collected");
        assert!(v1.join(&f2).exists() && v1.join(&f3).exists());
    }

    #[test]
    fn tampered_previous_manifest_is_ignored() {
        let env = setup(TOML);
        populate(&env);
        run(&env.opts).unwrap();
        fs::write(env.opts.out.join(MANIFEST_PATH), "{ not json").unwrap();
        let r = run(&env.opts).unwrap();
        assert!(r
            .rows
            .iter()
            .all(|r| r.status == Status::Updated || r.status == Status::Unchanged));
    }
}
