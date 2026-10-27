mod detect;
mod status;
mod store;

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize, Serializer};

pub use crate::accounts::detect::Candidate;
pub use crate::accounts::status::AuthStatus;
use crate::accounts::store::Store;
use crate::programs::Finder;

pub type AccountId = u32;

const STATUS_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, thiserror::Error)]
pub enum AccountsError {
    #[error("the accounts file is not valid: {0}")]
    Corrupt(String),
    #[error("the accounts file is version {0}, which this app does not know")]
    UnknownVersion(u32),
    #[error("could not read or write the accounts file: {0}")]
    Io(#[from] io::Error),
    #[error("an account needs a name")]
    EmptyName,
    #[error("an account named {0} already exists")]
    DuplicateName(String),
    #[error("not a full path: {0}")]
    RelativeDir(String),
    #[error("another account already uses {0}")]
    DuplicateDir(String),
    #[error("folder not found: {0}")]
    NoSuchDir(String),
    #[error("no account {0}")]
    NoAccount(AccountId),
    #[error("unknown program {0}")]
    UnknownProgram(String),
}

impl Serialize for AccountsError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: AccountId,
    pub name: String,
    pub config_dir: PathBuf,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profiles {
    pub accounts: Vec<Account>,
    pub default_account: Option<AccountId>,
    pub default_program: Option<String>,
    pub default_cwd: Option<PathBuf>,
    // Ids are never reused, so something saved under a removed account cannot land on a new one.
    #[serde(skip)]
    next_id: AccountId,
}

#[derive(Debug, Deserialize)]
pub struct Defaults {
    pub account: Option<AccountId>,
    pub program: Option<String>,
    pub cwd: Option<PathBuf>,
}

impl Profiles {
    pub fn add(&mut self, name: &str, config_dir: PathBuf) -> Result<AccountId, AccountsError> {
        let name = self.checked_name(name, None)?;
        if !config_dir.is_absolute() {
            return Err(AccountsError::RelativeDir(shown(&config_dir)));
        }
        if self
            .accounts
            .iter()
            .any(|a| same_dir(&a.config_dir, &config_dir))
        {
            return Err(AccountsError::DuplicateDir(shown(&config_dir)));
        }
        let id = self.next_id;
        self.next_id += 1;
        self.accounts.push(Account {
            id,
            name,
            config_dir,
        });
        self.default_account.get_or_insert(id);
        Ok(id)
    }

    pub fn rename(&mut self, id: AccountId, name: &str) -> Result<(), AccountsError> {
        let name = self.checked_name(name, Some(id))?;
        let account = self
            .accounts
            .iter_mut()
            .find(|a| a.id == id)
            .ok_or(AccountsError::NoAccount(id))?;
        account.name = name;
        Ok(())
    }

    /// Forgets the account; its folder and everything in it stay on disk.
    pub fn remove(&mut self, id: AccountId) -> Result<(), AccountsError> {
        self.account(id)?;
        self.accounts.retain(|a| a.id != id);
        if self.default_account == Some(id) {
            self.default_account = self.accounts.first().map(|a| a.id);
        }
        Ok(())
    }

    pub fn set_defaults(&mut self, defaults: Defaults) -> Result<(), AccountsError> {
        if let Some(id) = defaults.account {
            self.account(id)?;
        }
        if let Some(cwd) = defaults.cwd.as_deref().filter(|cwd| !cwd.is_absolute()) {
            return Err(AccountsError::RelativeDir(shown(cwd)));
        }
        self.default_account = defaults.account;
        self.default_program = defaults.program;
        self.default_cwd = defaults.cwd;
        Ok(())
    }

    pub fn account(&self, id: AccountId) -> Result<&Account, AccountsError> {
        self.accounts
            .iter()
            .find(|a| a.id == id)
            .ok_or(AccountsError::NoAccount(id))
    }

    fn checked_name(
        &self,
        name: &str,
        renaming: Option<AccountId>,
    ) -> Result<String, AccountsError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(AccountsError::EmptyName);
        }
        let taken = self
            .accounts
            .iter()
            .any(|a| Some(a.id) != renaming && a.name.eq_ignore_ascii_case(name));
        if taken {
            return Err(AccountsError::DuplicateName(name.to_owned()));
        }
        Ok(name.to_owned())
    }

    /// Checks a file read from disk by replaying each account through the rules `add` enforces.
    fn validate(&self) -> Result<(), AccountsError> {
        let mut replay = Self::default();
        for account in &self.accounts {
            if account.id >= self.next_id || replay.account(account.id).is_ok() {
                return Err(AccountsError::Corrupt(format!(
                    "account id {} is invalid",
                    account.id
                )));
            }
            replay.next_id = account.id;
            replay.add(&account.name, account.config_dir.clone())?;
        }
        replay.set_defaults(Defaults {
            account: self.default_account,
            program: self.default_program.clone(),
            cwd: self.default_cwd.clone(),
        })
    }
}

pub struct Accounts {
    store: Store,
    home: PathBuf,
}

