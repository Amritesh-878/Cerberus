use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::run::run;

const TIMEOUT: Duration = Duration::from_secs(15);
const READY_TIMEOUT: Duration = Duration::from_secs(30);
/// GitHub is asked about a repo at most this often, however many tabs or refreshes want it.
pub const EVERY: Duration = Duration::from_secs(60);
// Merged and closed history is capped; open PRs and drafts are asked for on their own, so none is ever cut off.
const MOST_PRS: usize = 100;

/// One PR as `gh pr list --json` gives it; other fields are ignored.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pr {
    pub number: u64,
    pub title: String,
    pub url: String,
    pub state: String,
    pub is_draft: bool,
    pub updated_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PrState {
    Draft,
    Open,
    Merged,
    Closed,
}

/// What the webview shows; the URL stays in the core, which opens or copies it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PrRow {
    pub number: u64,
    pub title: String,
    pub state: PrState,
    /// `owner/repo#n`, which a dismissal is saved under.
    pub key: String,
    /// Changes whenever the PR does, so a dismissed one comes back.
    pub updated: String,
}

impl PrRow {
    fn from(pr: &Pr) -> Option<Self> {
        let state = match (pr.state.as_str(), pr.is_draft) {
            ("OPEN", true) => PrState::Draft,
            ("OPEN", false) => PrState::Open,
            ("MERGED", _) => PrState::Merged,
            ("CLOSED", _) => PrState::Closed,
            _ => return None,
        };
        Some(Self {
            number: pr.number,
            title: pr.title.clone(),
            state,
            key: format!("{}#{}", repo_of(&pr.url)?, pr.number),
            updated: pr.updated_at.clone(),
        })
    }
}

