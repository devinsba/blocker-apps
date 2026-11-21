//! Closing blocked apps.
//!
//! The network content filter already cuts a blocked app's traffic; this
//! module closes the app itself. The daemon calls [`Enforcer::enforce`] every
//! couple of seconds with the [`BlockedApps`] of the currently active rules.
//! A matching process gets `SIGTERM`, and `SIGKILL` if it is still alive 3
//! seconds or more after that.
//!
//! A process matches when its bundle id (`CFBundleIdentifier` of the
//! outermost `.app` in its executable path; macOS only) equals a blocked
//! bundle id exactly, or when a blocked process name equals its process name
//! or executable file name, ignoring case.
//!
//! On Windows there is no `SIGTERM`: a matching process is ended with
//! `TerminateProcess` straight away.
//!
//! Never touched: pid 0 and 1, the daemon itself and its parent, system
//! binaries (under `/System/`, `/usr/`, `/sbin/` and `/bin/` on Unix, under
//! `C:\Windows\` on Windows), and anything [`Guard::protects_app`] allows.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use blocker_core::engine::BlockedApps;
use blocker_core::guard::Guard;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

/// How long a process may linger after `SIGTERM` before it gets `SIGKILL`.
const KILL_AFTER: Duration = Duration::from_secs(3);

/// Cache size at which [`BundleIdCache`] is cleared.
const CACHE_LIMIT: usize = 4096;

/// Executable path prefixes that are never closed.
#[cfg(unix)]
const SYSTEM_PREFIXES: &[&str] = &["/System/", "/usr/", "/sbin/", "/bin/"];
/// Executable path prefixes (compared ignoring ASCII case) that are never closed.
#[cfg(windows)]
const SYSTEM_PREFIXES: &[&str] = &["C:\\Windows\\"];

fn is_system_path(exe: &str) -> bool {
    SYSTEM_PREFIXES.iter().any(|p| {
        if cfg!(windows) {
            exe.get(..p.len())
                .is_some_and(|h| h.eq_ignore_ascii_case(p))
        } else {
            exe.starts_with(p)
        }
    })
}

/// A running process and the app it belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunningApp {
    pub pid: u32,
    pub name: String,
    pub exe: Option<PathBuf>,
    pub bundle_id: Option<String>,
}

/// What [`Enforcer::enforce`] did to a process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Sent `SIGTERM`.
    Terminated(RunningApp),
    /// Sent `SIGKILL` after the process outlived an earlier `SIGTERM`.
    Killed(RunningApp),
}

/// Returns the outermost `.app` directory in `exe`, if any. Helper apps nested
/// inside another app's bundle resolve to the outer app.
pub fn outermost_app_bundle(exe: &Path) -> Option<PathBuf> {
    let mut acc = PathBuf::new();
    for comp in exe.components() {
        acc.push(comp);
        let is_app = comp
            .as_os_str()
            .to_str()
            .is_some_and(|s| s.len() > 4 && s.to_ascii_lowercase().ends_with(".app"));
        // The executable itself is never the bundle directory.
        if is_app && acc != exe {
            return Some(acc);
        }
    }
    None
}

/// Caches exe path to bundle id lookups, since parsing `Info.plist` is the
/// expensive part. Cleared when it grows past 4096 entries.
#[derive(Default)]
pub struct BundleIdCache {
    map: HashMap<PathBuf, Option<String>>,
}

impl BundleIdCache {
    /// The `CFBundleIdentifier` of the outermost `.app` containing `exe`.
    pub fn bundle_id_for(&mut self, exe: &Path) -> Option<String> {
        if let Some(hit) = self.map.get(exe) {
            return hit.clone();
        }
        if self.map.len() >= CACHE_LIMIT {
            self.map.clear();
        }
        let id = outermost_app_bundle(exe).and_then(|app| read_bundle_id(&app));
        self.map.insert(exe.to_path_buf(), id.clone());
        id
    }
}

#[cfg(target_os = "macos")]
fn read_bundle_id(app: &Path) -> Option<String> {
    let value = plist::Value::from_file(app.join("Contents/Info.plist")).ok()?;
    let id = value
        .as_dictionary()?
        .get("CFBundleIdentifier")?
        .as_string()?
        .trim();
    (!id.is_empty()).then(|| id.to_string())
}

