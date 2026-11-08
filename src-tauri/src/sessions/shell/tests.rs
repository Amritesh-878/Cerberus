use std::cell::Cell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Instant, SystemTime};

use super::*;
use crate::accounts::testdir::TestDir;
use crate::saved::tests::{SESSION, two_projects};
use crate::sessions::slug;

const SHELL: u32 = 1_000;
const WRAPPER: u32 = 1_500;
const CLAUDE: u32 = 2_000;
const OTHER: u32 = 3_000;

thread_local! {
    static WALKS: Cell<usize> = const { Cell::new(0) };
}

fn walks() -> usize {
    WALKS.with(Cell::get)
}

fn child(root: u32) -> Vec<u32> {
    WALKS.with(|w| w.set(w.get() + 1));
    if root == SHELL {
        vec![CLAUDE]
    } else {
        Vec::new()
    }
}

fn through_a_wrapper(root: u32) -> Vec<u32> {
    WALKS.with(|w| w.set(w.get() + 1));
    if root == SHELL {
        vec![WRAPPER, CLAUDE]
    } else {
        Vec::new()
    }
}

fn nothing(_: u32) -> Vec<u32> {
    WALKS.with(|w| w.set(w.get() + 1));
    Vec::new()
}

fn running(_: u32) -> bool {
    true
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(0))
}

fn shell_on(dir: &Path, started_ms: u64, below: fn(u32) -> Vec<u32>) -> Shell {
    Shell {
        config_dir: dir.to_path_buf(),
        pid: SHELL,
        cwd: dir.to_string_lossy().into_owned(),
        started_ms,
        probes: Probes {
            below,
            is_running: running,
        },
    }
}

fn write_session(config_dir: &Path, pid: u32, cwd: &Path, status: &str) {
    let sessions = config_dir.join("sessions");
    fs::create_dir_all(&sessions).unwrap();
    let file = serde_json::json!({
        "pid": pid,
        "sessionId": SESSION,
        "cwd": cwd,
        "status": status,
        "startedAt": now_ms(),
    });
    fs::write(sessions.join(format!("{pid}.json")), file.to_string()).unwrap();
}

fn write_transcript(config_dir: &Path, cwd: &Path) {
    let folder = config_dir
        .join("projects")
        .join(slug(&cwd.to_string_lossy()));
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join(format!("{SESSION}.jsonl")), "{}\n").unwrap();
}

fn spotter(shell: &Shell) -> Spotter {
    WALKS.with(|w| w.set(0));
    Spotter::new(shell)
}

#[test]
fn only_numbered_json_files_name_a_process() {
    assert_eq!(pid_of("4242.json"), Some(4242));
    assert_eq!(pid_of("4242.abc123.key"), None);
    assert_eq!(pid_of("-1.json"), None);
    assert_eq!(pid_of("+7.json"), None);
    assert_eq!(pid_of(".json"), None);
    assert_eq!(pid_of("99999999999.json"), None);
}

#[test]
fn a_claude_started_in_the_shell_is_found() {
    let dir = TestDir::new();
    let shell = shell_on(dir.path(), now_ms() - 1_000, child);
    let mut spotter = spotter(&shell);
    write_session(dir.path(), CLAUDE, dir.path(), "busy");

    let found = spotter.spot(shell.probes.below);

    assert_eq!(found.map(|(pid, _)| pid), Some(CLAUDE));
}

#[test]
fn a_claude_below_a_wrapper_is_found() {
    let dir = TestDir::new();
    let shell = shell_on(dir.path(), now_ms() - 1_000, through_a_wrapper);
    let mut spotter = spotter(&shell);
    write_session(dir.path(), CLAUDE, dir.path(), "idle");

    let found = spotter.spot(shell.probes.below);

    assert_eq!(found.map(|(pid, _)| pid), Some(CLAUDE));
}

#[test]
fn another_claude_in_the_same_folder_is_not_the_shells_and_is_walked_for_once() {
    let dir = TestDir::new();
    let shell = shell_on(dir.path(), now_ms() - 1_000, child);
    let mut spotter = spotter(&shell);
    write_session(dir.path(), OTHER, dir.path(), "busy");

    assert!(spotter.spot(shell.probes.below).is_none());
    write_session(dir.path(), OTHER, dir.path(), "idle");
    assert!(spotter.spot(shell.probes.below).is_none());

    assert_eq!(walks(), 1);
}

