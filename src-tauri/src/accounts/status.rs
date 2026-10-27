use std::io::{self, Read};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AuthState {
    LoggedIn,
    LoggedOut,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthStatus {
    pub state: AuthState,
    pub summary: String,
}

// Only the fields shown are parsed, so renamed or new keys elsewhere change nothing.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Reported {
    logged_in: bool,
    email: Option<String>,
    org_name: Option<String>,
    subscription_type: Option<String>,
}

#[derive(Debug, thiserror::Error)]
enum RunError {
    #[error("could not start Claude Code: {0}")]
    Start(io::Error),
    #[error("could not read Claude Code's answer: {0}")]
    Read(io::Error),
    #[error("Claude Code did not answer in {} s", .0.as_secs())]
    TimedOut(Duration),
}

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Runs `claude auth status` under `config_dir`; a missing or failing `claude` reads as unknown.
pub fn read(claude: Option<&Path>, config_dir: Option<&Path>, timeout: Duration) -> AuthStatus {
    let Some(claude) = claude else {
        return unknown("Claude Code is not installed");
    };
    let mut command = Command::new(claude);
    command.args(["auth", "status", "--json"]);
    match config_dir {
        Some(dir) => command.env("CLAUDE_CONFIG_DIR", dir),
        None => command.env_remove("CLAUDE_CONFIG_DIR"),
    };
    match run(command, timeout) {
        Ok(stdout) => parse(&stdout),
        Err(e) => unknown(&e.to_string()),
    }
}

// The exit code is ignored: a logged-out account exits 1 and still prints its status.
fn run(mut command: Command, timeout: Duration) -> Result<Vec<u8>, RunError> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(&mut command, CREATE_NO_WINDOW);
    let mut child = command.spawn().map_err(RunError::Start)?;
    let Some(mut stdout) = child.stdout.take() else {
        return Err(RunError::Read(io::Error::other("no output pipe")));
    };
    let (tx, rx) = mpsc::channel();
    thread::Builder::new()
        .spawn(move || {
            let mut out = Vec::new();
            let _ = tx.send(stdout.read_to_end(&mut out).map(|_| out));
        })
        .map_err(RunError::Read)?;
    let result = match rx.recv_timeout(timeout) {
        Ok(read) => read.map_err(RunError::Read),
        Err(_) => Err(RunError::TimedOut(timeout)),
    };
    if result.is_err() {
        kill_tree(&mut child);
    }
    let _ = child.wait();
    result
}

// A `.cmd` shim's child is cmd.exe, so killing only that would leave Claude Code running.
fn kill_tree(child: &mut Child) {
    #[cfg(windows)]
    {
        let mut taskkill = Command::new("taskkill");
        taskkill
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        std::os::windows::process::CommandExt::creation_flags(&mut taskkill, CREATE_NO_WINDOW);
        if taskkill.status().is_ok_and(|status| status.success()) {
            return;
        }
    }
    let _ = child.kill();
}

/// Never echoes the output on failure, since it may hold the account's email.
fn parse(stdout: &[u8]) -> AuthStatus {
    let Ok(reported) = serde_json::from_slice::<Reported>(stdout) else {
        return unknown("Claude Code gave a status this app cannot read");
    };
    if !reported.logged_in {
        return AuthStatus {
            state: AuthState::LoggedOut,
            summary: "not logged in".to_owned(),
        };
    }
    let plan = reported.subscription_type.as_deref().map(capitalized);
    // A personal account's org is named after its email, which would say the same thing twice.
    let org = reported.org_name.filter(|org| {
        reported
            .email
            .as_deref()
            .is_none_or(|email| !org.contains(email))
    });
    let parts: Vec<String> = [reported.email, plan, org].into_iter().flatten().collect();
    AuthStatus {
        state: AuthState::LoggedIn,
        summary: if parts.is_empty() {
            "logged in".to_owned()
        } else {
            parts.join(" · ")
        },
    }
}

