use std::collections::HashMap;
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use serde::{Serialize, Serializer};

use crate::accounts::{AccountId, Accounts, AccountsError};
use crate::saved::{Saved, TabId};
use crate::sessions::carry::{CarryError, copy_chat};

// Long enough for a closed tab's `claude` to finish exiting, short enough to answer a click.
const EXIT_WAIT: Duration = Duration::from_secs(5);

#[derive(Debug, thiserror::Error)]
pub enum ContinueError {
    #[error("this tab is not saved")]
    NoTab,
    #[error("this tab already runs on that account")]
    SameAccount,
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

/// Which tabs have a program running, so a chat is never copied while `claude` may still write to it.
#[derive(Default)]
pub struct RunningTabs {
    count: Mutex<HashMap<TabId, usize>>,
    changed: Condvar,
}

// A panicked holder leaves the counts whole, so poisoning is not a reason to fail.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl RunningTabs {
    pub fn started(&self, tab: TabId) {
        *lock(&self.count).entry(tab).or_default() += 1;
    }

    pub fn ended(&self, tab: TabId) {
        let mut count = lock(&self.count);
        if let Some(n) = count.get_mut(&tab) {
            *n -= 1;
            if *n == 0 {
                count.remove(&tab);
            }
        }
        self.changed.notify_all();
    }

    fn wait_ended(&self, tab: TabId, wait: Duration) -> bool {
        let count = lock(&self.count);
        let (count, _) = self
            .changed
            .wait_timeout_while(count, wait, |count| count.contains_key(&tab))
            .unwrap_or_else(PoisonError::into_inner);
        !count.contains_key(&tab)
    }
}

/// Copies the tab's chat to `account`; the webview then moves the tab there, which resumes it.
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
    let saved_tab = saved.tab(tab).ok_or(ContinueError::NoTab)?;
    let from = accounts.claude_home(saved_tab.account)?;
    let to = accounts.claude_home(Some(account))?;
    if saved_tab.account == Some(account) || from == to {
        return Err(ContinueError::SameAccount);
    }
    if !running.wait_ended(tab, wait) {
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
    copy_chat(&from, &to, &cwd, &id)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use super::*;
    use crate::accounts::testdir::TestDir;
    use crate::saved::{SavedProject, SavedTab, SavedTree};
    use crate::sessions::slug;

    const ID: &str = "0f6c3a52-7d1e-4b8a-9c2f-5e4d3b2a1f00";
    const CWD: &str = r"C:\work\app";

    struct World {
        dir: TestDir,
        accounts: Accounts,
        saved: Saved,
        running: RunningTabs,
    }

    /// Two accounts (0 and 1) and one saved Claude tab (id 2) on account 0 whose chat is `ID`.
    fn world() -> World {
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
        fs::write(project.join(format!("{ID}.jsonl")), "{\"fake\":\"chat\"}\n").unwrap();
        saved.set_session(2, ID);
        World {
            dir,
            accounts,
            saved,
            running: RunningTabs::default(),
        }
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
                Duration::from_millis(50),
            )
        }

        fn copied(&self) -> PathBuf {
            self.dir
                .path()
                .join(".claude-work")
                .join("projects")
                .join(slug(CWD))
                .join(format!("{ID}.jsonl"))
        }
    }

    #[test]
    fn a_stopped_chat_is_copied_to_a_signed_in_account() {
        let world = world();

        world.continue_to(1, true).unwrap();

        assert!(world.copied().is_file());
    }

    #[test]
    fn a_running_chat_is_refused() {
        let world = world();
        world.running.started(2);

        assert!(matches!(
            world.continue_to(1, true),
            Err(ContinueError::Running)
        ));
        assert!(!world.copied().exists());
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

    #[test]
    fn a_signed_out_target_is_refused_by_name() {
        let world = world();

        let result = world.continue_to(1, false);

        assert!(matches!(result, Err(ContinueError::SignedOut(name)) if name == "work"));
        assert!(!world.copied().exists());
    }

    #[test]
    fn a_target_that_has_the_chat_is_refused() {
        let world = world();
        fs::create_dir_all(world.copied().parent().unwrap()).unwrap();
        fs::write(world.copied(), "theirs").unwrap();

        let result = world.continue_to(1, true);

        assert!(matches!(
            result,
            Err(ContinueError::Carry(CarryError::Exists))
        ));
        assert_eq!(fs::read_to_string(world.copied()).unwrap(), "theirs");
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
