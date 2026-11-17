//! Running the `blocker` CLI as root from the GUI.
//!
//! Changing Blocker's configuration needs root, but the menu-bar app runs as
//! the logged-in user. The standard macOS answer is AppleScript's
//! `do shell script ... with administrator privileges`, which shows the system
//! password dialog. Arguments (rule names, domains, ...) are user-typed, so
//! they are single-quoted for `/bin/sh` and then escaped for the AppleScript
//! string literal; see [`sh_quote`] and [`applescript_string`].

use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Name of the CLI executable shipped next to the UI in the app bundle.
const CLI_NAME: &str = "blocker";

/// Path of the `blocker` CLI next to this executable, falling back to
/// `blocker` on `PATH` when no sibling exists.
pub fn cli_path() -> PathBuf {
    match std::env::current_exe() {
        Ok(exe) => cli_path_from(&exe),
        Err(_) => PathBuf::from(CLI_NAME),
    }
}

/// Pure helper for [`cli_path`]: the sibling of `exe` named `blocker` if it
/// exists, else bare `blocker`.
pub fn cli_path_from(exe: &Path) -> PathBuf {
    if let Some(dir) = exe.parent() {
        let candidate = dir.join(CLI_NAME);
        if candidate.is_file() {
            return candidate;
        }
    }
    PathBuf::from(CLI_NAME)
}

/// Single-quotes `arg` for `/bin/sh`: wraps it in `'...'` and turns each `'`
/// into `'\''`. Never returns an unquoted string.
pub fn sh_quote(arg: &str) -> String {
    let mut out = String::with_capacity(arg.len() + 2);
    out.push('\'');
    for c in arg.chars() {
        if c == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(c);
        }
    }
    out.push('\'');
    out
}

/// Escapes `s` for inclusion inside an AppleScript double-quoted string
/// literal (backslash and double quote).
pub fn applescript_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c == '\\' || c == '"' {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Builds the `osascript` argv (without the program itself):
/// `["-e", "do shell script \"...\" with administrator privileges with prompt \"...\""]`.
pub fn osascript_args(program: &Path, args: &[&str], prompt: &str) -> Vec<String> {
    let mut parts = vec![sh_quote(&program.to_string_lossy())];
    parts.extend(args.iter().map(|a| sh_quote(a)));
    let cmd = parts.join(" ");
    let script = format!(
        "do shell script \"{}\" with administrator privileges with prompt \"{}\"",
        applescript_string(&cmd),
        applescript_string(prompt)
    );
    vec!["-e".to_string(), script]
}

/// Why [`run_as_admin`] failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdminError {
    /// The user dismissed the password dialog.
    Cancelled,
    /// The command (or osascript) failed; carries the cleaned-up message.
    Failed(String),
}

impl fmt::Display for AdminError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AdminError::Cancelled => write!(f, "cancelled"),
            AdminError::Failed(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for AdminError {}

/// Runs `program args...` as root via `osascript` (blocking; call it off the
/// UI thread). Returns stdout on success, [`AdminError::Cancelled`] when the
/// user dismisses the password dialog, and [`AdminError::Failed`] otherwise.
pub fn run_as_admin(program: &Path, args: &[&str], prompt: &str) -> Result<String, AdminError> {
    let output = Command::new("osascript")
        .args(osascript_args(program, args, prompt))
        .output()
        .map_err(|e| AdminError::Failed(format!("failed to run osascript: {e}")))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(classify_error(&String::from_utf8_lossy(&output.stderr)))
    }
}