/// `owner/repo` from `https://<host>/<owner>/<repo>/pull/<n>`.
fn repo_of(url: &str) -> Option<String> {
    let mut parts = url.strip_prefix("https://")?.split('/');
    let (_host, owner, repo, pull) = (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
    (pull == "pull" && !owner.is_empty() && !repo.is_empty()).then(|| format!("{owner}/{repo}"))
}

struct Asked {
    at: Instant,
    prs: Vec<Pr>,
    /// PRs made ready here and when, so a list asked for before that cannot show them as drafts again.
    readied: Vec<(u64, Instant)>,
}

/// The last list per repo, so `gh` runs once a minute per repo at most.
#[derive(Default)]
pub struct PrCache {
    repos: Mutex<HashMap<PathBuf, Asked>>,
}

impl PrCache {
    /// The repo's PRs; within a minute of the last ask the kept list, and a failed ask keeps the last one.
    pub fn get(&self, root: &Path, now: Instant, ask: impl FnOnce() -> Option<Vec<Pr>>) -> Vec<Pr> {
        {
            let Ok(mut repos) = self.repos.lock() else {
                return Vec::new();
            };
            if let Some(last) = repos.get(root)
                && now.saturating_duration_since(last.at) < EVERY
            {
                return last.prs.clone();
            }
            // Taken before asking, so a second caller meanwhile waits for the next minute.
            repos
                .entry(root.to_path_buf())
                .or_insert(Asked {
                    at: now,
                    prs: Vec::new(),
                    readied: Vec::new(),
                })
                .at = now;
        }
        let asked = ask();
        let Ok(mut repos) = self.repos.lock() else {
            return asked.unwrap_or_default();
        };
        let Some(last) = repos.get_mut(root) else {
            return asked.unwrap_or_default();
        };
        if let Some(mut prs) = asked
            && last.at == now
        {
            for pr in &mut prs {
                if last
                    .readied
                    .iter()
                    .any(|&(number, at)| number == pr.number && at > now)
                {
                    pr.is_draft = false;
                }
            }
            last.prs = prs;
            // A list asked for after a Ready already shows it.
            last.readied.retain(|&(_, at)| at > now);
        }
        last.prs.clone()
    }

    fn find<T>(&self, root: &Path, number: u64, read: impl FnOnce(&mut Pr) -> T) -> Option<T> {
        let mut repos = self.repos.lock().ok()?;
        let pr = repos
            .get_mut(root)?
            .prs
            .iter_mut()
            .find(|pr| pr.number == number)?;
        Some(read(pr))
    }

    pub fn url(&self, root: &Path, number: u64) -> Option<String> {
        self.find(root, number, |pr| pr.url.clone())
    }

    pub fn is_draft(&self, root: &Path, number: u64) -> bool {
        self.find(root, number, |pr| pr.state == "OPEN" && pr.is_draft)
            .unwrap_or(false)
    }

    /// Shows the PR as open at once, without asking GitHub again before the minute is up.
    pub fn made_ready(&self, root: &Path, number: u64, at: Instant) {
        let Ok(mut repos) = self.repos.lock() else {
            return;
        };
        let Some(last) = repos.get_mut(root) else {
            return;
        };
        last.readied.push((number, at));
        if let Some(pr) = last.prs.iter_mut().find(|pr| pr.number == number) {
            pr.is_draft = false;
        }
    }
}

fn gh_command(gh: &Path, folder: &Path) -> Command {
    let mut command = Command::new(gh);
    command
        .current_dir(folder)
        .env("GH_PROMPT_DISABLED", "1")
        .env("GH_NO_UPDATE_NOTIFIER", "1")
        .env("NO_COLOR", "1");
    command
}

fn list_state(gh: &Path, folder: &Path, state: &str) -> Option<Vec<Pr>> {
    let mut command = gh_command(gh, folder);
    command.args([
        "pr",
        "list",
        &format!("--state={state}"),
        "--json=number,title,url,state,isDraft,updatedAt",
        &format!("--limit={MOST_PRS}"),
    ]);
    let ran = run(command, TIMEOUT).ok().filter(|ran| ran.success)?;
    parse(&ran.stdout)
}

/// Every open PR and draft, and the newest merged and closed ones; `None` when `gh` is missing, signed out or offline.
pub fn list(gh: &Path, folder: &Path) -> Option<Vec<Pr>> {
    let recent = list_state(gh, folder, "all")?;
    let open = list_state(gh, folder, "open")?;
    Some(merged(recent, open))
}

/// Both lists as one, newest first, each PR once.
fn merged(recent: Vec<Pr>, open: Vec<Pr>) -> Vec<Pr> {
    let mut all = recent;
    for pr in open {
        if !all.iter().any(|known| known.number == pr.number) {
            all.push(pr);
        }
    }
    all.sort_by_key(|pr| std::cmp::Reverse(pr.number));
    all
}

/// Marks a draft ready for review; the number is one argument, never part of a command line.
pub fn ready(gh: &Path, folder: &Path, number: u64) -> bool {
    let mut command = gh_command(gh, folder);
    command.args(["pr", "ready", &number.to_string()]);
    run(command, READY_TIMEOUT).is_ok_and(|ran| ran.success)
}

/// Newest first; PRs whose link is not an https GitHub pull link are dropped.
fn parse(stdout: &[u8]) -> Option<Vec<Pr>> {
    let mut prs: Vec<Pr> = serde_json::from_slice(stdout).ok()?;
    prs.retain(|pr| PrRow::from(pr).is_some());
    prs.sort_by_key(|pr| std::cmp::Reverse(pr.number));
    Some(prs)
}

pub fn rows(prs: &[Pr]) -> Vec<PrRow> {
    prs.iter().filter_map(PrRow::from).collect()
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    fn pr(number: u64, state: &str, is_draft: bool) -> Pr {
        Pr {
            number,
            title: format!("PR {number}"),
            url: format!("https://github.com/example/repo/pull/{number}"),
            state: state.to_owned(),
            is_draft,
            updated_at: "2026-01-02T03:04:05Z".to_owned(),
        }
    }

    #[test]
    fn gh_runs_once_a_minute_per_repo() {
        let cache = PrCache::default();
        let asks = Cell::new(0);
        let ask = |n| {
            asks.set(asks.get() + 1);
            Some(vec![pr(n, "OPEN", false)])
        };
        let start = Instant::now();
        let repo = Path::new("/r/one");

        assert_eq!(cache.get(repo, start, || ask(1))[0].number, 1);
        let soon = start + Duration::from_secs(59);
        assert_eq!(cache.get(repo, soon, || ask(2))[0].number, 1);
        // Another repo has its own minute.
        assert_eq!(cache.get(Path::new("/r/two"), soon, || ask(3))[0].number, 3);
        let later = start + EVERY;
        assert_eq!(cache.get(repo, later, || ask(4))[0].number, 4);
        assert_eq!(asks.get(), 3);
    }

    #[test]
    fn a_failed_ask_keeps_the_last_list() {
        let cache = PrCache::default();
        let start = Instant::now();
        let repo = Path::new("/r/one");
        cache.get(repo, start, || Some(vec![pr(1, "OPEN", false)]));

        let after = cache.get(repo, start + EVERY, || None);

        assert_eq!(after, [pr(1, "OPEN", false)]);
    }

    #[test]
    fn the_list_is_read_strictly_newest_first_with_every_state() {
        let out = br#"[
            {"number":3,"title":"merged","url":"https://github.com/a/b/pull/3","state":"MERGED","isDraft":false,"updatedAt":"u3","x":1},
            {"number":9,"title":"draft","url":"https://github.com/a/b/pull/9","state":"OPEN","isDraft":true,"updatedAt":"u9"},
            {"number":5,"title":"open","url":"https://github.com/a/b/pull/5","state":"OPEN","isDraft":false,"updatedAt":"u5"},
            {"number":4,"title":"closed","url":"https://github.com/a/b/pull/4","state":"CLOSED","isDraft":false,"updatedAt":"u4"},
            {"number":7,"title":"odd link","url":"file:///c:/x","state":"OPEN","isDraft":false,"updatedAt":"u7"},
            {"number":8,"title":"odd state","url":"https://github.com/a/b/pull/8","state":"LOCKED","isDraft":false,"updatedAt":"u8"}
        ]"#;

        let rows = rows(&parse(out).unwrap());

        let shown: Vec<_> = rows.iter().map(|r| (r.number, r.state)).collect();
        assert_eq!(
            shown,
            [
                (9, PrState::Draft),
                (5, PrState::Open),
                (4, PrState::Closed),
                (3, PrState::Merged)
            ]
        );
        assert_eq!(rows[0].key, "a/b#9");
        assert_eq!(rows[0].updated, "u9");
        assert_eq!(parse(b"not json"), None);
        assert_eq!(parse(b"[]"), Some(Vec::new()));
    }

    #[test]
    fn the_url_is_kept_for_opening_and_never_sent_with_the_row() {
        let cache = PrCache::default();
        let repo = Path::new("/r/one");
        let found = cache.get(repo, Instant::now(), || Some(vec![pr(7, "OPEN", true)]));

        let row = serde_json::to_string(&rows(&found)).unwrap();

        assert!(!row.contains("https://"), "{row}");
        assert_eq!(
            cache.url(repo, 7).as_deref(),
            Some("https://github.com/example/repo/pull/7")
        );
        assert_eq!(cache.url(repo, 8), None);
    }

    #[test]
    fn only_a_known_draft_can_be_made_ready_and_then_shows_open() {
        let cache = PrCache::default();
        let repo = Path::new("/r/one");
        let now = Instant::now();
        cache.get(repo, now, || {
            Some(vec![
                pr(7, "OPEN", true),
                pr(6, "OPEN", false),
                pr(5, "MERGED", true),
            ])
        });

        assert!(cache.is_draft(repo, 7));
        assert!(!cache.is_draft(repo, 6));
        assert!(!cache.is_draft(repo, 5));
        assert!(!cache.is_draft(repo, 99));
        cache.made_ready(repo, 7, now);
        let row = &rows(&cache.get(repo, now + Duration::from_secs(1), || None))[0];
        assert_eq!(row.state, PrState::Open);
    }

    #[test]
    fn a_missing_gh_is_no_list_and_no_ready() {
        assert_eq!(list(Path::new("/no/such/gh"), Path::new(".")), None);
        assert!(!ready(Path::new("/no/such/gh"), Path::new("."), 1));
    }

    #[test]
    fn a_list_asked_for_before_a_ready_cannot_show_it_as_a_draft_again() {
        let cache = PrCache::default();
        let repo = Path::new("/r/one");
        let start = Instant::now();
        cache.get(repo, start, || Some(vec![pr(7, "OPEN", true)]));
        let asked = start + EVERY;

        // The next list is asked for, then Ready lands before that stale answer does.
        let shown = cache.get(repo, asked, || {
            cache.made_ready(repo, 7, asked + Duration::from_secs(1));
            Some(vec![pr(7, "OPEN", true)])
        });

        assert!(!shown[0].is_draft);
        assert!(!cache.is_draft(repo, 7));
        // A list asked for after the Ready is trusted as it is.
        let later = cache.get(repo, asked + EVERY, || Some(vec![pr(7, "OPEN", true)]));
        assert!(later[0].is_draft);
    }
    #[test]
    fn an_old_open_pr_beyond_the_recent_list_is_still_listed_once() {
        let recent = vec![pr(90, "MERGED", false), pr(80, "OPEN", false)];
        let open = vec![pr(80, "OPEN", false), pr(3, "OPEN", true)];

        let numbers: Vec<_> = merged(recent, open).iter().map(|p| p.number).collect();

        assert_eq!(numbers, [90, 80, 3]);
    }
}
