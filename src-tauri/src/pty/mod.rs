mod coalesce;
mod flow;

use std::collections::HashMap;
use std::fmt::Display;
use std::io::{self, ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{self, SyncSender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::Duration;

use portable_pty::{Child, ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};
use serde::{Deserialize, Serialize, Serializer};

use crate::programs::Finder;
use crate::pty::coalesce::Batching;
use crate::pty::flow::FlowGate;

pub type SessionId = u32;

const BATCHING: Batching = Batching {
    window: Duration::from_millis(4),
    max: 256 * 1024,
};
// Unread output the reader may queue before it stops reading and the program blocks on its writes.
const QUEUED_READS: usize = 16;
const READ_CHUNK: usize = 64 * 1024;
const UNACKED_HIGH: usize = 1024 * 1024;
const UNACKED_LOW: usize = 256 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum PtyError {
    #[error("program not found: {0}")]
    NotFound(String),
    #[error("folder not found: {0}")]
    NoSuchDir(String),
    #[error("could not start {program}: {reason}")]
    Spawn { program: String, reason: String },
    #[error("pseudo-terminal failed: {0}")]
    Pty(String),
    #[error("could not start a terminal thread: {0}")]
    Thread(io::Error),
    #[error("no terminal session {0}")]
    NoSession(SessionId),
    #[error("terminal input failed: {0}")]
    Io(#[from] io::Error),
}

impl Serialize for PtyError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

#[derive(Debug, Deserialize)]
pub struct SpawnRequest {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
    pub cols: u16,
    pub rows: u16,
    /// Set by the core from the tab's account, never by the webview.
    #[serde(skip)]
    pub claude_config_dir: Option<PathBuf>,
}

struct Session {
    master: Mutex<Box<dyn MasterPty + Send>>,
    writer: Mutex<Box<dyn Write + Send>>,
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
    flow: Arc<FlowGate>,
    pid: Option<u32>,
}

type Sessions = Arc<Mutex<HashMap<SessionId, Arc<Session>>>>;

/// How a program ended: its code, and whether it ended by itself rather than by `kill` or `kill_all`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Exit {
    pub code: Option<u32>,
    pub by_itself: bool,
}

#[derive(Default)]
pub struct PtyManager {
    sessions: Sessions,
    next_id: AtomicU32,
}

// A panicked holder leaves the map or handle usable, so poisoning is not a reason to fail.
fn lock<T: ?Sized>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn size(cols: u16, rows: u16) -> PtySize {
    PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    }
}

impl PtyManager {
    /// Starts `request.program`; output goes to `on_output`, then `on_exit` once when it ends.
    pub fn spawn(
        &self,
        request: &SpawnRequest,
        on_output: impl FnMut(Vec<u8>) + Send + 'static,
        on_exit: impl FnOnce(Exit) + Send + 'static,
    ) -> Result<SessionId, PtyError> {
        let command = command_for(request)?;
        let pair = native_pty_system()
            .openpty(size(request.cols, request.rows))
            .map_err(pty_failed)?;
        let mut child = pair
            .slave
            .spawn_command(command)
            .map_err(|e| PtyError::Spawn {
                program: request.program.clone(),
                reason: e.to_string(),
            })?;
        // The pseudo-console closes only when every handle to it is gone, so keep just the master's.
        drop(pair.slave);
        let handles = pair
            .master
            .try_clone_reader()
            .and_then(|reader| Ok((reader, pair.master.take_writer()?)));
        let (reader, writer) = match handles {
            Ok(handles) => handles,
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(pty_failed(e));
            }
        };

        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let flow = Arc::new(FlowGate::new(UNACKED_HIGH, UNACKED_LOW));
        let gate = Arc::clone(&flow);
        let mut on_output = on_output;
        let on_output = move |bytes: Vec<u8>| {
            gate.reserve(bytes.len());
            on_output(bytes);
        };
        let session = Session {
            master: Mutex::new(pair.master),
            writer: Mutex::new(writer),
            killer: Mutex::new(child.clone_killer()),
            flow,
            pid: child.process_id(),
        };
        // Registered before the waiter starts, or a fast exit would find nothing to close.
        lock(&self.sessions).insert(id, Arc::new(session));
        if let Err(e) = self.start_threads(id, reader, child, on_output, on_exit) {
            self.kill(id);
            return Err(PtyError::Thread(e));
        }
        Ok(id)
    }

    fn start_threads(
        &self,
        id: SessionId,
        reader: Box<dyn Read + Send>,
        mut child: Box<dyn Child + Send + Sync>,
        on_output: impl FnMut(Vec<u8>) + Send + 'static,
        on_exit: impl FnOnce(Exit) + Send + 'static,
    ) -> io::Result<()> {
        let (tx, rx) = mpsc::sync_channel(QUEUED_READS);
        thread::Builder::new().spawn(move || read_into(reader, &tx))?;
        let pump =
            thread::Builder::new().spawn(move || coalesce::pump(&rx, &BATCHING, on_output))?;
        let sessions = Arc::clone(&self.sessions);
        thread::Builder::new().spawn(move || {
            let code = child.wait().ok().map(|status| status.exit_code());
            // Closing the pseudo-console ends the reader, so all output is sent before the exit.
            // `kill` and `kill_all` take the session out first, so finding it here means it ended by itself.
            let closed = lock(&sessions).remove(&id);
            let by_itself = closed.is_some();
            if let Some(session) = closed {
                end(&session);
            }
            // A panicked pump lost only output; the tab still needs its exit.
            let _ = pump.join();
            on_exit(Exit { code, by_itself });
        })?;
        Ok(())
    }

    pub fn write(&self, id: SessionId, data: &[u8]) -> Result<(), PtyError> {
        let session = self.session(id)?;
        let mut writer = lock(&session.writer);
        writer.write_all(data)?;
        writer.flush()?;
        Ok(())
    }

    pub fn resize(&self, id: SessionId, cols: u16, rows: u16) -> Result<(), PtyError> {
        lock(&self.session(id)?.master)
            .resize(size(cols, rows))
            .map_err(pty_failed)
    }

    /// Returns output credit once the webview has drawn `bytes` of it.
    pub fn ack(&self, id: SessionId, bytes: usize) {
        if let Ok(session) = self.session(id) {
            session.flow.ack(bytes);
        }
    }

    /// Ends the program and closes its pseudo-console, which ends anything else attached to it.
    pub fn kill(&self, id: SessionId) {
        let session = lock(&self.sessions).remove(&id);
        if let Some(session) = session {
            end(&session);
        }
    }

    pub fn pid(&self, id: SessionId) -> Option<u32> {
        self.session(id).ok()?.pid
    }

    pub fn kill_all(&self) {
        let sessions: Vec<_> = lock(&self.sessions).drain().map(|(_, s)| s).collect();
        sessions.iter().for_each(|session| end(session));
    }

    fn session(&self, id: SessionId) -> Result<Arc<Session>, PtyError> {
        lock(&self.sessions)
            .get(&id)
            .cloned()
            .ok_or(PtyError::NoSession(id))
    }
}

