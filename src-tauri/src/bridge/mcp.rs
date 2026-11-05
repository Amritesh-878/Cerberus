//! The helper every `claude` tab starts as its `cerberus` MCP server: newline-delimited JSON-RPC on stdio.

use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde_json::{Value, json};

use crate::bridge::inbox::{self, Letter};
use crate::bridge::link;
use crate::bridge::lock;
use crate::bridge::wire::{
    Ask, AskKind, BRIDGE_VAR, CHILD_TOKEN_VAR, MOST_LINE, MOST_MESSAGE, Order, Outcome, Reply,
    SOCKET_VAR, TOKEN_VAR, Written,
};

const PROTOCOL: &str = "2025-06-18";
const FACTS_TRIES: u32 = 300;
const FACTS_GAP: Duration = Duration::from_secs(1);

const INSTRUCTIONS: &str = "Cerberus runs this chat in a tab. To message a Claude chat on another account, call send_to_tab (list_tabs shows the tabs it can reach). The user reads every message first and approves, edits or rejects it, so a call can wait a while; its result says what happened. A message from another Cerberus tab starts with \"[Cerberus: a message from tab ...]\": reply to it with send_to_tab to that tab, not with SendMessage, which cannot reach other accounts. Chats on this same account keep using SendMessage.";

fn tools() -> Value {
    json!([
        {
            "name": "send_to_tab",
            "description": "Send a message to a Claude chat in another Cerberus tab, on another account. The user approves, edits or rejects it before it is delivered; the result says which, and a reply comes back as a new message.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "tab": { "type": "string", "description": "The tab id, from list_tabs or from the message being answered." },
                    "message": { "type": "string", "description": "The whole message, at most 64 KB." }
                },
                "required": ["tab", "message"],
                "additionalProperties": false
            }
        },
        {
            "name": "list_tabs",
            "description": "List the Claude tabs on other accounts that send_to_tab can reach: id, \"account · project / tab\", and whether each is busy.",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false },
            "annotations": { "readOnlyHint": true }
        }
    ])
}

/// What the helper is given by Cerberus and by Claude Code, read once at start.
#[derive(Debug, Clone, Default)]
pub struct Env {
    pub bridge: Option<String>,
    pub token: Option<String>,
    pub socket: Option<String>,
    pub child_token: Option<String>,
    pub config_dir: Option<PathBuf>,
}

impl Env {
    pub fn from_process() -> Self {
        let var = |name: &str| std::env::var(name).ok().filter(|v| !v.is_empty());
        Self {
            bridge: var(BRIDGE_VAR),
            token: var(TOKEN_VAR),
            socket: var(SOCKET_VAR),
            child_token: var(CHILD_TOKEN_VAR),
            config_dir: var("CLAUDE_CONFIG_DIR")
                .map(PathBuf::from)
                .or_else(|| dirs::home_dir().map(|home| home.join(".claude"))),
        }
    }

    fn messaging(&self) -> bool {
        self.socket.is_some() && self.child_token.is_some()
    }
}

/// One ask per connection to the app's bridge.
struct Client {
    name: String,
    token: String,
}

impl Client {
    fn connect(&self) -> io::Result<link::Stream> {
        link::connect(&self.name)?
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Cerberus is not running"))
    }

    fn write_ask(&self, stream: &link::Stream, kind: AskKind) -> io::Result<()> {
        let ask = Ask {
            token: self.token.clone(),
            kind,
        };
        let mut line = serde_json::to_vec(&ask)?;
        line.push(b'\n');
        let mut out = stream;
        out.write_all(&line)?;
        out.flush()
    }

    fn ask(&self, kind: AskKind) -> io::Result<Reply> {
        let stream = self.connect()?;
        self.write_ask(&stream, kind)?;
        let mut line = String::new();
        BufReader::new((&stream).take(MOST_LINE)).read_line(&mut line)?;
        if line.is_empty() {
            return Err(io::Error::other("Cerberus does not know this tab"));
        }
        Ok(serde_json::from_str(&line)?)
    }
}

type Out = Arc<Mutex<Box<dyn Write + Send>>>;

