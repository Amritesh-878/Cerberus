mod detect;
mod palette;
mod removal;
mod status;
mod store;

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize, Serializer};

pub use crate::accounts::detect::Candidate;
pub use crate::accounts::removal::Removal;
pub use crate::accounts::status::{AuthState, AuthStatus};
use crate::accounts::store::Store;
use crate::programs::Finder;

pub type AccountId = u32;

const SIDEBAR_WIDTHS: std::ops::RangeInclusive<u32> = 200..=560;
const PANEL_WIDTHS: std::ops::RangeInclusive<u32> = 240..=640;
/// The left chat's share of a split, in thousandths, so neither side can be dragged shut.
const SPLIT_SHARES: std::ops::RangeInclusive<u32> = 200..=800;
/// The right panel's cards that can be collapsed; a name not here is dropped when saved.
const PANEL_CARDS: [&str; 4] = ["environment", "changes", "connectors", "skills"];
/// Cards that start folded, so for these the saved list is the ones the user opened.
const PANEL_FOLDED_AT_FIRST: [&str; 1] = ["connectors"];
const MAX_FAVOURITES: usize = 200;
const MAX_SKILL_NAME: usize = 200;

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
    #[error(
        "{0} is the only account, and Cerberus needs one. Add another account before removing it."
    )]
    LastAccount(String),
    #[error("not a shell Cerberus knows: {0}")]
    UnknownShell(String),
    #[error("not a colour Cerberus knows: {0}")]
    UnknownColor(String),
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
    /// A palette name; `None` keeps the colour the account's id gave it before colours were saved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

/// The right panel: whether it shows, its width, and the cards folded to their title.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PanelPrefs {
    pub open: bool,
    /// In pixels; `None` keeps the app's default.
    pub width: Option<u32>,
    pub collapsed: Vec<String>,
    /// Cards folded at first that the user opened.
    pub expanded: Vec<String>,
    /// Skill names starred per account id, kept as text since JSON keys are strings.
    pub favourites: BTreeMap<String, Vec<String>>,
}

/// A walkthrough step, saved so a restart part way through setup reopens it there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SetupStep {
    Install,
    Accounts,
    Defaults,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profiles {
    pub accounts: Vec<Account>,
    pub default_account: Option<AccountId>,
    /// The program id the Shell button opens; `None` opens the first shell installed.
    pub shell: Option<String>,
    pub start_folder: Option<PathBuf>,
    /// In pixels; `None` keeps the app's default.
    #[serde(default)]
    pub sidebar_width: Option<u32>,
    #[serde(default)]
    pub panel: PanelPrefs,
    /// The left chat's share of a split in thousandths; `None` keeps half and half.
    #[serde(default)]
    pub split_share: Option<u32>,
    /// Desktop notifications when a chat in another tab needs you.
    #[serde(default = "on")]
    pub alerts: bool,
    /// Shell tabs suggest from history as you type, through each shell's own feature.
    #[serde(default = "on")]
    pub suggestions: bool,
    /// Claude tabs can message tabs on other accounts, each message approved first.
    #[serde(default = "on")]
    pub messages: bool,
    /// Where setup is, until it is finished or skipped; a file from before the setting has finished it.
    #[serde(default)]
    pub setup: Option<SetupStep>,
    /// Removed accounts, kept so a tab left on one can still carry its chat to another account.
    #[serde(default)]
    pub retired: Vec<Account>,
    // Raised on every save, so the webview can drop a reply that a newer one already replaced.
    #[serde(default)]
    pub revision: u64,
    // Ids are never reused, so something saved under a removed account cannot land on a new one.
    #[serde(skip)]
    next_id: AccountId,
}

// Written out because alerts, suggestions and messages start on, on a first run as in a file saved before the setting existed.
impl Default for Profiles {
    fn default() -> Self {
        Self {
            accounts: Vec::new(),
            default_account: None,
            shell: None,
            start_folder: None,
            sidebar_width: None,
            panel: PanelPrefs::default(),
            split_share: None,
            alerts: true,
            suggestions: true,
            messages: true,
            setup: Some(SetupStep::Install),
            retired: Vec::new(),
            revision: 0,
            next_id: 0,
        }
    }
}

