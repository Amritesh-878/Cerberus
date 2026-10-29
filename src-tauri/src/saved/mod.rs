mod db;

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::Path;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize, Serializer};

use crate::accounts::AccountId;
use crate::saved::db::Db;
use crate::sessions::is_session_id;

pub type TabId = u32;
pub type ProjectId = u32;

// Saves wait for a quiet moment, but never longer than the cap, so a burst of changes is one write.
const QUIET: Duration = Duration::from_millis(300);
const MOST_DELAY: Duration = Duration::from_secs(2);

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

struct Live {
    snapshot: Arc<Mutex<Snapshot>>,
    saver: Mutex<Sender<Message>>,
}

/// The tab tree and each tab's chat id, kept in memory and written to SQLite by one thread.
#[derive(Clone)]
pub struct Saved(Arc<Result<Live, String>>);

// A panicked holder leaves the snapshot whole, so poisoning is not a reason to fail.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Saved {
    /// A file that cannot be read leaves saving off, so the app never writes over tabs it failed to load.
    pub fn open(path: &Path) -> Self {
        Self(Arc::new(
            Db::open(path)
                .and_then(|db| Ok((db.load()?, db)))
                .map(|(snapshot, db)| Self::start(snapshot, db))
                .map_err(|e| e.to_string()),
        ))
    }

    fn start(snapshot: Snapshot, db: Db) -> Live {
        let snapshot = Arc::new(Mutex::new(snapshot));
        let (tx, rx) = mpsc::channel();
        let shared = Arc::clone(&snapshot);
        let written = lock(&snapshot).clone();
        thread::spawn(move || save_loop(db, &shared, written, &rx));
        Live {
            snapshot,
            saver: Mutex::new(tx),
        }
    }

    fn live(&self) -> Result<&Live, TreeError> {
        self.0
            .as_ref()
            .as_ref()
            .map_err(|e| TreeError::Unavailable(e.clone()))
    }

    pub fn load(&self) -> Result<SavedTree, TreeError> {
        Ok(lock(&self.live()?.snapshot).tree.clone())
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

    /// Waits until everything changed so far is on disk.
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

fn save_loop(
    mut db: Db,
    snapshot: &Mutex<Snapshot>,
    mut written: Snapshot,
    rx: &Receiver<Message>,
) {
    while let Ok(first) = rx.recv() {
        let mut waiting = Vec::new();
        if let Message::Flush(reply) = first {
            waiting.push(reply);
        } else {
            let deadline = Instant::now() + MOST_DELAY;
            loop {
                let wait = QUIET.min(deadline.saturating_duration_since(Instant::now()));
                match rx.recv_timeout(wait) {
                    Ok(Message::Changed) => {}
                    Ok(Message::Flush(reply)) => {
                        waiting.push(reply);
                        break;
                    }
                    Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => break,
                }
            }
        }
        let current = lock(snapshot).clone();
        if current != written {
            match db.write(&current) {
                Ok(()) => written = current,
                Err(e) => eprintln!("could not save tabs: {e}"),
            }
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
        let saved = Saved::open(&path);

        saved.save(two_projects()).unwrap();
        saved.set_session(6, SESSION);
        saved.flush();
        let restarted = Saved::open(&path);

        assert_eq!(restarted.load().unwrap(), two_projects());
        assert_eq!(restarted.session_of(6).as_deref(), Some(SESSION));
        assert_eq!(restarted.session_of(3), None);
    }

    #[test]
    fn a_new_chat_in_a_tab_replaces_the_saved_one() {
        let dir = TestDir::new();
        let saved = Saved::open(&dir.path().join("tree.db"));
        saved.save(two_projects()).unwrap();

        saved.set_session(3, SESSION);
        saved.set_session(3, OTHER_SESSION);

        assert_eq!(saved.session_of(3).as_deref(), Some(OTHER_SESSION));
    }

    #[test]
    fn a_closed_tab_forgets_its_chat() {
        let dir = TestDir::new();
        let saved = Saved::open(&dir.path().join("tree.db"));
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
        let saved = Saved::open(&dir.path().join("tree.db"));
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