fn capitalized(word: &str) -> String {
    let mut chars = word.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

fn unknown(reason: &str) -> AuthStatus {
    AuthStatus {
        state: AuthState::Unknown,
        summary: format!("status unknown: {reason}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EMAIL: &str = "user@example.com";

    #[test]
    fn a_logged_in_account_shows_email_plan_and_org() {
        let status = parse(
            br#"{"loggedIn":true,"authMethod":"claude.ai","email":"user@example.com",
                "orgId":"x","orgName":"Example Team","subscriptionType":"team","extra":[1]}"#,
        );

        assert_eq!(status.state, AuthState::LoggedIn);
        assert_eq!(status.summary, "user@example.com · Team · Example Team");
    }

    #[test]
    fn a_personal_org_named_after_the_email_is_not_repeated() {
        let status = parse(
            br#"{"loggedIn":true,"email":"user@example.com",
                "orgName":"user@example.com's Organization","subscriptionType":"pro"}"#,
        );

        assert_eq!(status.summary, "user@example.com · Pro");
    }

    #[test]
    fn missing_fields_show_as_unknown_parts_not_errors() {
        let status = parse(br#"{"loggedIn":true}"#);

        assert_eq!(status.state, AuthState::LoggedIn);
        assert_eq!(status.summary, "logged in");
    }

    #[test]
    fn a_logged_out_account_says_so() {
        let status = parse(br#"{"loggedIn":false,"authMethod":"none"}"#);

        assert_eq!(status.state, AuthState::LoggedOut);
        assert_eq!(status.summary, "not logged in");
    }

    #[test]
    fn garbage_output_is_unknown_and_never_echoed() {
        let status = parse(format!("Error: token for {EMAIL} expired").as_bytes());

        assert_eq!(status.state, AuthState::Unknown);
        assert!(!status.summary.contains(EMAIL), "{}", status.summary);
    }

    #[test]
    fn a_missing_claude_is_unknown() {
        let status = read(None, None, Duration::from_secs(1));

        assert_eq!(status.state, AuthState::Unknown);
        assert_eq!(
            status.summary,
            "status unknown: Claude Code is not installed"
        );
    }

    #[test]
    fn a_program_that_cannot_start_is_unknown() {
        let status = read(
            Some(Path::new(r"C:\no\such\claude.exe")),
            None,
            Duration::from_secs(1),
        );

        assert_eq!(status.state, AuthState::Unknown);
        assert!(
            status.summary.contains("could not start"),
            "{}",
            status.summary
        );
    }

    #[cfg(windows)]
    #[test]
    fn output_is_read_whatever_the_exit_code() {
        let mut command = Command::new("cmd");
        // Raw, because Rust's quoting for an argument with quotes is not what cmd's echo expects.
        std::os::windows::process::CommandExt::raw_arg(
            &mut command,
            r#"/c echo {"loggedIn":false}& exit 1"#,
        );

        let stdout = run(command, Duration::from_secs(20)).unwrap();

        assert_eq!(parse(&stdout).state, AuthState::LoggedOut);
    }

    #[cfg(windows)]
    #[test]
    fn a_hung_program_times_out() {
        let mut command = Command::new("powershell");
        command.args(["-NoProfile", "-Command", "Start-Sleep -Seconds 30"]);

        let result = run(command, Duration::from_millis(300));

        assert!(matches!(result, Err(RunError::TimedOut(_))));
    }

    #[cfg(windows)]
    #[test]
    fn a_timeout_ends_what_a_wrapper_started() {
        let dir = crate::accounts::testdir::TestDir::new();
        let pid_file = dir.path().join("pid.txt");
        let mut command = Command::new("cmd");
        command.args([
            "/c",
            "powershell",
            "-NoProfile",
            "-Command",
            &format!(
                "$PID | Set-Content '{}'; Start-Sleep -Seconds 30",
                pid_file.display()
            ),
        ]);

        let result = run(command, Duration::from_secs(5));

        assert!(matches!(result, Err(RunError::TimedOut(_))));
        let pid = std::fs::read_to_string(&pid_file).unwrap();
        let listed = Command::new("tasklist")
            .args(["/FI", &format!("PID eq {}", pid.trim()), "/NH"])
            .output()
            .unwrap();
        let listed = String::from_utf8_lossy(&listed.stdout);
        assert!(!listed.contains(pid.trim()), "still running: {listed}");
    }
}
