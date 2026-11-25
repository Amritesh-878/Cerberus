//! A repo's worktrees for the Changes card: which are stale, and removing them without losing work.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::panel::gh::{Pr, PrState};
use crate::panel::git::{Branch, Git};
use crate::saved::TabId;

/// A written file's folder is asked of git again after this, in case it became or stopped being a repo.
const ROOT_EVERY: Duration = Duration::from_secs(60);
/// Folders remembered at most; past it the memory starts over.
const MOST_ROOTS: usize = 512;
/// `git status` runs this many at once, one per worktree.
const AT_ONCE: usize = 8;

/// A worktree as `git worktree list --porcelain -z` names it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Listed {
    pub path: PathBuf,
    /// The full commit id it is on.
    pub head: Option<String>,
    pub branch: Option<String>,
    pub bare: bool,
    pub locked: bool,
    /// Git's record of it remains while its folder is gone.
    pub prunable: bool,
}

/// Fields end in NUL, and an empty field ends a worktree.
pub fn parse(out: &str) -> Vec<Listed> {
    let mut all = Vec::new();
    let mut current: Option<Listed> = None;
    for field in out.split('\0') {
        if field.is_empty() {
            all.extend(current.take());
            continue;
        }
        let (key, value) = field.split_once(' ').unwrap_or((field, ""));
        if key == "worktree" {
            all.extend(current.take());
            current = Some(Listed {
                path: PathBuf::from(value),
                ..Listed::default()
            });
            continue;
        }
        let Some(listed) = current.as_mut() else {
            continue;
        };
        match key {
            "HEAD" => listed.head = Some(value.to_owned()),
            "branch" => listed.branch = value.strip_prefix("refs/heads/").map(str::to_owned),
            "bare" => listed.bare = true,
            "locked" => listed.locked = true,
            "prunable" => listed.prunable = true,
            _ => {}
        }
    }
    all.extend(current);
    all
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrMark {
    pub number: u64,
    pub state: PrState,
}

/// Why a worktree is not removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Refusal {
    /// The repo's own folder, which `git worktree remove` never removes.
    Main,
    Locked,
    /// Uncommitted or untracked files, which removing it would lose.
    Dirty,
    /// Git could not say whether it is clean.
    Unread,
    /// A tab's folder, or a folder added to one, is inside it.
    InUse,
    /// Not one of this repo's worktrees.
    Unknown,
    /// Git refused or failed to remove it.
    Failed,
    /// Its branch has a commit after the one its PR merged.
    Moved,
    /// The saved tabs could not be read, so it can't be shown to be out of use.
    NoTabs,
}

/// A worktree to remove and, for an automatic removal, the commit its branch must still be on.
pub struct Wanted {
    pub path: PathBuf,
    pub tip: Option<String>,
}

/// One row of the Worktrees list, decided in the core.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Worktree {
    pub path: String,
    pub name: String,
    /// `None` when detached; `commit` names it then.
    pub branch: Option<String>,
    pub commit: Option<String>,
    pub main: bool,
    pub dirty: bool,
    pub missing: bool,
    pub locked: bool,
    pub pr: Option<PrMark>,
    /// Its branch tracked a remote branch that is gone.
    pub gone: bool,
    /// Tabs whose folder or added folder is inside it.
    pub tabs: Vec<TabId>,
    pub stale: bool,
    pub refusal: Option<Refusal>,
    #[serde(skip)]
    head: Option<String>,
}

impl Worktree {
    /// The commit its branch is on.
    pub fn tip(&self) -> Option<&str> {
        self.head.as_deref()
    }
}

/// A tab's folder and the folders added to it, from the saved tabs.
#[derive(Debug, Clone)]
pub struct TabFolders {
    pub tab: TabId,
    pub folders: Vec<PathBuf>,
}

/// What came of removing one worktree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Removed {
    pub path: String,
    pub name: String,
    /// Why it is still there; `None` once removed.
    pub refusal: Option<Refusal>,
    pub branch: Option<String>,
    /// Deleted only when merged, and never on the remote.
    pub branch_deleted: bool,
}

