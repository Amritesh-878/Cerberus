use std::collections::{HashMap, HashSet};
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde::{Serialize, Serializer};

use crate::accounts::{AccountId, Accounts, AccountsError};
use crate::saved::{Saved, TabId};
use crate::sessions::carry::{CarryError, copy_chat, copy_to_folder};
use crate::sessions::procs::Probes;

// Long enough for a closed tab's `claude` to finish exiting, short enough to answer a click.
const EXIT_WAIT: Duration = Duration::from_secs(5);
const TREE_POLL: Duration = Duration::from_millis(100);
// A chat's own background shells and servers get this long to end with it before they are ended.
const LEFT_GRACE: Duration = Duration::from_millis(1500);
const WATCHER_POLL: Duration = Duration::from_millis(20);

#[derive(Debug, thiserror::Error)]
pub enum ContinueError {
    #[error("this tab is not saved")]
    NoTab,
    #[error("that folder does not exist")]
    NoFolder,
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
    #[error("the move stopped before it finished")]
    Stopped,
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
    /// Counts every start, so a program that came and went while a move waited is still noticed.
    starts: u64,
    /// The tab's last program and when it started (Unix milliseconds), kept after it exits so anything it left running can be waited for.
    root: Option<(u32, u64)>,
    watchers: Vec<JoinHandle<()>>,
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
        Self::with(Probes::SYSTEM)
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
        let mut tabs = lock(&self.tabs);
        let procs = tabs.entry(tab).or_default();
        procs.live += 1;
        procs.starts += 1;
    }

    pub fn spawned(&self, tab: TabId, pid: u32, started_ms: u64) {
        lock(&self.tabs).entry(tab).or_default().root = Some((pid, started_ms));
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
        let Some(procs) = tabs.get_mut(&tab) else {
            return true;
        };
        if procs.live > 0 {
            return false;
        }
        // The entry stays in the map, so a start or an end that lands during the waits below still counts.
        let root = procs.root;
        let starts = procs.starts;
        let watchers = std::mem::take(&mut procs.watchers);
        drop(tabs);
        let keep = |watchers: Vec<JoinHandle<()>>| {
            lock(&self.tabs)
                .entry(tab)
                .or_default()
                .watchers
                .extend(watchers);
        };
        // A wrapper such as `claude.cmd` can exit while the `claude` it started is still writing.
        if let Some((root, since)) = root {
            // The chat was stopped before it could end its own background shells, so after a grace they are ended for it.
            let grace = Instant::now() + LEFT_GRACE.min(wait / 2);
            let mut stopped = false;
            loop {
                let left = (self.probes.below)(root);
                if left.is_empty() {
                    break;
                }
                // The root exited, so a process holding its pid now is another program, whose children are not this chat's.
                if !stopped && Instant::now() >= grace && !(self.probes.is_running)(root) {
                    left.into_iter()
                        .for_each(|pid| (self.probes.stop)(pid, since));
                    stopped = true;
                    // Looked at again at once, since a stopped process is usually gone already.
                    continue;
                }
                if Instant::now() >= deadline {
                    keep(watchers);
                    return false;
                }
                thread::sleep(TREE_POLL);
            }
        }
        // A watcher's last poll could still save a newer chat id; it must end first, but never holds the move past the wait.
        while watchers.iter().any(|w| !w.is_finished()) {
            if Instant::now() >= deadline {
                keep(watchers);
                return false;
            }
            thread::sleep(WATCHER_POLL);
        }
        for watcher in watchers {
            let _ = watcher.join();
        }
        let mut tabs = lock(&self.tabs);
        // A program that started during the waits may have left a tree or a watcher not waited on.
        if tabs
            .get(&tab)
            .is_some_and(|p| p.live > 0 || p.starts != starts)
        {
            return false;
        }
        tabs.remove(&tab);
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
    let from = accounts.chat_home(saved_tab.account)?;
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
    // The chat goes on under the new account, so the tab resumes it there instead of opening the shell.
    saved.set_ended(tab, false);
    Ok(())
}

/// Readies a stopped tab to resume its chat as a Claude tab once its shell, and the `claude` started in it, have ended.
pub fn reopen_chat(saved: &Saved, running: &RunningTabs, tab: TabId) -> Result<(), ContinueError> {
    reopen_within(saved, running, tab, EXIT_WAIT)
}

fn reopen_within(
    saved: &Saved,
    running: &RunningTabs,
    tab: TabId,
    wait: Duration,
) -> Result<(), ContinueError> {
    let _moving = running.begin_move(tab).ok_or(ContinueError::Busy)?;
    // Two `claude` processes on one chat would both write its file.
    if !running.settle(tab, wait) {
        return Err(ContinueError::Running);
    }
    // A chat that ended just before the shell closed is the one the user asked to reopen.
    saved.set_ended(tab, false);
    Ok(())
}

/// Copies the tab's chat to `folder`'s project once its program has stopped, so it resumes there.
pub fn move_to_folder(
    saved: &Saved,
    accounts: &Accounts,
    running: &RunningTabs,
    tab: TabId,
    folder: &str,
) -> Result<(), ContinueError> {
    move_within(saved, accounts, running, tab, folder, EXIT_WAIT)
}