fn on() -> bool {
    true
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
        let color = palette::first_free(&self.accounts);
        self.accounts.push(Account {
            id,
            name,
            config_dir,
            color,
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
        let account = self.account(id)?.clone();
        if self.accounts.len() == 1 {
            return Err(AccountsError::LastAccount(account.name));
        }
        self.accounts.retain(|a| a.id != id);
        self.retired.push(account);
        if self.default_account == Some(id) {
            self.default_account = self.accounts.first().map(|a| a.id);
        }
        Ok(())
    }

    pub fn set_color(&mut self, id: AccountId, color: &str) -> Result<(), AccountsError> {
        if !palette::is_known(color) {
            return Err(AccountsError::UnknownColor(color.to_owned()));
        }
        let account = self
            .accounts
            .iter_mut()
            .find(|a| a.id == id)
            .ok_or(AccountsError::NoAccount(id))?;
        account.color = Some(color.to_owned());
        Ok(())
    }

    /// A colour from a newer build's palette is dropped, so the account shows its old colour, not an error.
    fn forget_unknown_colors(&mut self) {
        for account in self.accounts.iter_mut().chain(self.retired.iter_mut()) {
            if account
                .color
                .as_deref()
                .is_some_and(|c| !palette::is_known(c))
            {
                account.color = None;
            }
        }
    }

    pub fn set_default_account(&mut self, id: AccountId) -> Result<(), AccountsError> {
        self.account(id)?;
        self.default_account = Some(id);
        Ok(())
    }

    /// `None` starts tabs in the home folder.
    pub fn set_start_folder(&mut self, folder: Option<PathBuf>) -> Result<(), AccountsError> {
        if let Some(folder) = folder.as_deref().filter(|f| !f.is_absolute()) {
            return Err(AccountsError::RelativeDir(shown(folder)));
        }
        self.start_folder = folder;
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
        if let Some(id) = self.default_account {
            replay.set_default_account(id)?;
        }
        for account in &self.retired {
            if account.id >= self.next_id || replay.account(account.id).is_ok() {
                return Err(AccountsError::Corrupt(format!(
                    "removed account id {} is invalid",
                    account.id
                )));
            }
        }
        replay.set_start_folder(self.start_folder.clone())
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
            // A missing folder compares by its text, so a link in its path is only seen once it exists.
            p.validate()
        })
    }

    pub fn set_sidebar_width(&self, width: u32) -> Result<Profiles, AccountsError> {
        let width = width.clamp(*SIDEBAR_WIDTHS.start(), *SIDEBAR_WIDTHS.end());
        self.update(|p| {
            p.sidebar_width = Some(width);
            Ok(())
        })
    }

    pub fn set_split_share(&self, share: u32) -> Result<Profiles, AccountsError> {
        let share = share.clamp(*SPLIT_SHARES.start(), *SPLIT_SHARES.end());
        self.update(|p| {
            p.split_share = Some(share);
            Ok(())
        })
    }

    pub fn set_panel(&self, prefs: PanelPrefs) -> Result<Profiles, AccountsError> {
        let width = prefs
            .width
            .map(|w| w.clamp(*PANEL_WIDTHS.start(), *PANEL_WIDTHS.end()));
        let known = |cards: &[String], folded_at_first: bool| -> Vec<String> {
            PANEL_CARDS
                .into_iter()
                .filter(|card| PANEL_FOLDED_AT_FIRST.contains(card) == folded_at_first)
                .filter(|card| cards.iter().any(|c| c == card))
                .map(str::to_owned)
                .collect()
        };
        let collapsed = known(&prefs.collapsed, false);
        let expanded = known(&prefs.expanded, true);
        self.update(|p| {
            let favourites = prefs
                .favourites
                .into_iter()
                // Keyed by the canonical id, since the panel looks favourites up by String(account).
                .filter_map(|(id, names)| {
                    let id: AccountId = id.parse().ok()?;
                    p.account(id).ok()?;
                    Some((id.to_string(), skill_names(names)))
                })
                .filter(|(_, names)| !names.is_empty())
                .collect();
            p.panel = PanelPrefs {
                open: prefs.open,
                width,
                collapsed,
                expanded,
                favourites,
            };
            Ok(())
        })
    }

    /// `None` ends setup, by finishing or skipping it.
    pub fn set_setup(&self, step: Option<SetupStep>) -> Result<Profiles, AccountsError> {
        self.update(|p| {
            p.setup = step;
            Ok(())
        })
    }

    pub fn set_alerts(&self, on: bool) -> Result<Profiles, AccountsError> {
        self.update(|p| {
            p.alerts = on;
            Ok(())
        })
    }

    pub fn set_suggestions(&self, on: bool) -> Result<Profiles, AccountsError> {
        self.update(|p| {
            p.suggestions = on;
            Ok(())
        })
    }

    pub fn set_messages(&self, on: bool) -> Result<Profiles, AccountsError> {
        self.update(|p| {
            p.messages = on;
            Ok(())
        })
    }

    pub fn set_shell(&self, shell: String) -> Result<Profiles, AccountsError> {
        if !Finder::from_env()
            .choices()
            .iter()
            .any(|c| c.id == shell && c.is_shell)
        {
            return Err(AccountsError::UnknownShell(shell));
        }
        self.update(|p| {
            p.shell = Some(shell);
            Ok(())
        })
    }

    pub fn set_start_folder(&self, folder: Option<PathBuf>) -> Result<Profiles, AccountsError> {
        if let Some(folder) = folder.as_deref().filter(|f| !f.is_dir()) {
            return Err(AccountsError::NoSuchDir(shown(folder)));
        }
        self.update(|p| p.set_start_folder(folder))
    }

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

    /// Where a tab's chat lives, a removed account's folder included, so the chat can still move.
    pub fn chat_home(&self, account: Option<AccountId>) -> Result<PathBuf, AccountsError> {
        let Some(id) = account else {
            return self.claude_home(None);
        };
        let profiles = self.load()?;
        match profiles.account(id) {
            Ok(account) => Ok(account.config_dir.clone()),
            Err(e) => profiles
                .retired
                .iter()
                .find(|a| a.id == id)
                .map(|a| a.config_dir.clone())
                .ok_or(e),
        }
    }

    /// The folder Claude Code uses for this account, where `sessions/` and `projects/` live.
    pub fn claude_home(&self, account: Option<AccountId>) -> Result<PathBuf, AccountsError> {
        Ok(self
            .claude_config_dir(account)?
            .unwrap_or_else(|| self.home.join(".claude")))
    }

    pub fn home(&self) -> &Path {
        &self.home
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

// Names typed into a chat, so anything that is not one plain line is dropped.
fn skill_names(names: Vec<String>) -> Vec<String> {
    let mut kept: Vec<String> = Vec::new();
    for name in names {
        let plain = !name.trim().is_empty()
            && name.chars().count() <= MAX_SKILL_NAME
            && !name.chars().any(char::is_control);
        if plain && !kept.contains(&name) && kept.len() < MAX_FAVOURITES {
            kept.push(name);
        }
    }
    kept
}

// Claude Code reads `~/.claude` when the variable is unset, so the default account leaves it unset.
fn env_dir(home: &Path, dir: PathBuf) -> Option<PathBuf> {
    (!same_dir(&dir, &home.join(".claude"))).then_some(dir)
}

fn same_dir(a: &Path, b: &Path) -> bool {
    dir_key(a) == dir_key(b)
}

// Windows paths ignore case, accept either slash, and reach one folder through `..` or a short name.
#[cfg(windows)]
fn dir_key(path: &Path) -> String {
    resolved(path)
        .to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase()
}

// Kept as spelled, since Claude Code uses the folder exactly as given; links and `..` still resolve.
#[cfg(not(windows))]
fn dir_key(path: &Path) -> String {
    resolved(path)
        .to_string_lossy()
        .trim_end_matches('/')
        .to_owned()
}

// A folder not made yet resolves through the part that exists, so a link and `..` before it land where they lead.
fn resolved(path: &Path) -> PathBuf {
    path.ancestors()
        .find_map(|known| {
            let base = fs::canonicalize(known).ok()?;
            let rest = path.strip_prefix(known).ok()?;
            Some(lexical(&plain(base).join(rest)))
        })
        .unwrap_or_else(|| lexical(path))
}

#[cfg(not(windows))]
fn plain(path: PathBuf) -> PathBuf {
    path
}

fn lexical(path: &Path) -> PathBuf {
    let mut resolved = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                resolved.pop();
            }
            other => resolved.push(other),
        }
    }
    resolved
}

