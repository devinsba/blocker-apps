//! `blocker`: command-line front end and root daemon for the Blocker content
//! filter. Ships as `Blocker.app/Contents/MacOS/blocker`.

#[cfg(target_os = "macos")]
mod activation;
mod apps;
mod daemon;
mod launchd;
mod updater;

use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use blocker_core::config::{Rule, ServerConfig};
use blocker_core::engine::{normalize_app_id, Proto};
use blocker_core::schedule::{parse_window, Schedule};
use blocker_core::{Config, FlowInfo, Store, Verdict, WeekTime};
use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "blocker",
    version,
    about = "Block websites and apps without fighting your VPN"
)]
struct Cli {
    /// Configuration directory (normally only changed for testing).
    #[arg(long, env = "BLOCKER_DIR", default_value = blocker_core::config::SYSTEM_DIR, global = true)]
    dir: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Manage blocking rules.
    #[command(subcommand)]
    Rule(RuleCommand),
    /// Point at the central blocklist server.
    Server {
        /// Base URL, e.g. https://lists.example.com
        url: String,
        /// Base64 Ed25519 public key printed by `blocker-server keygen`.
        #[arg(long)]
        public_key: String,
        #[arg(long, default_value_t = 60)]
        refresh_minutes: u64,
    },
    /// Never block these, whatever the rules or lists say.
    Allow(AllowArgs),
    /// Download new blocklists from the server now.
    Update {
        /// Ignore the cached ETag and re-check every category.
        #[arg(long)]
        force: bool,
    },
    /// Show whether a host (and optionally an app) would be blocked right now.
    Check {
        host: String,
        /// Bundle id of the connecting app.
        #[arg(long)]
        app: Option<String>,
    },
    /// Show rules, lists, the daemon and the content filter.
    Status,
    /// List running apps and their bundle ids (to write app rules).
    Apps,
    /// Install the content filter extension and turn filtering on.
    Activate,
    /// Turn filtering off and remove the extension.
    Deactivate,
    /// Install the root LaunchDaemon (list updates + app enforcement).
    Install,
    /// Remove the LaunchDaemon.
    Uninstall,
    /// Run the daemon in the foreground (what launchd runs).
    Daemon,
}

#[derive(Subcommand)]
enum RuleCommand {
    /// Add or replace a rule.
    Add(RuleArgs),
    Remove {
        name: String,
    },
    Enable {
        name: String,
    },
    Disable {
        name: String,
    },
    List,
}

#[derive(Args)]
struct RuleArgs {
    name: String,
    /// Domain to block, including subdomains (repeatable).
    #[arg(long = "domain", short = 'd')]
    domains: Vec<String>,
    /// Server category to block, e.g. ads, social (repeatable).
    #[arg(long = "category", short = 'c')]
    categories: Vec<String>,
    /// App bundle id to close and cut off (repeatable).
    #[arg(long = "bundle-id", short = 'b')]
    bundle_ids: Vec<String>,
    /// Process name to close (repeatable).
    #[arg(long = "process", short = 'p')]
    process_names: Vec<String>,
    /// When the rule applies, e.g. "mon-fri 09:00-17:00" (repeatable; default: always).
    #[arg(long = "window", short = 'w')]
    windows: Vec<String>,
    /// Refuse edits, removal and deactivation while the rule is active.
    #[arg(long)]
    locked: bool,
}

