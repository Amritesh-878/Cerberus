use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use crate::run::run;

const TIMEOUT: Duration = Duration::from_secs(5);
// Untracked files count by their lines up to these limits, so a stray build folder costs little.
const UNTRACKED_FILES: usize = 200;
const UNTRACKED_BYTES: u64 = 1 << 20;

pub struct Head {
    pub root: PathBuf,
    /// `None` when HEAD is detached; `commit` names it then.
    pub branch: Option<String>,
    pub commit: Option<String>,
    /// `false` before the first commit.
    born: bool,
}

/// A folder's repo as the Changes card shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repo {
    pub root: PathBuf,
    /// `None` when HEAD is detached; `commit` names it then.
    pub branch: Option<String>,
    pub commit: Option<String>,
    /// What the counts are measured against, such as `origin/main`; `None` counts uncommitted work only.
    pub base: Option<String>,
    pub added: u64,
    pub removed: u64,
}

pub struct Git {
    program: PathBuf,
}

impl Git {
    pub fn new(program: PathBuf) -> Self {
        Self { program }
    }

    /// Runs in `folder` with no shell and no prompt; `None` for any failure, which callers read as "not known".
    fn output(&self, folder: &Path, args: &[&str]) -> Option<String> {
        let mut command = Command::new(&self.program);
        command
            .args(args)
            .current_dir(folder)
            // Read-only callers must not take the index lock a commit in a terminal is waiting for.
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env("GIT_TERMINAL_PROMPT", "0");
        let ran = run(command, TIMEOUT).ok()?;
        ran.success
            .then(|| String::from_utf8_lossy(&ran.stdout).trim_end().to_owned())
    }

    /// The repo holding `folder` and its branch, or `None` when it is not in one.
    pub fn head(&self, folder: &Path) -> Option<Head> {
        if let Some(out) = self.output(
            folder,
            &[
                "rev-parse",
                "--show-toplevel",
                "--symbolic-full-name",
                "HEAD",
            ],
        ) {
            let mut lines = out.lines();
            let root = PathBuf::from(lines.next().filter(|r| !r.is_empty())?);
            let branch = lines
                .next()
                .and_then(|full| full.strip_prefix("refs/heads/"))
                .map(str::to_owned);
            let commit = if branch.is_none() {
                self.output(folder, &["rev-parse", "--short", "HEAD"])
            } else {
                None
            };
            return Some(Head {
                root,
                branch,
                commit,
                born: true,
            });
        }
        // A repo with no commits fails the line above but still has a root and a branch name.
        let root = self.output(folder, &["rev-parse", "--show-toplevel"])?;
        Some(Head {
            root: PathBuf::from(root),
            branch: self.output(folder, &["symbolic-ref", "-q", "--short", "HEAD"]),
            commit: None,
            born: false,
        })
    }

    pub fn repo(&self, folder: &Path) -> Option<Repo> {
        let head = self.head(folder)?;
        let base = self.base(folder);
        let against_base = base
            .as_deref()
            .and_then(|b| self.output(folder, &["diff", "--numstat", "--merge-base", b]));
        let (base, diff) = match against_base {
            Some(diff) => (base, diff),
            // No base or no common history: uncommitted work only, against an empty tree before the first commit.
            None => {
                let from = if head.born {
                    "HEAD".to_owned()
                } else {
                    self.output(folder, &["hash-object", "-t", "tree", "--stdin"])?
                };
                (None, self.output(folder, &["diff", "--numstat", &from])?)
            }
        };
        let (mut added, removed) = numstat(&diff);
        added += self.untracked_lines(folder, &head.root);
        Some(Repo {
            root: head.root,
            branch: head.branch,
            commit: head.commit,
            base,
            added,
            removed,
        })
    }

    // The remote's default branch, else a usual name for one, local as a last resort; one git call.
    fn base(&self, folder: &Path) -> Option<String> {
        let listed = self.output(
            folder,
            &[
                "for-each-ref",
                "--format=%(refname)%09%(symref)",
                "refs/remotes/origin/HEAD",
                "refs/remotes/origin/main",
                "refs/remotes/origin/master",
                "refs/heads/main",
                "refs/heads/master",
            ],
        )?;
        let refs: Vec<(&str, &str)> = listed.lines().filter_map(|l| l.split_once('\t')).collect();
        let default = refs
            .iter()
            .find(|(name, target)| *name == "refs/remotes/origin/HEAD" && !target.is_empty())
            .map(|(_, target)| *target);
        default
            .or_else(|| {
                [
                    "refs/remotes/origin/main",
                    "refs/remotes/origin/master",
                    "refs/heads/main",
                    "refs/heads/master",
                ]
                .into_iter()
                .find(|want| refs.iter().any(|(name, _)| name == want))
            })
            .map(|full| {
                full.trim_start_matches("refs/remotes/")
                    .trim_start_matches("refs/heads/")
                    .to_owned()
            })
    }

    fn untracked_lines(&self, folder: &Path, root: &Path) -> u64 {
        let Some(listed) = self.output(
            folder,
            &[
                "ls-files",
                "--others",
                "--exclude-standard",
                "--full-name",
                "-z",
                ":/",
            ],
        ) else {
            return 0;
        };
        listed
            .split('\0')
            .filter(|path| !path.is_empty())
            .take(UNTRACKED_FILES)
            .map(|path| text_lines(&root.join(path)))
            .sum()
    }
}

