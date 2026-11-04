mod gh;
mod git;
mod mcp;
mod reveal;
mod skills;

use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::{Serialize, Serializer};
use tauri::State;

use crate::accounts::{AccountId, Accounts, AccountsError};
use crate::programs::Finder;

pub use crate::panel::gh::PrCache;
use crate::panel::gh::PrRow;
use crate::panel::git::Git;
use crate::panel::mcp::Connectors;
use crate::panel::skills::Skill;

#[derive(Debug, thiserror::Error)]
pub enum PanelError {
    #[error(transparent)]
    Accounts(#[from] AccountsError),
    #[error("folder not found")]
    NoFolder,
    #[error("no pull request is known for this folder yet")]
    NoPr,
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
        branch: Option<String>,
        commit: Option<String>,
        base: Option<String>,
        added: u64,
        removed: u64,
        /// Whether Create PR has a `gh` to type for.
        gh: bool,
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
        branch: repo.branch,
        commit: repo.commit,
        base: repo.base,
        added: repo.added,
        removed: repo.removed,
        gh: Finder::from_env().find("gh").is_some(),
    })
}

/// The branch's open PR, asked of `gh` at most once a minute per repo.
#[tauri::command(async)]
pub fn panel_pr(prs: State<'_, PrCache>, folder: PathBuf) -> Result<Option<PrRow>, PanelError> {
    let folder = self::folder(&folder)?;
    let (Some(git), Some(gh)) = (git(), Finder::from_env().find("gh")) else {
        return Ok(None);
    };
    let Some((root, Some(branch))) = git.head(folder).map(|h| (h.root, h.branch)) else {
        return Ok(None);
    };
    let pr = prs.get(&root, &branch, Instant::now(), || {
        gh::open_pr(&gh, folder, &branch)
    });
    Ok(pr.as_ref().map(PrRow::from))
}

#[tauri::command(async)]
pub fn panel_open_pr(prs: State<'_, PrCache>, folder: PathBuf) -> Result<(), PanelError> {
    let folder = self::folder(&folder)?;
    let url = git()
        .and_then(|git| git.head(folder))
        .map(|head| head.root)
        .and_then(|root| prs.url(&root))
        .ok_or(PanelError::NoPr)?;
    Ok(reveal::open(url.as_ref())?)
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
    account: Option<AccountId>,
    folder: Option<PathBuf>,
) -> Result<Connectors, PanelError> {
    let config = account_config(&accounts, account)?;
    let folder = folder.as_deref().map(self::folder).transpose()?;
    Ok(mcp::read(&config, folder))
}

#[tauri::command(async)]
pub fn panel_skills(
    accounts: State<'_, Accounts>,
    account: Option<AccountId>,
    folder: Option<PathBuf>,
) -> Result<Vec<Skill>, PanelError> {
    let home = accounts.claude_home(account)?;
    let folder = folder.as_deref().map(self::folder).transpose()?;
    Ok(skills::read(folder, &home))
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
