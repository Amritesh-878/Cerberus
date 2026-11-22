mod gh;
pub(crate) mod git;
mod mcp;
mod plugins;
pub(crate) mod reveal;
mod skills;
pub(crate) mod vscode;
mod worktrees;

use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::{Serialize, Serializer};
use tauri::{AppHandle, Manager};

use crate::accounts::{AccountId, Accounts, AccountsError};
use crate::blocking::blocking;
use crate::bridge::Bridge;
use crate::live::Live;
use crate::programs::Finder;
use crate::saved::{Saved, SavedTab, TabId};

pub use crate::panel::gh::PrCache;
use crate::panel::gh::{Pr, PrRow};
use crate::panel::git::Git;
use crate::panel::git::Head;
use crate::panel::mcp::Connectors;
use crate::panel::skills::Skill;
pub use crate::panel::worktrees::Roots;
use crate::panel::worktrees::{Removed, TabFolders, Worktree};

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
    #[error("that folder is not one of this tab's")]
    NotTabs,
    #[error("the saved tabs could not be read, so nothing was removed")]
    NoTabs,
    #[error("git could not list the worktrees")]
    NoWorktrees,
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

fn changes_of(git: &Git, folder: &Path) -> Changes {
    let Some(repo) = git.repo(folder) else {
        return Changes::NotRepo;
    };
    let name = repo
        .root
        .file_name()
        .map_or_else(|| repo.root.to_string_lossy(), |n| n.to_string_lossy())
        .into_owned();
    Changes::Repo {
        name,
        root: repo.root.to_string_lossy().into_owned(),
        branch: repo.branch,
        commit: repo.commit,
        base: repo.base,
        added: repo.added,
        removed: repo.removed,
        gh: Finder::from_env().find("gh").is_some(),
        vscode: vscode::find().is_some(),
    }
}

#[tauri::command]
pub async fn panel_changes(folder: PathBuf) -> Result<Changes, PanelError> {
    blocking(move || {
        let folder = self::folder(&folder)?;
        let Some(git) = git() else {
            return Ok(Changes::NoGit);
        };
        Ok(changes_of(&git, folder))
    })
    .await
}

/// A repo the tab's chat wrote into, with its folder in the platform's own form for adding to the tab.
#[derive(Debug, Serialize)]
pub struct Written {
    pub folder: String,
    pub changes: Changes,
}

// Only from files the tab's own hooks reported, never from a path the webview names.
fn written_roots(app: &AppHandle, git: &Git, tab: TabId) -> Vec<PathBuf> {
    let files = app.state::<Live>().written(tab);
    let roots = app.state::<Roots>();
    let now = Instant::now();
    worktrees::written_roots(&files, |folder| {
        if !folder.is_dir() {
            return None;
        }
        roots.get(folder, now, || git.toplevel(folder))
    })
}

/// The repos the tab's chat wrote into this session, each read like a folder of the tab.
#[tauri::command]
pub async fn panel_written(app: AppHandle, tab: TabId) -> Result<Vec<Written>, PanelError> {
    blocking(move || {
        let Some(git) = git() else {
            return Ok(Vec::new());
        };
        Ok(written_roots(&app, &git, tab)
            .into_iter()
            .map(|root| Written {
                changes: changes_of(&git, &root),
                folder: root.to_string_lossy().into_owned(),
            })
            .collect())
    })
    .await
}

/// The folder, if it is the tab's own, one added to it, or a repo its chat wrote into.
fn tabs_folder(
    app: &AppHandle,
    git: &Git,
    tab: TabId,
    folder: &Path,
) -> Result<PathBuf, PanelError> {
    let folder = self::folder(folder)?;
    let saved = app.state::<Saved>().tab(tab);
    if is_tabs(saved.as_ref(), || written_roots(app, git, tab), folder) {
        Ok(folder.to_path_buf())
    } else {
        Err(PanelError::NotTabs)
    }
}

fn is_tabs(
    saved: Option<&SavedTab>,
    written: impl FnOnce() -> Vec<PathBuf>,
    folder: &Path,
) -> bool {
    let own = saved.is_some_and(|saved| {
        saved.cwd.as_deref().map(Path::new) == Some(folder)
            || saved.dirs.iter().any(|d| Path::new(d) == folder)
    });
    own || written().iter().any(|r| r == folder)
}

/// Every saved tab's folders, for which worktrees are in use.
fn tab_folders(app: &AppHandle) -> Result<Vec<TabFolders>, PanelError> {
    let loaded = app
        .state::<Saved>()
        .load()
        .map_err(|_| PanelError::NoTabs)?;
    Ok(loaded
        .tree
        .tabs
        .iter()
        .map(|t| TabFolders {
            tab: t.id,
            folders: t.cwd.iter().chain(&t.dirs).map(PathBuf::from).collect(),
        })
        .collect())
}

