use std::path::Path;
use std::process::Command;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::run::{RunError, run};

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
    /// Shown on its own in the right panel, and `None` when signed out or not reported.
    pub email: Option<String>,
    pub plan: Option<String>,
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
    // The exit code is ignored because a logged-out account exits 1 and still prints its status.
    match run(command, timeout) {
        Ok(ran) => parse(&ran.stdout),
        Err(RunError::Start(e)) => unknown(&format!("could not start Claude Code: {e}")),
        Err(RunError::Read(e)) => unknown(&format!("could not read Claude Code's answer: {e}")),
        Err(RunError::TimedOut(t)) => {
            unknown(&format!("Claude Code did not answer in {} s", t.as_secs()))
        }
    }
}

/// Never echoes the output on failure, since it may hold the account's email.
fn parse(stdout: &[u8]) -> AuthStatus {
    let Ok(reported) = serde_json::from_slice::<Reported>(stdout) else {
        return unknown("Claude Code gave a status this app cannot read");
    };
    if !reported.logged_in {
        return AuthStatus {
            state: AuthState::LoggedOut,
            summary: "signed out".to_owned(),
            email: None,
            plan: None,
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
    let parts: Vec<String> = [reported.email.clone(), plan.clone(), org]
        .into_iter()
        .flatten()
        .collect();
    AuthStatus {
        state: AuthState::LoggedIn,
        summary: if parts.is_empty() {
            "signed in".to_owned()
        } else {
            parts.join(" · ")
        },
        email: reported.email,
        plan,
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
        email: None,
        plan: None,
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
        assert_eq!(status.email.as_deref(), Some("user@example.com"));
        assert_eq!(status.plan.as_deref(), Some("Team"));
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
        assert_eq!(status.summary, "signed in");
    }

    #[test]
    fn a_logged_out_account_says_so() {
        let status = parse(br#"{"loggedIn":false,"authMethod":"none"}"#);

        assert_eq!(status.state, AuthState::LoggedOut);
        assert_eq!(status.summary, "signed out");
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
}