impl Accounts {
    pub fn new(app_config_dir: &Path, home: PathBuf) -> Self {
        Self {
            store: Store::new(app_config_dir.join("profiles.json")),
            home,
        }
    }

    pub fn load(&self) -> Result<Profiles, AccountsError> {
        self.store.load()
    }

    pub fn reset(&self) -> Result<Profiles, AccountsError> {
        self.store.reset()
    }

    pub fn update(
        &self,
        change: impl FnOnce(&mut Profiles) -> Result<(), AccountsError>,
    ) -> Result<Profiles, AccountsError> {
        self.store.update(change)
    }

    /// Creates the account's folder when it is new, so `claude` starts there on first login.
    pub fn add(&self, name: &str, config_dir: PathBuf) -> Result<Profiles, AccountsError> {
        self.update(|p| {
            p.add(name, config_dir.clone())?;
            fs::create_dir_all(&config_dir)?;
            Ok(())
        })
    }

    pub fn set_defaults(&self, defaults: Defaults) -> Result<Profiles, AccountsError> {
        if let Some(program) = &defaults.program {
            let known = Finder::from_env().choices().iter().any(|c| c.id == program);
            if !known {
                return Err(AccountsError::UnknownProgram(program.clone()));
            }
        }
        if let Some(cwd) = defaults.cwd.as_deref().filter(|cwd| !cwd.is_dir()) {
            return Err(AccountsError::NoSuchDir(shown(cwd)));
        }
        self.update(|p| p.set_defaults(defaults))
    }

    /// Config dirs found in the home folder that no saved account uses yet.
    pub fn candidates(&self) -> Result<Vec<Candidate>, AccountsError> {
        let profiles = self.load()?;
        let mut found = detect::candidates(&self.home);
        found.retain(|c| {
            !profiles
                .accounts
                .iter()
                .any(|a| same_dir(&a.config_dir, &c.config_dir))
        });
        Ok(found)
    }

    pub fn suggested_dir(&self, name: &str) -> PathBuf {
        let slug: String = name
            .trim()
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .collect();
        let slug = slug.trim_matches('-');
        self.home.join(format!(
            ".claude-{}",
            if slug.is_empty() { "new" } else { slug }
        ))
    }

    /// The `CLAUDE_CONFIG_DIR` a tab under `account` needs, or `None` for the default folder.
    pub fn claude_config_dir(
        &self,
        account: Option<AccountId>,
    ) -> Result<Option<PathBuf>, AccountsError> {
        let Some(id) = account else {
            return Ok(None);
        };
        let dir = self.load()?.account(id)?.config_dir.clone();
        Ok(env_dir(&self.home, dir))
    }

    pub fn status(&self, id: AccountId) -> Result<AuthStatus, AccountsError> {
        let config_dir = self.claude_config_dir(Some(id))?;
        let claude = Finder::from_env().find("claude");
        Ok(status::read(
            claude.as_deref(),
            config_dir.as_deref(),
            STATUS_TIMEOUT,
        ))
    }
}

// Claude Code reads `~/.claude` when the variable is unset, so the default account leaves it unset.
fn env_dir(home: &Path, dir: PathBuf) -> Option<PathBuf> {
    (!same_dir(&dir, &home.join(".claude"))).then_some(dir)
}

// Windows paths ignore case and accept either slash.
fn same_dir(a: &Path, b: &Path) -> bool {
    let key = |p: &Path| {
        p.to_string_lossy()
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_lowercase()
    };
    key(a) == key(b)
}