#[test]
fn a_claude_on_another_config_dir_is_not_seen() {
    let dir = TestDir::new();
    let tabs_account = dir.path().join("work");
    let set_by_hand = dir.path().join("personal");
    let shell = shell_on(&tabs_account, now_ms() - 1_000, child);
    let mut spotter = spotter(&shell);
    write_session(&set_by_hand, CLAUDE, dir.path(), "busy");

    assert!(spotter.spot(shell.probes.below).is_none());
    assert_eq!(walks(), 0);
}

#[test]
fn an_idle_shell_never_walks_the_process_tree() {
    let dir = TestDir::new();
    let shell = shell_on(dir.path(), now_ms() - 1_000, nothing);
    let mut spotter = spotter(&shell);

    for _ in 0..3 {
        assert!(spotter.spot(shell.probes.below).is_none());
    }
    fs::create_dir_all(dir.path().join("sessions")).unwrap();
    assert!(spotter.spot(shell.probes.below).is_none());

    assert_eq!(walks(), 0);
}

#[test]
fn a_file_from_before_the_shell_started_is_skipped_without_a_walk() {
    let dir = TestDir::new();
    write_session(dir.path(), CLAUDE, dir.path(), "busy");
    let shell = shell_on(dir.path(), now_ms() + 60_000, child);
    let mut spotter = spotter(&shell);

    assert!(spotter.spot(shell.probes.below).is_none());
    assert_eq!(walks(), 0);
}

#[test]
fn a_pid_whose_file_went_away_is_judged_again() {
    let dir = TestDir::new();
    let shell = shell_on(dir.path(), now_ms() - 1_000, child);
    let mut spotter = spotter(&shell);
    write_session(dir.path(), OTHER, dir.path(), "busy");
    assert!(spotter.spot(shell.probes.below).is_none());

    fs::remove_file(dir.path().join("sessions").join(format!("{OTHER}.json"))).unwrap();
    assert!(spotter.spot(shell.probes.below).is_none());
    write_session(dir.path(), OTHER, dir.path(), "busy");
    assert!(spotter.spot(shell.probes.below).is_none());

    assert_eq!(walks(), 2);
}

/// Everything the watcher reports, as text, with `open` saying whether the shell is still Cerberus's.
#[derive(Clone)]
struct Seen {
    events: Arc<Mutex<Vec<String>>>,
    open: Arc<AtomicBool>,
    keeper: Option<Arc<Keeper>>,
}

impl Seen {
    fn new(keeper: Option<Keeper>) -> Self {
        Self {
            events: Arc::default(),
            open: Arc::new(AtomicBool::new(true)),
            keeper: keeper.map(Arc::new),
        }
    }

    fn events(&self) -> Vec<String> {
        self.events.lock().unwrap().clone()
    }

    fn push(&self, event: String) {
        self.events.lock().unwrap().push(event);
    }

    fn wait_for(&self, done: impl Fn(&[String]) -> bool) -> Vec<String> {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !done(&self.events()) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(50));
        }
        self.events()
    }
}

impl Report for Seen {
    fn chat(&mut self, chat: &HandChat) {
        if let Some(keeper) = &self.keeper {
            keeper.chat(chat);
        }
        self.push(format!("chat {}", serde_json::to_string(chat).unwrap()));
    }

    fn session(&mut self, id: &str) {
        if let Some(keeper) = &self.keeper {
            keeper.session(id);
        }
        self.push(format!("session {id}"));
    }

    fn state(&mut self, state: TabState) {
        self.push(format!("state {}", serde_json::to_string(&state).unwrap()));
    }

    fn open(&self) -> bool {
        self.open.load(Ordering::SeqCst)
    }
}

fn has(events: &[String], text: &str) -> bool {
    events.iter().any(|e| e.contains(text))
}

fn started(cwd: &Path) -> String {
    format!(
        "chat {}",
        serde_json::json!({"chat": "started", "cwd": cwd.to_string_lossy()})
    )
}

