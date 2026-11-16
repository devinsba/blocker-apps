//! `blocker-server`: builds, signs and serves the central category blocklists.
//!
//! Workflow: `keygen` once (pin the printed public key in client configs),
//! then `build` on a schedule, then publish `<public-dir>` via `serve` or any
//! static host.

mod build;
mod serve;
mod sources;

use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use anyhow::{Context, Result};
use blocker_core::manifest;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "blocker-server",
    version,
    about = "Central blocklist server for the blocker apps"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Generate an Ed25519 signing key; prints the public key to stdout.
    Keygen {
        /// Where to write the secret (base64 seed, mode 0600). Never overwritten.
        #[arg(long)]
        out: PathBuf,
    },
    /// Download sources, compile category lists and publish a signed manifest.
    Build {
        /// sources.toml describing the categories.
        #[arg(long)]
        sources: PathBuf,
        /// Public output directory (files are written under <out>/v1/).
        #[arg(long)]
        out: PathBuf,
        /// Secret key file from `keygen`.
        #[arg(long)]
        key: PathBuf,
        /// Publish categories even if they shrank by more than 50%.
        #[arg(long)]
        allow_shrink: bool,
    },
    /// Serve a public directory over HTTP.
    ///
    /// The output directory is plain static files: it can equally be served
    /// by any CDN or object bucket. Use HTTPS in production (put a TLS proxy
    /// or CDN in front); manifest signatures protect integrity regardless.
    Serve {
        /// Public directory produced by `build`.
        #[arg(long)]
        dir: PathBuf,
        /// Listen address.
        #[arg(long, default_value = "127.0.0.1:8080")]
        addr: String,
        /// Periodically rebuild from this sources.toml (requires --key).
        #[arg(long, requires = "key")]
        rebuild_sources: Option<PathBuf>,
        /// Secret key file for rebuilds.
        #[arg(long)]
        key: Option<PathBuf>,
        /// Minutes between rebuilds.
        #[arg(long, default_value_t = 360)]
        rebuild_every_minutes: u64,
    },
}

fn keygen(out: &PathBuf) -> Result<()> {
    let (secret, public) = manifest::generate_keypair();
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(out)
        .with_context(|| format!("creating {} (refusing to overwrite)", out.display()))?;
    writeln!(f, "{secret}")?;
    println!("{public}");
    Ok(())
}

fn run() -> Result<()> {
    match Cli::parse().command {
        Command::Keygen { out } => keygen(&out),
        Command::Build {
            sources,
            out,
            key,
            allow_shrink,
        } => {
            let report = build::run(&build::BuildOptions {
                sources,
                out,
                key,
                allow_shrink,
            })?;
            report.print();
            Ok(())
        }
        Command::Serve {
            dir,
            addr,
            rebuild_sources,
            key,
            rebuild_every_minutes,
        } => {
            let running = serve::start(&dir, &addr)?;
            eprintln!("serving {} on http://{}", dir.display(), running.addr());
            if let (Some(sources), Some(key)) = (rebuild_sources, key) {
                let opts = build::BuildOptions {
                    sources,
                    out: dir.clone(),
                    key,
                    allow_shrink: false,
                };
                let every = Duration::from_secs(rebuild_every_minutes.max(1) * 60);
                std::thread::spawn(move || {
                    if !opts.out.join(manifest::MANIFEST_PATH).exists() {
                        rebuild(&opts);
                    }
                    loop {
                        std::thread::sleep(every);
                        rebuild(&opts);
                    }
                });
            }
            running.wait();
            Ok(())
        }
    }
}

fn rebuild(opts: &build::BuildOptions) {
    eprintln!("rebuilding lists");
    match build::run(opts) {
        Ok(r) => r.print(),
        Err(e) => eprintln!("rebuild failed: {e:#}"),
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}