/// The path with the platform's own separators, as a tab's folder is written.
pub fn native(path: &Path) -> PathBuf {
    path.components().collect()
}

// Case is ignored everywhere, so a doubtful match counts as in use rather than free.
fn key(path: &Path) -> Vec<String> {
    path.components()
        .filter(|c| !matches!(c, Component::CurDir))
        .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
        .collect()
}

/// The folder as the file system names it, so a link or a short name matches the path git prints.
/// A part that does not exist yet is kept as written on top of its nearest existing parent.
fn real(path: &Path) -> PathBuf {
    let mut missing = Vec::new();
    let mut at = path;
    loop {
        if let Ok(resolved) = std::fs::canonicalize(at) {
            let mut found = plain(resolved);
            found.extend(missing.iter().rev());
            return found;
        }
        match (at.parent(), at.file_name()) {
            (Some(parent), Some(name)) => {
                missing.push(name.to_os_string());
                at = parent;
            }
            _ => return path.to_path_buf(),
        }
    }
}

// Windows answers `\\?\C:\…`, which would never match a plain `C:\…` of a folder that is gone.
fn plain(resolved: PathBuf) -> PathBuf {
    match resolved.to_str().and_then(|s| s.strip_prefix(r"\\?\")) {
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => PathBuf::from(rest),
        _ => resolved,
    }
}

fn inside(path: &Path, folder: &Path) -> bool {
    key(path).starts_with(&key(folder))
}

fn same(a: &Path, b: &Path) -> bool {
    key(a) == key(b)
}

fn name_of(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.to_string_lossy().into_owned(),
        |n| n.to_string_lossy().into_owned(),
    )
}

/// The branch's PR: an open one if there is one, else its newest; PRs from forks never match.
fn pr_for<'a>(prs: &'a [Pr], branch: &str) -> Option<&'a Pr> {
    let mine = || {
        prs.iter()
            .filter(move |pr| !pr.is_cross_repository && pr.head_ref_name == branch)
    };
    mine()
        .filter(|pr| pr.state == "OPEN")
        .max_by_key(|pr| pr.number)
        .or_else(|| mine().max_by_key(|pr| pr.number))
}

fn refusal(main: bool, locked: bool, dirty: Option<bool>, tabs: &[TabId]) -> Option<Refusal> {
    if main {
        Some(Refusal::Main)
    } else if locked {
        Some(Refusal::Locked)
    } else {
        match dirty {
            Some(true) => Some(Refusal::Dirty),
            None => Some(Refusal::Unread),
            Some(false) if !tabs.is_empty() => Some(Refusal::InUse),
            Some(false) => None,
        }
    }
}

/// Stale: its work is over (PR merged or closed, remote branch gone, or folder gone) and nothing would be lost or interrupted.
fn is_stale(pr: Option<PrState>, gone: bool, missing: bool, refused: bool) -> bool {
    let over = matches!(pr, Some(PrState::Merged | PrState::Closed)) || gone || missing;
    over && !refused
}

fn row(
    main: bool,
    listed: &Listed,
    dirty: Option<bool>,
    branches: &[Branch],
    prs: &[Pr],
    tabs: &[TabFolders],
) -> Worktree {
    let missing = !listed.bare && (listed.prunable || !listed.path.is_dir());
    let pr = listed.branch.as_deref().and_then(|b| pr_for(prs, b));
    let mark = pr.and_then(|pr| {
        pr.shown_state().map(|state| PrMark {
            number: pr.number,
            state,
        })
    });
    let gone = listed
        .branch
        .as_deref()
        .is_some_and(|b| branches.iter().any(|known| known.name == b && known.gone));
    let in_tabs: Vec<TabId> = tabs
        .iter()
        .filter(|t| t.folders.iter().any(|f| inside(f, &real(&listed.path))))
        .map(|t| t.tab)
        .collect();
    let refused = refusal(main, listed.locked, dirty, &in_tabs);
    let path = native(&listed.path);
    Worktree {
        name: name_of(&path),
        path: path.to_string_lossy().into_owned(),
        branch: listed.branch.clone(),
        commit: listed
            .head
            .as_deref()
            .filter(|_| listed.branch.is_none())
            .map(|h| h.chars().take(7).collect()),
        main,
        dirty: dirty == Some(true),
        missing,
        locked: listed.locked,
        pr: mark,
        gone,
        stale: is_stale(mark.map(|m| m.state), gone, missing, refused.is_some()),
        tabs: in_tabs,
        refusal: refused,
        head: listed.head.clone(),
    }
}