/// Parses `osascript` stderr: `-128` / "User canceled" means cancelled;
/// otherwise the message with the `... execution error: ` prefix and the
/// trailing ` (number)` removed.
fn classify_error(stderr: &str) -> AdminError {
    let trimmed = stderr.trim();
    if trimmed.contains("(-128)") || trimmed.contains("-128") || trimmed.contains("User canceled") {
        return AdminError::Cancelled;
    }
    let mut msg = trimmed;
    if let Some(i) = msg.find("execution error: ") {
        msg = &msg[i + "execution error: ".len()..];
    }
    let msg = msg.trim();
    // Strip a trailing " (123)" / " (-1)" error number.
    let msg = match msg.rfind(" (") {
        Some(i)
            if msg.ends_with(')')
                && msg[i + 2..msg.len() - 1]
                    .trim_start_matches('-')
                    .chars()
                    .all(|c| c.is_ascii_digit())
                && msg.len() - 1 > i + 2 =>
        {
            msg[..i].trim_end()
        }
        _ => msg,
    };
    AdminError::Failed(msg.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const NASTY: &[&str] = &[
        "plain",
        "with space",
        "it's",
        "''",
        "'",
        "say \"hi\"",
        "back\\slash",
        "\\'\"\\",
        "$(touch /tmp/pwned)",
        "`touch /tmp/pwned`",
        "a;b && c | d > e",
        "$HOME ${HOME} *?[a]",
        "line1\nline2",
        "tab\there",
        "héllo ☃ 日本語 🚀",
        "",
        "-n",
        "!history ~",
    ];

    fn sh_run(cmd: &str) -> String {
        let out = Command::new("/bin/sh").arg("-c").arg(cmd).output().unwrap();
        assert!(out.status.success());
        String::from_utf8(out.stdout).unwrap()
    }

    #[test]
    fn sh_quote_basic() {
        assert_eq!(sh_quote("abc"), "'abc'");
        assert_eq!(sh_quote(""), "''");
        assert_eq!(sh_quote("it's"), "'it'\\''s'");
    }

    #[test]
    fn sh_quote_roundtrips_through_sh() {
        for a in NASTY {
            // NUL-free; use a delimiter-safe check via printf %s
            let out = sh_run(&format!("printf %s {}", sh_quote(a)));
            assert_eq!(&out, a, "arg {a:?}");
        }
        // Multiple args stay separate.
        let joined: Vec<String> = NASTY.iter().map(|a| sh_quote(a)).collect();
        let out = sh_run(&format!("printf '%s\\0' {}", joined.join(" ")));
        let got: Vec<&str> = out.split('\0').collect();
        assert_eq!(&got[..NASTY.len()], NASTY);
    }

    #[test]
    fn sh_quote_does_not_execute_injection() {
        let marker = std::env::temp_dir().join(format!("blocker-inj-{}", std::process::id()));
        let _ = std::fs::remove_file(&marker);
        let m = marker.to_string_lossy();
        let evil = format!("x'; touch '{m}'; echo '");
        let out = sh_run(&format!("printf %s {}", sh_quote(&evil)));
        assert_eq!(out, evil);
        assert!(!marker.exists());
        let evil2 = format!("$(touch '{m}')`touch '{m}'`");
        let out = sh_run(&format!("printf %s {}", sh_quote(&evil2)));
        assert_eq!(out, evil2);
        assert!(!marker.exists());
    }

    #[test]
    fn applescript_string_escapes() {
        assert_eq!(applescript_string("plain"), "plain");
        assert_eq!(applescript_string(r#"a"b"#), r#"a\"b"#);
        assert_eq!(applescript_string(r"a\b"), r"a\\b");
        assert_eq!(applescript_string(r#"\""#), r#"\\\""#);
    }

    #[test]
    fn osascript_args_shape() {
        let args = osascript_args(
            Path::new("/Applications/Blocker.app/Contents/MacOS/blocker"),
            &["rule", "add", "it's \"x\""],
            "Blocker wants to \"change\" rules",
        );
        assert_eq!(args.len(), 2);
        assert_eq!(args[0], "-e");
        assert_eq!(
            args[1],
            "do shell script \"'/Applications/Blocker.app/Contents/MacOS/blocker' 'rule' 'add' 'it'\\\\''s \\\"x\\\"'\" \
             with administrator privileges with prompt \"Blocker wants to \\\"change\\\" rules\""
        );
    }

    #[test]
    fn cli_path_from_prefers_sibling() {
        let dir = std::env::temp_dir().join(format!("blocker-cli-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("blocker-ui");
        assert_eq!(cli_path_from(&exe), PathBuf::from("blocker"));
        std::fs::write(dir.join("blocker"), "").unwrap();
        assert_eq!(cli_path_from(&exe), dir.join("blocker"));
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(cli_path_from(Path::new("/")), PathBuf::from("blocker"));
    }

    #[test]
    fn classify_cancelled() {
        assert_eq!(
            classify_error("0:97: execution error: User canceled. (-128)\n"),
            AdminError::Cancelled
        );
    }

    #[test]
    fn classify_failed() {
        assert_eq!(
            classify_error("76:80: execution error: blocker: no such rule 'x' (1)\n"),
            AdminError::Failed("blocker: no such rule 'x'".into())
        );
        assert_eq!(
            classify_error("execution error: boom (127)"),
            AdminError::Failed("boom".into())
        );
        assert_eq!(
            classify_error("  plain failure \n"),
            AdminError::Failed("plain failure".into())
        );
        assert_eq!(
            classify_error("execution error: ends with (parens)"),
            AdminError::Failed("ends with (parens)".into())
        );
        assert_eq!(classify_error(""), AdminError::Failed(String::new()));
    }

    #[test]
    fn admin_error_display() {
        assert_eq!(AdminError::Cancelled.to_string(), "cancelled");
        assert_eq!(AdminError::Failed("nope".into()).to_string(), "nope");
    }

    /// Runs the AppleScript layer WITHOUT administrator privileges.
    #[cfg(target_os = "macos")]
    fn osascript_plain(shell_cmd: &str) -> String {
        let script = format!("do shell script \"{}\"", applescript_string(shell_cmd));
        let out = Command::new("osascript")
            .arg("-e")
            .arg(&script)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "osascript failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn osascript_roundtrip_without_admin() {
        for a in NASTY {
            let cmd = format!("printf %s {}", sh_quote(a));
            let got = osascript_plain(&cmd);
            // osascript appends a newline to the result and may turn \n into \r.
            let got = got.strip_suffix('\n').unwrap_or(&got).replace('\r', "\n");
            assert_eq!(&got, a, "arg {a:?}");
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn osascript_roundtrip_multiple_args_without_admin() {
        let cmd = format!(
            "printf '%s|' {} {} {}",
            sh_quote("a b"),
            sh_quote("it's \"q\" \\ $(x) `y`"),
            sh_quote("日本;語")
        );
        let got = osascript_plain(&cmd);
        assert_eq!(
            got.trim_end_matches('\n'),
            "a b|it's \"q\" \\ $(x) `y`|日本;語|"
        );
    }
}