#[test]
fn a_followed_chat_saves_its_id_and_ends_as_the_tabs_ended_chat() {
    let dir = TestDir::new();
    let db = TestDir::new();
    let saved = Saved::open(&db.path().join("tree.db"), |_| {});
    saved.save(two_projects()).unwrap();
    let seen = Seen::new(Some(Keeper {
        saved: saved.clone(),
        tab: 5,
    }));
    let (stop, stopped) = mpsc::channel::<()>();
    let watcher = watch_shell(
        shell_on(dir.path(), now_ms() - 1_000, child),
        stopped,
        seen.clone(),
    )
    .unwrap();

    write_session(dir.path(), CLAUDE, dir.path(), "busy");
    seen.wait_for(|e| has(e, "working"));
    write_session(dir.path(), CLAUDE, dir.path(), "idle");
    write_transcript(dir.path(), dir.path());
    seen.wait_for(|e| has(e, "session"));
    assert_eq!(saved.session_of(5).as_deref(), Some(SESSION));
    assert!(!saved.is_ended(5), "a running chat is not ended");
    fs::remove_file(dir.path().join("sessions").join(format!("{CLAUDE}.json"))).unwrap();
    let events = seen.wait_for(|e| has(e, "ended"));
    drop(stop);
    watcher.join().unwrap();
    // The store is closed only once every holder is gone, the watcher's own copy included.
    drop(seen);

    assert_eq!(
        events,
        [
            started(dir.path()),
            r#"state {"state":"working"}"#.to_owned(),
            r#"state {"state":"idle"}"#.to_owned(),
            format!("session {SESSION}"),
            r#"chat {"chat":"ended","saved":true}"#.to_owned(),
        ]
    );
    assert!(saved.is_ended(5));
    saved.flush();
    drop(saved);
    let restarted = Saved::open(&db.path().join("tree.db"), |_| {});
    assert_eq!(restarted.session_of(5).as_deref(), Some(SESSION));
    assert!(restarted.is_ended(5));
}