/// Bundle ids are a macOS concept; elsewhere only process names match.
#[cfg(not(target_os = "macos"))]
fn read_bundle_id(_app: &Path) -> Option<String> {
    None
}

/// Lists running processes with the bundle id of the `.app` they live in.
/// Refreshes `sys` first.
pub fn running_apps(sys: &mut System, cache: &mut BundleIdCache) -> Vec<RunningApp> {
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet),
    );
    sys.processes()
        .iter()
        .map(|(pid, p)| {
            let exe = p.exe().map(Path::to_path_buf);
            let bundle_id = exe.as_deref().and_then(|e| cache.bundle_id_for(e));
            RunningApp {
                pid: pid.as_u32(),
                name: p.name().to_string_lossy().into_owned(),
                exe,
                bundle_id,
            }
        })
        .collect()
}

/// True if `app` matches `blocked` and is not one of the processes that must
/// never be closed.
fn should_close(
    app: &RunningApp,
    blocked: &BlockedApps,
    guard: &Guard,
    self_pid: u32,
    parent_pid: Option<u32>,
) -> bool {
    if app.pid <= 1 || app.pid == self_pid || Some(app.pid) == parent_pid {
        return false;
    }
    if let Some(exe) = app.exe.as_deref().and_then(Path::to_str) {
        if is_system_path(exe) {
            return false;
        }
    }
    let exe_name = app
        .exe
        .as_deref()
        .and_then(Path::file_name)
        .and_then(|n| n.to_str());
    if guard.protects_app(&app.name)
        || exe_name.is_some_and(|n| guard.protects_app(n))
        || app
            .bundle_id
            .as_deref()
            .is_some_and(|b| guard.protects_app(b))
    {
        return false;
    }
    let by_bundle = app
        .bundle_id
        .as_deref()
        .is_some_and(|b| blocked.bundle_ids.iter().any(|x| x == b));
    let by_name = blocked.process_names.iter().any(|n| {
        n.eq_ignore_ascii_case(&app.name) || exe_name.is_some_and(|e| n.eq_ignore_ascii_case(e))
    });
    by_bundle || by_name
}

/// Terminates blocked apps; keeps the state needed to escalate to `SIGKILL`.
pub struct Enforcer {
    sys: System,
    cache: BundleIdCache,
    pending_kill: HashMap<u32, Instant>,
}

impl Enforcer {
    pub fn new() -> anyhow::Result<Self> {
        let sys = System::new().map_err(|e| anyhow::anyhow!("initializing sysinfo: {e}"))?;
        Ok(Self {
            sys,
            cache: BundleIdCache::default(),
            pending_kill: HashMap::new(),
        })
    }

    /// Closes processes matching `blocked`. A process is sent `SIGTERM` the
    /// first time it is seen; if the same pid is still alive on a call made 3
    /// seconds or more after that, it gets `SIGKILL`. Returns what it acted on.
    pub fn enforce(&mut self, blocked: &BlockedApps, guard: &Guard) -> Vec<Action> {
        if blocked.bundle_ids.is_empty() && blocked.process_names.is_empty() {
            self.pending_kill.clear();
            return Vec::new();
        }
        let apps = running_apps(&mut self.sys, &mut self.cache);
        let self_pid = std::process::id();
        let parent_pid = self
            .sys
            .process(Pid::from_u32(self_pid))
            .and_then(|p| p.parent())
            .map(Pid::as_u32);
        let now = Instant::now();
        let mut actions = Vec::new();
        let mut still_pending = HashMap::new();
        for app in apps {
            if !should_close(&app, blocked, guard, self_pid, parent_pid) {
                continue;
            }
            match self.pending_kill.get(&app.pid) {
                Some(&sent) if now.duration_since(sent) >= KILL_AFTER => {
                    if send_signal(&self.sys, app.pid, Signal::Kill) {
                        actions.push(Action::Killed(app));
                    }
                }
                Some(&sent) => {
                    still_pending.insert(app.pid, sent);
                }
                None => {
                    if send_signal(&self.sys, app.pid, Signal::Term) {
                        still_pending.insert(app.pid, now);
                        actions.push(Action::Terminated(app));
                    }
                }
            }
        }
        self.pending_kill = still_pending;
        actions
    }
}

#[derive(Debug, Clone, Copy)]
enum Signal {
    Term,
    Kill,
}

