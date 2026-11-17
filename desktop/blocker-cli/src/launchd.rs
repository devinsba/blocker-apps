//! LaunchDaemon management for the root `blocker daemon` process.
//!
//! The daemon plist lives at [`PLIST_PATH`] and is loaded into the `system`
//! launchd domain:
//!
//! ```text
//! launchctl bootstrap system /Library/LaunchDaemons/dev.blocker.daemon.plist
//! launchctl bootout   system/dev.blocker.daemon
//! ```
//!
//! [`render_plist`] and [`parse_print`] are pure so they can be unit tested;
//! [`install`], [`uninstall`] and [`status`] shell out to `launchctl`.

use std::fs;
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};

/// launchd label of the daemon.
pub const LABEL: &str = "dev.blocker.daemon";
/// Where the LaunchDaemon plist is installed.
pub const PLIST_PATH: &str = "/Library/LaunchDaemons/dev.blocker.daemon.plist";
/// Directory for the daemon's stdout/stderr log.
const LOG_DIR: &str = "/Library/Logs/Blocker";
/// Daemon log file.
const LOG_PATH: &str = "/Library/Logs/Blocker/daemon.log";

/// Runtime state of the loaded daemon, as reported by launchd.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonStatus {
    pub running: bool,
    pub pid: Option<u32>,
}

/// Renders the LaunchDaemon plist (XML) for the given executable path.
pub fn render_plist(exe: &Path) -> Result<String> {
    let exe = exe.to_str().context("executable path is not valid UTF-8")?;
    let mut d = plist::Dictionary::new();
    d.insert("Label".into(), LABEL.into());
    d.insert(
        "ProgramArguments".into(),
        plist::Value::Array(vec![exe.into(), "daemon".into()]),
    );
    d.insert("RunAtLoad".into(), true.into());
    d.insert("KeepAlive".into(), true.into());
    d.insert("ThrottleInterval".into(), 10u64.into());
    d.insert("StandardOutPath".into(), LOG_PATH.into());
    d.insert("StandardErrorPath".into(), LOG_PATH.into());
    let mut buf = Vec::new();
    plist::Value::Dictionary(d)
        .to_writer_xml(&mut buf)
        .context("serializing plist")?;
    String::from_utf8(buf).context("plist is not UTF-8")
}

fn require_root() -> Result<()> {
    // SAFETY: geteuid has no preconditions and cannot fail.
    if unsafe { libc::geteuid() } != 0 {
        bail!("this command must run as root; run with sudo");
    }
    Ok(())
}

fn launchctl(args: &[&str]) -> Result<std::process::Output> {
    Command::new("/bin/launchctl")
        .args(args)
        .output()
        .context("running launchctl")
}

/// Installs and (re)starts the daemon. Requires root.
///
/// Writes the plist (0644 root:wheel) atomically, creates the log directory,
/// then boots out any previous instance and bootstraps the new one.
pub fn install(exe: &Path) -> Result<()> {
    require_root()?;
    let xml = render_plist(exe)?;

    fs::create_dir_all(LOG_DIR).with_context(|| format!("creating {LOG_DIR}"))?;

    let tmp = format!("{PLIST_PATH}.tmp");
    {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o644)
            .open(&tmp)
            .with_context(|| format!("creating {tmp}"))?;
        f.write_all(xml.as_bytes())?;
        f.sync_all()?;
    }
    fs::set_permissions(&tmp, fs::Permissions::from_mode(0o644))?;
    std::os::unix::fs::chown(&tmp, Some(0), Some(0)).context("chown root:wheel")?;
    fs::rename(&tmp, PLIST_PATH).with_context(|| format!("installing {PLIST_PATH}"))?;

    let target = format!("system/{LABEL}");
    if let Err(e) = launchctl(&["bootout", &target]) {
        log::debug!("bootout failed (ignored): {e:#}");
    }
    let out = launchctl(&["bootstrap", "system", PLIST_PATH])?;
    if !out.status.success() {
        bail!(
            "launchctl bootstrap failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(())
}

/// Stops the daemon and removes its plist. Requires root; idempotent.
pub fn uninstall() -> Result<()> {
    require_root()?;
    let target = format!("system/{LABEL}");
    if let Err(e) = launchctl(&["bootout", &target]) {
        log::debug!("bootout failed (ignored): {e:#}");
    }
    match fs::remove_file(PLIST_PATH) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e).with_context(|| format!("removing {PLIST_PATH}")),
    }
}

/// Queries launchd for the daemon; `None` if it is not loaded.
pub fn status() -> Option<DaemonStatus> {
    let out = launchctl(&["print", &format!("system/{LABEL}")]).ok()?;
    if !out.status.success() {
        return None;
    }
    Some(parse_print(&String::from_utf8_lossy(&out.stdout)))
}

/// Parses the output of `launchctl print system/<label>`.
fn parse_print(text: &str) -> DaemonStatus {
    let mut running = false;
    let mut pid = None;
    for line in text.lines() {
        let line = line.trim();
        if line == "state = running" {
            running = true;
        } else if let Some(rest) = line.strip_prefix("pid = ") {
            if pid.is_none() {
                pid = rest.trim().parse().ok();
            }
        }
    }
    DaemonStatus { running, pid }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plist_has_expected_keys() {
        let xml = render_plist(Path::new(
            "/Applications/Blocker.app/Contents/MacOS/blocker",
        ))
        .unwrap();
        let v = plist::Value::from_reader_xml(xml.as_bytes()).unwrap();
        let d = v.as_dictionary().unwrap();
        assert_eq!(d["Label"].as_string(), Some(LABEL));
        let args: Vec<_> = d["ProgramArguments"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a.as_string().unwrap())
            .collect();
        assert_eq!(
            args,
            ["/Applications/Blocker.app/Contents/MacOS/blocker", "daemon"]
        );
        assert_eq!(d["RunAtLoad"].as_boolean(), Some(true));
        assert_eq!(d["KeepAlive"].as_boolean(), Some(true));
        assert_eq!(d["ThrottleInterval"].as_unsigned_integer(), Some(10));
        assert_eq!(d["StandardOutPath"].as_string(), Some(LOG_PATH));
        assert_eq!(d["StandardErrorPath"].as_string(), Some(LOG_PATH));
    }

    #[test]
    fn parses_running() {
        let sample = "system/dev.blocker.daemon = {\n\tactive count = 1\n\tpath = /Library/LaunchDaemons/dev.blocker.daemon.plist\n\ttype = LaunchDaemon\n\tstate = running\n\n\tprogram = /Applications/Blocker.app/Contents/MacOS/blocker\n\tdefault environment = {\n\t\tPATH => /usr/bin:/bin\n\t}\n\n\truns = 1\n\tpid = 4242\n\timmediate reason = inefficient\n}\n";
        assert_eq!(
            parse_print(sample),
            DaemonStatus {
                running: true,
                pid: Some(4242)
            }
        );
    }

    #[test]
    fn parses_waiting() {
        let sample = "system/dev.blocker.daemon = {\n\tstate = waiting\n\truns = 0\n\tlast exit code = 1\n}\n";
        assert_eq!(
            parse_print(sample),
            DaemonStatus {
                running: false,
                pid: None
            }
        );
    }
}