// `canonicalize` returns `\\?\C:\...`, which never equals a path the user typed.
#[cfg(windows)]
fn plain(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    if let Some(share) = text.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{share}"))
    } else if let Some(local) = text.strip_prefix(r"\\?\") {
        PathBuf::from(local)
    } else {
        path
    }
}

fn shown(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
pub(crate) mod testdir {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::{fs, process};

    /// A fixture path written the Windows way, as this platform spells it: `C:\h\x` is `/h/x` on Unix.
    pub fn native(path: &str) -> PathBuf {
        #[cfg(windows)]
        return PathBuf::from(path);
        #[cfg(not(windows))]
        PathBuf::from(path.trim_start_matches("C:").replace('\\', "/"))
    }

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
        super::testdir::native(path)
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
        #[cfg(windows)]
        assert!(matches!(
            profiles.add("again", dir("c:/H/.CLAUDE-work/")),
            Err(AccountsError::DuplicateDir(_))
        ));
        #[cfg(not(windows))]
        assert!(matches!(
            profiles.add("again", dir("/h/.claude-work/")),
            Err(AccountsError::DuplicateDir(_))
        ));
    }

    #[cfg(not(windows))]
    #[test]
    fn a_folder_spelled_in_another_case_is_another_folder() {
        let mut profiles = two_accounts();

        profiles.add("other", dir("/h/.CLAUDE-work")).unwrap();

        assert_eq!(
            env_dir(&dir("/Users/u"), dir("/Users/u/.Claude")),
            Some(dir("/Users/u/.Claude"))
        );
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
    fn a_new_account_takes_the_first_colour_no_other_shows() {
        let mut profiles = two_accounts();
        profiles.accounts[0].color = None;
        profiles.accounts[1].color = Some("blue".to_owned());

        let id = profiles.add("third", dir(r"C:\h\.claude-3")).unwrap();

        // Account 0 shows terracotta by its id, and blue is saved on account 1.
        assert_eq!(
            profiles.account(id).unwrap().color.as_deref(),
            Some("green")
        );
    }

    #[test]
    fn a_colour_outside_the_palette_is_refused() {
        let mut profiles = two_accounts();

        assert!(matches!(
            profiles.set_color(0, "#ff0000"),
            Err(AccountsError::UnknownColor(c)) if c == "#ff0000"
        ));
        assert!(matches!(
            profiles.set_color(9, "teal"),
            Err(AccountsError::NoAccount(9))
        ));
        profiles.set_color(0, "teal").unwrap();
        assert_eq!(profiles.account(0).unwrap().color.as_deref(), Some("teal"));
    }

    #[test]
    fn ids_are_not_reused_after_a_remove() {
        let mut profiles = two_accounts();

        profiles.remove(1).unwrap();
        let id = profiles.add("new", dir(r"C:\h\.claude-new")).unwrap();

        assert_eq!(id, 2);
    }

    #[test]
    fn the_default_account_must_be_saved() {
        let mut profiles = two_accounts();

        assert!(matches!(
            profiles.set_default_account(9),
            Err(AccountsError::NoAccount(9))
        ));
        profiles.set_default_account(1).unwrap();
        assert_eq!(profiles.default_account, Some(1));
    }

    #[test]
    fn the_start_folder_is_a_full_path_or_home() {
        let mut profiles = two_accounts();

        assert!(matches!(
            profiles.set_start_folder(Some(dir("code"))),
            Err(AccountsError::RelativeDir(_))
        ));
        profiles.set_start_folder(Some(dir(r"C:\code"))).unwrap();
        assert_eq!(profiles.start_folder, Some(dir(r"C:\code")));
        profiles.set_start_folder(None).unwrap();
        assert_eq!(profiles.start_folder, None);
    }

    #[test]
    fn a_missing_start_folder_is_refused() {
        let home = TestDir::new();
        let accounts = Accounts::new(&home.path().join("app"), home.path().to_path_buf());

        let result = accounts.set_start_folder(Some(home.path().join("gone")));

        assert!(matches!(result, Err(AccountsError::NoSuchDir(_))));
        accounts
            .set_start_folder(Some(home.path().to_path_buf()))
            .unwrap();
        assert_eq!(
            accounts.load().unwrap().start_folder.as_deref(),
            Some(home.path())
        );
    }

    #[test]
    fn the_sidebar_width_is_saved_inside_its_limits() {
        let home = TestDir::new();
        let accounts = Accounts::new(&home.path().join("app"), home.path().to_path_buf());

        assert_eq!(accounts.load().unwrap().sidebar_width, None);
        assert_eq!(
            accounts.set_sidebar_width(320).unwrap().sidebar_width,
            Some(320)
        );
        assert_eq!(
            accounts.set_sidebar_width(50).unwrap().sidebar_width,
            Some(200)
        );
        assert_eq!(
            accounts.set_sidebar_width(9000).unwrap().sidebar_width,
            Some(560)
        );
        assert_eq!(accounts.load().unwrap().sidebar_width, Some(560));
    }

    #[test]
    fn the_split_share_is_saved_inside_its_limits() {
        let home = TestDir::new();
        let accounts = Accounts::new(&home.path().join("app"), home.path().to_path_buf());

        assert_eq!(accounts.load().unwrap().split_share, None);
        assert_eq!(
            accounts.set_split_share(620).unwrap().split_share,
            Some(620)
        );
        assert_eq!(accounts.set_split_share(0).unwrap().split_share, Some(200));
        assert_eq!(
            accounts.set_split_share(1000).unwrap().split_share,
            Some(800)
        );
        assert_eq!(accounts.load().unwrap().split_share, Some(800));
    }

    #[test]
    fn the_panel_is_saved_closed_by_default_with_its_width_and_known_cards() {
        let home = TestDir::new();
        let app = home.path().join("app");
        let accounts = Accounts::new(&app, home.path().to_path_buf());
        assert_eq!(accounts.load().unwrap().panel, PanelPrefs::default());

        let saved = accounts
            .set_panel(PanelPrefs {
                open: true,
                width: Some(9000),
                collapsed: ["skills", "bogus", "changes", "skills", "connectors"]
                    .map(String::from)
                    .to_vec(),
                expanded: ["connectors", "skills"].map(String::from).to_vec(),
                favourites: BTreeMap::new(),
            })
            .unwrap();

        let expected = PanelPrefs {
            open: true,
            width: Some(640),
            collapsed: vec!["changes".to_owned(), "skills".to_owned()],
            expanded: vec!["connectors".to_owned()],
            favourites: BTreeMap::new(),
        };
        assert_eq!(saved.panel, expected);
        assert_eq!(accounts.load().unwrap().panel, expected);
        let raw = fs::read_to_string(app.join("profiles.json")).unwrap();
        assert!(raw.contains(r#""open": true"#), "{raw}");
    }

    #[test]
    fn favourite_skills_are_saved_per_known_account_as_plain_names() {
        let home = TestDir::new();
        let app = home.path().join("app");
        let accounts = Accounts::new(&app, home.path().to_path_buf());
        accounts
            .add("work", home.path().join(".claude-work"))
            .unwrap();
        let work = accounts.load().unwrap().accounts[0].id;
        let names = |list: &[&str]| list.iter().map(|n| (*n).to_owned()).collect::<Vec<_>>();

        accounts
            .set_panel(PanelPrefs {
                favourites: BTreeMap::from([
                    (
                        work.to_string(),
                        names(&["verify", "verify", "", "bad\rname", "release"]),
                    ),
                    ((work + 7).to_string(), names(&["gone"])),
                    ("x".to_owned(), names(&["odd"])),
                ]),
                ..PanelPrefs::default()
            })
            .unwrap();

        let reloaded = Accounts::new(&app, home.path().to_path_buf())
            .load()
            .unwrap();
        assert_eq!(
            reloaded.panel.favourites,
            BTreeMap::from([(work.to_string(), names(&["verify", "release"]))])
        );

        let padded = accounts
            .set_panel(PanelPrefs {
                favourites: BTreeMap::from([(format!("0{work}"), names(&["verify"]))]),
                ..PanelPrefs::default()
            })
            .unwrap();
        assert_eq!(
            padded.panel.favourites,
            BTreeMap::from([(work.to_string(), names(&["verify"]))])
        );
    }

    #[test]
    fn alerts_start_on_and_the_choice_is_saved_in_the_profiles_file() {
        let home = TestDir::new();
        let app = home.path().join("app");
        let accounts = Accounts::new(&app, home.path().to_path_buf());

        assert!(accounts.load().unwrap().alerts);
        assert!(!accounts.set_alerts(false).unwrap().alerts);
        assert!(!accounts.load().unwrap().alerts);
        let saved = fs::read_to_string(app.join("profiles.json")).unwrap();
        assert!(saved.contains(r#""alerts": false"#));
        assert!(accounts.set_alerts(true).unwrap().alerts);
    }

    #[test]
    fn suggestions_start_on_and_the_choice_is_saved_in_the_profiles_file() {
        let home = TestDir::new();
        let app = home.path().join("app");
        let accounts = Accounts::new(&app, home.path().to_path_buf());

        assert!(accounts.load().unwrap().suggestions);
        assert!(!accounts.set_suggestions(false).unwrap().suggestions);
        assert!(!accounts.load().unwrap().suggestions);
        let saved = fs::read_to_string(app.join("profiles.json")).unwrap();
        assert!(saved.contains(r#""suggestions": false"#));
        assert!(accounts.set_suggestions(true).unwrap().suggestions);
    }

    #[test]
    fn messages_between_accounts_start_on_and_the_choice_is_saved() {
        let home = TestDir::new();
        let accounts = Accounts::new(&home.path().join("app"), home.path().to_path_buf());

        assert!(accounts.load().unwrap().messages);
        assert!(!accounts.set_messages(false).unwrap().messages);
        assert!(!accounts.load().unwrap().messages);
    }

    #[test]
    fn only_a_known_shell_is_saved() {
        let home = TestDir::new();
        let accounts = Accounts::new(&home.path().join("app"), home.path().to_path_buf());

        for refused in ["notepad", "claude"] {
            assert!(matches!(
                accounts.set_shell(refused.to_owned()),
                Err(AccountsError::UnknownShell(_))
            ));
        }
        // A shell every runner of this platform has.
        let installed = if cfg!(windows) { "git-bash" } else { "bash" };
        let saved = accounts.set_shell(installed.to_owned()).unwrap();
        assert_eq!(saved.shell.as_deref(), Some(installed));
    }

    #[test]
    fn only_a_folder_other_than_the_default_sets_the_variable() {
        let home = dir(r"C:\Users\u");

        assert_eq!(env_dir(&home, dir(r"C:\Users\u\.claude")), None);
        #[cfg(windows)]
        assert_eq!(env_dir(&home, dir("c:/users/U/.Claude/")), None);
        #[cfg(not(windows))]
        assert_eq!(env_dir(&home, dir("/Users/u/.claude/")), None);
        assert_eq!(
            env_dir(&home, dir(r"C:\Users\u\.claude-acc2")),
            Some(dir(r"C:\Users\u\.claude-acc2"))
        );
    }

    #[test]
    fn dots_and_parents_reach_the_same_folder() {
        let home = dir(r"C:\Users\u");
        let mut profiles = two_accounts();

        assert_eq!(env_dir(&home, dir(r"C:\Users\u\.claude\.")), None);
        assert_eq!(
            env_dir(&home, dir(r"C:\Users\u\.claude-x\..\.claude")),
            None
        );
        assert!(matches!(
            profiles.add("again", dir(r"C:\h\.claude\..\.claude-work")),
            Err(AccountsError::DuplicateDir(_))
        ));
    }

    #[cfg(windows)]
    #[test]
    fn a_short_name_reaches_the_same_folder() {
        let home = TestDir::new();
        let long = home.path().join(".claude-a-rather-long-folder-name");
        fs::create_dir_all(&long).unwrap();
        let script = format!(
            "(New-Object -ComObject Scripting.FileSystemObject).GetFolder('{}').ShortPath",
            long.display()
        );
        let output = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", &script])
            .output()
            .unwrap();
        let short = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());

        assert!(short.is_dir(), "no short path: {short:?}");
        assert!(same_dir(&short, &long), "{short:?} vs {long:?}");
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

    #[cfg(windows)]
    #[test]
    fn a_folder_reached_through_a_junction_is_a_duplicate() {
        let home = TestDir::new();
        let real = home.path().join("real");
        let link = home.path().join("link");
        fs::create_dir_all(&real).unwrap();
        let made = std::process::Command::new("cmd")
            .arg("/c")
            .arg("mklink")
            .arg("/J")
            .arg(&link)
            .arg(&real)
            .output()
            .unwrap();
        assert!(made.status.success(), "no junction: {made:?}");
        let accounts = Accounts::new(&home.path().join("app"), home.path().to_path_buf());
        accounts.add("one", real.join(".claude-x")).unwrap();
        fs::remove_dir(real.join(".claude-x")).unwrap();

        let result = accounts.add("two", link.join(".claude-x"));

        assert!(matches!(result, Err(AccountsError::DuplicateDir(_))));
        assert_eq!(accounts.load().unwrap().accounts.len(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn a_folder_reached_through_a_symlink_is_a_duplicate() {
        let home = TestDir::new();
        let real = home.path().join("real");
        let link = home.path().join("link");
        fs::create_dir_all(&real).unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let accounts = Accounts::new(&home.path().join("app"), home.path().to_path_buf());
        accounts.add("one", real.join(".claude-x")).unwrap();
        fs::remove_dir(real.join(".claude-x")).unwrap();

        let result = accounts.add("two", link.join(".claude-x"));

        assert!(matches!(result, Err(AccountsError::DuplicateDir(_))));
        assert_eq!(accounts.load().unwrap().accounts.len(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn a_missing_folder_after_a_link_and_dots_is_where_the_link_leads() {
        let home = TestDir::new();
        let base = home.path().join("base");
        let child = home.path().join("other").join("child");
        fs::create_dir_all(&base).unwrap();
        fs::create_dir_all(&child).unwrap();
        std::os::unix::fs::symlink(&child, base.join("link")).unwrap();
        let spelled = base.join("link").join("..").join("foo");

        assert!(same_dir(&spelled, &home.path().join("other").join("foo")));
        assert!(!same_dir(&spelled, &base.join("foo")));
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
            accounts.suggested_dir(" Work Team "),
            dir(r"C:\Users\u\.claude-work-team")
        );
        assert_eq!(accounts.suggested_dir("!!"), dir(r"C:\Users\u\.claude-new"));
    }
}