fn command_for(request: &SpawnRequest) -> Result<CommandBuilder, PtyError> {
    let program = Finder::from_env()
        .find(&request.program)
        .ok_or_else(|| PtyError::NotFound(request.program.clone()))?;
    let mut command = CommandBuilder::new(&program);
    command.args(&request.args);
    if let Some(cwd) = &request.cwd {
        if !Path::new(cwd).is_dir() {
            return Err(PtyError::NoSuchDir(cwd.clone()));
        }
        command.cwd(cwd);
    }
    strip_inherited_session_vars(&mut command);
    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "truecolor");
    #[cfg(target_os = "macos")]
    if lacks_locale(|name| std::env::var_os(name).is_some()) {
        command.env("LANG", "en_US.UTF-8");
    }
    match &request.claude_config_dir {
        Some(dir) => command.env("CLAUDE_CONFIG_DIR", dir),
        // An inherited value would move a default-account tab onto another account.
        None => command.env_remove("CLAUDE_CONFIG_DIR"),
    }
    Ok(command)
}

// An app opened from Finder gets no locale, and then shells and Claude Code mangle anything outside ASCII.
#[cfg(any(target_os = "macos", test))]
fn lacks_locale(is_set: impl Fn(&str) -> bool) -> bool {
    !["LANG", "LC_ALL", "LC_CTYPE"].into_iter().any(is_set)
}

// Set by a Claude Code session for its own shells; settings such as `CLAUDE_CODE_USE_BEDROCK` are not here.
const SESSION_VARS: [&str; 11] = [
    "CLAUDECODE",
    "CLAUDE_CODE_CHILD_SESSION",
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_EXECPATH",
    "CLAUDE_CODE_MESSAGING_SOCKET",
    "CLAUDE_CODE_MESSAGING_TOKEN",
    "CLAUDE_CODE_SESSION_ATTENDED",
    "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_EFFORT",
    "CLAUDE_PID",
    "AI_AGENT",
];