/// Every worktree of the folder's repo, the main one first, with what decides whether it can go.
pub fn list(git: &Git, folder: &Path, prs: &[Pr], tabs: &[TabFolders]) -> Option<Vec<Worktree>> {
    let listed = parse(&git.worktrees(folder)?);
    // Resolved once here, so each worktree compares against them without asking the disk again.
    let tabs: Vec<TabFolders> = tabs
        .iter()
        .map(|t| TabFolders {
            tab: t.tab,
            folders: t.folders.iter().map(|f| real(f)).collect(),
        })
        .collect();
    let tabs = tabs.as_slice();
    let branches = git.branches(folder);
    let mut dirty = Vec::with_capacity(listed.len());
    for chunk in listed.chunks(AT_ONCE) {
        std::thread::scope(|scope| {
            let asks: Vec<_> = chunk
                .iter()
                .map(|w| {
                    scope.spawn(move || {
                        // A bare repo has no files, and a gone folder has none left to lose.
                        if w.bare || w.prunable || !w.path.is_dir() {
                            Some(false)
                        } else {
                            git.dirty(&w.path)
                        }
                    })
                })
                .collect();
            dirty.extend(asks.into_iter().map(|ask| ask.join().unwrap_or(None)));
        });
    }
    Some(
        listed
            .iter()
            .zip(dirty)
            .enumerate()
            .map(|(i, (w, dirty))| row(i == 0, w, dirty, &branches, prs, tabs))
            .collect(),
    )
}

/// The repo's main folder as `list` names it, which keys its removal lines.
pub fn main_of(git: &Git, folder: &Path) -> Option<String> {
    let listed = parse(&git.worktrees(folder)?);
    let main = listed.first()?;
    Some(native(&main.path).to_string_lossy().into_owned())
}

/// Merged into the default branch's history, or its PR merged at exactly this commit (a squash merge leaves no history link).
fn merged(
    branch: &str,
    tip: &str,
    base: Option<&str>,
    pr: Option<&Pr>,
    in_base: impl FnOnce(&str) -> bool,
) -> bool {
    let base_branch = base.map(|b| b.strip_prefix("origin/").unwrap_or(b));
    if base_branch == Some(branch) {
        return false;
    }
    let by_pr = pr.is_some_and(|pr| pr.state == "MERGED" && pr.head_ref_oid == tip);
    by_pr || base.is_some_and(in_base)
}

/// Removes each asked-for worktree the list allows, each checked just before it goes; the local branch goes only if merged.
pub fn remove(
    git: &Git,
    folder: &Path,
    wanted: &[Wanted],
    prs: &[Pr],
    tabs: &dyn Fn() -> Option<Vec<TabFolders>>,
) -> Option<Vec<Removed>> {
    // Git runs from the main worktree, never from inside the one it removes.
    let from = PathBuf::from(main_of(git, folder)?);
    let base = git.base(&from);
    let mut done = Vec::new();
    for want in wanted {
        let refused = |refusal: Refusal| Removed {
            path: want.path.to_string_lossy().into_owned(),
            name: name_of(&want.path),
            refusal: Some(refusal),
            branch: None,
            branch_deleted: false,
        };
        // Read again for each one, so a tab opened or a commit made since the last removal counts.
        let Some(tabs_now) = tabs() else {
            done.push(refused(Refusal::NoTabs));
            continue;
        };
        let Some(rows) = list(git, folder, prs, &tabs_now) else {
            done.push(refused(Refusal::Unread));
            continue;
        };
        let want_real = real(&want.path);
        let Some(row) = rows
            .iter()
            .find(|r| same(&real(Path::new(&r.path)), &want_real))
        else {
            done.push(refused(Refusal::Unknown));
            continue;
        };
        let moved = want
            .tip
            .as_deref()
            .is_some_and(|tip| row.head.as_deref() != Some(tip));
        let mut result = Removed {
            path: row.path.clone(),
            name: row.name.clone(),
            refusal: row.refusal.or(moved.then_some(Refusal::Moved)),
            branch: row.branch.clone(),
            branch_deleted: false,
        };
        if result.refusal.is_none() && !git.remove_worktree(&from, Path::new(&row.path)) {
            result.refusal = Some(Refusal::Failed);
        }
        if result.refusal.is_none()
            && let (Some(branch), Some(tip)) = (row.branch.as_deref(), row.head.as_deref())
        {
            result.branch_deleted =
                merged(branch, tip, base.as_deref(), pr_for(prs, branch), |b| {
                    git.contains(&from, b, tip)
                }) && !checked_out(git, &from, branch)
                    && git.delete_branch(&from, branch, tip);
        }
        done.push(result);
    }
    Some(done)
}