#[derive(Args)]
struct AllowArgs {
    #[arg(long = "domain")]
    domains: Vec<String>,
    #[arg(long = "app")]
    apps: Vec<String>,
    #[arg(long = "cidr")]
    cidrs: Vec<String>,
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_secs()
        .init();
    let cli = Cli::parse();
    if let Err(e) = run(cli) {
        eprintln!("error: {e:#}");
        if e.chain().any(|c| {
            c.downcast_ref::<std::io::Error>()
                .is_some_and(|io| io.kind() == std::io::ErrorKind::PermissionDenied)
        }) {
            eprintln!("hint: changing Blocker's configuration requires sudo");
        }
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    let store = Store::new(&cli.dir);
    match cli.command {
        Command::Rule(cmd) => rule_command(&store, cmd),
        Command::Server {
            url,
            public_key,
            refresh_minutes,
        } => {
            let mut config = store.load_config()?;
            ensure_unlocked(&config, "change the list server")?;
            config.server = Some(ServerConfig {
                url,
                public_key,
                refresh_minutes,
            });
            store.save_config(&config)?;
            println!("Server set. Run `sudo blocker update` to download lists.");
            Ok(())
        }
        Command::Allow(args) => {
            let mut config = store.load_config()?;
            ensure_unlocked(&config, "add allow entries")?;
            config.settings.allow_domains.extend(args.domains);
            config.settings.allow_apps.extend(args.apps);
            config.settings.allow_cidrs.extend(args.cidrs);
            store.save_config(&config)?;
            print_warnings(&store)?;
            Ok(())
        }
        Command::Update { force } => {
            let config = store.load_config()?;
            let report = updater::update(&store, &config, force)?;
            println!("{report:#?}");
            Ok(())
        }
        Command::Check { host, app } => check(&store, &host, app.as_deref()),
        Command::Status => status(&store),
        Command::Apps => {
            let mut sys = sysinfo::System::new().context("reading the process table")?;
            let mut cache = apps::BundleIdCache::default();
            let mut list = apps::running_apps(&mut sys, &mut cache);
            list.retain(|a| a.bundle_id.is_some());
            list.sort_by(|a, b| a.bundle_id.cmp(&b.bundle_id).then(a.pid.cmp(&b.pid)));
            list.dedup_by(|a, b| a.bundle_id == b.bundle_id);
            for a in list {
                println!("{:<50} {}", a.bundle_id.unwrap_or_default(), a.name);
            }
            Ok(())
        }
        Command::Activate => platform::activate(),
        Command::Deactivate => {
            ensure_unlocked(&store.load_config()?, "deactivate the filter")?;
            platform::deactivate()
        }
        Command::Install => {
            let exe = std::env::current_exe().context("locating blocker executable")?;
            launchd::install(&exe)?;
            println!("Daemon installed ({}).", launchd::LABEL);
            Ok(())
        }
        Command::Uninstall => {
            ensure_unlocked(&store.load_config()?, "uninstall the daemon")?;
            launchd::uninstall()
        }
        Command::Daemon => daemon::run(&store),
    }
}

fn rule_command(store: &Store, cmd: RuleCommand) -> Result<()> {
    let mut config = store.load_config()?;
    match cmd {
        RuleCommand::List => {
            let at = WeekTime::now();
            for r in &config.rules {
                let active = r.enabled && r.schedule.is_active_at(at.day, at.minute);
                println!(
                    "{} {}{}{}  [{}]",
                    if active { "●" } else { "○" },
                    r.name,
                    if r.enabled { "" } else { " (disabled)" },
                    if r.locked { " (locked)" } else { "" },
                    r.schedule.describe()
                );
                for (label, items) in [
                    ("domains", &r.domains),
                    ("categories", &r.categories),
                    ("bundle ids", &r.bundle_ids),
                    ("processes", &r.process_names),
                ] {
                    if !items.is_empty() {
                        println!("    {label}: {}", items.join(", "));
                    }
                }
            }
            return Ok(());
        }
        RuleCommand::Add(args) => {
            let windows = args
                .windows
                .iter()
                .map(|w| parse_window(w).with_context(|| format!("bad --window {w:?}")))
                .collect::<Result<Vec<_>>>()?;
            let rule = Rule {
                name: args.name,
                enabled: true,
                locked: args.locked,
                domains: args.domains,
                categories: args.categories,
                bundle_ids: args.bundle_ids,
                process_names: args.process_names,
                schedule: if windows.is_empty() {
                    Schedule::Always
                } else {
                    Schedule::Windows { windows }
                },
            };
            if rule.domains.is_empty()
                && rule.categories.is_empty()
                && rule.bundle_ids.is_empty()
                && rule.process_names.is_empty()
            {
                bail!("a rule needs at least one --domain, --category, --bundle-id or --process");
            }
            match config.rules.iter().position(|r| r.name == rule.name) {
                Some(i) => {
                    ensure_rule_unlocked(&config.rules[i], "replace it")?;
                    config.rules[i] = rule;
                }
                None => config.rules.push(rule),
            }
        }
        RuleCommand::Remove { name } => {
            let i = find_rule(&config, &name)?;
            ensure_rule_unlocked(&config.rules[i], "remove it")?;
            config.rules.remove(i);
        }
        RuleCommand::Enable { name } => {
            let i = find_rule(&config, &name)?;
            config.rules[i].enabled = true;
        }
        RuleCommand::Disable { name } => {
            let i = find_rule(&config, &name)?;
            ensure_rule_unlocked(&config.rules[i], "disable it")?;
            config.rules[i].enabled = false;
        }
    }
    store.save_config(&config)?;
    print_warnings(store)
}

fn find_rule(config: &Config, name: &str) -> Result<usize> {
    config
        .rules
        .iter()
        .position(|r| r.name == name)
        .with_context(|| format!("no rule named {name:?}"))
}

fn ensure_rule_unlocked(rule: &Rule, action: &str) -> Result<()> {
    if rule.locked && rule.enabled && rule.schedule.is_active_now() {
        bail!(
            "rule {:?} is locked while active ({}); refusing to {action}",
            rule.name,
            rule.schedule.describe()
        );
    }
    Ok(())
}

/// Allow-lists, server changes and turning the filter off would undo a locked
/// rule, so they wait until no locked rule is active.
fn ensure_unlocked(config: &Config, action: &str) -> Result<()> {
    for rule in &config.rules {
        ensure_rule_unlocked(rule, action)?;
    }
    Ok(())
}

fn print_warnings(store: &Store) -> Result<()> {
    let (_, warnings) = store.load_engine()?;
    for w in warnings {
        eprintln!("warning: {w}");
    }
    Ok(())
}

fn check(store: &Store, host: &str, app: Option<&str>) -> Result<()> {
    let (engine, warnings) = store.load_engine()?;
    for w in warnings {
        eprintln!("warning: {w}");
    }
    let app = app.map(normalize_app_id);
    let flow = FlowInfo {
        hostname: Some(host),
        remote_ip: host.parse().ok(),
        remote_port: 443,
        proto: Proto::Tcp,
        app_id: app,
        ech: false,
    };
    match engine.decide(&flow, None, WeekTime::now()) {
        Verdict::Block(reason) => println!("BLOCK  {host}: {reason}"),
        Verdict::Allow if engine.guard().protects_host(host) => {
            println!("ALLOW  {host}: protected (VPN/system/allow list)")
        }
        Verdict::Allow | Verdict::Peek => println!("ALLOW  {host}"),
    }
    Ok(())
}

fn status(store: &Store) -> Result<()> {
    let config = store.load_config()?;
    let at = WeekTime::now();
    println!("Config: {}", store.config_path().display());
    let (engine, warnings) = store.load_engine()?;
    let active = engine.active_rules(at);
    println!(
        "Rules:  {} total, active now: {}",
        config.rules.len(),
        if active.is_empty() {
            "none".to_string()
        } else {
            active.join(", ")
        }
    );

    let state = store.load_state()?;
    match &config.server {
        Some(s) => println!(
            "Server: {} (serial {}, checked {})",
            s.url,
            state.serial,
            ago(state.checked_at)
        ),
        None => println!("Server: not configured (`blocker server <url> --public-key <key>`)"),
    }
    let (sets, _) = store.load_categories(&config);
    for name in config.wanted_categories() {
        match sets.get(&name) {
            Some(set) => println!("  {name:<16} {:>10} domains", set.len()),
            None => println!("  {name:<16} not downloaded"),
        }
    }
    for w in warnings {
        println!("warning: {w}");
    }

    match launchd::status() {
        Some(d) if d.running => println!(
            "Daemon: running (pid {})",
            d.pid.map_or("?".into(), |p| p.to_string())
        ),
        Some(_) => println!("Daemon: loaded, not running"),
        None => println!("Daemon: not installed (`sudo blocker install`)"),
    }
    platform::print_filter_status();
    Ok(())
}

fn ago(unix: u64) -> String {
    if unix == 0 {
        return "never".into();
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    format!("{} min ago", now.saturating_sub(unix) / 60)
}

#[cfg(target_os = "macos")]
mod platform {
    pub use crate::activation::{activate, deactivate};

    pub fn print_filter_status() {
        match crate::activation::status() {
            Ok(s) => {
                let ext = match &s.extensions {
                    Some(list) if list.iter().any(|e| e.enabled) => {
                        let e = list.iter().find(|e| e.enabled).unwrap();
                        format!(
                            "{} installed{}",
                            e.version,
                            if e.uninstalling {
                                " (uninstalling)"
                            } else {
                                ""
                            }
                        )
                    }
                    Some(list) if list.iter().any(|e| e.awaiting_approval) => {
                        "awaiting approval in System Settings".to_string()
                    }
                    Some(_) => "not installed (`blocker activate`)".to_string(),
                    None => "unknown (run from /Applications/Blocker.app)".to_string(),
                };
                println!(
                    "Filter: extension {ext}; content filter {}",
                    match (s.configured, s.enabled) {
                        (true, true) => "enabled",
                        (true, false) => "configured but disabled",
                        _ => "not configured",
                    }
                );
            }
            Err(e) => println!("Filter: unknown ({e:#})"),
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    pub fn activate() -> anyhow::Result<()> {
        anyhow::bail!("the content filter is macOS-only")
    }
    pub fn deactivate() -> anyhow::Result<()> {
        activate()
    }
    pub fn print_filter_status() {
        println!("Filter: unsupported platform");
    }
}