fn move_within(
    saved: &Saved,
    accounts: &Accounts,
    running: &RunningTabs,
    tab: TabId,
    folder: &str,
    wait: Duration,
) -> Result<(), ContinueError> {
    let path = std::path::Path::new(folder);
    if !path.is_absolute() || !path.is_dir() {
        return Err(ContinueError::NoFolder);
    }
    let _moving = running.begin_move(tab).ok_or(ContinueError::Busy)?;
    let saved_tab = saved.tab(tab).ok_or(ContinueError::NoTab)?;
    if !running.settle(tab, wait) {
        return Err(ContinueError::Running);
    }
    // A shell, or a chat that never started, has nothing to carry.
    let Some(id) = saved.session_of(tab) else {
        return Ok(());
    };
    let home = accounts.chat_home(saved_tab.account)?;
    let from = saved_tab
        .cwd
        .unwrap_or_else(|| accounts.home().to_string_lossy().into_owned());
    if from == folder {
        return Ok(());
    }
    match copy_to_folder(&home, &from, folder, &id, &running.probes.is_running) {
        // A chat whose file is gone starts anew in the new folder, as it would have in the old one.
        Err(CarryError::Missing) => Ok(()),
        other => Ok(other?),
    }
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

    fn stop_nothing(_: u32, _: u64) {}

    fn quiet() -> RunningTabs {
        RunningTabs::with(Probes {
            below: nothing_below,
            is_running: nothing_running,
            stop: stop_nothing,
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
                    dirs: Vec::new(),
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
    fn a_stopped_chat_is_copied_to_the_new_folder_on_its_own_account() {
        let world = world();
        let plans = world.dir.path().join("plans");
        fs::create_dir_all(&plans).unwrap();
        let plans = plans.to_string_lossy().into_owned();

        move_within(
            &world.saved,
            &world.accounts,
            &world.running,
            2,
            &plans,
            Duration::from_millis(300),
        )
        .unwrap();

        let home = world.dir.path().join(".claude-personal");
        assert!(crate::sessions::transcript(&home, &plans, ID).is_some());
        assert!(!world.copied(ID).exists(), "the chat left its account");
    }

    #[test]
    fn a_folder_move_waits_for_a_running_chat_and_refuses_a_missing_folder() {
        let world = world();
        let plans = world.dir.path().join("plans");
        fs::create_dir_all(&plans).unwrap();
        let plans = plans.to_string_lossy().into_owned();
        let gone = world.dir.path().join("gone").to_string_lossy().into_owned();
        let moving = |folder: &str| {
            move_within(
                &world.saved,
                &world.accounts,
                &world.running,
                2,
                folder,
                Duration::from_millis(50),
            )
        };

        assert!(matches!(moving(&gone), Err(ContinueError::NoFolder)));
        assert!(matches!(moving("relative"), Err(ContinueError::NoFolder)));
        world.running.started(2);
        assert!(matches!(moving(&plans), Err(ContinueError::Running)));
    }

    #[test]
    fn a_reopen_waits_for_the_shell_and_its_chat_then_resumes_even_an_ended_chat() {
        let world = world();
        world.saved.set_ended(2, true);
        world.running.started(2);
        let reopen = || reopen_within(&world.saved, &world.running, 2, Duration::from_millis(50));

        assert!(matches!(reopen(), Err(ContinueError::Running)));
        assert!(world.saved.is_ended(2));
        world.running.ended(2);
        reopen().unwrap();

        assert!(!world.saved.is_ended(2));
        assert_eq!(world.saved.session_of(2).as_deref(), Some(ID));
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
            stop: stop_nothing,
        }));
        world.running.started(2);
        world.running.spawned(2, 7, 0);
        world.running.ended(2);

        assert!(world.continue_to(1, true).is_ok());
        assert_eq!(LEFT_RUNNING.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn a_program_that_never_ends_below_a_wrapper_is_refused() {
        let world = world_with(RunningTabs::with(Probes {
            below: below_for_ever,
            is_running: nothing_running,
            stop: stop_nothing,
        }));
        world.running.started(2);
        world.running.spawned(2, 7, 0);
        world.running.ended(2);

        assert!(matches!(
            world.continue_to(1, true),
            Err(ContinueError::Running)
        ));
        assert!(!world.copied(ID).exists());
    }

    static STOPPED: AtomicBool = AtomicBool::new(false);
    static STOPPED_SINCE: AtomicUsize = AtomicUsize::new(0);

    fn below_until_stopped(_: u32) -> Vec<u32> {
        if STOPPED.load(Ordering::SeqCst) {
            Vec::new()
        } else {
            vec![99]
        }
    }

    fn stop_and_record(pid: u32, since: u64) {
        assert_eq!(pid, 99);
        STOPPED_SINCE.store(usize::try_from(since).unwrap(), Ordering::SeqCst);
        STOPPED.store(true, Ordering::SeqCst);
    }

    static NEVER_STOPPED: AtomicBool = AtomicBool::new(false);

    fn stop_must_not_run(_: u32, _: u64) {
        NEVER_STOPPED.store(true, Ordering::SeqCst);
    }

    fn root_pid_taken_again(pid: u32) -> bool {
        pid == 7
    }

    #[test]
    fn a_root_pid_taken_by_another_program_is_never_used_to_stop_anything() {
        let world = world_with(RunningTabs::with(Probes {
            below: below_for_ever,
            is_running: root_pid_taken_again,
            stop: stop_must_not_run,
        }));
        world.running.started(2);
        world.running.spawned(2, 7, 1_234);
        world.running.ended(2);

        assert!(matches!(
            world.continue_to(1, true),
            Err(ContinueError::Running)
        ));
        assert!(!NEVER_STOPPED.load(Ordering::SeqCst));
    }

    #[test]
    fn what_a_stopped_chat_left_running_is_ended_after_a_grace_so_the_move_goes_on() {
        let world = world_with(RunningTabs::with(Probes {
            below: below_until_stopped,
            is_running: nothing_running,
            stop: stop_and_record,
        }));
        world.running.started(2);
        world.running.spawned(2, 7, 1_234);
        world.running.ended(2);

        // A wait well past the grace, so a slow runner still sees the move finish.
        let moved = continue_within(
            &world.saved,
            &world.accounts,
            &world.running,
            |_| Ok(true),
            2,
            1,
            Duration::from_secs(10),
        );

        assert!(moved.is_ok(), "{moved:?}");
        assert_eq!(STOPPED_SINCE.load(Ordering::SeqCst), 1_234);
        assert!(world.copied(ID).is_file());
    }

    static TREE_HELD: AtomicBool = AtomicBool::new(false);
    static TREE_POLLED: AtomicBool = AtomicBool::new(false);

    type Event = fn(&RunningTabs, TabId);

    fn below_while_held(_: u32) -> Vec<u32> {
        TREE_POLLED.store(true, Ordering::SeqCst);
        if TREE_HELD.load(Ordering::SeqCst) {
            vec![99]
        } else {
            Vec::new()
        }
    }

    #[test]
    fn starts_and_ends_during_the_tree_wait_are_counted() {
        let world = world_with(RunningTabs::with(Probes {
            below: below_while_held,
            is_running: nothing_running,
            stop: stop_nothing,
        }));
        world.running.started(2);
        world.running.spawned(2, 7, 0);
        world.running.ended(2);
        let during_the_wait = |events: &[Event]| {
            TREE_HELD.store(true, Ordering::SeqCst);
            TREE_POLLED.store(false, Ordering::SeqCst);
            thread::scope(|scope| {
                scope.spawn(|| {
                    // The events wait for the tree wait to begin, not for a guess at how long that takes.
                    let begun = Instant::now() + Duration::from_secs(20);
                    while !TREE_POLLED.load(Ordering::SeqCst) && Instant::now() < begun {
                        thread::sleep(Duration::from_millis(1));
                    }
                    for event in events {
                        event(&world.running, 2);
                    }
                    TREE_HELD.store(false, Ordering::SeqCst);
                });
                // Far past any scheduling delay, since the hold ends on its own once the events are in.
                continue_within(
                    &world.saved,
                    &world.accounts,
                    &world.running,
                    |_| Ok(true),
                    2,
                    1,
                    Duration::from_secs(20),
                )
            })
        };

        let restarted = during_the_wait(&[RunningTabs::started as Event]);
        assert!(matches!(restarted, Err(ContinueError::Running)));
        assert!(!world.copied(ID).exists());

        world.running.ended(2);
        // Ended before its pid was read, so it left no root or watcher behind.
        let came_and_went = during_the_wait(&[RunningTabs::started as Event, RunningTabs::ended]);
        assert!(matches!(came_and_went, Err(ContinueError::Running)));
        assert!(!world.copied(ID).exists());

        let quiet = during_the_wait(&[]);
        assert!(quiet.is_ok(), "an end was lost: {quiet:?}");
        assert!(world.copied(ID).is_file());
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
    fn a_watcher_that_never_ends_fails_the_move_after_the_wait() {
        let world = world();
        let (release, held) = std::sync::mpsc::channel::<()>();
        world.running.watching(
            2,
            thread::spawn(move || {
                let _ = held.recv();
            }),
        );
        let started = Instant::now();

        let result = world.continue_to(1, true);

        assert!(matches!(result, Err(ContinueError::Running)), "{result:?}");
        assert!(started.elapsed() < Duration::from_secs(3));
        assert!(!world.copied(ID).exists());
        release.send(()).unwrap();
        assert!(
            world.continue_to(1, true).is_ok(),
            "the kept watcher was waited for"
        );
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
    fn a_moved_chat_is_no_longer_ended_and_a_refused_one_still_is() {
        let world = world();
        assert!(world.saved.set_ended(2, true));

        assert!(world.continue_to(1, false).is_err());
        assert!(world.saved.is_ended(2));

        world.continue_to(1, true).unwrap();
        assert!(!world.saved.is_ended(2));
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
