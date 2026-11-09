//! The `cerberus-live` helper: Claude Code runs it as a tab's status line command.

use std::ffi::OsString;
use std::io::{self, Read, Write};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::bridge::link;
use crate::live::event::{Event, Line, PIPE_VAR, TOKEN_VAR, status_from};

/// Passed when the user has a status line of his own, which reads what this one passes on.
pub const PASS: &str = "--pass";

/// Where the helper sends its line, from the tab's environment.
pub struct Env {
    pub pipe: Option<OsString>,
    pub token: Option<OsString>,
}

impl Env {
    pub fn from_process() -> Self {
        Self {
            pipe: std::env::var_os(PIPE_VAR),
            token: std::env::var_os(TOKEN_VAR),
        }
    }
}

/// Never fails the status line: a problem only means the app misses this one update.
pub fn run(args: &[String], stdin: impl Read, stdout: impl Write, env: &Env) {
    if args.first().map(String::as_str) != Some("status") {
        return;
    }
    let mut stdin = stdin;
    let mut input = Vec::new();
    // All of it, since his command must get every byte; only the parse is capped.
    if stdin.read_to_end(&mut input).is_err() {
        return;
    }
    if args.iter().any(|a| a == PASS) {
        // His command reads it first, so the app's send never delays his line.
        let _ = pass_on(stdout, &input);
    }
    let at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| u64::try_from(d.as_millis()).ok())
        .unwrap_or(0);
    if let Some(status) = status_from(&input, at) {
        let _ = send(env, Event::Status(status));
    }
}

fn pass_on(mut stdout: impl Write, input: &[u8]) -> io::Result<()> {
    stdout.write_all(input)?;
    stdout.flush()
}

fn send(env: &Env, event: Event) -> io::Result<()> {
    let (Some(pipe), Some(token)) = (&env.pipe, &env.token) else {
        return Ok(());
    };
    let (Some(pipe), Some(token)) = (pipe.to_str(), token.to_str()) else {
        return Ok(());
    };
    let Some(stream) = link::connect(pipe)? else {
        return Ok(());
    };
    let line = Line {
        token: token.to_owned(),
        event,
    };
    let mut bytes = serde_json::to_vec(&line)?;
    bytes.push(b'\n');
    (&stream).write_all(&bytes)?;
    link::finish(&stream)
}
