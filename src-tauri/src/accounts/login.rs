//! Who is signed in to an account folder, by Claude Code's own two ids in its `.claude.json`; nothing else is read.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::AccountId;

// Only the two ids, never the email, name or anything else of the signed-in account.
#[derive(Deserialize)]
struct SignedIn {
    #[serde(rename = "accountUuid")]
    account: Option<String>,
    #[serde(rename = "organizationUuid")]
    organization: Option<String>,
}

#[derive(Deserialize)]
struct Config {
    #[serde(rename = "oauthAccount")]
    signed_in: Option<SignedIn>,
}

/// The login and organization a folder is signed in to; `None` for a folder never signed in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Login {
    pub account: String,
    pub organization: Option<String>,
}

/// Read from the account's `.claude.json`; a missing or unreadable file is a folder never signed in.
pub fn read(config_file: &Path) -> Option<Login> {
    let config: Config = serde_json::from_slice(&fs::read(config_file).ok()?).ok()?;
    let signed_in = config.signed_in?;
    Some(Login {
        account: signed_in.account.filter(|a| !a.is_empty())?,
        organization: signed_in.organization.filter(|o| !o.is_empty()),
    })
}

/// What the Accounts screen and Move need to know about one account's login, with no id leaving the core.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginFacts {
    pub id: AccountId,
    /// The first account signed in to the same login, when this one is a second door to it.
    pub same_as: Option<AccountId>,
    /// Accounts with the same number are in one organization; `None` when unknown.
    pub organization: Option<u32>,
}

/// Each account's facts, in order, from its login (or `None` when never signed in).
pub fn facts(logins: &[(AccountId, Option<Login>)]) -> Vec<LoginFacts> {
    let mut organizations: Vec<&str> = Vec::new();
    logins
        .iter()
        .enumerate()
        .map(|(at, (id, login))| {
            let same_as = login.as_ref().and_then(|login| {
                logins[..at]
                    .iter()
                    .find(|(_, earlier)| {
                        earlier.as_ref().is_some_and(|e| e.account == login.account)
                    })
                    .map(|(earlier, _)| *earlier)
            });
            let organization = login
                .as_ref()
                .and_then(|l| l.organization.as_deref())
                .map(|org| {
                    let index = organizations
                        .iter()
                        .position(|o| *o == org)
                        .unwrap_or_else(|| {
                            organizations.push(org);
                            organizations.len() - 1
                        });
                    u32::try_from(index).unwrap_or(u32::MAX)
                });
            LoginFacts {
                id: *id,
                same_as,
                organization,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn login(account: &str, organization: &str) -> Option<Login> {
        Some(Login {
            account: account.to_owned(),
            organization: Some(organization.to_owned()),
        })
    }

    #[test]
    fn only_the_two_ids_are_read_and_a_folder_never_signed_in_has_none() {
        let dir = std::env::temp_dir().join(format!("cerberus-login-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join(".claude.json");
        fs::write(
            &file,
            r#"{"oauthAccount":{"accountUuid":"a1","organizationUuid":"o1","emailAddress":"x@example.com"}}"#,
        )
        .unwrap();
        let signed = read(&file);
        fs::write(&file, r#"{"numStartups":3}"#).unwrap();
        let never = read(&file);
        let missing = read(&dir.join("absent.json"));
        let _ = fs::remove_dir_all(&dir);

        assert_eq!(signed, login("a1", "o1"));
        assert_eq!(never, None);
        assert_eq!(missing, None);
    }

    #[test]
    fn a_second_folder_on_one_login_points_at_the_first_and_organizations_are_numbered() {
        let facts = facts(&[
            (0, login("a1", "o1")),
            (1, None),
            (2, login("a2", "o2")),
            (3, login("a1", "o1")),
        ]);

        let same: Vec<_> = facts.iter().map(|f| f.same_as).collect();
        let orgs: Vec<_> = facts.iter().map(|f| f.organization).collect();
        assert_eq!(same, [None, None, None, Some(0)]);
        assert_eq!(orgs, [Some(0), None, Some(1), Some(0)]);
    }
}