// The panel's own PR list for the repo, so `gh` still runs at most once a minute per repo.
fn prs_of(app: &AppHandle, git: &Git, folder: &Path) -> Vec<Pr> {
    let (Some(gh), Some(root)) = (Finder::from_env().find("gh"), git.toplevel(folder)) else {
        return Vec::new();
    };
    app.state::<PrCache>()
        .get(&root, Instant::now(), || gh::list(&gh, folder))
}

/// The worktrees of a repo the tab shows, with which are stale and why any cannot be removed.
#[tauri::command]
pub async fn panel_worktrees(
    app: AppHandle,
    tab: TabId,
    folder: PathBuf,
) -> Result<Vec<Worktree>, PanelError> {
    blocking(move || {
        let git = git().ok_or(PanelError::NoWorktrees)?;
        let folder = tabs_folder(&app, &git, tab, &folder)?;
        let tabs = tab_folders(&app).unwrap_or_default();
        let prs = prs_of(&app, &git, &folder);
        worktrees::list(&git, &folder, &prs, &tabs).ok_or(PanelError::NoWorktrees)
    })
    .await
}

/// Runs `git worktree remove` (never forced) for each listed worktree the user confirmed, checked again first.
#[tauri::command]
pub async fn panel_remove_worktrees(
    app: AppHandle,
    tab: TabId,
    folder: PathBuf,
    paths: Vec<PathBuf>,
) -> Result<Vec<Removed>, PanelError> {
    blocking(move || {
        let git = git().ok_or(PanelError::NoWorktrees)?;
        let folder = tabs_folder(&app, &git, tab, &folder)?;
        // Without the saved tabs nothing can be shown to be out of use, so nothing goes.
        let tabs = tab_folders(&app)?;
        let prs = prs_of(&app, &git, &folder);
        worktrees::remove(&git, &folder, &paths, &prs, &tabs).ok_or(PanelError::NoWorktrees)
    })
    .await
}

fn repo_root(folder: &Path) -> Result<PathBuf, PanelError> {
    git()
        .and_then(|git| git.head(self::folder(folder).ok()?))
        .map(|head: Head| head.root)
        .ok_or(PanelError::NoPr)
}

/// Every PR of the folder's repo, asked of `gh` at most once a minute per repo.
#[tauri::command]
pub async fn panel_prs(app: AppHandle, folder: PathBuf) -> Result<Vec<PrRow>, PanelError> {
    blocking(move || {
        let prs = app.state::<PrCache>();
        let folder = self::folder(&folder)?;
        let (Some(git), Some(gh)) = (git(), Finder::from_env().find("gh")) else {
            return Ok(Vec::new());
        };
        let Some(root) = git.head(folder).map(|h| h.root) else {
            return Ok(Vec::new());
        };
        let found = prs.get(&root, Instant::now(), || gh::list(&gh, folder));
        Ok(gh::rows(&found))
    })
    .await
}

/// The link of the PR made from the folder's branch, through the same once-a-minute list as the Changes card.
pub fn branch_pr(prs: &PrCache, folder: &Path) -> Option<String> {
    let folder = self::folder(folder).ok()?;
    let (git, gh) = (git()?, Finder::from_env().find("gh")?);
    let head = git.head(folder)?;
    let branch = head.branch?;
    let found = prs.get(&head.root, Instant::now(), || gh::list(&gh, folder));
    gh::for_branch(&found, &branch).map(|pr| pr.url.clone())
}

fn pr_url(prs: &PrCache, folder: &Path, number: u64) -> Result<String, PanelError> {
    prs.url(&repo_root(folder)?, number).ok_or(PanelError::NoPr)
}

#[tauri::command]
pub async fn panel_open_pr(app: AppHandle, folder: PathBuf, number: u64) -> Result<(), PanelError> {
    blocking(move || {
        let prs = app.state::<PrCache>();
        let url = pr_url(&prs, &folder, number)?;
        Ok(reveal::open(url.as_ref())?)
    })
    .await
}

/// The PR's link for the clipboard; only a link `gh` gave for this repo is handed out.
#[tauri::command]
pub async fn panel_pr_link(
    app: AppHandle,
    folder: PathBuf,
    number: u64,
) -> Result<String, PanelError> {
    blocking(move || {
        let prs = app.state::<PrCache>();
        pr_url(&prs, &folder, number)
    })
    .await
}