#[test]
fn a_chat_that_never_got_a_transcript_ends_unsaved() {
    let dir = TestDir::new();
    let seen = Seen::new(None);
    let (stop, stopped) = mpsc::channel::<()>();
    watch_shell(
        shell_on(dir.path(), now_ms() - 1_000, child),
        stopped,
        seen.clone(),
    )
    .unwrap();

    write_session(dir.path(), CLAUDE, dir.path(), "idle");
    seen.wait_for(|e| has(e, "started"));
    fs::remove_file(dir.path().join("sessions").join(format!("{CLAUDE}.json"))).unwrap();
    let events = seen.wait_for(|e| has(e, "ended"));
    drop(stop);

    assert_eq!(
        events.last().map(String::as_str),
        Some(r#"chat {"chat":"ended","saved":false}"#)
    );
    assert!(!has(&events, "session"));
}

#[test]
fn a_chat_closed_with_its_shell_by_cerberus_is_not_ended() {
    let dir = TestDir::new();
    let seen = Seen::new(None);
    let (stop, stopped) = mpsc::channel::<()>();
    let watcher = watch_shell(
        shell_on(dir.path(), now_ms() - 1_000, child),
        stopped,
        seen.clone(),
    )
    .unwrap();

    write_session(dir.path(), CLAUDE, dir.path(), "busy");
    seen.wait_for(|e| has(e, "working"));
    seen.open.store(false, Ordering::SeqCst);
    fs::remove_file(dir.path().join("sessions").join(format!("{CLAUDE}.json"))).unwrap();
    let until = Instant::now() + Duration::from_secs(2);
    let events = seen.wait_for(|_| Instant::now() > until);
    drop(stop);
    watcher.join().unwrap();

    assert!(!has(&events, "ended"), "{events:?}");
}

#[test]
fn a_folder_that_does_not_exist_is_not_handed_on() {
    let dir = TestDir::new();
    let file = SessionFile {
        session_id: None,
        cwd: Some(dir.path().join("gone").to_string_lossy().into_owned()),
        status: crate::sessions::Status::Idle,
        started_at: None,
    };
    let here = SessionFile {
        cwd: Some(dir.path().to_string_lossy().into_owned()),
        ..file.clone()
    };

    assert_eq!(folder(&file), None);
    assert_eq!(folder(&here), here.cwd);
}

/// A stand-in `claude` that registers a session file under the account the shell gives it, waits, and leaves.
fn stand_in(bin: &Path) -> PathBuf {
    fs::create_dir_all(bin).unwrap();
    #[cfg(windows)]
    {
        let script = bin.join("claude.ps1");
        fs::write(
            &script,
            format!(
                "$f = Join-Path $env:CLAUDE_CONFIG_DIR ('sessions\\' + $PID + '.json')\r\n\
                 Set-Content -Encoding ASCII -Path $f -Value ('{{\"pid\":' + $PID + ',\"sessionId\":\"{SESSION}\",\"status\":\"idle\"}}')\r\n\
                 Start-Sleep -Seconds 3\r\n\
                 Remove-Item $f\r\n"
            ),
        )
        .unwrap();
        let shim = bin.join("claude.cmd");
        fs::write(
            &shim,
            "@powershell -NoProfile -ExecutionPolicy Bypass -File \"%~dp0claude.ps1\"\r\n",
        )
        .unwrap();
        shim
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let script = bin.join("claude");
        fs::write(
            &script,
            format!(
                "#!/bin/sh\n\
                 f=\"$CLAUDE_CONFIG_DIR/sessions/$$.json\"\n\
                 printf '{{\"pid\":%s,\"sessionId\":\"{SESSION}\",\"status\":\"idle\"}}' $$ > \"$f\"\n\
                 sleep 3\n\
                 rm -f \"$f\"\n"
            ),
        )
        .unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        script
    }
}

#[test]
fn a_stand_in_claude_typed_into_a_shell_tabs_terminal_is_matched() {
    use crate::pty::{PtyManager, SpawnRequest};

    let dir = TestDir::new();
    let account = dir.path().join("account");
    fs::create_dir_all(account.join("sessions")).unwrap();
    let claude = stand_in(&dir.path().join("bin"));
    #[cfg(windows)]
    let (program, args, typed) = (
        "cmd",
        vec!["/q".to_owned(), "/k".to_owned()],
        format!("\"{}\"\r", claude.display()),
    );
    #[cfg(unix)]
    let (program, args, typed) = ("sh", Vec::new(), format!("'{}'\n", claude.display()));
    let request = SpawnRequest {
        program: program.to_owned(),
        args,
        cwd: None,
        cols: 80,
        rows: 24,
        claude_config_dir: Some(account.clone()),
        env: Vec::new(),
        dirs: Vec::new(),
        tab_env: Vec::new(),
    };
    let manager = PtyManager::default();
    let started_ms = now_ms() - 2_000;
    let (output, from_pty) = mpsc::channel::<Vec<u8>>();
    let id = manager
        .spawn(
            &request,
            move |bytes| {
                let _ = output.send(bytes);
            },
            |_| {},
        )
        .unwrap();
    let seen = Seen::new(None);
    let (stop, stopped) = mpsc::channel::<()>();
    watch_shell(
        Shell {
            config_dir: account,
            pid: manager.pid(id).unwrap(),
            cwd: dir.path().to_string_lossy().into_owned(),
            started_ms,
            probes: Probes::SYSTEM,
        },
        stopped,
        seen.clone(),
    )
    .unwrap();

    manager.write(id, typed.as_bytes()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    while !has(&seen.events(), "ended") && Instant::now() < deadline {
        // ConPTY waits for the terminal's answer to its cursor query before the shell runs.
        if let Ok(bytes) = from_pty.recv_timeout(Duration::from_millis(100))
            && bytes.windows(4).any(|w| w == b"\x1b[6n")
        {
            let _ = manager.write(id, b"\x1b[1;1R");
        }
    }
    let events = seen.events();
    drop(stop);
    manager.kill(id);

    assert_eq!(
        events,
        [
            r#"chat {"chat":"started","cwd":null}"#.to_owned(),
            r#"state {"state":"idle"}"#.to_owned(),
            r#"chat {"chat":"ended","saved":false}"#.to_owned(),
        ]
    );
}
