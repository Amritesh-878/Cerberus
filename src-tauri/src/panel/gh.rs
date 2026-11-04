use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::run::run;

const TIMEOUT: Duration = Duration::from_secs(15);
/// GitHub is asked about a repo at most this often, however many tabs or refreshes want it.
pub const EVERY: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Pr {
    pub number: u64,
    pub title: String,
    pub url: String,
}

/// What the webview shows; the URL stays in the core, which opens it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PrRow {
    pub number: u64,
    pub title: String,
}

impl From<&Pr> for PrRow {
    fn from(pr: &Pr) -> Self {
        Self {
            number: pr.number,
            title: pr.title.clone(),
        }
    }
}

struct Asked {
    at: Instant,
    branch: String,
    pr: Option<Pr>,
}

/// The last answer per repo, so `gh` runs once a minute per repo at most.
#[derive(Default)]
pub struct PrCache {
    repos: Mutex<HashMap<PathBuf, Asked>>,
}

impl PrCache {
    /// The branch's open PR; within a minute of the last ask, the cached answer, or none for another branch.
    pub fn get(
        &self,
        root: &Path,
        branch: &str,
        now: Instant,
        ask: impl FnOnce() -> Option<Pr>,
    ) -> Option<Pr> {
        {
            let mut repos = self.repos.lock().ok()?;
            if let Some(last) = repos.get(root)
                && now.saturating_duration_since(last.at) < EVERY
            {
                return (last.branch == branch).then(|| last.pr.clone()).flatten();
            }
            // Taken before asking, so a second caller meanwhile waits for the next minute.
            repos.insert(
                root.to_path_buf(),
                Asked {
                    at: now,
                    branch: branch.to_owned(),
                    pr: None,
                },
            );
        }
        let pr = ask();
        if let Ok(mut repos) = self.repos.lock()
            && let Some(last) = repos.get_mut(root)
            && last.at == now
        {
            last.pr.clone_from(&pr);
        }
        pr
    }

    pub fn url(&self, root: &Path) -> Option<String> {
        let repos = self.repos.lock().ok()?;
        repos.get(root)?.pr.as_ref().map(|pr| pr.url.clone())
    }
}

/// The open PR whose head is `branch`; `None` when `gh` is missing, signed out, offline or finds none.
pub fn open_pr(gh: &Path, folder: &Path, branch: &str) -> Option<Pr> {
    let mut command = Command::new(gh);
    command
        .args([
            "pr",
            "list",
            &format!("--head={branch}"),
            "--state=open",
            "--json=number,title,url",
            "--limit=1",
        ])
        .current_dir(folder)
        .env("GH_PROMPT_DISABLED", "1")
        .env("GH_NO_UPDATE_NOTIFIER", "1")
        .env("NO_COLOR", "1");
    let ran = run(command, TIMEOUT).ok().filter(|ran| ran.success)?;
    parse(&ran.stdout)
}

fn parse(stdout: &[u8]) -> Option<Pr> {
    let prs: Vec<Pr> = serde_json::from_slice(stdout).ok()?;
    prs.into_iter().find(|pr| pr.url.starts_with("https://"))
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    fn pr(number: u64) -> Pr {
        Pr {
            number,
            title: format!("PR {number}"),
            url: format!("https://github.com/example/repo/pull/{number}"),
        }
    }

    #[test]
    fn gh_runs_once_a_minute_per_repo() {
        let cache = PrCache::default();
        let asks = Cell::new(0);
        let ask = |n| {
            asks.set(asks.get() + 1);
            Some(pr(n))
        };
        let start = Instant::now();
        let repo = Path::new("/r/one");

        assert_eq!(cache.get(repo, "feature", start, || ask(1)), Some(pr(1)));
        let soon = start + Duration::from_secs(59);
        assert_eq!(cache.get(repo, "feature", soon, || ask(2)), Some(pr(1)));
        // Another repo has its own minute.
        assert_eq!(
            cache.get(Path::new("/r/two"), "x", soon, || ask(3)),
            Some(pr(3))
        );
        let later = start + EVERY;
        assert_eq!(cache.get(repo, "feature", later, || ask(4)), Some(pr(4)));
        assert_eq!(asks.get(), 3);
    }

    #[test]
    fn a_branch_switched_within_the_minute_shows_no_pr_until_it_is_asked_about() {
        let cache = PrCache::default();
        let start = Instant::now();
        let repo = Path::new("/r/one");
        cache.get(repo, "feature", start, || Some(pr(1)));

        let switched = cache.get(repo, "other", start + Duration::from_secs(5), || {
            panic!("gh must wait for the minute")
        });

        assert_eq!(switched, None);
        assert_eq!(
            cache.get(repo, "other", start + EVERY, || Some(pr(2))),
            Some(pr(2))
        );
    }

    #[test]
    fn the_url_is_kept_for_opening_and_never_sent_with_the_row() {
        let cache = PrCache::default();
        let repo = Path::new("/r/one");
        let found = cache.get(repo, "feature", Instant::now(), || Some(pr(7)));

        let row = serde_json::to_string(&PrRow::from(&found.unwrap())).unwrap();

        assert_eq!(row, r#"{"number":7,"title":"PR 7"}"#);
        assert_eq!(
            cache.url(repo).as_deref(),
            Some("https://github.com/example/repo/pull/7")
        );
    }

    #[test]
    fn gh_output_is_read_strictly() {
        assert_eq!(
            parse(br#"[{"number":3,"title":"t","url":"https://github.com/a/b/pull/3","x":1}]"#),
            Some(Pr {
                number: 3,
                title: "t".to_owned(),
                url: "https://github.com/a/b/pull/3".to_owned()
            })
        );
        assert_eq!(parse(b"[]"), None);
        assert_eq!(parse(b"not json"), None);
        assert_eq!(
            parse(br#"[{"number":3,"title":"t","url":"file:///c:/x"}]"#),
            None
        );
    }

    #[test]
    fn a_missing_gh_is_no_pr() {
        assert_eq!(
            open_pr(Path::new("/no/such/gh"), Path::new("."), "main"),
            None
        );
    }
}
