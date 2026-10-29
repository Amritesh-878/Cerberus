mod db;

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use rusqlite::ErrorCode;
use serde::{Deserialize, Serialize, Serializer};

use crate::accounts::AccountId;
use crate::saved::db::Db;
use crate::sessions::is_session_id;

pub type TabId = u32;
pub type ProjectId = u32;

// Saves wait for a quiet moment, but never longer than the cap, so a burst of changes is one write.
const QUIET: Duration = Duration::from_millis(300);
const MOST_DELAY: Duration = Duration::from_secs(2);
const RETRY: Duration = Duration::from_secs(2);

#[derive(Debug, thiserror::Error)]
pub enum TreeError {
    #[error("the saved tabs file could not be read or written: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("could not create the saved tabs folder: {0}")]
    Io(#[from] io::Error),
    #[error("the saved tabs file is version {0}, which this app does not know")]
    UnknownVersion(i64),
    #[error("the tab tree is not valid: {0}")]
    Invalid(String),
    #[error("saved tabs are unavailable: {0}")]
    Unavailable(String),
    #[error("another Cerberus window has the saved tabs open")]
    InUse,
}

impl TreeError {
    /// A file that is not a database, or holds a tree that breaks the rules; a newer or busy file is not corrupt.
    fn is_corrupt(&self) -> bool {
        match self {
            Self::Invalid(_) => true,
            Self::Db(e) => matches!(
                e.sqlite_error_code(),
                Some(ErrorCode::NotADatabase | ErrorCode::DatabaseCorrupt)
            ),
            _ => false,
        }
    }
}

impl Serialize for TreeError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedProject {
    pub id: ProjectId,
    pub name: String,
    pub folder: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedTab {
    pub id: TabId,
    pub project_id: ProjectId,
    pub parent_id: Option<TabId>,
    pub title: String,
    /// The program id from `list_programs`, so a moved install is found again on restore.
    pub program: String,
    pub cwd: Option<String>,
    pub account: Option<AccountId>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedTree {
    pub projects: Vec<SavedProject>,
    pub tabs: Vec<SavedTab>,
    pub next_id: u32,
}

impl SavedTree {
    fn validate(&self) -> Result<(), TreeError> {
        let invalid = |reason: String| Err(TreeError::Invalid(reason));
        let mut ids = HashSet::new();
        let projects: HashSet<_> = self.projects.iter().map(|p| p.id).collect();
        for project in &self.projects {
            if !ids.insert(project.id) {
                return invalid(format!("id {} is used twice", project.id));
            }
            if project.name.trim().is_empty() || project.folder.is_empty() {
                return invalid(format!("project {} has no name or folder", project.id));
            }
        }
        let tabs: HashMap<_, _> = self.tabs.iter().map(|t| (t.id, t)).collect();
        for tab in &self.tabs {
            if !ids.insert(tab.id) {
                return invalid(format!("id {} is used twice", tab.id));
            }
            if !projects.contains(&tab.project_id) {
                return invalid(format!("tab {} has no project", tab.id));
            }
            if tab.title.trim().is_empty() || tab.program.is_empty() {
                return invalid(format!("tab {} has no title or program", tab.id));
            }
            if let Some(parent) = tab.parent_id {
                let fits = tabs.get(&parent).is_some_and(|p| {
                    p.id != tab.id && p.parent_id.is_none() && p.project_id == tab.project_id
                });
                if !fits {
                    return invalid(format!(
                        "tab {} sits under a tab that cannot hold it",
                        tab.id
                    ));
                }
            }
        }
        if ids.iter().any(|&id| id >= self.next_id) {
            return invalid("an id is not below the next id".to_owned());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Snapshot {
    tree: SavedTree,
    sessions: HashMap<TabId, String>,
}

enum Message {
    Changed,
    Flush(Sender<()>),
}

/// Where the snapshot is written; a trait so a failing disk can be faked in tests.
trait Store: Send + 'static {
    fn write(&mut self, snapshot: &Snapshot) -> Result<(), TreeError>;
}

impl Store for Db {
    fn write(&mut self, snapshot: &Snapshot) -> Result<(), TreeError> {
        Db::write(self, snapshot)
    }
}

/// The saved tree, and what to tell the user when an unreadable file was set aside to get it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Loaded {
    pub tree: SavedTree,
    pub notice: Option<String>,
}

struct Live {
    snapshot: Arc<Mutex<Snapshot>>,
    notice: Option<String>,
    saver: Mutex<Sender<Message>>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

#[derive(Clone)]
pub struct Saved(Arc<Result<Live, String>>);

// A panicked holder leaves the snapshot whole, so poisoning is not a reason to fail.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Saved {
    /// A newer or busy file leaves saving off, so it is never written over; a corrupt one is set aside.
    pub fn open(path: &Path, report: impl Fn(Option<&str>) + Send + 'static) -> Self {
        let load = || Db::open(path).and_then(|db| Ok((db.load()?, db)));
        let opened = match load() {
            Err(e) if e.is_corrupt() => {
                eprintln!("set aside an unreadable tabs file: {e}");
                set_aside(path).map_err(TreeError::from).and_then(|kept| {
                    let notice = format!(
                        "The saved tabs could not be read, so Cerberus started with none. The old file is kept as {kept}."
                    );
                    load().map(|(snapshot, db)| (snapshot, db, Some(notice)))
                })
            }
            other => other.map(|(snapshot, db)| (snapshot, db, None)),
        };
        Self(Arc::new(
            opened
                .map(|(snapshot, db, notice)| Self::start(snapshot, db, notice, report))
                .map_err(|e| e.to_string()),
        ))
    }

    fn start(
        snapshot: Snapshot,
        store: impl Store,
        notice: Option<String>,
        report: impl Fn(Option<&str>) + Send + 'static,
    ) -> Live {
        let snapshot = Arc::new(Mutex::new(snapshot));
        let (tx, rx) = mpsc::channel();
        let shared = Arc::clone(&snapshot);
        let written = lock(&snapshot).clone();
        let thread = thread::spawn(move || save_loop(store, &shared, written, &rx, report));
        Live {
            snapshot,
            notice,
            saver: Mutex::new(tx),
            thread: Mutex::new(Some(thread)),
        }
    }

    fn live(&self) -> Result<&Live, TreeError> {
        self.0
            .as_ref()
            .as_ref()
            .map_err(|e| TreeError::Unavailable(e.clone()))
    }

    pub fn load(&self) -> Result<Loaded, TreeError> {
        let live = self.live()?;
        Ok(Loaded {
            tree: lock(&live.snapshot).tree.clone(),
            notice: live.notice.clone(),
        })
    }

    pub fn save(&self, tree: SavedTree) -> Result<(), TreeError> {
        tree.validate()?;
        let live = self.live()?;
        {
            let mut snapshot = lock(&live.snapshot);
            let kept: HashSet<_> = tree.tabs.iter().map(|t| t.id).collect();
            snapshot.sessions.retain(|id, _| kept.contains(id));
            snapshot.tree = tree;
        }
        live.changed();
        Ok(())
    }

    /// Follows a tab onto a new chat, as after `/clear`; ids that could not be a session are ignored.
    pub fn set_session(&self, tab: TabId, session: &str) {
        let Ok(live) = self.live() else { return };
        if !is_session_id(session) {
            return;
        }
        let changed = {
            let mut snapshot = lock(&live.snapshot);
            snapshot.tree.tabs.iter().any(|t| t.id == tab)
                && snapshot.sessions.insert(tab, session.to_owned()).as_deref() != Some(session)
        };
        if changed {
            live.changed();
        }
    }

    pub fn session_of(&self, tab: TabId) -> Option<String> {
        let live = self.live().ok()?;
        lock(&live.snapshot).sessions.get(&tab).cloned()
    }

    pub fn flush(&self) {
        let Ok(live) = self.live() else { return };
        let (tx, rx) = mpsc::channel();
        if lock(&live.saver).send(Message::Flush(tx)).is_ok() {
            let _ = rx.recv_timeout(Duration::from_secs(5));
        }
    }
}

impl Live {
    fn changed(&self) {
        // Fails only once the saver thread is gone, and then nothing is left to tell.
        let _ = lock(&self.saver).send(Message::Changed);
    }
}

// Ends the writer and waits for it, so the file and its lock are free once this is dropped.
impl Drop for Live {
    fn drop(&mut self) {
        drop(std::mem::replace(
            &mut *lock(&self.saver),
            mpsc::channel().0,
        ));
        if let Some(thread) = lock(&self.thread).take() {
            let _ = thread.join();
        }
    }
}

/// Renames the file, and its journal, to the first free `.bad`, `.bad2`, ... name and returns that name.
fn set_aside(path: &Path) -> io::Result<String> {
    let name = |n: u32| -> PathBuf {
        let suffix = if n == 1 {
            "bad".to_owned()
        } else {
            format!("bad{n}")
        };
        path.with_extension(format!("db.{suffix}"))
    };
    let kept = (1..1000)
        .map(name)
        .find(|candidate| !candidate.exists())
        .ok_or_else(|| io::Error::other("no free name to set the tabs file aside"))?;
    fs::rename(path, &kept)?;
    let mut journal = path.as_os_str().to_owned();
    journal.push("-journal");
    let mut kept_journal = kept.as_os_str().to_owned();
    kept_journal.push("-journal");
    match fs::rename(&journal, &kept_journal) {
        Err(e) if e.kind() != ErrorKind::NotFound => return Err(e),
        _ => {}
    }
    Ok(kept
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned()))
}

fn settle(rx: &Receiver<Message>, waiting: &mut Vec<Sender<()>>) {
    let deadline = Instant::now() + MOST_DELAY;
    loop {
        let wait = QUIET.min(deadline.saturating_duration_since(Instant::now()));
        match rx.recv_timeout(wait) {
            Ok(Message::Changed) => {}
            Ok(Message::Flush(reply)) => {
                waiting.push(reply);
                return;
            }
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => return,
        }
    }
}

fn save_loop(
    mut store: impl Store,
    snapshot: &Mutex<Snapshot>,
    mut written: Snapshot,
    rx: &Receiver<Message>,
    report: impl Fn(Option<&str>),
) {
    let mut failure: Option<String> = None;
    loop {
        let next = if failure.is_some() {
            rx.recv_timeout(RETRY)
        } else {
            rx.recv().map_err(|_| RecvTimeoutError::Disconnected)
        };
        let mut waiting = Vec::new();
        match next {
            Ok(Message::Flush(reply)) => waiting.push(reply),
            Ok(Message::Changed) => settle(rx, &mut waiting),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        let current = lock(snapshot).clone();
        // A tree changed back to what is on disk needs no write, and any earlier failure is over.
        let outcome = if current == written {
            None
        } else {
            match store.write(&current) {
                Ok(()) => {
                    written = current;
                    None
                }
                Err(e) => {
                    eprintln!("could not save tabs, retrying: {e}");
                    Some(e.to_string())
                }
            }
        };
        if outcome != failure {
            report(outcome.as_deref());
            failure = outcome;
        }
        for reply in waiting {
            let _ = reply.send(());
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::accounts::testdir::TestDir;

    pub const SESSION: &str = "0f6c3a52-7d1e-4b8a-9c2f-5e4d3b2a1f00";
    const OTHER_SESSION: &str = "a1b2c3d4-0000-4000-8000-000000000001";

    pub fn project(id: ProjectId, name: &str) -> SavedProject {
        SavedProject {
            id,
            name: name.to_owned(),
            folder: format!(r"C:\work\{name}"),
        }
    }

    pub fn tab(
        id: TabId,
        project_id: ProjectId,
        parent_id: Option<TabId>,
        program: &str,
    ) -> SavedTab {
        SavedTab {
            id,
            project_id,
            parent_id,
            title: format!("tab {id}"),
            program: program.to_owned(),
            cwd: Some(r"C:\work\app".to_owned()),
            account: Some(2),
        }
    }

    /// Two projects, five tabs: three Claude, two shells, one a sub-tab.
    pub fn two_projects() -> SavedTree {
        SavedTree {
            projects: vec![project(1, "app"), project(2, "site")],
            tabs: vec![
                tab(3, 1, None, "claude"),
                tab(4, 1, Some(3), "claude"),
                tab(5, 1, None, "powershell"),
                tab(6, 2, None, "claude"),
                tab(7, 2, None, "git-bash"),
            ],
            next_id: 8,
        }
    }

    #[test]
    fn saves_reach_the_file_and_a_restart_reads_them_back() {
        let dir = TestDir::new();
        let path = dir.path().join("tree.db");
        let saved = Saved::open(&path, |_| {});

        saved.save(two_projects()).unwrap();
        saved.set_session(6, SESSION);
        saved.flush();
        drop(saved);
        let restarted = Saved::open(&path, |_| {});

        assert_eq!(
            restarted.load().unwrap(),
            Loaded {
                tree: two_projects(),
                notice: None
            }
        );
        assert_eq!(restarted.session_of(6).as_deref(), Some(SESSION));
        assert_eq!(restarted.session_of(3), None);
    }

    #[test]
    fn a_corrupt_file_is_set_aside_and_saving_starts_fresh() {
        let dir = TestDir::new();
        let path = dir.path().join("tree.db");
        fs::write(&path, b"not a database, just fake bytes").unwrap();

        let saved = Saved::open(&path, |_| {});
        let loaded = saved.load().unwrap();
        saved.save(two_projects()).unwrap();
        saved.flush();
        drop(saved);

        assert_eq!(loaded.tree.tabs, []);
        assert!(loaded.notice.is_some_and(|n| n.contains("tree.db.bad")));
        assert_eq!(
            fs::read(dir.path().join("tree.db.bad")).unwrap(),
            b"not a database, just fake bytes"
        );
        assert_eq!(
            Saved::open(&path, |_| {}).load().unwrap().tree,
            two_projects()
        );
    }

    #[test]
    fn a_second_corrupt_file_takes_the_next_free_name() {
        let dir = TestDir::new();
        let path = dir.path().join("tree.db");
        fs::write(dir.path().join("tree.db.bad"), b"older fake bytes").unwrap();
        fs::write(&path, b"newer fake bytes").unwrap();

        let loaded = Saved::open(&path, |_| {}).load().unwrap();

        assert!(loaded.notice.is_some_and(|n| n.contains("tree.db.bad2")));
        assert_eq!(
            fs::read(dir.path().join("tree.db.bad2")).unwrap(),
            b"newer fake bytes"
        );
        assert_eq!(
            fs::read(dir.path().join("tree.db.bad")).unwrap(),
            b"older fake bytes"
        );
    }

    #[test]
    fn a_second_app_on_the_same_file_saves_nothing() {
        let dir = TestDir::new();
        let path = dir.path().join("tree.db");
        let first = Saved::open(&path, |_| {});

        let started = Instant::now();
        let second = Saved::open(&path, |_| {});

        assert!(
            started.elapsed() < Duration::from_secs(1),
            "the second app waited for the lock"
        );
        assert!(first.save(two_projects()).is_ok());
        assert!(
            matches!(second.load(), Err(TreeError::Unavailable(e)) if e.contains("another Cerberus"))
        );
        assert!(second.save(two_projects()).is_err());
    }

    /// Fails its first `failures` writes, then keeps what it is given.
    struct FlakyDisk {
        failures: usize,
        kept: Arc<Mutex<Option<Snapshot>>>,
    }

    impl Store for FlakyDisk {
        fn write(&mut self, snapshot: &Snapshot) -> Result<(), TreeError> {
            if self.failures > 0 {
                self.failures -= 1;
                return Err(TreeError::Invalid("disk full (fake)".to_owned()));
            }
            *lock(&self.kept) = Some(snapshot.clone());
            Ok(())
        }
    }

    type Kept = Arc<Mutex<Option<Snapshot>>>;

    fn flaky(failures: usize) -> (Saved, Kept, Receiver<Option<String>>) {
        let kept = Arc::new(Mutex::new(None));
        let disk = FlakyDisk {
            failures,
            kept: Arc::clone(&kept),
        };
        let (tx, reports) = mpsc::channel();
        let report = move |failure: Option<&str>| {
            let _ = tx.send(failure.map(str::to_owned));
        };
        let saved = Saved(Arc::new(Ok(Saved::start(
            Snapshot::default(),
            disk,
            None,
            report,
        ))));
        (saved, kept, reports)
    }

    #[test]
    fn a_failed_write_is_reported_then_retried_until_it_lands() {
        let (saved, kept, reports) = flaky(1);

        saved.save(two_projects()).unwrap();

        let wait = RETRY * 3;
        assert!(
            reports
                .recv_timeout(wait)
                .unwrap()
                .is_some_and(|e| e.contains("disk full"))
        );
        assert_eq!(reports.recv_timeout(wait).unwrap(), None);
        assert_eq!(lock(&kept).as_ref().map(|s| &s.tree), Some(&two_projects()));
    }

    #[test]
    fn changing_the_tree_back_ends_a_failure_without_a_write() {
        let (saved, kept, reports) = flaky(usize::MAX);
        saved.save(two_projects()).unwrap();
        assert!(reports.recv_timeout(RETRY).unwrap().is_some());

        saved.save(SavedTree::default()).unwrap();

        assert_eq!(reports.recv_timeout(RETRY * 3).unwrap(), None);
        assert!(lock(&kept).is_none());
    }

    #[test]
    fn a_new_chat_in_a_tab_replaces_the_saved_one() {
        let dir = TestDir::new();
        let saved = Saved::open(&dir.path().join("tree.db"), |_| {});
        saved.save(two_projects()).unwrap();

        saved.set_session(3, SESSION);
        saved.set_session(3, OTHER_SESSION);

        assert_eq!(saved.session_of(3).as_deref(), Some(OTHER_SESSION));
    }

    #[test]
    fn a_closed_tab_forgets_its_chat() {
        let dir = TestDir::new();
        let saved = Saved::open(&dir.path().join("tree.db"), |_| {});
        saved.save(two_projects()).unwrap();
        saved.set_session(7, SESSION);

        let mut closed = two_projects();
        closed.tabs.retain(|t| t.id != 7);
        saved.save(closed).unwrap();

        assert_eq!(saved.session_of(7), None);
    }

    #[test]
    fn a_session_id_that_could_be_a_path_or_flag_is_ignored() {
        let dir = TestDir::new();
        let saved = Saved::open(&dir.path().join("tree.db"), |_| {});
        saved.save(two_projects()).unwrap();

        saved.set_session(3, r"..\..\x");
        saved.set_session(3, "--dangerously-skip-permissions");

        assert_eq!(saved.session_of(3), None);
    }

    fn rejected(change: impl FnOnce(&mut SavedTree)) -> bool {
        let mut tree = two_projects();
        change(&mut tree);
        matches!(tree.validate(), Err(TreeError::Invalid(_)))
    }

    #[test]
    fn a_tree_that_breaks_the_rules_is_refused() {
        assert!(two_projects().validate().is_ok());
        assert!(rejected(|t| t.tabs[1].id = 3));
        assert!(rejected(|t| t.tabs[0].project_id = 9));
        assert!(rejected(|t| t.tabs[1].parent_id = Some(4)));
        assert!(rejected(|t| t.tabs[1].parent_id = Some(99)));
        assert!(rejected(|t| t.tabs[1].parent_id = Some(6)));
        assert!(rejected(|t| t.tabs[0].parent_id = Some(4)));
        assert!(rejected(|t| t.next_id = 7));
        assert!(rejected(|t| t.tabs[0].title = " ".to_owned()));
        assert!(rejected(|t| t.projects[0].folder = String::new()));
    }
}
