//! Removes the worktrees whose PR merged, for projects that asked, through the same checks as Remove.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use crate::panel::gh::{Pr, PrState};
use crate::panel::git::Git;
use crate::panel::worktrees::{self, TabFolders, Worktree};

/// How often the projects that asked are swept, besides at start.
pub const EVERY: Duration = Duration::from_secs(60 * 60);
/// The most removal lines a repo's list keeps.
const MOST_LINES: usize = 20;

/// The lines automatic removals leave in each repo's Worktrees list, oldest first, by the repo's main folder.
#[derive(Default)]
pub struct CleanLog(Mutex<HashMap<String, Vec<String>>>);

impl CleanLog {
    pub fn add(&self, repo: &str, lines: Vec<String>) {
        if lines.is_empty() {
            return;
        }
        // A panic elsewhere leaves the map whole, so poisoning is no reason to lose lines.
        let mut all = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let kept = all.entry(repo.to_owned()).or_default();
        kept.extend(lines);
        let over = kept.len().saturating_sub(MOST_LINES);
        kept.drain(..over);
    }

    pub fn lines(&self, repo: &str) -> Vec<String> {
        let all = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        all.get(repo).cloned().unwrap_or_default()
    }
}

/// Worktrees stale because their PR merged, with that PR's number; a closed PR or a gone branch alone is not enough.
pub fn merged_ones(rows: &[Worktree]) -> Vec<(PathBuf, u64)> {
    rows.iter()
        .filter(|w| w.stale && !w.main)
        .filter_map(|w| {
            let pr = w.pr.filter(|pr| pr.state == PrState::Merged)?;
            Some((PathBuf::from(&w.path), pr.number))
        })
        .collect()
}

/// Removes the folder's repo's merged worktrees; the repo's main folder and a line per removal, or `None` when git can't list them.
pub fn sweep(
    git: &Git,
    folder: &Path,
    prs: &[Pr],
    tabs: &[TabFolders],
) -> Option<(String, Vec<String>)> {
    let rows = worktrees::list(git, folder, prs, tabs)?;
    let repo = rows.first()?.path.clone();
    let wanted = merged_ones(&rows);
    if wanted.is_empty() {
        return Some((repo, Vec::new()));
    }
    let paths: Vec<PathBuf> = wanted.iter().map(|(path, _)| path.clone()).collect();
    // `remove` lists them again and keeps any that now has changes or a tab.
    let removed = worktrees::remove(git, folder, &paths, prs, tabs)?;
    let lines = removed
        .iter()
        .filter(|r| r.refusal.is_none())
        .filter_map(|r| {
            let number = wanted
                .iter()
                .find(|(path, _)| Path::new(&r.path) == path.as_path())
                .map(|(_, n)| *n)?;
            Some(format!("Removed {}, merged in #{number}", r.name))
        })
        .collect();
    Some((repo, lines))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::process::Command;

    use super::*;
    use crate::accounts::testdir::TestDir;
    use crate::panel::git::tests::{git, sh};

    fn pr(number: u64, state: &str, branch: &str, oid: &str) -> Pr {
        Pr {
            number,
            title: format!("PR {number}"),
            url: format!("https://github.com/example/repo/pull/{number}"),
            state: state.to_owned(),
            is_draft: false,
            updated_at: "u".to_owned(),
            head_ref_name: branch.to_owned(),
            head_ref_oid: oid.to_owned(),
            is_cross_repository: false,
        }
    }

    fn commit(folder: &Path, file: &str) {
        fs::write(folder.join(file), format!("{file}\n")).unwrap();
        sh(folder, &["add", "."]);
        sh(folder, &["commit", "-q", "-m", file]);
    }

    fn tip(folder: &Path) -> String {
        let out = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(folder)
            .output()
            .unwrap();
        String::from_utf8(out.stdout).unwrap().trim().to_owned()
    }

    /// A repo with a worktree per name beside it, each on its own branch with one commit.
    fn repo_with(dir: &TestDir, names: &[&str]) -> PathBuf {
        let repo = dir.path().join("repo");
        fs::create_dir(&repo).unwrap();
        sh(&repo, &["init", "-q"]);
        commit(&repo, "base.txt");
        for name in names {
            let path = dir.path().join(name);
            sh(
                &repo,
                &["worktree", "add", "-q", "-b", name, path.to_str().unwrap()],
            );
            commit(&path, &format!("{name}.txt"));
        }
        repo
    }

    #[test]
    fn only_merged_worktrees_with_nothing_to_lose_or_interrupt_go_and_each_says_so() {
        let dir = TestDir::new();
        let names = ["merged", "dirty", "in_tab", "closed", "open"];
        let repo = repo_with(&dir, &names);
        let at = |name: &str| dir.path().join(name);
        fs::write(at("dirty").join("notes.txt"), "not committed").unwrap();
        let prs = [
            pr(11, "MERGED", "merged", &tip(&at("merged"))),
            pr(12, "MERGED", "dirty", &tip(&at("dirty"))),
            pr(13, "MERGED", "in_tab", &tip(&at("in_tab"))),
            pr(14, "CLOSED", "closed", ""),
            pr(15, "OPEN", "open", ""),
        ];
        let tabs = [TabFolders {
            tab: 4,
            folders: vec![at("in_tab")],
        }];

        let (main, lines) = sweep(&git(), &repo, &prs, &tabs).unwrap();

        assert_eq!(lines, ["Removed merged, merged in #11"]);
        assert!(!at("merged").exists());
        for kept in ["dirty", "in_tab", "closed", "open"] {
            assert!(at(kept).is_dir(), "{kept} was removed");
        }
        let names: Vec<String> = git().branches(&repo).into_iter().map(|b| b.name).collect();
        assert!(
            !names.contains(&"merged".to_owned()),
            "its PR merged at its tip, so the local branch went: {names:?}"
        );
        assert!(Path::new(&main).ends_with("repo"));
        assert_eq!(
            worktrees::main_of(&git(), &at("open")),
            Some(main),
            "a worktree names the same main folder, so its list finds the lines"
        );
    }

    #[test]
    fn a_repo_with_nothing_merged_is_left_alone() {
        let dir = TestDir::new();
        let repo = repo_with(&dir, &["working"]);

        let (_, lines) = sweep(&git(), &repo, &[pr(3, "OPEN", "working", "")], &[]).unwrap();

        assert!(lines.is_empty());
        assert!(dir.path().join("working").is_dir());
    }

    #[test]
    fn a_folder_outside_any_repo_sweeps_nothing() {
        let dir = TestDir::new();

        assert_eq!(sweep(&git(), dir.path(), &[], &[]), None);
    }

    #[test]
    fn each_repo_keeps_its_latest_lines() {
        let log = CleanLog::default();

        log.add("a", (0..25).map(|n| format!("line {n}")).collect());
        log.add("b", Vec::new());

        let kept = log.lines("a");
        assert_eq!(kept.len(), MOST_LINES);
        assert_eq!(kept[0], "line 5");
        assert!(log.lines("b").is_empty());
    }
}