/// Runs `gh pr ready` for a draft the list showed, after the user confirmed it in the panel.
#[tauri::command]
pub async fn panel_pr_ready(
    app: AppHandle,
    folder: PathBuf,
    number: u64,
) -> Result<(), PanelError> {
    blocking(move || {
        let prs = app.state::<PrCache>();
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
        prs.made_ready(&root, number, Instant::now());
        Ok(())
    })
    .await
}

/// Opens the folder in VS Code, whose Source Control view shows the diff.
#[tauri::command]
pub async fn panel_open_vscode(folder: PathBuf) -> Result<(), PanelError> {
    blocking(move || {
        let folder = self::folder(&folder)?;
        let code = vscode::find().ok_or(PanelError::NoVsCode)?;
        Ok(vscode::open(&code, folder)?)
    })
    .await
}

#[tauri::command]
pub async fn panel_open_folder(folder: PathBuf) -> Result<(), PanelError> {
    blocking(move || Ok(reveal::open(self::folder(&folder)?.as_os_str())?)).await
}

// Claude Code keeps its settings file in the config dir when one is set, else in the home folder.
fn account_config(
    accounts: &Accounts,
    account: Option<AccountId>,
) -> Result<PathBuf, AccountsError> {
    Ok(accounts.config_file(&accounts.claude_home(account)?))
}

#[tauri::command]
pub async fn panel_connectors(
    app: AppHandle,
    account: Option<AccountId>,
    folder: Option<PathBuf>,
    tab: Option<TabId>,
) -> Result<Connectors, PanelError> {
    blocking(move || {
        let accounts = app.state::<Accounts>();
        let bridge = app.state::<Bridge>();
        let config = account_config(&accounts, account)?;
        let folder = folder.as_deref().map(self::folder).transpose()?;
        let plugins = plugins::enabled(&accounts.claude_home(account)?, folder);
        // Whether this tab's running chat was given the server when it started, without reading the file it names.
        let app_server = tab.is_some_and(|tab| bridge.linked(tab));
        let mut found = mcp::read(&config, folder, &plugins.plugins, app_server);
        found.unreadable.extend(plugins.unreadable);
        Ok(found)
    })
    .await
}

#[tauri::command]
pub async fn panel_skills(
    app: AppHandle,
    account: Option<AccountId>,
    folder: Option<PathBuf>,
) -> Result<Vec<Skill>, PanelError> {
    blocking(move || {
        let accounts = app.state::<Accounts>();
        let home = accounts.claude_home(account)?;
        let signed_in = skills::account(&account_config(&accounts, account)?);
        let folder = folder.as_deref().map(self::folder).transpose()?;
        let plugins = plugins::enabled(&home, folder).plugins;
        Ok(skills::read(folder, &home, &plugins, &signed_in))
    })
    .await
}

#[cfg(test)]
mod tests {
    use tauri::async_runtime::block_on;

    use super::*;
    use crate::accounts::testdir::TestDir;

    #[test]
    fn a_folder_outside_any_repo_says_so_without_an_error() {
        let dir = TestDir::new();

        let changes = block_on(panel_changes(dir.path().to_path_buf())).unwrap();

        assert_eq!(
            serde_json::to_string(&changes).unwrap(),
            r#"{"kind":"notRepo"}"#
        );
    }

    #[test]
    fn a_missing_or_relative_folder_is_refused() {
        let dir = TestDir::new();

        assert!(matches!(
            block_on(panel_changes(dir.path().join("gone"))),
            Err(PanelError::NoFolder)
        ));
        assert!(matches!(
            block_on(panel_open_folder(PathBuf::from("relative"))),
            Err(PanelError::NoFolder)
        ));
    }

    #[test]
    fn worktrees_are_read_only_for_the_tabs_folders_and_the_repos_its_chat_wrote_into() {
        let tab = SavedTab {
            id: 3,
            project_id: 1,
            parent_id: None,
            title: "coder".into(),
            program: "claude".into(),
            cwd: Some(r"C:\code\app".into()),
            account: None,
            dirs: vec![r"C:\code\plans".into()],
            handoff: None,
        };
        let written = || vec![PathBuf::from("C:/code/app-worktree")];

        assert!(is_tabs(Some(&tab), written, Path::new(r"C:\code\app")));
        assert!(is_tabs(Some(&tab), written, Path::new(r"C:\code\plans")));
        assert!(is_tabs(None, written, Path::new("C:/code/app-worktree")));
        assert!(!is_tabs(Some(&tab), written, Path::new(r"C:\code\other")));
        assert!(!is_tabs(None, Vec::new, Path::new(r"C:\code\app")));
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