#[cfg(unix)]
fn send_signal(_sys: &System, pid: u32, sig: Signal) -> bool {
    let sig = match sig {
        Signal::Term => libc::SIGTERM,
        Signal::Kill => libc::SIGKILL,
    };
    let Ok(pid) = libc::pid_t::try_from(pid) else {
        return false;
    };
    // SAFETY: kill(2) takes plain integers and has no memory-safety preconditions.
    let rc = unsafe { libc::kill(pid, sig) };
    if rc != 0 {
        log::warn!(
            "kill({pid}, {sig}) failed: {}",
            std::io::Error::last_os_error()
        );
    }
    rc == 0
}

/// Windows has no graceful signal for arbitrary processes, so both steps end
/// the process with `TerminateProcess` (via sysinfo).
#[cfg(windows)]
fn send_signal(sys: &System, pid: u32, sig: Signal) -> bool {
    let ok = sys.process(Pid::from_u32(pid)).is_some_and(|p| p.kill());
    if !ok {
        log::warn!("terminating pid {pid} ({sig:?}) failed");
    }
    ok
}

#[cfg(test)]
mod tests {
    use super::*;
    use blocker_core::config::Settings;

    fn guard() -> Guard {
        Guard::new(&Settings::default(), &[]).0
    }

    fn app(pid: u32, name: &str, exe: &str, bundle: Option<&str>) -> RunningApp {
        RunningApp {
            pid,
            name: name.into(),
            exe: Some(PathBuf::from(exe)),
            bundle_id: bundle.map(Into::into),
        }
    }