fn numstat(diff: &str) -> (u64, u64) {
    diff.lines().fold((0, 0), |(added, removed), line| {
        let mut fields = line.split('\t');
        // Binary files show `-` and count as no lines.
        let count = |field: Option<&str>| field.and_then(|f| f.parse::<u64>().ok()).unwrap_or(0);
        (added + count(fields.next()), removed + count(fields.next()))
    })
}

// Binary and oversized files count as none, as git's own numstat counts binaries.
fn text_lines(path: &Path) -> u64 {
    let Ok(file) = File::open(path) else {
        return 0;
    };
    if file
        .metadata()
        .map_or(true, |m| !m.is_file() || m.len() > UNTRACKED_BYTES)
    {
        return 0;
    }
    let mut bytes = Vec::new();
    if file.take(UNTRACKED_BYTES).read_to_end(&mut bytes).is_err() || bytes.contains(&0) {
        return 0;
    }
    let newlines = bytes.iter().filter(|&&b| b == b'\n').count() as u64;
    newlines + u64::from(bytes.last().is_some_and(|&b| b != b'\n'))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::accounts::testdir::TestDir;
    use crate::programs::Finder;

    fn git() -> Git {
        Git::new(Finder::from_env().find("git").expect("git is installed"))
    }

    // Identity and signing come from the command line, so the machine's own git config cannot fail a commit.
    fn sh(folder: &Path, args: &[&str]) {
        let status = Command::new("git")
            .args([
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "-c",
                "commit.gpgsign=false",
                "-c",
                "init.defaultBranch=main",
                "-c",
                "core.autocrlf=false",
            ])
            .args(args)
            .current_dir(folder)
            .output()
            .unwrap();
        assert!(status.status.success(), "git {args:?}: {status:?}");
    }

    fn lines(n: usize, word: &str) -> String {
        (0..n).map(|i| format!("{word} {i}\n")).collect()
    }

    #[test]
    fn a_branch_counts_its_commits_and_uncommitted_work_against_the_remote_default() {
        let dir = TestDir::new();
        let upstream = dir.path().join("upstream");
        fs::create_dir(&upstream).unwrap();
        sh(&upstream, &["init", "-q"]);
        fs::write(upstream.join("app.txt"), lines(10, "base")).unwrap();
        sh(&upstream, &["add", "."]);
        sh(&upstream, &["commit", "-q", "-m", "first"]);
        sh(dir.path(), &["clone", "-q", "upstream", "work"]);
        let work = dir.path().join("work");
        sh(&work, &["checkout", "-q", "-b", "feature"]);
        fs::write(work.join("new.txt"), lines(3, "feature")).unwrap();
        sh(&work, &["add", "."]);
        sh(&work, &["commit", "-q", "-m", "feature"]);
        // The default branch moves on; its commit must not count against the feature.
        fs::write(upstream.join("later.txt"), lines(50, "later")).unwrap();
        sh(&upstream, &["add", "."]);
        sh(&upstream, &["commit", "-q", "-m", "later"]);
        sh(&work, &["fetch", "-q"]);
        // Uncommitted: one line changed, two added untracked, one staged.
        let app = lines(10, "base").replacen("base 0", "edited 0", 1);
        fs::write(work.join("app.txt"), app).unwrap();
        fs::write(work.join("untracked.txt"), "one\ntwo").unwrap();
        fs::write(work.join("staged.txt"), "staged\n").unwrap();
        sh(&work, &["add", "staged.txt"]);
        fs::create_dir(work.join("sub")).unwrap();

        let repo = git().repo(&work.join("sub")).unwrap();

        assert_eq!(repo.branch.as_deref(), Some("feature"));
        assert_eq!(repo.base.as_deref(), Some("origin/main"));
        assert_eq!((repo.added, repo.removed), (3 + 1 + 2 + 1, 1));
        assert_eq!(repo.commit, None);
    }

    #[test]
    fn a_repo_with_no_remote_or_commits_counts_its_files() {
        let dir = TestDir::new();
        sh(dir.path(), &["init", "-q"]);
        fs::write(dir.path().join("a.txt"), lines(4, "a")).unwrap();
        fs::write(dir.path().join("bin.dat"), [0u8, 1, 2, b'\n']).unwrap();

        let repo = git().repo(dir.path()).unwrap();

        assert_eq!(repo.branch.as_deref(), Some("main"));
        assert_eq!(repo.base, None);
        assert_eq!((repo.added, repo.removed), (4, 0));
    }

    #[test]
    fn a_detached_head_names_its_commit() {
        let dir = TestDir::new();
        sh(dir.path(), &["init", "-q"]);
        fs::write(dir.path().join("a.txt"), "a\n").unwrap();
        sh(dir.path(), &["add", "."]);
        sh(dir.path(), &["commit", "-q", "-m", "a"]);
        sh(dir.path(), &["checkout", "-q", "--detach"]);

        let repo = git().repo(dir.path()).unwrap();

        assert_eq!(repo.branch, None);
        assert!(repo.commit.is_some_and(|c| c.len() >= 7));
        assert_eq!((repo.added, repo.removed), (0, 0));
    }

    #[test]
    fn a_folder_outside_any_repo_is_none() {
        let dir = TestDir::new();

        assert_eq!(git().repo(dir.path()), None);
        assert!(git().head(dir.path()).is_none());
    }

    #[test]
    fn numstat_sums_text_and_skips_binaries() {
        assert_eq!(
            numstat("3\t1\ta.txt\n-\t-\tlogo.png\n10\t0\tb => c\n"),
            (13, 1)
        );
        assert_eq!(numstat(""), (0, 0));
    }
}