fn shown(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
pub(crate) mod testdir {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::{fs, process};

    pub struct TestDir(PathBuf);

    impl TestDir {
        pub fn new() -> Self {
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let n = NEXT.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!("cerberus-test-{}-{n}", process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        pub fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testdir::TestDir;
    use super::*;

    fn dir(path: &str) -> PathBuf {
        PathBuf::from(path)
    }

    fn two_accounts() -> Profiles {
        let mut profiles = Profiles::default();
        profiles.add("personal", dir(r"C:\h\.claude")).unwrap();
        profiles.add("work", dir(r"C:\h\.claude-work")).unwrap();
        profiles
    }

    #[test]
    fn the_first_account_added_becomes_the_default() {
        let profiles = two_accounts();

        assert_eq!(profiles.default_account, Some(0));
        assert_eq!(profiles.accounts[1].name, "work");
    }

    #[test]
    fn names_are_trimmed_and_must_not_be_empty() {
        let mut profiles = Profiles::default();

        assert!(matches!(
            profiles.add("  ", dir(r"C:\a")),
            Err(AccountsError::EmptyName)
        ));
        profiles.add("  team ", dir(r"C:\a")).unwrap();
        assert_eq!(profiles.accounts[0].name, "team");
    }

    #[test]
    fn names_are_unique_ignoring_case() {
        let mut profiles = two_accounts();

        assert!(matches!(
            profiles.add("Work", dir(r"C:\other")),
            Err(AccountsError::DuplicateName(_))
        ));
        assert!(matches!(
            profiles.rename(0, "WORK"),
            Err(AccountsError::DuplicateName(_))
        ));
        profiles.rename(1, "Work").unwrap();
        assert_eq!(profiles.accounts[1].name, "Work");
    }

    #[test]
    fn a_folder_must_be_absolute_and_unused() {
        let mut profiles = two_accounts();

        assert!(matches!(
            profiles.add("rel", dir(r"h\.claude-rel")),
            Err(AccountsError::RelativeDir(_))
        ));
        assert!(matches!(
            profiles.add("again", dir("c:/H/.CLAUDE-work/")),
            Err(AccountsError::DuplicateDir(_))
        ));
    }

    #[test]
    fn removing_the_default_moves_the_default_to_the_next_account() {
        let mut profiles = two_accounts();

        profiles.remove(0).unwrap();

        assert_eq!(profiles.default_account, Some(1));
        assert!(matches!(
            profiles.remove(0),
            Err(AccountsError::NoAccount(0))
        ));
    }

    #[test]
    fn ids_are_not_reused_after_a_remove() {
        let mut profiles = two_accounts();

        profiles.remove(1).unwrap();
        let id = profiles.add("new", dir(r"C:\h\.claude-new")).unwrap();

        assert_eq!(id, 2);
    }

    #[test]
    fn defaults_must_name_a_saved_account_and_a_full_path() {
        let mut profiles = two_accounts();
        let defaults = |account, cwd: &str| Defaults {
            account: Some(account),
            program: Some("claude".to_owned()),
            cwd: Some(dir(cwd)),
        };

        assert!(matches!(
            profiles.set_defaults(defaults(9, r"C:\code")),
            Err(AccountsError::NoAccount(9))
        ));
        assert!(matches!(
            profiles.set_defaults(defaults(1, "code")),
            Err(AccountsError::RelativeDir(_))
        ));
        profiles.set_defaults(defaults(1, r"C:\code")).unwrap();
        assert_eq!(profiles.default_account, Some(1));
        assert_eq!(profiles.default_cwd, Some(dir(r"C:\code")));
    }

    #[test]
    fn only_a_folder_other_than_the_default_sets_the_variable() {
        let home = dir(r"C:\Users\u");

        assert_eq!(env_dir(&home, dir(r"C:\Users\u\.claude")), None);
        assert_eq!(env_dir(&home, dir("c:/users/U/.Claude/")), None);
        assert_eq!(
            env_dir(&home, dir(r"C:\Users\u\.claude-acc2")),
            Some(dir(r"C:\Users\u\.claude-acc2"))
        );
    }

    #[test]
    fn a_tab_under_an_account_gets_its_config_dir() {
        let home = TestDir::new();
        let accounts = Accounts::new(&home.path().join("app"), home.path().to_path_buf());
        accounts.add("main", home.path().join(".claude")).unwrap();
        accounts
            .add("second", home.path().join(".claude-acc2"))
            .unwrap();

        assert_eq!(accounts.claude_config_dir(None).unwrap(), None);
        assert_eq!(accounts.claude_config_dir(Some(0)).unwrap(), None);
        assert_eq!(
            accounts.claude_config_dir(Some(1)).unwrap(),
            Some(home.path().join(".claude-acc2"))
        );
        assert!(matches!(
            accounts.claude_config_dir(Some(7)),
            Err(AccountsError::NoAccount(7))
        ));
    }

    #[test]
    fn adding_an_account_creates_its_folder() {
        let home = TestDir::new();
        let accounts = Accounts::new(&home.path().join("app"), home.path().to_path_buf());

        accounts
            .add("fresh", home.path().join(".claude-fresh"))
            .unwrap();

        assert!(home.path().join(".claude-fresh").is_dir());
    }

    #[test]
    fn a_folder_that_cannot_be_created_saves_no_account() {
        let home = TestDir::new();
        let accounts = Accounts::new(&home.path().join("app"), home.path().to_path_buf());
        let file = home.path().join("taken");
        fs::write(&file, "").unwrap();

        let result = accounts.add("blocked", file.join("inside"));

        assert!(matches!(result, Err(AccountsError::Io(_))));
        assert!(accounts.load().unwrap().accounts.is_empty());
    }

    #[test]
    fn saved_folders_are_not_offered_again() {
        let home = TestDir::new();
        fs::create_dir_all(home.path().join(".claude-acc2").join("projects")).unwrap();
        fs::create_dir_all(home.path().join(".claude-acc3").join("projects")).unwrap();
        let accounts = Accounts::new(&home.path().join("app"), home.path().to_path_buf());
        accounts
            .add("two", home.path().join(".claude-acc2"))
            .unwrap();

        let names: Vec<_> = accounts
            .candidates()
            .unwrap()
            .into_iter()
            .map(|c| c.name)
            .collect();

        assert_eq!(names, ["acc3"]);
    }

    #[test]
    fn a_suggested_folder_is_named_after_the_account() {
        let accounts = Accounts::new(&dir(r"C:\app"), dir(r"C:\Users\u"));

        assert_eq!(
            accounts.suggested_dir(" ISL Team "),
            dir(r"C:\Users\u\.claude-isl-team")
        );
        assert_eq!(accounts.suggested_dir("!!"), dir(r"C:\Users\u\.claude-new"));
    }
}
