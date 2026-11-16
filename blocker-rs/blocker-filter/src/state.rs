//! The engine the provider consults, hot-reloaded when the CLI changes the
//! config or the updater installs new lists.

use std::sync::{Arc, RwLock};
use std::time::Duration;

use blocker_core::{Engine, Store};

pub const SUBSYSTEM: &str = "dev.blocker.filter";

const RELOAD_INTERVAL: Duration = Duration::from_secs(3);

static ENGINE: RwLock<Option<Arc<Engine>>> = RwLock::new(None);

/// The current engine, or `None` before the first successful load (in which
/// case every flow is allowed: failing open keeps the Mac usable if the
/// config is broken, while the CLI's `status` surfaces the error).
pub fn engine() -> Option<Arc<Engine>> {
    ENGINE.read().ok().and_then(|g| g.clone())
}

fn reload(store: &Store) {
    match store.load_engine() {
        Ok((engine, warnings)) => {
            for w in &warnings {
                log::warn!("{w}");
            }
            if let Ok(mut slot) = ENGINE.write() {
                *slot = Some(Arc::new(engine));
            }
            log::info!("engine loaded ({} warnings)", warnings.len());
        }
        // Keep the previous engine: a half-written or invalid config must not
        // silently turn blocking off.
        Err(e) => log::error!("failed to load config, keeping previous rules: {e:#}"),
    }
}

/// Loads the engine now and polls for changes in the background.
pub fn start_reloader() {
    let store = Store::system();
    let mut seen = store.fingerprint();
    reload(&store);
    std::thread::Builder::new()
        .name("blocker-reload".into())
        .spawn(move || loop {
            std::thread::sleep(RELOAD_INTERVAL);
            let now = store.fingerprint();
            if now != seen {
                seen = now;
                reload(&store);
            }
        })
        .expect("spawning reload thread");
}
