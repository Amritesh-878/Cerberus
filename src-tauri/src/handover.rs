use std::collections::{HashMap, HashSet};
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde::{Serialize, Serializer};

use crate::accounts::{AccountId, Accounts, AccountsError};
use crate::saved::{Saved, TabId};
use crate::sessions::carry::{CarryError, copy_chat};
use crate::sessions::procs;

// Long enough for a closed tab's `claude` to finish exiting, short enough to answer a click.
const EXIT_WAIT: Duration = Duration::from_secs(5);
const TREE_POLL: Duration = Duration::from_millis(100);

#[derive(Debug, thiserror::Error)]
pub enum ContinueError {
    #[error("this tab is not saved")]
    NoTab,
    #[error("this tab already runs on that account")]
    SameAccount,
    #[error("this chat is already moving")]
    Busy,
    #[error("the chat is still running; close it first")]
    Running,
    #[error("this tab has no saved chat yet")]
    NoChat,
    #[error("{0} is signed out; sign it in from Accounts first")]
    SignedOut(String),
    #[error(transparent)]
    Accounts(#[from] AccountsError),
    #[error(transparent)]
    Carry(#[from] CarryError),
}

impl Serialize for ContinueError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

#[derive(Default)]
struct TabProcs {
    live: usize,
    /// The tab's last program, kept after it exits so anything it left running can be waited for.
    root: Option<u32>,
    watchers: Vec<JoinHandle<()>>,
}

/// What Windows says about processes, as plain functions so tests can stand in for it.
pub struct Probes {
    pub below: fn(u32) -> Vec<u32>,
    pub is_running: fn(u32) -> bool,
}

/// Each tab's programs and watchers, so a chat is never copied while anything may still write to it.
pub struct RunningTabs {
    tabs: Mutex<HashMap<TabId, TabProcs>>,
    changed: Condvar,
    moving: Mutex<HashSet<TabId>>,
    probes: Probes,
}

impl Default for RunningTabs {
    fn default() -> Self {
        Self::with(Probes {
            below: procs::descendants,
            is_running: procs::is_running,
        })
    }
}

// A panicked holder leaves the counts whole, so poisoning is not a reason to fail.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Holds a tab's place in the moving set until dropped, so a second move of it is refused.
struct Moving<'a>(&'a RunningTabs, TabId);

impl Drop for Moving<'_> {
    fn drop(&mut self) {
        lock(&self.0.moving).remove(&self.1);
    }
}

impl RunningTabs {
    fn with(probes: Probes) -> Self {
        Self {
            tabs: Mutex::default(),
            changed: Condvar::new(),
            moving: Mutex::default(),
            probes,
        }
    }

    pub fn started(&self, tab: TabId) {
        lock(&self.tabs).entry(tab).or_default().live += 1;
    }

    pub fn spawned(&self, tab: TabId, pid: u32) {
        lock(&self.tabs).entry(tab).or_default().root = Some(pid);
    }

    pub fn watching(&self, tab: TabId, watcher: JoinHandle<()>) {
        lock(&self.tabs)
            .entry(tab)
            .or_default()
            .watchers
            .push(watcher);
    }

    pub fn ended(&self, tab: TabId) {
        if let Some(procs) = lock(&self.tabs).get_mut(&tab) {
            procs.live = procs.live.saturating_sub(1);
        }
        self.changed.notify_all();
    }

    fn begin_move(&self, tab: TabId) -> Option<Moving<'_>> {
        let inserted = lock(&self.moving).insert(tab);
        inserted.then(|| Moving(self, tab))
    }

    /// Waits for the tab's program, everything it started, and its watchers to end.
    fn settle(&self, tab: TabId, wait: Duration) -> bool {
        let deadline = Instant::now() + wait;
        let tabs = lock(&self.tabs);
        let (mut tabs, _) = self
            .changed
            .wait_timeout_while(tabs, wait, |tabs| {
                tabs.get(&tab).is_some_and(|p| p.live > 0)
            })
            .unwrap_or_else(PoisonError::into_inner);
        let Some(procs) = tabs.remove(&tab) else {
            return true;
        };
        drop(tabs);
        if procs.live > 0 {
            lock(&self.tabs).insert(tab, procs);
            return false;
        }
        // A wrapper such as `claude.cmd` can exit while the `claude` it started is still writing.
        if let Some(root) = procs.root {
            while !(self.probes.below)(root).is_empty() {
                if Instant::now() >= deadline {
                    lock(&self.tabs).insert(tab, procs);
                    return false;
                }
                thread::sleep(TREE_POLL);
            }
        }
        // A watcher's last poll could still save a newer chat id; joining it first settles which chat moves.
        for watcher in procs.watchers {
            let _ = watcher.join();
        }
        true
    }
}