fn send(out: &Out, message: &Value) {
    let mut out = lock(out);
    // Claude Code gone means the helper is done anyway.
    let _ = writeln!(out, "{message}").and_then(|()| out.flush());
}

/// What one incoming JSON-RPC message asks for.
#[derive(Debug, PartialEq)]
enum Step {
    Reply(Value),
    Call {
        id: Value,
        name: String,
        args: Value,
    },
    Cancel(String),
    Nothing,
}

fn step(message: &Value) -> Step {
    let id = message.get("id").cloned();
    let params = message.get("params").cloned().unwrap_or(Value::Null);
    let result = |id: Value, result: Value| {
        Step::Reply(json!({"jsonrpc": "2.0", "id": id, "result": result}))
    };
    match (message.get("method").and_then(Value::as_str), id) {
        (Some("initialize"), Some(id)) => {
            let asked = params.get("protocolVersion").and_then(Value::as_str);
            result(
                id,
                json!({
                    "protocolVersion": asked.unwrap_or(PROTOCOL),
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "cerberus", "version": env!("CARGO_PKG_VERSION") },
                    "instructions": INSTRUCTIONS
                }),
            )
        }
        (Some("ping"), Some(id)) => result(id, json!({})),
        (Some("tools/list"), Some(id)) => result(id, json!({ "tools": tools() })),
        (Some("tools/call"), Some(id)) => Step::Call {
            id,
            name: params
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            args: params.get("arguments").cloned().unwrap_or(Value::Null),
        },
        (Some("notifications/cancelled"), None) => params
            .get("requestId")
            .map_or(Step::Nothing, |id| Step::Cancel(id.to_string())),
        (Some(_), Some(id)) => Step::Reply(json!({
            "jsonrpc": "2.0", "id": id,
            "error": { "code": -32601, "message": "no such method" }
        })),
        _ => Step::Nothing,
    }
}

fn text_result(text: &str, is_error: bool) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "isError": is_error })
}

fn call_tool(client: Option<&Client>, call: String, name: &str, args: &Value) -> Value {
    let Some(client) = client else {
        return text_result(
            "Cerberus messages are off in this tab: it was not started by Cerberus with messages on.",
            true,
        );
    };
    match name {
        "list_tabs" => match client.ask(AskKind::List) {
            Ok(Reply::Tabs { tabs }) if tabs.is_empty() => text_result(
                "No Claude tabs on other accounts are open in Cerberus.",
                false,
            ),
            Ok(Reply::Tabs { tabs }) => {
                let lines: Vec<String> = tabs
                    .iter()
                    .map(|t| {
                        let state = if t.busy { "busy" } else { "idle" };
                        format!("tab \"{}\": {} ({state})", t.id, t.label)
                    })
                    .collect();
                text_result(&lines.join("\n"), false)
            }
            Ok(_) => text_result("Cerberus sent an answer this helper cannot read.", true),
            Err(e) => text_result(&format!("Cerberus is not reachable: {e}"), true),
        },
        "send_to_tab" => send_to_tab(client, call, args),
        other => text_result(&format!("There is no tool named {other}."), true),
    }
}

fn send_to_tab(client: &Client, call: String, args: &Value) -> Value {
    let tab = match args.get("tab") {
        Some(Value::String(tab)) => tab.clone(),
        Some(Value::Number(tab)) => tab.to_string(),
        _ => return text_result("Not sent: send_to_tab needs a tab id.", true),
    };
    let Some(message) = args.get("message").and_then(Value::as_str) else {
        return text_result("Not sent: send_to_tab needs a message.", true);
    };
    if message.len() > MOST_MESSAGE {
        let reason = format!(
            "Not sent: the message is {} bytes and the most is {MOST_MESSAGE}. Shorten it or split it.",
            message.len()
        );
        return text_result(&reason, true);
    }
    let kind = AskKind::Send {
        call,
        to: tab,
        message: message.to_owned(),
    };
    let outcome = match client.ask(kind) {
        Ok(Reply::Outcome { outcome }) => outcome,
        Ok(_) => Outcome::Failed {
            reason: "Cerberus sent an answer this helper cannot read.".to_owned(),
        },
        Err(e) => Outcome::Failed {
            reason: format!("Cerberus closed or is not reachable ({e})."),
        },
    };
    let (text, is_error) = outcome.tool_text();
    text_result(&text, is_error)
}