    fn blocked(bundles: &[&str], names: &[&str]) -> BlockedApps {
        BlockedApps {
            bundle_ids: bundles.iter().map(|s| s.to_string()).collect(),
            process_names: names.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn outermost_bundle() {
        let p = |s: &str| outermost_app_bundle(Path::new(s));
        assert_eq!(
            p("/Applications/Foo.app/Contents/MacOS/Foo"),
            Some(PathBuf::from("/Applications/Foo.app"))
        );
        assert_eq!(
            p("/Applications/Google Chrome.app/Contents/Frameworks/X.framework/Helpers/Google Chrome Helper.app/Contents/MacOS/Google Chrome Helper"),
            Some(PathBuf::from("/Applications/Google Chrome.app"))
        );
        assert_eq!(p("/usr/bin/sleep"), None);
        assert_eq!(p("/opt/.app/bin/x"), None);
        assert_eq!(p("/Applications/Foo.app"), None);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn cache_reads_plist() {
        let dir = tempfile::tempdir().unwrap();
        let contents = dir.path().join("X.app/Contents");
        std::fs::create_dir_all(contents.join("MacOS")).unwrap();
        std::fs::write(
            contents.join("Info.plist"),
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>com.example.x</string>
</dict></plist>"#,
        )
        .unwrap();
        let exe = contents.join("MacOS/X");
        let mut cache = BundleIdCache::default();
        assert_eq!(cache.bundle_id_for(&exe).as_deref(), Some("com.example.x"));
        // Cached even after the plist disappears.
        std::fs::remove_file(contents.join("Info.plist")).unwrap();
        assert_eq!(cache.bundle_id_for(&exe).as_deref(), Some("com.example.x"));
        assert_eq!(cache.bundle_id_for(Path::new("/usr/bin/sleep")), None);
        let missing = dir.path().join("Y.app/Contents/MacOS/Y");
        assert_eq!(cache.bundle_id_for(&missing), None);
    }

    #[cfg(unix)]
    #[test]
    fn should_close_cases() {
        let g = guard();
        let b = blocked(&["com.valvesoftware.steam"], &["Discord"]);
        let steam = app(
            500,
            "steam_osx",
            "/Applications/Steam.app/Contents/MacOS/steam_osx",
            Some("com.valvesoftware.steam"),
        );
        assert!(should_close(&steam, &b, &g, 10, Some(2)));
        let discord = app(
            501,
            "discord",
            "/Applications/Discord.app/Contents/MacOS/Discord",
            None,
        );
        assert!(should_close(&discord, &b, &g, 10, Some(2)));
        let by_exe = app(
            502,
            "Helper",
            "/Applications/Discord.app/Contents/MacOS/DISCORD",
            None,
        );
        assert!(should_close(&by_exe, &b, &g, 10, Some(2)));
        let other = app(
            503,
            "Safari",
            "/Applications/Safari.app/Contents/MacOS/Safari",
            Some("com.apple.Safari"),
        );
        assert!(!should_close(&other, &b, &g, 10, Some(2)));
        // Bundle ids are exact, not prefix.
        let near = app(
            504,
            "x",
            "/Applications/X.app/Contents/MacOS/x",
            Some("com.valvesoftware.steam.helper"),
        );
        assert!(!should_close(&near, &b, &g, 10, Some(2)));

        let all = blocked(
            &["com.tailscale.ipn.macos", "com.briandevins.blocker"],
            &["Tailscale", "tailscaled", "Discord"],
        );
        let ts = app(
            600,
            "Tailscale",
            "/Applications/Tailscale.app/Contents/MacOS/Tailscale",
            Some("com.tailscale.ipn.macos"),
        );
        assert!(!should_close(&ts, &all, &g, 10, Some(2)));
        let tsd = app(601, "tailscaled", "/opt/homebrew/bin/tailscaled", None);
        assert!(!should_close(&tsd, &all, &g, 10, Some(2)));
        let own = app(
            602,
            "blocker",
            "/Applications/Blocker.app/Contents/MacOS/blocker",
            Some("com.briandevins.blocker"),
        );
        assert!(!should_close(&own, &all, &g, 10, Some(2)));

        // Own pid, parent, pid 1, system paths.
        assert!(!should_close(
            &app(10, "Discord", "/x/Discord", None),
            &b,
            &g,
            10,
            Some(2)
        ));
        assert!(!should_close(
            &app(2, "Discord", "/x/Discord", None),
            &b,
            &g,
            10,
            Some(2)
        ));
        assert!(!should_close(
            &app(1, "Discord", "/x/Discord", None),
            &b,
            &g,
            10,
            None
        ));
        assert!(!should_close(
            &app(0, "Discord", "/x/Discord", None),
            &b,
            &g,
            10,
            None
        ));
        assert!(!should_close(
            &app(700, "Discord", "/System/Library/CoreServices/Discord", None),
            &b,
            &g,
            10,
            None
        ));
        assert!(!should_close(
            &app(701, "Discord", "/usr/libexec/Discord", None),
            &b,
            &g,
            10,
            None
        ));
    }

    #[cfg(windows)]
    #[test]
    fn should_close_windows() {
        let g = guard();
        let b = blocked(&[], &["discord.exe"]);
        let discord = app(
            500,
            "Discord.exe",
            r"C:\Users\me\AppData\Local\Discord\app-1.0\Discord.exe",
            None,
        );
        assert!(should_close(&discord, &b, &g, 10, Some(2)));
        let system = app(501, "discord.exe", r"c:\windows\System32\discord.exe", None);
        assert!(!should_close(&system, &b, &g, 10, Some(2)));
        assert!(!should_close(&discord, &b, &g, 500, Some(2)));
    }

    #[test]
    fn running_apps_finds_self() {
        let mut sys = System::new().unwrap();
        let mut cache = BundleIdCache::default();
        let apps = running_apps(&mut sys, &mut cache);
        let me = std::process::id();
        let mine = apps
            .iter()
            .find(|a| a.pid == me)
            .expect("own process listed");
        assert!(mine.exe.is_some());
        assert!(!mine.name.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn enforce_terminates_process() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("blockertestproc");
        std::fs::copy("/bin/sleep", &bin).unwrap();
        let mut child = std::process::Command::new(&bin).arg("30").spawn().unwrap();
        let g = guard();
        let b = blocked(&[], &["blockertestproc"]);
        let mut enforcer = Enforcer::new().unwrap();
        // The child may need a moment to exec under its new name.
        let mut acted = Vec::new();
        for _ in 0..20 {
            acted = enforcer.enforce(&b, &g);
            if !acted.is_empty() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let status = child.wait().unwrap();
        assert!(
            matches!(acted.as_slice(), [Action::Terminated(a)] if a.pid == child.id()),
            "{acted:?}"
        );
        // The OS may SIGKILL a copied system binary, and some `sleep`
        // implementations catch SIGTERM and exit 1, so only require that it
        // did not finish normally.
        assert!(!status.success(), "{status:?}");
    }
}
