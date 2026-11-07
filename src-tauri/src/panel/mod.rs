mod gh;
mod git;
mod mcp;
mod plugins;
mod reveal;
mod skills;
mod vscode;

use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::{Serialize, Serializer};
use tauri::State;

use crate::accounts::{AccountId, Accounts, AccountsError};
use crate::bridge::Bridge;
use crate::programs::Finder;
use crate::saved::TabId;

pub use crate::panel::gh::PrCache;
use crate::panel::gh::PrRow;
use crate::panel::git::Git;
use crate::panel::git::Head;
use crate::panel::mcp::Connectors;
use crate::panel::skills::Skill;

#[derive(Debug, thiserror::Error)]
pub enum PanelError {
    #[error(transparent)]
    Accounts(#[from] AccountsError),
    #[error("folder not found")]
    NoFolder,
    #[error("that pull request is not known for this folder yet")]
    NoPr,
    #[error("that pull request is not a draft")]
    NotDraft,
    #[error("gh could not mark #{0} ready for review; check `gh auth status` in a shell")]
    NotReady(u64),
    #[error("VS Code was not found")]
    NoVsCode,
    #[error("could not open it: {0}")]
    Open(#[from] std::io::Error),
}

impl Serialize for PanelError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

/// What the Changes card and the branch row show for a folder.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Changes {
    NoGit,
    NotRepo,
    #[serde(rename_all = "camelCase")]
    Repo {
        name: String,
        /// The repo's top folder, so folders in one repo list its PRs once.
        root: String,
        branch: Option<String>,
        commit: Option<String>,
        base: Option<String>,
        added: u64,
        removed: u64,
        /// Whether Create PR has a `gh` to type for.
        gh: bool,
        /// Whether Open in VS Code has an editor to start.
        vscode: bool,
    },
}

fn folder(folder: &Path) -> Result<&Path, PanelError> {
    if folder.is_absolute() && folder.is_dir() {
        Ok(folder)
    } else {
        Err(PanelError::NoFolder)
    }
}

fn git() -> Option<Git> {
    Finder::from_env().find("git").map(Git::new)
}

#[tauri::command(async)]
pub fn panel_changes(folder: PathBuf) -> Result<Changes, PanelError> {
    let folder = self::folder(&folder)?;
    let Some(git) = git() else {
        return Ok(Changes::NoGit);
    };
    let Some(repo) = git.repo(folder) else {
        return Ok(Changes::NotRepo);
    };
    let name = repo
        .root
        .file_name()
        .map_or_else(|| repo.root.to_string_lossy(), |n| n.to_string_lossy())
        .into_owned();
    Ok(Changes::Repo {
        name,
        root: repo.root.to_string_lossy().into_owned(),
        branch: repo.branch,
        commit: repo.commit,
        base: repo.base,
        added: repo.added,
        removed: repo.removed,
        gh: Finder::from_env().find("gh").is_some(),
        vscode: vscode::find().is_some(),
    })
}

fn repo_root(folder: &Path) -> Result<PathBuf, PanelError> {
    git()
        .and_then(|git| git.head(self::folder(folder).ok()?))
        .map(|head: Head| head.root)
        .ok_or(PanelError::NoPr)
}

/// Every PR of the folder's repo, asked of `gh` at most once a minute per repo.
#[tauri::command(async)]
pub fn panel_prs(prs: State<'_, PrCache>, folder: PathBuf) -> Result<Vec<PrRow>, PanelError> {
    let folder = self::folder(&folder)?;
    let (Some(git), Some(gh)) = (git(), Finder::from_env().find("gh")) else {
        return Ok(Vec::new());
    };
    let Some(root) = git.head(folder).map(|h| h.root) else {
        return Ok(Vec::new());
    };
    let found = prs.get(&root, Instant::now(), || gh::list(&gh, folder));
    Ok(gh::rows(&found))
}

fn pr_url(prs: &PrCache, folder: &Path, number: u64) -> Result<String, PanelError> {
    prs.url(&repo_root(folder)?, number).ok_or(PanelError::NoPr)
}

#[tauri::command(async)]
pub fn panel_open_pr(
    prs: State<'_, PrCache>,
    folder: PathBuf,
    number: u64,
) -> Result<(), PanelError> {
    let url = pr_url(&prs, &folder, number)?;
    Ok(reveal::open(url.as_ref())?)
}

/// The PR's link for the clipboard; only a link `gh` gave for this repo is handed out.
#[tauri::command(async)]
pub fn panel_pr_link(
    prs: State<'_, PrCache>,
    folder: PathBuf,
    number: u64,
) -> Result<String, PanelError> {
    pr_url(&prs, &folder, number)
}

/// Runs `gh pr ready` for a draft the list showed, after the user confirmed it in the panel.
#[tauri::command(async)]
pub fn panel_pr_ready(
    prs: State<'_, PrCache>,
    folder: PathBuf,
    number: u64,
) -> Result<(), PanelError> {
    let folder = self::folder(&folder)?;
    let root = repo_root(folder)?;
    if !prs.is_draft(&root, number) {
        return Err(PanelError::NotDraft);
    }
    let gh = Finder::from_env()
        .find("gh")
        .ok_or(PanelError::NotReady(number))?;
    if !gh::ready(&gh, folder, number) {
        return Err(PanelError::NotReady(number));
    }
    prs.made_ready(&root, number);
    Ok(())
}

/// Opens the folder in VS Code, whose Source Control view shows the diff.
#[tauri::command(async)]
pub fn panel_open_vscode(folder: PathBuf) -> Result<(), PanelError> {
    let folder = self::folder(&folder)?;
    let code = vscode::find().ok_or(PanelError::NoVsCode)?;
    Ok(vscode::open(&code, folder)?)
}

#[tauri::command(async)]
pub fn panel_open_folder(folder: PathBuf) -> Result<(), PanelError> {
    Ok(reveal::open(self::folder(&folder)?.as_os_str())?)
}

// Claude Code keeps its settings file in the config dir when one is set, else in the home folder.
fn account_config(
    accounts: &Accounts,
    account: Option<AccountId>,
) -> Result<PathBuf, AccountsError> {
    Ok(accounts
        .claude_config_dir(account)?
        .unwrap_or_else(|| accounts.home().to_path_buf())
        .join(".claude.json"))
}

#[tauri::command(async)]
pub fn panel_connectors(
    accounts: State<'_, Accounts>,
    bridge: State<'_, Bridge>,
    account: Option<AccountId>,
    folder: Option<PathBuf>,
    tab: Option<TabId>,
) -> Result<Connectors, PanelError> {
    let config = account_config(&accounts, account)?;
    let folder = folder.as_deref().map(self::folder).transpose()?;
    let plugins = plugins::enabled(&accounts.claude_home(account)?, folder);
    // Whether this tab's running chat was given the server when it started, without reading the file it names.
    let app_server = tab.is_some_and(|tab| bridge.linked(tab));
    let mut found = mcp::read(&config, folder, &plugins.plugins, app_server);
    found.unreadable.extend(plugins.unreadable);
    Ok(found)
}

#[tauri::command(async)]
pub fn panel_skills(
    accounts: State<'_, Accounts>,
    account: Option<AccountId>,
    folder: Option<PathBuf>,
) -> Result<Vec<Skill>, PanelError> {
    let home = accounts.claude_home(account)?;
    let signed_in = skills::account(&account_config(&accounts, account)?);
    let folder = folder.as_deref().map(self::folder).transpose()?;
    let plugins = plugins::enabled(&home, folder).plugins;
    Ok(skills::read(folder, &home, &plugins, &signed_in))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::testdir::TestDir;

    #[test]
    fn a_folder_outside_any_repo_says_so_without_an_error() {
        let dir = TestDir::new();

        let changes = panel_changes(dir.path().to_path_buf()).unwrap();

        assert_eq!(
            serde_json::to_string(&changes).unwrap(),
            r#"{"kind":"notRepo"}"#
        );
    }

    #[test]
    fn a_missing_or_relative_folder_is_refused() {
        let dir = TestDir::new();

        assert!(matches!(
            panel_changes(dir.path().join("gone")),
            Err(PanelError::NoFolder)
        ));
        assert!(matches!(
            panel_open_folder(PathBuf::from("relative")),
            Err(PanelError::NoFolder)
        ));
    }

    #[test]
    fn the_default_account_reads_the_settings_file_in_the_home_folder() {
        let dir = TestDir::new();
        let home = dir.path().to_path_buf();
        let accounts = Accounts::new(&home.join("app"), home.clone());
        let profiles = accounts.add("work", home.join(".claude-work")).unwrap();
        let work = profiles.accounts[0].id;
        accounts.add("main", home.join(".claude")).unwrap();
        let main = accounts.load().unwrap().accounts[1].id;

        assert_eq!(
            account_config(&accounts, Some(work)).unwrap(),
            home.join(".claude-work").join(".claude.json")
        );
        assert_eq!(
            account_config(&accounts, Some(main)).unwrap(),
            home.join(".claude.json")
        );
        assert_eq!(
            account_config(&accounts, None).unwrap(),
            home.join(".claude.json")
        );
    }
}