/// Writes an approved message into this tab's own inbox, with the token Claude Code gave its children.
fn write_inbox(env: &Env, from_pipe: Option<&str>, from_name: &str, body: &str) -> Written {
    let no = |reason: &str| Written::No {
        reason: reason.to_owned(),
    };
    let (Some(socket), Some(token)) = (&env.socket, &env.child_token) else {
        return no("Claude Code gave this tab no inbox");
    };
    let Some(from_pipe) = from_pipe else {
        return no("the sender has no inbox address");
    };
    let Some(facts) = env
        .config_dir
        .as_deref()
        .and_then(|dir| inbox::find_session(dir, socket))
    else {
        return no("this tab's session file was not found");
    };
    if !inbox::passes_gate(facts.version.as_deref(), facts.peer_protocol) {
        return no("this Claude Code version is not on the tested list");
    }
    let Some(session_id) = facts.session_id.as_deref() else {
        return no("this tab's session has no id yet");
    };
    let letter = Letter {
        from_pipe,
        from_name,
        body,
        session_id,
    };
    let posted = inbox::msg_id()
        .and_then(|id| inbox::frame(&letter, token, &id).map_err(io::Error::from))
        .and_then(|bytes| inbox::post(socket, &bytes));
    match posted {
        Ok(()) => Written::Yes,
        Err(e) => Written::No {
            reason: format!("the inbox write failed: {e}"),
        },
    }
}

/// Reports the tab's Claude Code version once its session file appears, which can wait on a trust prompt.
fn report_facts(client: &Client, env: &Env) {
    let (Some(socket), Some(dir)) = (&env.socket, &env.config_dir) else {
        return;
    };
    for _ in 0..FACTS_TRIES {
        if let Some(facts) = inbox::find_session(dir, socket) {
            let kind = AskKind::Facts {
                version: facts.version,
                peer_protocol: facts.peer_protocol,
            };
            if let Err(e) = client.ask(kind) {
                eprintln!("cerberus helper: could not report the session: {e}");
            }
            return;
        }
        thread::sleep(FACTS_GAP);
    }
}

/// Says hello, then carries out the app's orders for as long as the app keeps the link open.
fn follow_orders(client: &Arc<Client>, env: &Arc<Env>, run: String) -> io::Result<()> {
    let stream = client.connect()?;
    client.write_ask(
        &stream,
        AskKind::Listen {
            messaging: env.messaging(),
            pipe: env.socket.clone(),
            run,
        },
    )?;
    for line in BufReader::new(&stream).lines() {
        match serde_json::from_str::<Order>(&line?)? {
            Order::Ready => {
                let (client, env) = (Arc::clone(client), Arc::clone(env));
                thread::spawn(move || report_facts(&client, &env));
            }
            Order::Deliver {
                id,
                from_pipe,
                from_name,
                body,
            } => {
                let written = write_inbox(env, from_pipe.as_deref(), &from_name, &body);
                client.ask(AskKind::Delivered { id, written })?;
            }
        }
    }
    Ok(())
}

