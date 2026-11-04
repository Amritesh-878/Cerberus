//! The whole road: a helper's MCP call, the app's queue and decision, and the target helper's inbox write.

use std::collections::HashMap;
use std::fs;
use std::io::{self, BufRead, BufReader, PipeWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::bridge::check::prefill;
use crate::bridge::inbox::{self, Letter};
use crate::bridge::mcp::{self, Env};
use crate::bridge::wire::{Ask, AskKind};
use crate::bridge::{
    Bridge, Decided, Decision, Hooks, Label, Note, Snapshot, TabNote, Ticket, link, lock,
};
use crate::saved::TabId;

const WAIT: Duration = Duration::from_secs(20);

/// What was typed into which PTY.
type Typed = Arc<Mutex<Vec<(u32, Vec<u8>)>>>;
// What Claude Code draws when a peer message lands, cursor moves and all.
const ARRIVAL: &[u8] = b"\x1b[1m\xe2\x80\xba Message\x1b[1Cfrom @a-p-t1: hi";

struct TestHooks(Mutex<Sender<Snapshot>>);

impl Hooks for TestHooks {
    fn label(&self, tab: TabId) -> Label {
        let account = if tab % 2 == 1 { "A" } else { "B" };
        Label {
            account: account.to_owned(),
            place: format!("p / t{tab}"),
        }
    }

    fn changed(&self, snapshot: Snapshot) {
        let _ = lock(&self.0).send(snapshot);
    }
}

/// A helper run in this process, as Claude Code would run it, with its stdio in reach.
struct Helper {
    stdin: PipeWriter,
    lines: Receiver<Value>,
    next: u64,
}

impl Helper {
    fn start(ticket: &Ticket, socket: Option<&str>, config_dir: Option<&Path>) -> Self {
        let env_of = |name: &str| {
            ticket
                .env
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, v)| v.to_string_lossy().into_owned())
        };
        let env = Env {
            bridge: env_of("CERBERUS_BRIDGE"),
            token: env_of("CERBERUS_BRIDGE_TOKEN"),
            socket: socket.map(str::to_owned),
            child_token: socket.map(|_| format!("child-{}", &ticket.token[..6])),
            config_dir: config_dir.map(Path::to_path_buf),
        };
        let (stdin_r, stdin) = io::pipe().unwrap();
        let (stdout_r, stdout_w) = io::pipe().unwrap();
        thread::spawn(move || mcp::run(BufReader::new(stdin_r), stdout_w, env));
        let (tx, lines) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout_r).lines() {
                let Ok(line) = line else { return };
                let _ = tx.send(serde_json::from_str(&line).unwrap());
            }
        });
        Self {
            stdin,
            lines,
            next: 1,
        }
    }

    fn send_to_tab(&mut self, tab: &str, message: &str) -> u64 {
        self.call("send_to_tab", &json!({ "tab": tab, "message": message }))
    }

    fn call(&mut self, name: &str, args: &Value) -> u64 {
        let id = self.next;
        self.next += 1;
        let request = json!({"jsonrpc": "2.0", "id": id, "method": "tools/call",
            "params": {"name": name, "arguments": args}});
        writeln!(self.stdin, "{request}").unwrap();
        id
    }

    fn cancel(&mut self, id: u64) {
        let note = json!({"jsonrpc": "2.0", "method": "notifications/cancelled",
            "params": {"requestId": id, "reason": "user pressed Esc"}});
        writeln!(self.stdin, "{note}").unwrap();
    }

    /// The text of the next tool result, and whether it is an error.
    fn result(&self) -> (String, bool) {
        let reply = self.lines.recv_timeout(WAIT).expect("no tool result");
        let result = &reply["result"];
        (
            result["content"][0]["text"].as_str().unwrap().to_owned(),
            result["isError"].as_bool().unwrap(),
        )
    }
}

/// An inbox pipe standing in for a Claude Code session's; each connection's bytes are passed on whole.
fn fake_inbox() -> (String, Receiver<Vec<u8>>) {
    let name = link::new_name().unwrap();
    let mut listener = link::Listener::bind(&name).unwrap();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        while let Ok(Ok(stream)) = listener.accept() {
            let mut bytes = Vec::new();
            let _ = (&stream).read_to_end(&mut bytes);
            if tx.send(bytes).is_err() {
                return;
            }
        }
    });
    (name, rx)
}

