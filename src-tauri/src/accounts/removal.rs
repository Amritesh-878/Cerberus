use serde::Serialize;

use crate::accounts::{AccountId, Accounts, AccountsError, Profiles};

/// What a remove did: asked first because saved tabs use the account, or removed it.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Removal {
    /// Nothing changed; these tabs, as "project / tab", run on the account.
    InUse {
        tabs: Vec<String>,
    },
    Removed {
        profiles: Profiles,
    },
}

impl Accounts {
    /// Refuses the last account before asking anything, so the user is never asked and then refused.
    pub fn remove(
        &self,
        id: AccountId,
        confirmed: bool,
        tabs_on: impl FnOnce(AccountId) -> Vec<String>,
    ) -> Result<Removal, AccountsError> {
        let profiles = self.load()?;
        let account = profiles.account(id)?;
        if profiles.accounts.len() == 1 {
            return Err(AccountsError::LastAccount(account.name.clone()));
        }
        if !confirmed {
            let tabs = tabs_on(id);
            if !tabs.is_empty() {
                return Ok(Removal::InUse { tabs });
            }
        }
        let profiles = self.update(|p| p.remove(id))?;
        Ok(Removal::Removed { profiles })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::testdir::TestDir;

    fn two(dir: &TestDir) -> Accounts {
        let accounts = Accounts::new(&dir.path().join("app"), dir.path().to_path_buf());
        accounts
            .add("work", dir.path().join(".claude-work"))
            .unwrap();
        accounts
            .add("personal", dir.path().join(".claude-personal"))
            .unwrap();
        accounts
    }

    fn planner(_: AccountId) -> Vec<String> {
        vec!["app / planner".to_owned()]
    }

    #[test]
    fn an_account_in_use_is_reported_and_kept_until_confirmed() {
        let dir = TestDir::new();
        let accounts = two(&dir);

        let asked = accounts.remove(0, false, planner).unwrap();

        assert!(
            matches!(&asked, Removal::InUse { tabs } if tabs == &["app / planner"]),
            "{asked:?}"
        );
        assert_eq!(accounts.load().unwrap().accounts.len(), 2);
    }

    #[test]
    fn a_confirmed_remove_forgets_the_account_but_keeps_its_folder_for_chats() {
        let dir = TestDir::new();
        let accounts = two(&dir);

        let done = accounts.remove(0, true, planner).unwrap();

        let Removal::Removed { profiles } = done else {
            panic!("not removed");
        };
        assert_eq!(profiles.accounts.len(), 1);
        assert_eq!(profiles.default_account, Some(1));
        assert!(dir.path().join(".claude-work").is_dir());
        assert_eq!(
            accounts.chat_home(Some(0)).unwrap(),
            dir.path().join(".claude-work")
        );
        assert!(matches!(
            accounts.claude_config_dir(Some(0)),
            Err(AccountsError::NoAccount(0))
        ));
    }

    #[test]
    fn an_account_no_tab_uses_is_removed_without_asking() {
        let dir = TestDir::new();
        let accounts = two(&dir);

        let done = accounts.remove(1, false, |_| Vec::new()).unwrap();

        assert!(matches!(done, Removal::Removed { .. }));
    }

    #[test]
    fn the_last_account_is_refused_even_when_tabs_use_it() {
        let dir = TestDir::new();
        let accounts = two(&dir);
        accounts.remove(1, true, planner).unwrap();

        let refused = accounts.remove(0, true, planner);

        assert!(
            matches!(&refused, Err(AccountsError::LastAccount(name)) if name == "work"),
            "{refused:?}"
        );
        assert_eq!(accounts.load().unwrap().accounts.len(), 1);
    }

    #[test]
    fn a_removed_account_survives_a_reload_and_its_id_stays_taken() {
        let dir = TestDir::new();
        let accounts = two(&dir);
        accounts.remove(0, true, planner).unwrap();

        let fresh = Accounts::new(&dir.path().join("app"), dir.path().to_path_buf());
        let profiles = fresh.add("third", dir.path().join(".claude-3")).unwrap();

        assert_eq!(profiles.retired.len(), 1);
        assert_eq!(profiles.accounts.last().map(|a| a.id), Some(2));
    }
}
