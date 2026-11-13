//! What leaves the helper from a hook: a written file's path and each task's id, kind and state.

use serde::{Deserialize, Serialize};

use crate::live::event::{Event, MOST_INPUT};

const MOST_ID: usize = 64;
const MOST_NAME: usize = 64;
const MOST_PATH: usize = 4096;
/// More running tasks than this in one report is not a real session.
const MOST_TASKS: usize = 64;
/// A task's description shows in one row; the rest is Claude Code's own `/tasks` view.
const MOST_DESCRIPTION: usize = 120;

/// The tools whose `file_path` (or `notebook_path`) is a file the chat wrote.
const WRITE_TOOLS: [&str; 4] = ["Write", "Edit", "MultiEdit", "NotebookEdit"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Wrote {
    /// Milliseconds since the Unix epoch, when the helper ran.
    pub at: u64,
    pub tool: String,
    pub path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TaskKind {
    Shell,
    Subagent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub kind: TaskKind,
    pub running: bool,
    /// A subagent's type, such as `general-purpose`; a name from settings, not the chat's words.
    pub agent_type: Option<String>,
    /// Claude Code's short summary, held in memory only and never the command it runs.
    #[serde(default)]
    pub description: Option<String>,
}

/// The session's background tasks as Claude Code listed them when a turn ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tasks {
    pub at: u64,
    pub tasks: Vec<Task>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Subagent {
    pub at: u64,
    pub id: String,
    pub agent_type: Option<String>,
    pub started: bool,
}

// Only these names are read; every other field, tool input and output included, is skipped unkept.
#[derive(Deserialize)]
struct HookIn {
    #[serde(default)]
    hook_event_name: Option<String>,
    #[serde(default)]
    tool_name: Option<String>,
    #[serde(default)]
    tool_input: Option<PathsIn>,
    #[serde(default)]
    background_tasks: Option<Vec<TaskIn>>,
    #[serde(default)]
    agent_id: Option<String>,
    #[serde(default)]
    agent_type: Option<String>,
}

#[derive(Deserialize)]
struct PathsIn {
    #[serde(default)]
    file_path: Option<String>,
    #[serde(default)]
    notebook_path: Option<String>,
}

#[derive(Deserialize)]
struct TaskIn {
    #[serde(default)]
    id: Option<String>,
    #[serde(default, rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    agent_type: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

/// The kept fields of one hook input, or `None` when it is not one Cerberus listens to.
pub fn hook_from(input: &[u8], at: u64) -> Option<Event> {
    if input.len() > MOST_INPUT {
        return None;
    }
    // A field of an unexpected type fails the whole input; a hook Cerberus can't read reports nothing.
    let parsed: HookIn = serde_json::from_slice(input).ok()?;
    match parsed.hook_event_name.as_deref()? {
        "PostToolUse" => {
            let tool = parsed
                .tool_name
                .filter(|t| WRITE_TOOLS.contains(&t.as_str()))?;
            let paths = parsed.tool_input?;
            let path = paths.file_path.or(paths.notebook_path)?;
            checked(Event::Wrote(Wrote { at, tool, path }))
        }
        "Stop" => {
            let listed = parsed.background_tasks?;
            Some(Event::Tasks(Tasks {
                at,
                tasks: listed
                    .into_iter()
                    .filter_map(task)
                    .take(MOST_TASKS)
                    .collect(),
            }))
        }
        name @ ("SubagentStart" | "SubagentStop") => {
            let id = parsed.agent_id.filter(|id| id_like(id))?;
            Some(Event::Subagent(Subagent {
                at,
                id,
                agent_type: parsed.agent_type.as_deref().and_then(name_like),
                started: name == "SubagentStart",
            }))
        }
        _ => None,
    }
}

/// The same checks again on the app's side, since anything holding the token could send a line.
pub fn checked(event: Event) -> Option<Event> {
    match event {
        Event::Status(_) => Some(event),
        Event::Wrote(w) => {
            let tool = WRITE_TOOLS.contains(&w.tool.as_str()).then_some(w.tool)?;
            let path = path_like(&w.path).then_some(w.path)?;
            Some(Event::Wrote(Wrote {
                at: w.at,
                tool,
                path,
            }))
        }
        Event::Tasks(t) => Some(Event::Tasks(Tasks {
            at: t.at,
            tasks: t
                .tasks
                .into_iter()
                .filter(|task| id_like(&task.id))
                .map(|task| Task {
                    agent_type: task.agent_type.as_deref().and_then(name_like),
                    description: task.description.as_deref().and_then(description_like),
                    ..task
                })
                .take(MOST_TASKS)
                .collect(),
        })),
        Event::Subagent(s) => id_like(&s.id).then(|| {
            Event::Subagent(Subagent {
                agent_type: s.agent_type.as_deref().and_then(name_like),
                ..s
            })
        }),
    }
}

fn task(found: TaskIn) -> Option<Task> {
    let kind = match found.kind.as_deref()? {
        "shell" => TaskKind::Shell,
        "subagent" => TaskKind::Subagent,
        _ => return None,
    };
    Some(Task {
        id: found.id.filter(|id| id_like(id))?,
        kind,
        running: found.status.as_deref() == Some("running"),
        agent_type: found.agent_type.as_deref().and_then(name_like),
        description: found.description.as_deref().and_then(description_like),
    })
}

/// One line of plain text: control characters become spaces, runs of space close up, and it stops at the cap.
fn description_like(s: &str) -> Option<String> {
    let line: String = s
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let capped: String = line.chars().take(MOST_DESCRIPTION).collect();
    (!capped.is_empty()).then_some(capped)
}

fn id_like(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= MOST_ID
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn name_like(s: &str) -> Option<String> {
    let ok = !s.is_empty()
        && s.len() <= MOST_NAME
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_:.".contains(&b));
    ok.then(|| s.to_owned())
}

// Absolute, as Claude Code passes it, and one line with no control characters.
fn path_like(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= MOST_PATH
        && !s.chars().any(char::is_control)
        && std::path::Path::new(s).is_absolute()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn the_largest_tasks_or_wrote_event_fits_in_one_line() {
        let fits = |event: Event| {
            let line = crate::live::event::Line {
                token: "f".repeat(64),
                event: checked(event).unwrap(),
            };
            let bytes = serde_json::to_string(&line).unwrap().len();
            assert!(
                u64::try_from(bytes).unwrap() <= crate::live::event::MOST_LINE,
                "{bytes} bytes"
            );
        };
        // Four UTF-8 bytes per character is the most a description can cost.
        let task = |n: usize| Task {
            id: format!("{n:0>64}"),
            kind: TaskKind::Subagent,
            running: true,
            agent_type: Some("a".repeat(MOST_NAME)),
            description: Some("\u{1F600}".repeat(MOST_DESCRIPTION)),
        };

        fits(Event::Tasks(Tasks {
            at: u64::MAX,
            tasks: (0..MOST_TASKS).map(task).collect(),
        }));
        // Each backslash doubles in JSON.
        fits(Event::Wrote(Wrote {
            at: u64::MAX,
            tool: "NotebookEdit".to_owned(),
            // An absolute root on this platform; backslashes after it are ordinary characters on Unix.
            path: {
                let root = if cfg!(windows) { r"C:\" } else { "/" };
                format!("{root}{}", r"\".repeat(MOST_PATH - root.len()))
            },
        }));
    }

    /// Shaped like 2.1.319's PostToolUse for Write, with fake secrets everywhere but the kept fields.
    pub const FAKE_WRITE: &str = r#"{
        "session_id": "747dc9d6-05c0-4623-aa7a-06d23f871b07",
        "transcript_path": "C:\\Users\\someone\\FAKE-SECRET-transcript.jsonl",
        "cwd": "C:\\FAKE-SECRET-cwd",
        "prompt_id": "FAKE-SECRET-prompt-id",
        "permission_mode": "acceptEdits",
        "hook_event_name": "PostToolUse",
        "tool_name": "Write",
        "tool_input": {"file_path": "C:\\work\\out.txt", "content": "FAKE-SECRET-content sk-ant-000"},
        "tool_response": {"type": "create", "filePath": "C:/work/out.txt", "content": "FAKE-SECRET-content"},
        "tool_use_id": "toolu_FAKE-SECRET",
        "duration_ms": 29
    }"#;

    /// Shaped like 2.1.319's Stop with one shell and one subagent running.
    pub const FAKE_STOP: &str = r#"{
        "session_id": "747dc9d6-05c0-4623-aa7a-06d23f871b07",
        "hook_event_name": "Stop",
        "stop_hook_active": false,
        "last_assistant_message": "FAKE-SECRET-reply",
        "background_tasks": [
            {"id": "b2pmf3p7m", "type": "shell", "status": "running",
             "description": "Run the\u001b[31m dev\nserver", "command": "FAKE-SECRET-command --token x"},
            {"id": "aec41a0ac0cf62716", "type": "subagent", "status": "running",
             "description": "Review the diff", "agent_type": "general-purpose"},
            {"id": "x", "type": "cron", "status": "running"}
        ],
        "session_crons": [{"prompt": "FAKE-SECRET-cron"}]
    }"#;

    #[cfg(windows)]
    const WRITTEN: &str = r"C:\work\out.txt";
    #[cfg(not(windows))]
    const WRITTEN: &str = "/work/out.txt";

    fn native(input: &str) -> String {
        // On Unix an absolute path starts at `/`, so the fixture's Windows path is swapped.
        if cfg!(windows) {
            input.to_owned()
        } else {
            input.replace(r#"C:\\work\\out.txt"#, "/work/out.txt")
        }
    }

    fn json_of(kept: &Event) -> String {
        serde_json::to_string(kept).unwrap()
    }

    #[test]
    fn a_write_keeps_the_tool_and_path_and_nothing_of_its_content() {
        let kept = hook_from(native(FAKE_WRITE).as_bytes(), 9).unwrap();

        assert_eq!(
            kept,
            Event::Wrote(Wrote {
                at: 9,
                tool: "Write".into(),
                path: WRITTEN.into()
            })
        );
        assert!(!json_of(&kept).contains("FAKE-SECRET"));
    }

    #[test]
    fn a_stop_keeps_each_tasks_id_kind_state_and_description_never_its_command() {
        let kept = hook_from(FAKE_STOP.as_bytes(), 9).unwrap();

        assert_eq!(
            kept,
            Event::Tasks(Tasks {
                at: 9,
                tasks: vec![
                    Task {
                        id: "b2pmf3p7m".into(),
                        kind: TaskKind::Shell,
                        running: true,
                        agent_type: None,
                        description: Some("Run the [31m dev server".into()),
                    },
                    Task {
                        id: "aec41a0ac0cf62716".into(),
                        kind: TaskKind::Subagent,
                        running: true,
                        agent_type: Some("general-purpose".into()),
                        description: Some("Review the diff".into()),
                    },
                ]
            })
        );
        assert!(!json_of(&kept).contains("FAKE-SECRET"));
    }

    #[test]
    fn subagents_start_and_stop_by_id() {
        let start = r#"{"hook_event_name":"SubagentStart","agent_id":"aec41a0ac0cf62716",
            "agent_type":"general-purpose","session_id":"s","transcript_path":"FAKE-SECRET"}"#;
        let stop = r#"{"hook_event_name":"SubagentStop","agent_id":"aec41a0ac0cf62716",
            "agent_transcript_path":"FAKE-SECRET","last_assistant_message":"FAKE-SECRET",
            "background_tasks":[{"id":"aec41a0ac0cf62716","type":"subagent","status":"running"}]}"#;

        assert_eq!(
            hook_from(start.as_bytes(), 1),
            Some(Event::Subagent(Subagent {
                at: 1,
                id: "aec41a0ac0cf62716".into(),
                agent_type: Some("general-purpose".into()),
                started: true,
            }))
        );
        let stopped = hook_from(stop.as_bytes(), 2).unwrap();
        assert!(matches!(
            stopped,
            Event::Subagent(Subagent { started: false, .. })
        ));
        assert!(!json_of(&stopped).contains("FAKE-SECRET"));
    }

    #[test]
    fn other_tools_and_events_and_odd_values_report_nothing() {
        let bash = r#"{"hook_event_name":"PostToolUse","tool_name":"Bash",
            "tool_input":{"command":"FAKE-SECRET","file_path":"C:\\x"}}"#;
        let relative = r#"{"hook_event_name":"PostToolUse","tool_name":"Write",
            "tool_input":{"file_path":"out.txt"}}"#;
        let notify = r#"{"hook_event_name":"Notification","message":"FAKE-SECRET"}"#;
        let bad_id = r#"{"hook_event_name":"SubagentStart","agent_id":"../../x"}"#;

        for input in [bash, relative, notify, bad_id, "not json", "[]"] {
            assert_eq!(hook_from(input.as_bytes(), 1), None, "{input}");
        }
    }

    #[test]
    fn a_notebook_edit_gives_its_notebook() {
        let input = native(
            r#"{"hook_event_name":"PostToolUse","tool_name":"NotebookEdit",
            "tool_input":{"notebook_path":"C:\\work\\out.txt","new_source":"FAKE-SECRET"}}"#,
        );

        let kept = hook_from(input.as_bytes(), 1).unwrap();

        assert!(matches!(kept, Event::Wrote(Wrote { ref path, .. }) if path == WRITTEN));
    }

    #[test]
    fn the_app_checks_a_line_again() {
        let forged = Event::Wrote(Wrote {
            at: 1,
            tool: "Bash".into(),
            path: WRITTEN.into(),
        });
        let control = Event::Wrote(Wrote {
            at: 1,
            tool: "Write".into(),
            path: format!("{WRITTEN}\u{1b}[2J"),
        });

        assert_eq!(checked(forged), None);
        assert_eq!(checked(control), None);
    }
}