/// A config dir whose session file names `socket` as its inbox.
fn config_dir(tag: &str, socket: &str, version: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "cerberus-bridge-{tag}-{}-{}",
        std::process::id(),
        link::random_hex(4).unwrap()
    ));
    fs::create_dir_all(dir.join("sessions")).unwrap();
    let file = json!({"pid": 1, "sessionId": format!("session-{tag}"), "messagingSocketPath": socket,
        "version": version, "peerProtocol": 1, "status": "idle"});
    fs::write(dir.join("sessions").join("1.json"), file.to_string()).unwrap();
    dir
}

struct Tab {
    ticket: Ticket,
    helper: Helper,
    inbox: Receiver<Vec<u8>>,
    socket: String,
    dir: PathBuf,
}

struct World {
    bridge: Bridge,
    snapshots: Receiver<Snapshot>,
    tabs: HashMap<TabId, Tab>,
}

impl World {
    /// Tabs 1 and 3 on account A, tab 2 on account B; `version` is what tab 2's Claude Code reports.
    fn new(version: &str) -> Self {
        let (tx, snapshots) = mpsc::channel();
        let bridge = Bridge::start(TestHooks(Mutex::new(tx))).unwrap();
        let mut tabs = HashMap::new();
        for (tab, home, version) in [
            (1, "acc-a", "2.1.317"),
            (2, "acc-b", version),
            (3, "acc-a", "2.1.317"),
        ] {
            let (socket, inbox) = fake_inbox();
            let dir = config_dir(home, &socket, version);
            let ticket = bridge.register(tab, Path::new(home)).unwrap();
            bridge.attach(&ticket.token, 100 + tab);
            let helper = Helper::start(&ticket, Some(&socket), Some(&dir));
            tabs.insert(
                tab,
                Tab {
                    ticket,
                    helper,
                    inbox,
                    socket,
                    dir,
                },
            );
        }
        let world = Self {
            bridge,
            snapshots,
            tabs,
        };
        world.until(|state| (1..=3).all(|t| state.by_tab(t).is_some_and(|l| l.facts.is_some())));
        world
    }