// Inherited when Cerberus runs from a Claude Code shell; `claude` in a tab would then draw no colour and save no transcript.
fn strip_inherited_session_vars(command: &mut CommandBuilder) {
    let names: Vec<String> = command
        .iter_full_env_as_str()
        .map(|(name, _)| name.to_owned())
        .collect();
    let is_session = |name: &String| SESSION_VARS.contains(&name.to_ascii_uppercase().as_str());
    // A NO_COLOR next to a session's variables came from Claude Code, not from the user.
    let from_claude = names.iter().any(is_session);
    let removed: Vec<String> = names
        .into_iter()
        .filter(|name| is_session(name) || (from_claude && name.eq_ignore_ascii_case("NO_COLOR")))
        .collect();
    for name in removed {
        command.env_remove(name);
    }
}

fn pty_failed(error: impl Display) -> PtyError {
    PtyError::Pty(error.to_string())
}

fn end(session: &Session) {
    session.flow.close();
    // Fails only when the process already exited, which is the outcome wanted.
    let _ = lock(&session.killer).kill();
}

fn read_into(mut reader: Box<dyn Read + Send>, tx: &SyncSender<Vec<u8>>) {
    let mut buf = vec![0; READ_CHUNK];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                if tx.send(buf[..n].to_vec()).is_err() {
                    break;
                }
            }
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            // A closed pseudo-console reads as a broken pipe: the normal end of output.
            Err(_) => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::Receiver;
    use std::time::Instant;

    use super::*;

    const TIMEOUT: Duration = Duration::from_secs(20);

    // The same checks on each platform, through its own shell.
    #[cfg(windows)]
    const SHELL: (&str, &[&str]) = ("cmd", &["/q", "/k"]);
    #[cfg(unix)]
    const SHELL: (&str, &[&str]) = ("sh", &[]);
    #[cfg(windows)]
    const ECHO: (&str, &[&str]) = ("cmd", &["/c", "echo", "hello-pty"]);
    #[cfg(unix)]
    const ECHO: (&str, &[&str]) = ("echo", &["hello-pty"]);
    // The shell expands what was typed, so the echoed keys alone cannot match.
    #[cfg(windows)]
    const TYPED: (&[u8], &str) = (b"echo typed-%OS%\r", "typed-Windows_NT");
    #[cfg(unix)]
    const TYPED: (&[u8], &str) = (b"echo typed-$((40+2))\r", "typed-42");
    #[cfg(windows)]
    const STILL: (&[u8], &str) = (b"echo still-%OS%\r", "still-Windows_NT");
    #[cfg(unix)]
    const STILL: (&[u8], &str) = (b"echo still-$((40+2))\r", "still-42");
    #[cfg(windows)]
    const SIZE_COMMAND: &[u8] = b"mode con\r";
    #[cfg(unix)]
    const SIZE_COMMAND: &[u8] = b"stty size\r";
    #[cfg(windows)]
    const NO_FOLDER: &str = r"C:\no\such\folder\cerberus";
    #[cfg(unix)]
    const NO_FOLDER: &str = "/no/such/folder/cerberus";

    fn shell(manager: &PtyManager) -> (SessionId, Receiver<Event>) {
        start(manager, SHELL.0, SHELL.1)
    }

    enum Event {
        Output(Vec<u8>),
        Exit(Exit),
    }

    fn start(manager: &PtyManager, program: &str, args: &[&str]) -> (SessionId, Receiver<Event>) {
        let (tx, rx) = mpsc::channel();
        let exit_tx = tx.clone();
        let request = SpawnRequest {
            program: program.to_owned(),
            args: args.iter().map(|a| (*a).to_owned()).collect(),
            cwd: None,
            cols: 80,
            rows: 24,
            claude_config_dir: None,
        };
        let id = manager
            .spawn(
                &request,
                move |bytes| {
                    let _ = tx.send(Event::Output(bytes));
                },
                move |exit| {
                    let _ = exit_tx.send(Event::Exit(exit));
                },
            )
            .unwrap();
        (id, rx)
    }

    /// Collects output until `done` holds or the program exits, answering ConPTY's cursor query.
    fn collect(
        manager: &PtyManager,
        id: SessionId,
        rx: &Receiver<Event>,
        done: impl Fn(&str) -> bool,
    ) -> (String, Option<Exit>) {
        let deadline = Instant::now() + TIMEOUT;
        let mut text = String::new();
        while Instant::now() < deadline {
            match rx.recv_timeout(deadline - Instant::now()) {
                Ok(Event::Output(bytes)) => {
                    let chunk = String::from_utf8_lossy(&bytes);
                    for _ in chunk.matches("[6n") {
                        let _ = manager.write(id, b"[1;1R");
                    }
                    text.push_str(&chunk);
                    if done(&text) {
                        return (text, None);
                    }
                }
                Ok(Event::Exit(exit)) => return (text, Some(exit)),
                Err(_) => break,
            }
        }
        (text, None)
    }

    #[test]
    fn unacked_output_stalls_at_the_high_mark_and_kill_still_ends_it() {
        let file = std::env::temp_dir().join(format!("cerberus-flood-{}.txt", std::process::id()));
        std::fs::write(
            &file,
            "0123456789abcdefghijklmnopqrstuvwxyz\r\n".repeat(200_000),
        )
        .unwrap();
        let manager = PtyManager::default();
        #[cfg(windows)]
        let (id, rx) = start(&manager, "cmd", &["/c", "type", file.to_str().unwrap()]);
        #[cfg(unix)]
        let (id, rx) = start(&manager, "cat", &[file.to_str().unwrap()]);

        let mut received = 0;
        while let Ok(Event::Output(bytes)) = rx.recv_timeout(Duration::from_secs(1)) {
            if bytes.windows(4).any(|w| w == b"\x1b[6n") {
                let _ = manager.write(id, b"\x1b[1;1R");
            }
            received += bytes.len();
        }
        manager.kill(id);
        let (_, exit) = collect(&manager, id, &rx, |_| false);
        std::fs::remove_file(&file).unwrap();

        // The gate admits one more batch below the high mark, and a batch can pass `max` by a chunk.
        let most = UNACKED_HIGH - 1 + BATCHING.max - 1 + READ_CHUNK;
        assert!(
            (UNACKED_HIGH..=most).contains(&received),
            "received {received} bytes with nothing acked"
        );
        assert!(exit.is_some(), "no exit after kill");
    }

    #[test]
    fn output_streams_and_the_exit_code_follows() {
        let manager = PtyManager::default();
        let (id, rx) = start(&manager, ECHO.0, ECHO.1);

        let (text, exit) = collect(&manager, id, &rx, |_| false);

        assert!(text.contains("hello-pty"), "output was {text:?}");
        assert_eq!(
            exit,
            Some(Exit {
                code: Some(0),
                by_itself: true
            })
        );
        assert!(lock(&manager.sessions).is_empty());
    }

    #[test]
    fn input_reaches_the_program() {
        let manager = PtyManager::default();
        let (id, rx) = shell(&manager);

        manager.write(id, TYPED.0).unwrap();
        let (text, _) = collect(&manager, id, &rx, |t| t.contains(TYPED.1));

        assert!(text.contains(TYPED.1), "output was {text:?}");
        manager.kill(id);
    }

    #[test]
    fn resize_reaches_a_live_session() {
        let manager = PtyManager::default();
        let (id, rx) = shell(&manager);

        manager.resize(id, 101, 33).unwrap();
        manager.write(id, SIZE_COMMAND).unwrap();
        let (text, _) = collect(&manager, id, &rx, |t| t.contains("101") && t.contains("33"));

        assert!(
            text.contains("101") && text.contains("33"),
            "output was {text:?}"
        );
        manager.kill(id);
    }

    #[test]
    fn kill_ends_the_child() {
        let manager = PtyManager::default();
        let (id, rx) = shell(&manager);

        manager.kill(id);
        let (_, exit) = collect(&manager, id, &rx, |_| false);

        assert_eq!(exit.map(|e| e.by_itself), Some(false), "no killed exit");
        assert!(matches!(
            manager.write(id, b"x"),
            Err(PtyError::NoSession(_))
        ));
        manager.kill(id);
    }

    #[test]
    fn killing_one_session_leaves_the_others_running() {
        let manager = PtyManager::default();
        let (closed_id, closed) = shell(&manager);
        let (open_id, open) = shell(&manager);

        manager.kill(closed_id);
        let (_, exit) = collect(&manager, closed_id, &closed, |_| false);
        manager.write(open_id, STILL.0).unwrap();
        let (text, open_exit) = collect(&manager, open_id, &open, |t| t.contains(STILL.1));

        assert!(exit.is_some(), "no exit after kill");
        assert_eq!(open_exit, None, "the other session ended");
        assert!(text.contains(STILL.1), "output was {text:?}");
        manager.kill(open_id);
    }

    #[test]
    fn kill_all_ends_every_session() {
        let manager = PtyManager::default();
        let (first_id, first) = shell(&manager);
        let (second_id, second) = shell(&manager);

        manager.kill_all();

        for (id, rx) in [(first_id, &first), (second_id, &second)] {
            let exit = collect(&manager, id, rx, |_| false).1;
            assert_eq!(exit.map(|e| e.by_itself), Some(false));
        }
    }

    #[test]
    fn a_missing_program_is_an_error() {
        let manager = PtyManager::default();
        let request = SpawnRequest {
            program: "no-such-program-cerberus".to_owned(),
            args: Vec::new(),
            cwd: None,
            cols: 80,
            rows: 24,
            claude_config_dir: None,
        };

        let result = manager.spawn(&request, |_| {}, |_| {});

        assert!(matches!(result, Err(PtyError::NotFound(_))));
    }

    #[test]
    fn a_missing_folder_is_an_error() {
        let manager = PtyManager::default();
        let request = SpawnRequest {
            program: SHELL.0.to_owned(),
            args: Vec::new(),
            cwd: Some(NO_FOLDER.to_owned()),
            cols: 80,
            rows: 24,
            claude_config_dir: None,
        };

        let result = manager.spawn(&request, |_| {}, |_| {});

        assert!(matches!(result, Err(PtyError::NoSuchDir(_))));
    }

    fn shell_under(claude_config_dir: Option<&str>) -> CommandBuilder {
        command_for(&SpawnRequest {
            program: SHELL.0.to_owned(),
            args: Vec::new(),
            cwd: None,
            cols: 80,
            rows: 24,
            claude_config_dir: claude_config_dir.map(PathBuf::from),
        })
        .unwrap()
    }

    #[test]
    fn a_tab_under_a_named_account_gets_its_config_dir() {
        let command = shell_under(Some(r"C:\Users\u\.claude-acc2"));

        assert_eq!(
            command.get_env("CLAUDE_CONFIG_DIR"),
            Some(std::ffi::OsStr::new(r"C:\Users\u\.claude-acc2"))
        );
    }

    #[test]
    fn a_tab_under_the_default_account_has_no_config_dir() {
        assert_eq!(shell_under(None).get_env("CLAUDE_CONFIG_DIR"), None);
    }

    #[test]
    fn variables_from_a_parent_claude_session_are_removed() {
        let mut command = CommandBuilder::new("cmd");
        command.env_clear();
        for name in [
            "NO_COLOR",
            "CLAUDECODE",
            "CLAUDE_CODE_CHILD_SESSION",
            "claude_code_messaging_socket",
            "CLAUDE_PID",
            "AI_AGENT",
        ] {
            command.env(name, "1");
        }
        command.env("CLAUDE_CONFIG_DIR", r"C:\Users\u\.claude-acc2");
        command.env("CLAUDE_CODE_USE_BEDROCK", "1");

        strip_inherited_session_vars(&mut command);

        assert_eq!(
            env_names(&command),
            ["CLAUDE_CODE_USE_BEDROCK", "CLAUDE_CONFIG_DIR"]
        );
    }

    #[test]
    fn no_color_stays_when_no_claude_session_set_it() {
        let mut command = CommandBuilder::new("cmd");
        command.env_clear();
        command.env("NO_COLOR", "1");

        strip_inherited_session_vars(&mut command);

        assert_eq!(env_names(&command), ["NO_COLOR"]);
    }

    // With no job object on Unix, this is what ends a tab's program when the app dies.
    #[cfg(unix)]
    #[test]
    fn closing_the_terminal_hangs_up_its_program() {
        let pair = native_pty_system().openpty(size(80, 24)).unwrap();
        let mut command = CommandBuilder::new("/bin/sh");
        // The trailing `true` keeps sh in charge, so the hangup reaches a shell and its child.
        command.args(["-c", "sleep 30; true"]);
        let mut child = pair.slave.spawn_command(command).unwrap();
        let pid = child.process_id();
        drop(pair);

        let deadline = Instant::now() + TIMEOUT;
        let mut ended = false;
        while !ended && Instant::now() < deadline {
            ended = child.try_wait().unwrap().is_some();
            std::thread::sleep(Duration::from_millis(20));
        }
        if !ended {
            let _ = child.kill();
        }

        assert!(ended, "pid {pid:?} outlived its terminal");
    }

    #[test]
    fn a_locale_is_added_only_when_none_is_set() {
        assert!(lacks_locale(|_| false));
        assert!(!lacks_locale(|name| name == "LANG"));
        assert!(!lacks_locale(|name| name == "LC_CTYPE"));
    }

    fn env_names(command: &CommandBuilder) -> Vec<String> {
        let mut names: Vec<_> = command
            .iter_full_env_as_str()
            .map(|(name, _)| name.to_ascii_uppercase())
            .collect();
        names.sort();
        names
    }
}
