//! Blocker's menu-bar app: a tray icon that shows whether blocking is active
//! right now, and a Rune window with rules, blocklists and system health.
//!
//! The app never filters anything itself. It reads the config directory,
//! asks the signed `blocker` CLI for daemon/filter status, and runs that CLI
//! with admin rights (macOS password prompt) for changes.
//!
//! ```sh
//! blocker-ui                      # menu-bar only; the window opens from the tray
//! blocker-ui --open               # also open the window at launch
//! blocker-ui --snap ui.png --scene blocking   # headless render (debug builds)
//! ```

mod model;
mod privileged;
mod tray;
mod view;

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use blocker_core::status::SystemStatus;
use blocker_core::{Store, WeekTime};
use rune::{App, AppHandle, AppSpec};

use crate::model::Snapshot;
use crate::privileged::AdminError;
use crate::view::{Notice, Request, Ui};

const REFRESH: Duration = Duration::from_secs(2);
const STATUS_EVERY: Duration = Duration::from_secs(10);
const STATUS_TIMEOUT: Duration = Duration::from_secs(8);

/// Named demo states for headless renders: `--scene blocking|idle|attention`.
type Scene = (&'static str, fn(&mut Ui));

static SCENES: &[Scene] = &[
    ("blocking", |s| view::demo(s, "blocking")),
    ("idle", |s| view::demo(s, "idle")),
    ("attention", |s| view::demo(s, "attention")),
];

fn config_dir() -> PathBuf {
    std::env::var_os("BLOCKER_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(blocker_core::config::SYSTEM_DIR))
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn main() {
    let (requests, inbox) = mpsc::channel::<Request>();
    let tray_requests = requests.clone();
    let mut app = App::new(AppSpec::<Ui> {
        scenes: SCENES,
        min_size: Some((400.0, 420.0)),
        ..AppSpec::new("Blocker", (460.0, 720.0), || Ui::new(None), view::view)
    })
    .init(move || Ui::new(Some(requests)))
    .keep_running()
    .accessory();
    if !std::env::args().any(|a| a == "--open") {
        app = app.windowless();
    }

    // Headless renders (`--snap`, `--tree`, scripts) get no tray, status
    // polling or worker: scenes supply the state.
    if rune::is_headless() {
        app.run();
        return;
    }

    let handle = app.handle();
    {
        let handle = handle.clone();
        std::thread::Builder::new()
            .name("blocker-ui-refresh".into())
            .spawn(move || refresh_loop(handle, tray_requests))
            .expect("spawning refresh thread");
    }
    {
        let handle = handle.clone();
        std::thread::Builder::new()
            .name("blocker-ui-worker".into())
            .spawn(move || worker(handle, inbox))
            .expect("spawning worker thread");
    }
    app.run();
}

/// Reloads the snapshot when the config or lists change (or the minute
/// ticks, for schedules), and the system status every few seconds.
fn refresh_loop(handle: AppHandle<Ui>, requests: mpsc::Sender<Request>) {
    let store = Store::new(config_dir());
    let mut seen: Option<(u64, u16)> = None;
    let mut last_status: Option<Instant> = None;
    // The tray is created by the first update, on the main thread.
    let tray_handle = handle.clone();
    handle.update(move |s, _| tray_init(s, tray_handle, requests));
    loop {
        let at = WeekTime::now();
        let key = (store.fingerprint(), at.minute);
        if seen != Some(key) {
            seen = Some(key);
            let snap = Snapshot::load(&store, at);
            handle.update(move |s, _| {
                s.snap = snap;
                s.now = now_unix();
                tray_sync(s);
            });
        }
        if last_status.is_none_or(|t| t.elapsed() >= STATUS_EVERY) {
            last_status = Some(Instant::now());
            let status = system_status();
            handle.update(move |s, _| {
                s.system = status;
                s.now = now_unix();
                tray_sync(s);
            });
        }
        std::thread::sleep(REFRESH);
    }
}

/// Asks the signed CLI for daemon/filter status; `None` if it can't answer.
fn system_status() -> Option<SystemStatus> {
    let mut child = Command::new(privileged::cli_path())
        .args(["status", "--json"])
        .env("BLOCKER_DIR", config_dir())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + STATUS_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(100)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    let out = child.wait_with_output().ok()?;
    serde_json::from_slice(&out.stdout).ok()
}

/// Runs window and tray requests with admin rights, one at a time.
fn worker(handle: AppHandle<Ui>, inbox: Receiver<Request>) {
    for request in inbox {
        let (busy, args, prompt): (&str, Vec<String>, String) = match &request {
            Request::SetRuleEnabled { rule, enabled } => (
                if *enabled {
                    "Enabling rule…"
                } else {
                    "Disabling rule…"
                },
                vec![
                    "rule".into(),
                    if *enabled { "enable" } else { "disable" }.into(),
                    rule.clone(),
                ],
                format!(
                    "Blocker wants to {} the rule \"{rule}\".",
                    if *enabled { "enable" } else { "disable" }
                ),
            ),
            Request::UpdateLists => (
                "Updating lists…",
                vec!["update".into()],
                "Blocker wants to update its blocklists.".into(),
            ),
        };
        handle.update(move |s, _| {
            s.busy = Some(busy.into());
            s.notice = None;
        });
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let result = privileged::run_as_admin(&privileged::cli_path(), &args, &prompt);
        let notice = match result {
            Ok(_) => Some(Notice {
                ok: true,
                text: match &request {
                    Request::SetRuleEnabled { rule, enabled } => {
                        format!(
                            "Rule \"{rule}\" {}.",
                            if *enabled { "enabled" } else { "disabled" }
                        )
                    }
                    Request::UpdateLists => "Blocklists are up to date.".into(),
                },
            }),
            Err(AdminError::Cancelled) => None,
            Err(AdminError::Failed(e)) => Some(Notice { ok: false, text: e }),
        };
        // The refresher picks up the config/list change on its next pass.
        handle.update(move |s, _| {
            s.busy = None;
            s.notice = notice;
        });
    }
}

thread_local! {
    static TRAY: std::cell::RefCell<Option<tray::Tray>> = const { std::cell::RefCell::new(None) };
}

/// Creates the tray icon. Runs inside `AppHandle::update`, i.e. on the main
/// thread, where AppKit requires it on macOS.
fn tray_init(s: &Ui, handle: AppHandle<Ui>, requests: mpsc::Sender<Request>) {
    let on_action = move |action| match action {
        tray::TrayAction::Open => handle.open(),
        tray::TrayAction::UpdateLists => {
            let _ = requests.send(Request::UpdateLists);
        }
        tray::TrayAction::Quit => handle.quit(),
    };
    #[cfg(target_os = "macos")]
    let tray = {
        let Some(mtm) = objc2::MainThreadMarker::new() else {
            return;
        };
        tray::Tray::new(mtm, on_action)
    };
    #[cfg(not(target_os = "macos"))]
    let tray = tray::Tray::new(on_action);
    TRAY.with(|cell| *cell.borrow_mut() = Some(tray));
    tray_sync(s);
}

/// Refreshes the tray icon, tooltip and menu from the current state.
/// With `BLOCKER_UI_TRACE=1`, logs the icon and menu whenever they change.
fn tray_sync(s: &Ui) {
    thread_local! {
        static LAST: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
    }
    TRAY.with(|cell| {
        if let Some(t) = cell.borrow_mut().as_mut() {
            t.show(&s.activity(), &s.snap);
            if std::env::var_os("BLOCKER_UI_TRACE").is_some() {
                let line = format!(
                    "tray: icon={} menu={:?}",
                    t.symbol_shown().unwrap_or("-"),
                    t.menu_titles()
                );
                LAST.with(|last| {
                    if *last.borrow() != line {
                        eprintln!("{line}");
                        *last.borrow_mut() = line;
                    }
                });
            }
        }
    });
}
