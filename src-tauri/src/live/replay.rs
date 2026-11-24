//! Real Claude Code sessions, recorded on a fake account and scrubbed, replayed through the helper's parsing and the live core.

use std::path::Path;

use serde_json::Value;

use crate::live::Live;
use crate::live::event::{Event, Line, status_from};
use crate::live::hooks::hook_from;
use crate::live::tests::start;

/// A reply, a Write, a second reply, `/compact`, on 2.1.331 with an API key: no limits are reported.
const SESSION_2_1_331: &str = include_str!("fixtures/session-2.1.331.jsonl");

/// The fixture's paths in this platform's spelling: `C:\fake\x` is `/fake/x` on Unix.
fn native(text: &str) -> String {
    if cfg!(windows) {
        text.to_owned()
    } else {
        text.replace(r"C:\\fake\\", "/fake/").replace(r"\\", "/")
    }
}

struct Step {
    status: bool,
    input: String,
}

fn steps(recording: &str) -> Vec<Step> {
    native(recording)
        .lines()
        .map(|line| {
            let line: Value = serde_json::from_str(line).unwrap();
            Step {
                status: line["source"] == "status",
                input: line["input"].to_string(),
            }
        })
        .collect()
}

/// What the helper would send for each step, handed to the core in order, as the pipe can't promise.
fn play(live: &Live, token: &str, steps: &[Step]) {
    let core = live.core.as_ref().unwrap();
    for (at, step) in (1_u64..).zip(steps) {
        let event = if step.status {
            status_from(step.input.as_bytes(), at).map(Event::Status)
        } else {
            hook_from(step.input.as_bytes(), at, true)
        };
        if let Some(event) = event {
            core.receive(Line {
                token: token.to_owned(),
                event,
            });
        }
    }
}

#[test]
fn a_recorded_session_fills_the_cards_keeps_the_reply_and_compacts() {
    let fx = start(85);
    let cwd = if cfg!(windows) {
        r"C:\fake\work"
    } else {
        "/fake/work"
    };
    let ticket = fx
        .live
        .register(7, 2, Path::new(cwd), fx.dir.path())
        .unwrap();
    let steps = steps(SESSION_2_1_331);
    let compact = steps
        .iter()
        .position(|s| s.input.contains("\"PreCompact\""))
        .expect("the recording has a /compact");

    play(&fx.live, &ticket.token, &steps[..compact]);
    let view = fx.live.view();

    let tab = &view.tabs[0];
    assert_eq!(tab.model.as_deref(), Some("Opus 5.5"));
    assert_eq!(tab.context, Some(24.0));
    assert_eq!(tab.context_tokens, Some(240_000));
    assert_eq!(tab.outputs.len(), 1, "the Write hook gave one output");
    assert_eq!(tab.outputs[0].name, "fake-out.txt");
    assert!(tab.tasks.is_empty());
    assert_eq!(fx.live.last_reply(7).as_deref(), Some("OK."));
    let meter = view.meters.iter().find(|m| m.account == 2);
    assert!(
        meter.is_some_and(|m| m.silent && m.five_hour.is_none()),
        "an API key reports no limits, so three replies mark it silent: {meter:?}"
    );

    play(&fx.live, &ticket.token, &steps[compact..]);
    let view = fx.live.view();

    assert_eq!(view.tabs[0].context, Some(0.0));
    assert_eq!(view.tabs[0].context_tokens, Some(0));
    assert!(
        view.tabs[0].tasks.is_empty(),
        "compacting's subagent stop, with no start, leaves no task behind"
    );
    assert_eq!(
        fx.live.last_reply(7).as_deref(),
        Some("OK."),
        "a subagent's summary is not the chat's reply"
    );
    let shown = serde_json::to_string(&view).unwrap();
    assert!(!shown.contains("summary"), "{shown}");
}