pub fn continue_chat(
    saved: &Saved,
    accounts: &Accounts,
    running: &RunningTabs,
    signed_in: impl FnOnce(AccountId) -> Result<bool, AccountsError>,
    tab: TabId,
    account: AccountId,
) -> Result<(), ContinueError> {
    continue_within(saved, accounts, running, signed_in, tab, account, EXIT_WAIT)
}

fn continue_within(
    saved: &Saved,
    accounts: &Accounts,
    running: &RunningTabs,
    signed_in: impl FnOnce(AccountId) -> Result<bool, AccountsError>,
    tab: TabId,
    account: AccountId,
    wait: Duration,
) -> Result<(), ContinueError> {
    let _moving = running.begin_move(tab).ok_or(ContinueError::Busy)?;
    let saved_tab = saved.tab(tab).ok_or(ContinueError::NoTab)?;
    let from = accounts.claude_home(saved_tab.account)?;
    let to = accounts.claude_home(Some(account))?;
    if saved_tab.account == Some(account) || from == to {
        return Err(ContinueError::SameAccount);
    }
    if !running.settle(tab, wait) {
        return Err(ContinueError::Running);
    }
    let id = saved.session_of(tab).ok_or(ContinueError::NoChat)?;
    if !signed_in(account)? {
        let name = accounts.load()?.account(account)?.name.clone();
        return Err(ContinueError::SignedOut(name));
    }
    let cwd = saved_tab
        .cwd
        .unwrap_or_else(|| accounts.home().to_string_lossy().into_owned());
    copy_chat(&from, &to, &cwd, &id, &running.probes.is_running)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use super::*;
    use crate::accounts::testdir::TestDir;
    use crate::saved::{SavedProject, SavedTab, SavedTree};
    use crate::sessions::slug;

    const ID: &str = "0f6c3a52-7d1e-4b8a-9c2f-5e4d3b2a1f00";
    const NEWER_ID: &str = "a1b2c3d4-0000-4000-8000-000000000001";
    const CWD: &str = r"C:\work\app";

    fn nothing_below(_: u32) -> Vec<u32> {
        Vec::new()
    }

    fn nothing_running(_: u32) -> bool {
        false
    }

    fn quiet() -> RunningTabs {
        RunningTabs::with(Probes {
            below: nothing_below,
            is_running: nothing_running,
        })
    }

    struct World {
        dir: TestDir,
        accounts: Accounts,
        saved: Saved,
        running: RunningTabs,
    }

    /// Two accounts (0 and 1) and one saved Claude tab (id 2) on account 0 whose chat is `ID`.
    fn world_with(running: RunningTabs) -> World {
        let dir = TestDir::new();
        let accounts = Accounts::new(&dir.path().join("app"), dir.path().to_path_buf());
        accounts
            .add("personal", dir.path().join(".claude-personal"))
            .unwrap();
        accounts
            .add("work", dir.path().join(".claude-work"))
            .unwrap();
        let saved = Saved::open(&dir.path().join("app").join("tree.db"), |_| {});
        saved
            .save(SavedTree {
                projects: vec![SavedProject {
                    id: 1,
                    name: "app".to_owned(),
                    folder: CWD.to_owned(),
                }],
                tabs: vec![SavedTab {
                    id: 2,
                    project_id: 1,
                    parent_id: None,
                    title: "planner".to_owned(),
                    program: "claude".to_owned(),
                    cwd: Some(CWD.to_owned()),
                    account: Some(0),
                }],
                next_id: 3,
            })
            .unwrap();
        let project = dir
            .path()
            .join(".claude-personal")
            .join("projects")
            .join(slug(CWD));
        fs::create_dir_all(&project).unwrap();
        for id in [ID, NEWER_ID] {
            fs::write(project.join(format!("{id}.jsonl")), "{\"fake\":\"chat\"}\n").unwrap();
        }
        saved.set_session(2, ID);
        World {
            dir,
            accounts,
            saved,
            running,
        }
    }

    fn world() -> World {
        world_with(quiet())
    }

    impl World {
        fn continue_to(&self, account: AccountId, signed_in: bool) -> Result<(), ContinueError> {
            continue_within(
                &self.saved,
                &self.accounts,
                &self.running,
                |_| Ok(signed_in),
                2,
                account,
                Duration::from_millis(300),
            )
        }

        fn copied(&self, id: &str) -> PathBuf {
            self.dir
                .path()
                .join(".claude-work")
                .join("projects")
                .join(slug(CWD))
                .join(format!("{id}.jsonl"))
        }
    }

    #[test]
    fn a_stopped_chat_is_copied_to_a_signed_in_account() {
        let world = world();

        world.continue_to(1, true).unwrap();

        assert!(world.copied(ID).is_file());
    }

    #[test]
    fn a_running_chat_is_refused() {
        let world = world();
        world.running.started(2);

        assert!(matches!(
            world.continue_to(1, true),
            Err(ContinueError::Running)
        ));
        assert!(!world.copied(ID).exists());
    }

    #[test]
    fn a_chat_closed_a_moment_ago_waits_for_its_program_to_end() {
        let world = world();
        world.running.started(2);
        std::thread::scope(|scope| {
            scope.spawn(|| {
                std::thread::sleep(Duration::from_millis(10));
                world.running.ended(2);
            });
            assert!(world.continue_to(1, true).is_ok());
        });
    }

    static LEFT_RUNNING: AtomicUsize = AtomicUsize::new(0);

    fn below_for_a_while(_: u32) -> Vec<u32> {
        if LEFT_RUNNING.load(Ordering::SeqCst) == 0 {
            return Vec::new();
        }
        LEFT_RUNNING.fetch_sub(1, Ordering::SeqCst);
        vec![99]
    }

    fn below_for_ever(_: u32) -> Vec<u32> {
        vec![99]
    }

    #[test]
    fn a_program_left_running_by_a_wrapper_is_waited_for() {
        LEFT_RUNNING.store(2, Ordering::SeqCst);
        let world = world_with(RunningTabs::with(Probes {
            below: below_for_a_while,
            is_running: nothing_running,
        }));
        world.running.started(2);
        world.running.spawned(2, 7);
        world.running.ended(2);

        assert!(world.continue_to(1, true).is_ok());
        assert_eq!(LEFT_RUNNING.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn a_program_that_never_ends_below_a_wrapper_is_refused() {
        let world = world_with(RunningTabs::with(Probes {
            below: below_for_ever,
            is_running: nothing_running,
        }));
        world.running.started(2);
        world.running.spawned(2, 7);
        world.running.ended(2);

        assert!(matches!(
            world.continue_to(1, true),
            Err(ContinueError::Running)
        ));
        assert!(!world.copied(ID).exists());
    }

    #[test]
    fn a_watchers_last_word_lands_before_the_chat_id_is_read() {
        let world = world();
        let done = Arc::new(AtomicBool::new(false));
        let finished = Arc::clone(&done);
        let saved = world.saved.clone();
        world.running.watching(
            2,
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(50));
                saved.set_session(2, NEWER_ID);
                finished.store(true, Ordering::SeqCst);
            }),
        );

        world.continue_to(1, true).unwrap();

        assert!(done.load(Ordering::SeqCst));
        assert!(world.copied(NEWER_ID).is_file());
        assert!(!world.copied(ID).exists());
    }

    #[test]
    fn a_second_move_of_the_same_tab_is_refused_while_the_first_runs() {
        let world = world();
        let first = world.running.begin_move(2);

        assert!(first.is_some());
        assert!(matches!(
            world.continue_to(1, true),
            Err(ContinueError::Busy)
        ));
        drop(first);
        assert!(world.continue_to(1, true).is_ok());
    }

    #[test]
    fn a_signed_out_target_is_refused_by_name() {
        let world = world();

        let result = world.continue_to(1, false);

        assert!(matches!(result, Err(ContinueError::SignedOut(name)) if name == "work"));
        assert!(!world.copied(ID).exists());
    }

    #[test]
    fn a_target_with_an_older_copy_gets_the_chat_and_keeps_the_old_one() {
        let world = world();
        fs::create_dir_all(world.copied(ID).parent().unwrap()).unwrap();
        fs::write(world.copied(ID), "older copy").unwrap();

        world.continue_to(1, true).unwrap();

        assert_eq!(
            fs::read_to_string(world.copied(ID)).unwrap(),
            "{\"fake\":\"chat\"}\n"
        );
        let kept = world.copied(ID).with_extension("jsonl.cerberus-old");
        assert_eq!(fs::read_to_string(kept).unwrap(), "older copy");
    }

    #[test]
    fn the_account_the_tab_already_runs_on_is_refused() {
        let world = world();

        assert!(matches!(
            world.continue_to(0, true),
            Err(ContinueError::SameAccount)
        ));
    }

    #[test]
    fn a_tab_with_no_saved_chat_is_refused() {
        let world = world();
        let mut tree = world.saved.load().unwrap().tree;
        tree.tabs[0].id = 3;
        tree.next_id = 4;
        world.saved.save(tree).unwrap();

        let result = continue_within(
            &world.saved,
            &world.accounts,
            &world.running,
            |_| Ok(true),
            3,
            1,
            Duration::ZERO,
        );

        assert!(matches!(result, Err(ContinueError::NoChat)));
    }
}