    fn until(&self, done: impl Fn(&super::State) -> bool) {
        let core = self.bridge.0.as_ref().unwrap();
        let deadline = Instant::now() + WAIT;
        while !done(&core.state()) {
            assert!(Instant::now() < deadline, "timed out");
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn helper(&mut self, tab: TabId) -> &mut Helper {
        &mut self.tabs.get_mut(&tab).unwrap().helper
    }

    /// The id of the one message waiting, once it is there.
    fn waiting(&self) -> u64 {
        self.until(|state| !state.queue.views().is_empty());
        self.bridge.snapshot().pending[0].id
    }

    /// Plays Claude Code drawing the peer message once the tab's inbox gets it, and hands back the bytes.
    fn show_arrival(&self, tab: TabId) -> Vec<u8> {
        let target = &self.tabs[&tab];
        let bytes = target
            .inbox
            .recv_timeout(WAIT)
            .expect("nothing reached the inbox");
        self.bridge.output(&target.ticket.token, ARRIVAL);
        bytes
    }
}

impl Drop for World {
    fn drop(&mut self) {
        for tab in self.tabs.values() {
            let _ = fs::remove_dir_all(&tab.dir);
        }
    }
}

fn no_typing(_: u32, _: &[u8]) -> io::Result<()> {
    panic!("nothing should be typed");
}

fn body(text: &str) -> String {
    format!(
        "[Cerberus: a message from tab \"1\" (A · p / t1), approved by the user. Reply with send_to_tab to tab \"1\".]\n{text}"
    )
}

#[test]
fn an_approved_message_reaches_the_other_accounts_inbox_byte_for_byte() {
    let mut world = World::new("2.1.317");
    world.helper(1).send_to_tab("2", "what is 2+2?");
    let id = world.waiting();
    assert_eq!(world.bridge.snapshot().pending[0].text, "what is 2+2?");
    assert!(
        world.tabs[&2].inbox.try_recv().is_err(),
        "delivered before approval"
    );

    let bridge = world.bridge.clone();
    let deciding = thread::spawn(move || {
        bridge.decide(
            id,
            Decision::Approve {
                text: "what is 2+2?".to_owned(),
            },
            no_typing,
        )
    });
    let bytes = world.show_arrival(2);
    let decided = deciding.join().unwrap().unwrap();

    assert_eq!(decided, Decided::Delivered { from: 1, to: 2 });
    let text = String::from_utf8(bytes.clone()).unwrap();
    let user: Value = serde_json::from_str(text.lines().nth(1).unwrap()).unwrap();
    let letter = Letter {
        from_pipe: &world.tabs[&1].socket,
        from_name: "A · p / t1",
        body: &body("what is 2+2?"),
        session_id: "session-acc-b",
    };
    let token = format!("child-{}", &world.tabs[&2].ticket.token[..6]);
    let expected = inbox::frame(&letter, &token, user["msg_id"].as_str().unwrap()).unwrap();
    assert_eq!(bytes, expected);
    let (result, is_error) = world.helper(1).result();
    assert!(result.starts_with("Delivered to B · p / t2."), "{result}");
    assert!(!is_error);
    assert!(world.bridge.snapshot().pending.is_empty());
}

#[test]
fn a_reply_takes_the_same_road_back() {
    let mut world = World::new("2.1.317");
    world.helper(2).send_to_tab("1", "4");
    let id = world.waiting();
    let bridge = world.bridge.clone();
    let deciding = thread::spawn(move || {
        bridge.decide(
            id,
            Decision::Approve {
                text: "4".to_owned(),
            },
            no_typing,
        )
    });
    let bytes = world.show_arrival(1);

    assert_eq!(
        deciding.join().unwrap().unwrap(),
        Decided::Delivered { from: 2, to: 1 }
    );
    assert!(
        String::from_utf8(bytes)
            .unwrap()
            .contains("from-name=\\\"B · p / t2\\\"")
    );
    assert!(
        world
            .helper(2)
            .result()
            .0
            .starts_with("Delivered to A · p / t1.")
    );
}

#[test]
fn a_rejected_message_goes_nowhere_and_the_sender_is_told() {
    let mut world = World::new("2.1.317");
    world.helper(1).send_to_tab("2", "delete everything");
    let id = world.waiting();

    let decided = world
        .bridge
        .decide(id, Decision::Reject, no_typing)
        .unwrap();

    assert_eq!(decided, Decided::Rejected);
    let (result, is_error) = world.helper(1).result();
    assert_eq!(
        result,
        "Not sent: the user rejected this message to B · p / t2."
    );
    assert!(is_error);
    assert!(
        world.tabs[&2]
            .inbox
            .recv_timeout(Duration::from_millis(300))
            .is_err()
    );
}

#[test]
fn an_edited_message_is_delivered_as_edited_and_the_sender_sees_the_edit() {
    let mut world = World::new("2.1.317");
    world.helper(1).send_to_tab("2", "a long question");
    let id = world.waiting();
    let bridge = world.bridge.clone();
    let deciding = thread::spawn(move || {
        bridge.decide(
            id,
            Decision::Approve {
                text: "shorter".to_owned(),
            },
            no_typing,
        )
    });
    let bytes = String::from_utf8(world.show_arrival(2)).unwrap();

    assert_eq!(
        deciding.join().unwrap().unwrap(),
        Decided::Delivered { from: 1, to: 2 }
    );
    assert!(
        bytes.contains("\\nshorter\\n</cross-session-message>"),
        "{bytes}"
    );
    assert!(!bytes.contains("a long question"));
    assert!(
        world
            .helper(1)
            .result()
            .0
            .ends_with("it now reads:\nshorter")
    );
}

#[test]
fn an_untested_version_is_typed_in_without_enter_and_the_tab_says_prefill_only() {
    let mut world = World::new("9.9.9");
    world.helper(1).send_to_tab("2", "hello\r\nthere");
    let id = world.waiting();
    let typed: Typed = Arc::default();
    let into = Arc::clone(&typed);

    let decided = world.bridge.decide(
        id,
        Decision::Approve {
            text: "hello\r\nthere".to_owned(),
        },
        move |pty, bytes| {
            lock(&into).push((pty, bytes.to_vec()));
            Ok(())
        },
    );

    assert_eq!(decided.unwrap(), Decided::Prefilled { from: 1, to: 2 });
    let typed = lock(&typed).clone();
    assert_eq!(
        typed,
        [(102, prefill(&body("hello\r\nthere")).into_bytes())]
    );
    assert!(!typed[0].1.contains(&b'\r'));
    assert!(world.helper(1).result().0.contains("presses Enter there"));
    assert!(world.bridge.snapshot().notes.contains(&TabNote {
        tab: 2,
        note: Note::PrefillOnly
    }));
    assert!(world.tabs[&2].inbox.try_recv().is_err());
}

#[test]
fn an_inbox_write_the_tab_never_shows_falls_back_to_prefill() {
    let mut world = World::new("2.1.317");
    world.helper(1).send_to_tab("2", "anyone there?");
    let id = world.waiting();
    let typed = Arc::new(Mutex::new(0));
    let into = Arc::clone(&typed);

    let decided = world.bridge.decide(
        id,
        Decision::Approve {
            text: "anyone there?".to_owned(),
        },
        move |_, _| {
            *lock(&into) += 1;
            Ok(())
        },
    );

    assert_eq!(decided.unwrap(), Decided::Prefilled { from: 1, to: 2 });
    assert_eq!(*lock(&typed), 1);
    assert!(
        world.tabs[&2].inbox.recv_timeout(WAIT).is_ok(),
        "the inbox write was not tried"
    );
}

#[test]
fn a_cancelled_call_leaves_the_queue_and_gets_no_answer() {
    let mut world = World::new("2.1.317");
    let call = world.helper(1).send_to_tab("2", "never mind");
    let id = world.waiting();

    world.helper(1).cancel(call);
    world.until(|state| state.queue.views().is_empty());

    assert!(
        world
            .helper(1)
            .lines
            .recv_timeout(Duration::from_millis(300))
            .is_err()
    );
    assert!(matches!(
        world.bridge.decide(id, Decision::Reject, no_typing),
        Err(super::BridgeError::Gone)
    ));
}

#[test]
fn several_messages_wait_in_order() {
    let mut world = World::new("2.1.317");
    world.helper(1).send_to_tab("2", "first");
    world.until(|state| state.queue.views().len() == 1);
    world.helper(3).send_to_tab("2", "second");
    world.until(|state| state.queue.views().len() == 2);

    let texts: Vec<String> = world
        .bridge
        .snapshot()
        .pending
        .into_iter()
        .map(|p| p.text)
        .collect();

    assert_eq!(texts, ["first", "second"]);
    let mut seen = Vec::new();
    while let Ok(snapshot) = world.snapshots.try_recv() {
        seen.push(snapshot.pending.len());
    }
    assert_eq!(seen.last(), Some(&2));
}

#[test]
fn the_same_account_is_refused_and_pointed_at_send_message() {
    let mut world = World::new("2.1.317");

    world.helper(1).send_to_tab("3", "hi");

    let (result, is_error) = world.helper(1).result();
    assert!(result.contains("use SendMessage"), "{result}");
    assert!(is_error);
    assert!(world.bridge.snapshot().pending.is_empty());
}

#[test]
fn list_tabs_shows_only_other_accounts() {
    let mut world = World::new("2.1.317");

    world.helper(1).call("list_tabs", &json!({}));

    assert_eq!(
        world.helper(1).result(),
        ("tab \"2\": B · p / t2 (idle)".to_owned(), false)
    );
}

#[test]
fn a_target_that_closes_fails_the_waiting_sender_plainly() {
    let mut world = World::new("2.1.317");
    world.helper(1).send_to_tab("2", "hi");
    world.waiting();

    world.bridge.closed(&world.tabs[&2].ticket.token.clone());

    let (result, is_error) = world.helper(1).result();
    assert_eq!(
        result,
        "Not sent: the target tab closed before the user approved it."
    );
    assert!(is_error);
    assert!(world.bridge.snapshot().pending.is_empty());
}

#[test]
fn a_closing_app_fails_every_waiting_sender_plainly() {
    let mut world = World::new("2.1.317");
    world.helper(1).send_to_tab("2", "hi");
    world.waiting();

    world.bridge.shutdown();

    assert_eq!(
        world.helper(1).result().0,
        "Not sent: Cerberus closed before the user decided."
    );
}

#[test]
fn a_helper_that_never_says_hello_turns_its_tab_off() {
    let (tx, snapshots) = mpsc::channel();
    let bridge = Bridge::start(TestHooks(Mutex::new(tx))).unwrap();

    bridge.register(5, Path::new("acc-a")).unwrap();

    let deadline = Instant::now() + WAIT;
    loop {
        let snapshot = snapshots.recv_timeout(WAIT).unwrap();
        if snapshot.notes
            == [TabNote {
                tab: 5,
                note: Note::Off,
            }]
        {
            break;
        }
        assert!(Instant::now() < deadline);
    }
}

#[test]
fn a_wrong_token_gets_no_answer() {
    let (tx, _snapshots) = mpsc::channel();
    let bridge = Bridge::start(TestHooks(Mutex::new(tx))).unwrap();
    let name = bridge.0.as_ref().unwrap().name.clone();
    let stream = link::connect(&name).unwrap().unwrap();
    let ask = Ask {
        token: "guess".to_owned(),
        kind: AskKind::List,
    };

    let mut out = &stream;
    writeln!(out, "{}", serde_json::to_string(&ask).unwrap()).unwrap();
    let mut answer = String::new();
    let _ = (&stream).read_to_string(&mut answer);

    assert_eq!(answer, "");
}
