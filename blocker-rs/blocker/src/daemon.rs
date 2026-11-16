//! The root LaunchDaemon: keeps blocklists current and closes blocked apps.
//!
//! Network enforcement lives in the content filter extension; the daemon
//! never touches the network stack either, so nothing here can interfere
//! with Tailscale, Twingate or other VPNs.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use blocker_core::{Engine, Store, WeekTime};

use crate::apps::{Action, Enforcer};
use crate::updater;

const TICK: Duration = Duration::from_secs(2);
const UPDATE_CHECK: Duration = Duration::from_secs(60);

pub fn run(store: &Store) -> Result<()> {
    log::info!("daemon starting, config dir {}", store.dir().display());
    let mut enforcer = Enforcer::new()?;
    let mut engine: Option<Engine> = None;
    let mut seen = None;
    let mut last_update_check: Option<Instant> = None;

    loop {
        let fp = store.fingerprint();
        if seen != Some(fp) {
            match store.load_engine() {
                Ok((e, warnings)) => {
                    for w in warnings {
                        log::warn!("{w}");
                    }
                    log::info!("rules loaded");
                    engine = Some(e);
                    seen = Some(fp);
                }
                // Keep enforcing the previous rules rather than none.
                Err(e) => log::error!("config error, keeping previous rules: {e:#}"),
            }
        }

        if let Some(engine) = &engine {
            let blocked = engine.blocked_apps(WeekTime::now());
            if !blocked.bundle_ids.is_empty() || !blocked.process_names.is_empty() {
                for action in enforcer.enforce(&blocked, engine.guard()) {
                    match action {
                        Action::Terminated(app) => {
                            log::info!("closed {} (pid {})", app.name, app.pid)
                        }
                        Action::Killed(app) => {
                            log::info!("force-quit {} (pid {})", app.name, app.pid)
                        }
                    }
                }
            }
        }

        if last_update_check.is_none_or(|t| t.elapsed() >= UPDATE_CHECK) {
            last_update_check = Some(Instant::now());
            check_lists(store);
        }

        std::thread::sleep(TICK);
    }
}

fn check_lists(store: &Store) {
    let Ok(config) = store.load_config() else {
        return;
    };
    let Some(server) = &config.server else { return };
    let Ok(state) = store.load_state() else {
        return;
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let missing = config
        .wanted_categories()
        .iter()
        .any(|c| !state.installed.contains_key(c));
    if !missing && !updater::due(&state, server, now) {
        return;
    }
    match updater::update(store, &config, false) {
        Ok(r) if r.not_modified && r.installed.is_empty() => {
            log::debug!("lists up to date (serial {})", r.serial)
        }
        Ok(r) => log::info!(
            "lists serial {}: installed {:?}, removed {:?}, missing {:?}, failed {:?}",
            r.serial,
            r.installed,
            r.removed,
            r.missing,
            r.failed
        ),
        Err(e) => log::warn!("list update failed, keeping current lists: {e:#}"),
    }
}