// A branch another worktree still has checked out is never deleted; an unreadable list counts as checked out.
fn checked_out(git: &Git, from: &Path, branch: &str) -> bool {
    git.worktrees(from).is_none_or(|out| {
        parse(&out)
            .iter()
            .any(|w| w.branch.as_deref() == Some(branch))
    })
}

/// Each written file's repo top folder, newest first and once each; files outside any repo add none.
pub fn written_roots(
    files: &[String],
    mut root_of: impl FnMut(&Path) -> Option<PathBuf>,
) -> Vec<PathBuf> {
    let mut folders: Vec<&Path> = Vec::new();
    for file in files {
        if let Some(parent) = Path::new(file).parent()
            && !folders.contains(&parent)
        {
            folders.push(parent);
        }
    }
    let mut roots: Vec<PathBuf> = Vec::new();
    for folder in folders {
        if let Some(root) = root_of(folder).map(|r| native(&r))
            && !roots.contains(&root)
        {
            roots.push(root);
        }
    }
    roots
}

/// Which repo each written file's folder is in, asked of git once a minute per folder.
#[derive(Default)]
pub struct Roots {
    known: Mutex<HashMap<PathBuf, (Instant, Option<PathBuf>)>>,
}

impl Roots {
    pub fn get(
        &self,
        folder: &Path,
        now: Instant,
        ask: impl FnOnce() -> Option<PathBuf>,
    ) -> Option<PathBuf> {
        if let Ok(known) = self.known.lock()
            && let Some((at, root)) = known.get(folder)
            && now.saturating_duration_since(*at) < ROOT_EVERY
        {
            return root.clone();
        }
        let root = ask();
        if let Ok(mut known) = self.known.lock() {
            if known.len() >= MOST_ROOTS {
                known.clear();
            }
            known.insert(folder.to_path_buf(), (now, root.clone()));
        }
        root
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::fs;

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

    fn tip(folder: &Path, rev: &str) -> String {
        let out = std::process::Command::new("git")
            .args(["rev-parse", rev])
            .current_dir(folder)
            .output()
            .unwrap();
        String::from_utf8(out.stdout).unwrap().trim().to_owned()
    }

    /// A repo with `main` and a worktree per name beside it, each on its own branch with one commit.
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

    /// Removes as the manual Remove does: no expected tip, the tabs as given.
    fn remove_now(repo: &Path, paths: &[PathBuf], prs: &[Pr], tabs: &[TabFolders]) -> Vec<Removed> {
        let wanted: Vec<Wanted> = paths
            .iter()
            .map(|path| Wanted {
                path: path.clone(),
                tip: None,
            })
            .collect();
        remove(&git(), repo, &wanted, prs, &|| Some(tabs.to_vec())).unwrap()
    }

    fn find<'a>(rows: &'a [Worktree], name: &str) -> &'a Worktree {
        rows.iter()
            .find(|r| r.name == name)
            .unwrap_or_else(|| panic!("no {name} in {rows:?}"))
    }

    #[test]
    fn the_porcelain_list_is_read_with_branches_detached_locked_prunable_and_bare() {
        let out = "worktree C:/code/repo\0HEAD 1111111111\0branch refs/heads/main\0\0\
                   worktree C:/code/feat\0HEAD 2222222222\0branch refs/heads/feat/x\0locked why not\0\0\
                   worktree C:/code/gone\0HEAD 3333333333\0detached\0prunable gitdir file points to non-existent location\0\0\
                   worktree /srv/bare.git\0bare\0\0";

        let listed = parse(out);

        assert_eq!(listed.len(), 4);
        assert_eq!(listed[0].path, PathBuf::from("C:/code/repo"));
        assert_eq!(listed[0].branch.as_deref(), Some("main"));
        assert_eq!(listed[0].head.as_deref(), Some("1111111111"));
        assert_eq!(listed[1].branch.as_deref(), Some("feat/x"));
        assert!(listed[1].locked && !listed[1].prunable);
        assert_eq!(listed[2].branch, None);
        assert!(listed[2].prunable && !listed[2].locked);
        assert!(listed[3].bare);
        assert_eq!(parse(""), []);
    }

    #[test]
    fn a_written_files_repo_is_its_worktree_or_repo_once_and_none_outside_a_repo() {
        let dir = TestDir::new();
        let repo = repo_with(&dir, &["wt"]);
        let deep = dir.path().join("wt").join("src").join("deep");
        fs::create_dir_all(&deep).unwrap();
        fs::create_dir_all(repo.join("docs")).unwrap();
        let loose = dir.path().join("loose");
        fs::create_dir(&loose).unwrap();
        let file = |p: PathBuf| p.to_string_lossy().into_owned();
        let files = [
            file(deep.join("a.rs")),
            file(repo.join("docs").join("b.md")),
            file(loose.join("c.txt")),
            file(dir.path().join("wt").join("d.txt")),
        ];

        let roots = written_roots(&files, |f| git().toplevel(f));

        let canonical = |p: PathBuf| fs::canonicalize(p).unwrap();
        let found: Vec<PathBuf> = roots.into_iter().map(canonical).collect();
        assert_eq!(
            found,
            [canonical(dir.path().join("wt")), canonical(repo)],
            "the worktree, then the main repo, each once, and nothing for the loose file"
        );
    }

    #[test]
    fn a_folders_repo_is_asked_of_git_once_a_minute() {
        let roots = Roots::default();
        let asks = Cell::new(0);
        let ask = || {
            asks.set(asks.get() + 1);
            Some(PathBuf::from("/r"))
        };
        let start = Instant::now();
        let folder = Path::new("/r/src");

        roots.get(folder, start, ask);
        roots.get(folder, start + Duration::from_secs(59), ask);
        assert_eq!(asks.get(), 1);
        roots.get(folder, start + ROOT_EVERY, ask);
        assert_eq!(asks.get(), 2);
    }

    #[test]
    fn the_stale_rule_needs_finished_work_and_nothing_to_lose_or_interrupt() {
        let dir = TestDir::new();
        let names = [
            "merged", "closed", "open", "dirty", "used", "fresh", "pushed",
        ];
        let repo = repo_with(&dir, &names);
        fs::write(dir.path().join("dirty").join("new.txt"), "x").unwrap();
        // "pushed" tracked a remote branch that was then deleted on the remote.
        let remote = dir.path().join("remote.git");
        sh(dir.path(), &["init", "-q", "--bare", "remote.git"]);
        sh(
            &repo,
            &["remote", "add", "origin", remote.to_str().unwrap()],
        );
        sh(
            &dir.path().join("pushed"),
            &["push", "-q", "-u", "origin", "pushed"],
        );
        sh(&repo, &["push", "-q", "origin", "--delete", "pushed"]);
        let prs = [
            pr(1, "MERGED", "merged", ""),
            pr(2, "CLOSED", "closed", ""),
            pr(3, "OPEN", "open", ""),
            pr(4, "MERGED", "open", ""),
            pr(5, "MERGED", "dirty", ""),
            pr(6, "MERGED", "used", ""),
        ];
        let tabs = [TabFolders {
            tab: 9,
            folders: vec![
                dir.path().join("elsewhere"),
                dir.path().join("used").join("src"),
            ],
        }];

        let rows = list(&git(), &repo, &prs, &tabs).unwrap();

        assert_eq!(rows.len(), names.len() + 1);
        assert!(rows[0].main && !rows[0].stale);
        assert_eq!(rows[0].refusal, Some(Refusal::Main));
        let merged = find(&rows, "merged");
        assert!(merged.stale && merged.refusal.is_none());
        assert_eq!(
            merged.pr,
            Some(PrMark {
                number: 1,
                state: PrState::Merged
            })
        );
        assert!(find(&rows, "closed").stale);
        let open = find(&rows, "open");
        assert!(!open.stale, "an open PR wins over an older merged one");
        assert_eq!(open.pr.map(|p| p.state), Some(PrState::Open));
        let dirty = find(&rows, "dirty");
        assert!(!dirty.stale && dirty.dirty);
        assert_eq!(dirty.refusal, Some(Refusal::Dirty));
        let used = find(&rows, "used");
        assert!(!used.stale);
        assert_eq!(
            (used.refusal, used.tabs.as_slice()),
            (Some(Refusal::InUse), &[9][..])
        );
        let fresh = find(&rows, "fresh");
        assert!(!fresh.stale && fresh.refusal.is_none() && fresh.pr.is_none());
        let pushed = find(&rows, "pushed");
        assert!(pushed.gone && pushed.stale);
    }

    #[test]
    fn a_worktree_whose_folder_is_gone_is_stale_and_its_record_removed() {
        let dir = TestDir::new();
        let repo = repo_with(&dir, &["lost"]);
        fs::remove_dir_all(dir.path().join("lost")).unwrap();

        let rows = list(&git(), &repo, &[], &[]).unwrap();
        let lost = find(&rows, "lost");
        assert!(lost.missing && lost.stale);
        let removed = remove_now(&repo, &[PathBuf::from(&lost.path)], &[], &[]);

        assert_eq!(removed[0].refusal, None);
        assert_eq!(list(&git(), &repo, &[], &[]).unwrap().len(), 1);
    }

    #[test]
    fn remove_refuses_dirty_in_use_main_locked_and_unknown_folders_and_keeps_them() {
        let dir = TestDir::new();
        let repo = repo_with(&dir, &["dirty", "used", "locked"]);
        fs::write(dir.path().join("dirty").join("new.txt"), "x").unwrap();
        sh(
            &repo,
            &[
                "worktree",
                "lock",
                dir.path().join("locked").to_str().unwrap(),
            ],
        );
        let tabs = [TabFolders {
            tab: 4,
            folders: vec![dir.path().join("used")],
        }];
        let wanted = [
            dir.path().join("dirty"),
            dir.path().join("used"),
            repo.clone(),
            dir.path().join("locked"),
            dir.path().join("elsewhere"),
        ];

        let removed = remove_now(&repo, &wanted, &[], &tabs);

        let refusals: Vec<Option<Refusal>> = removed.iter().map(|r| r.refusal).collect();
        assert_eq!(
            refusals,
            [
                Some(Refusal::Dirty),
                Some(Refusal::InUse),
                Some(Refusal::Main),
                Some(Refusal::Locked),
                Some(Refusal::Unknown),
            ]
        );
        for kept in &wanted[..4] {
            assert!(kept.is_dir(), "{} was removed", kept.display());
        }
        assert_eq!(list(&git(), &repo, &[], &tabs).unwrap().len(), 4);
    }

    #[test]
    fn the_local_branch_goes_only_when_merged_and_never_on_the_remote() {
        let dir = TestDir::new();
        let repo = repo_with(&dir, &["ancestor", "squashed", "closed", "moved_on"]);
        let remote = dir.path().join("remote.git");
        sh(dir.path(), &["init", "-q", "--bare", "remote.git"]);
        sh(
            &repo,
            &["remote", "add", "origin", remote.to_str().unwrap()],
        );
        sh(&repo, &["push", "-q", "-u", "origin", "main"]);
        sh(
            &dir.path().join("squashed"),
            &["push", "-q", "-u", "origin", "squashed"],
        );
        // "ancestor" is merged into main in history; "squashed" only by its PR at its tip.
        sh(&repo, &["merge", "-q", "--ff-only", "ancestor"]);
        sh(&repo, &["push", "-q", "origin", "main"]);
        sh(&repo, &["fetch", "-q"]);
        let squashed_tip = tip(&dir.path().join("squashed"), "HEAD");
        let moved_old = tip(&dir.path().join("moved_on"), "HEAD");
        // "moved_on" got a commit after its PR merged, so its local branch has unmerged work.
        commit(&dir.path().join("moved_on"), "later.txt");
        let prs = [
            pr(1, "MERGED", "squashed", &squashed_tip),
            pr(2, "CLOSED", "closed", ""),
            pr(3, "MERGED", "moved_on", &moved_old),
        ];
        let wanted: Vec<PathBuf> = ["ancestor", "squashed", "closed", "moved_on"]
            .iter()
            .map(|n| dir.path().join(n))
            .collect();

        let removed = remove_now(&repo, &wanted, &prs, &[]);

        let fates: Vec<(&str, Option<Refusal>, bool)> = removed
            .iter()
            .map(|r| (r.name.as_str(), r.refusal, r.branch_deleted))
            .collect();
        assert_eq!(
            fates,
            [
                ("ancestor", None, true),
                ("squashed", None, true),
                ("closed", None, false),
                ("moved_on", None, false),
            ]
        );
        for gone in &wanted {
            assert!(!gone.exists(), "{} is still there", gone.display());
        }
        let local = git().branches(&repo);
        let names: Vec<&str> = local.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, ["closed", "main", "moved_on"]);
        assert_eq!(
            tip(&remote, "refs/heads/squashed"),
            squashed_tip,
            "the remote branch is untouched"
        );
    }

    #[test]
    fn the_default_branch_and_a_branch_checked_out_elsewhere_are_never_deleted() {
        assert!(!merged("main", "abc", Some("origin/main"), None, |_| true));
        assert!(!merged("master", "abc", Some("master"), None, |_| true));
        assert!(merged("feat", "abc", Some("origin/main"), None, |_| true));
        assert!(!merged("feat", "abc", None, None, |_| true));

        let dir = TestDir::new();
        let repo = repo_with(&dir, &["one"]);
        assert!(checked_out(&git(), &repo, "one"));
        assert!(!checked_out(&git(), &repo, "other"));
    }

    // A junction needs no special rights on Windows, unlike a symlink.
    fn link(target: &Path, link: &Path) {
        #[cfg(windows)]
        {
            let made = std::process::Command::new("cmd")
                .args(["/C", "mklink", "/J"])
                .arg(link)
                .arg(target)
                .output()
                .unwrap();
            assert!(made.status.success(), "{made:?}");
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(target, link).unwrap();
    }

    #[test]
    fn a_tab_or_a_removal_naming_a_worktree_through_a_link_still_finds_it() {
        let dir = TestDir::new();
        let repo = repo_with(&dir, &["linked", "spare"]);
        let alias = dir.path().join("alias");
        let spare = dir.path().join("spare-alias");
        link(&dir.path().join("linked"), &alias);
        link(&dir.path().join("spare"), &spare);
        // A folder not made yet, behind the link, still counts.
        let tabs = [TabFolders {
            tab: 7,
            folders: vec![alias.join("src").join("later")],
        }];

        let rows = list(&git(), &repo, &[], &tabs).unwrap();
        let removed = remove_now(&repo, &[alias, spare], &[], &tabs);

        assert_eq!(find(&rows, "linked").tabs, [7]);
        let refusals: Vec<Option<Refusal>> = removed.iter().map(|r| r.refusal).collect();
        assert_eq!(refusals, [Some(Refusal::InUse), None]);
        assert!(dir.path().join("linked").is_dir());
        assert!(!dir.path().join("spare").exists());
    }

    #[test]
    fn paths_match_across_separators_and_case_for_in_use() {
        assert!(inside(Path::new("C:/Code/Wt/src"), Path::new("c:/code/wt")));
        assert!(!inside(Path::new("C:/code/wt2"), Path::new("C:/code/wt")));
        assert!(same(Path::new("/a/b/./c"), Path::new("/a/b/c")));
    }
}