/// Serves MCP until Claude Code closes stdin.
pub fn run(input: impl BufRead, output: impl Write + Send + 'static, env: Env) {
    let out: Out = Arc::new(Mutex::new(Box::new(output)));
    let client = match (&env.bridge, &env.token) {
        (Some(name), Some(token)) => Some(Arc::new(Client {
            name: name.clone(),
            token: token.clone(),
        })),
        _ => None,
    };
    let env = Arc::new(env);
    // Call ids start over in each run, so the bridge sees them under this run's own tag.
    let run_tag = link::random_hex(8).unwrap_or_else(|_| std::process::id().to_string());
    if let Some(client) = &client {
        let (client, env, run) = (Arc::clone(client), Arc::clone(&env), run_tag.clone());
        thread::spawn(move || {
            if let Err(e) = follow_orders(&client, &env, run) {
                eprintln!("cerberus helper: the link to Cerberus ended: {e}");
            }
        });
    }
    let cancelled: Arc<Mutex<HashMap<String, bool>>> = Arc::default();
    for line in input.lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            send(
                &out,
                &json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": "parse error"}}),
            );
            continue;
        };
        match step(&message) {
            Step::Reply(reply) => send(&out, &reply),
            Step::Call { id, name, args } => {
                let key = id.to_string();
                lock(&cancelled).insert(key.clone(), false);
                let call = format!("{run_tag}:{key}");
                let (out, client, cancelled) =
                    (Arc::clone(&out), client.clone(), Arc::clone(&cancelled));
                thread::spawn(move || {
                    let result = call_tool(client.as_deref(), call, &name, &args);
                    // A cancelled request gets no response.
                    if lock(&cancelled).remove(&key) == Some(false) {
                        send(&out, &json!({"jsonrpc": "2.0", "id": id, "result": result}));
                    }
                });
            }
            Step::Cancel(key) => {
                if let Some(flag) = lock(&cancelled).get_mut(&key) {
                    *flag = true;
                    if let Some(client) = client.clone() {
                        let call = format!("{run_tag}:{key}");
                        thread::spawn(move || {
                            let _ = client.ask(AskKind::Cancel { call });
                        });
                    }
                }
            }
            Step::Nothing => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initialize_names_the_tool_and_says_how_to_reply() {
        let Step::Reply(reply) = step(&json!({
            "jsonrpc": "2.0", "id": 0, "method": "initialize",
            "params": { "protocolVersion": "2025-03-26", "capabilities": {} }
        })) else {
            panic!("no reply");
        };

        let result = &reply["result"];
        assert_eq!(result["protocolVersion"], "2025-03-26");
        assert_eq!(result["serverInfo"]["name"], "cerberus");
        let instructions = result["instructions"].as_str().unwrap();
        assert!(instructions.contains("send_to_tab"));
        assert!(instructions.contains("not with SendMessage"));
    }

    #[test]
    fn the_tools_are_send_to_tab_and_list_tabs() {
        let Step::Reply(reply) = step(&json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}))
        else {
            panic!("no reply");
        };

        let names: Vec<&str> = reply["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["send_to_tab", "list_tabs"]);
    }

    #[test]
    fn calls_cancels_and_unknown_methods_are_told_apart() {
        assert_eq!(
            step(&json!({"jsonrpc": "2.0", "id": 4, "method": "tools/call",
                "params": {"name": "send_to_tab", "arguments": {"tab": "2", "message": "hi"}}})),
            Step::Call {
                id: json!(4),
                name: "send_to_tab".to_owned(),
                args: json!({"tab": "2", "message": "hi"}),
            }
        );
        assert_eq!(
            step(
                &json!({"jsonrpc": "2.0", "method": "notifications/cancelled", "params": {"requestId": 4}})
            ),
            Step::Cancel("4".to_owned())
        );
        assert_eq!(
            step(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"})),
            Step::Nothing
        );
        let Step::Reply(reply) =
            step(&json!({"jsonrpc": "2.0", "id": 9, "method": "resources/list"}))
        else {
            panic!("no reply");
        };
        assert_eq!(reply["error"]["code"], -32601);
    }

    #[test]
    fn a_tab_not_started_by_cerberus_says_messages_are_off() {
        let result = call_tool(None, "1".to_owned(), "send_to_tab", &json!({}));

        assert_eq!(result["isError"], true);
        assert!(
            result["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("off in this tab")
        );
    }

    #[test]
    fn a_message_over_64_kb_is_refused_before_it_is_sent() {
        let client = Client {
            name: link::name_for("nobody-listens"),
            token: "t".to_owned(),
        };
        let long = "x".repeat(MOST_MESSAGE + 1);

        let result = send_to_tab(
            &client,
            "1".to_owned(),
            &json!({"tab": "2", "message": long}),
        );

        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(
            text.starts_with("Not sent: the message is 65537 bytes"),
            "{text}"
        );
    }

    #[test]
    fn without_an_inbox_nothing_is_written() {
        assert_eq!(
            write_inbox(&Env::default(), Some("p"), "n", "b"),
            Written::No {
                reason: "Claude Code gave this tab no inbox".to_owned()
            }
        );
    }
}
