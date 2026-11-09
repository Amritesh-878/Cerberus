use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::json;

use super::*;
use crate::accounts::testdir::TestDir;
use crate::live::event::tests::FAKE_INPUT;
use crate::live::helper::{self, Env};

#[derive(Clone)]
struct FakeSink {
    views: Sender<View>,
    warnings: Arc<Mutex<Vec<Warning>>>,
    warn_at: u8,
}

impl Sink for FakeSink {
    fn accounts(&self) -> Vec<AccountId> {
        vec![1, 2, 3]
    }
    fn warn_at(&self) -> u8 {
        self.warn_at
    }
    fn changed(&self, view: View) {
        let _ = self.views.send(view);
    }
    fn warned(&self, warning: Warning) {
        self.warnings.lock().unwrap().push(warning);
    }
}

struct Fixture {
    live: Live,
    views: Receiver<View>,
    warnings: Arc<Mutex<Vec<Warning>>>,
    dir: TestDir,
}

fn start(warn_at: u8) -> Fixture {
    let dir = TestDir::new();
    let (tx, views) = mpsc::channel();
    let warnings = Arc::default();
    let sink = FakeSink {
        views: tx,
        warnings: Arc::clone(&warnings),
        warn_at,
    };
    let helper = PathBuf::from("/app/cerberus-live");
    let live = Live::start(dir.path(), helper, Shell::Posix, sink).unwrap();
    Fixture {
        live,
        views,
        warnings,
        dir,
    }
}

fn env_of(ticket: &Ticket) -> Env {
    let get = |name| {
        ticket
            .env
            .iter()
            .find(|(k, _)| *k == name)
            .map(|(_, v)| v.clone())
    };
    Env {
        pipe: get(PIPE_VAR),
        token: get(TOKEN_VAR),
    }
}

/// Runs the helper as Claude Code would, and returns what it printed.
fn status_line(env: &Env, input: &str, pass: bool) -> Vec<u8> {
    let mut args = vec!["status".to_owned()];
    if pass {
        args.push(helper::PASS.to_owned());
    }
    let mut out = Vec::new();
    helper::run(&args, input.as_bytes(), &mut out, env);
    out
}

fn next_view_where(views: &Receiver<View>, done: impl Fn(&View) -> bool) -> View {
    loop {
        let view = views.recv_timeout(Duration::from_secs(5)).unwrap();
        if done(&view) {
            return view;
        }
    }
}

#[test]
fn a_status_line_update_reaches_the_tab_card_and_its_accounts_meter() {
    let fx = start(95);
    let ticket = fx
        .live
        .register(7, 2, fx.dir.path(), fx.dir.path())
        .unwrap();

    let printed = status_line(&env_of(&ticket), FAKE_INPUT, false);
    let view = next_view_where(&fx.views, |v| !v.meters.is_empty());

    assert!(printed.is_empty(), "with no line of his own, nothing shows");
    assert_eq!(view.tabs[0].tab, 7);
    assert_eq!(view.tabs[0].context, Some(42.5));
    assert_eq!(view.tabs[0].model.as_deref(), Some("Opus 5.5"));
    assert_eq!(view.meters[0].account, 2);
    assert_eq!(view.meters[0].five_hour.map(|w| w.used), Some(87.2));
    assert!(view.warnings.is_empty(), "87.2 is below 95");
}

#[test]
fn no_dropped_field_reaches_the_view_or_the_settings_file() {
    let fx = start(85);
    let ticket = fx
        .live
        .register(7, 2, fx.dir.path(), fx.dir.path())
        .unwrap();

    status_line(&env_of(&ticket), FAKE_INPUT, false);
    let view = next_view_where(&fx.views, |v| !v.meters.is_empty());
    let settings = std::fs::read_to_string(fx.dir.path().join("live/tab-7.json")).unwrap();

    let shown = serde_json::to_string(&view).unwrap();
    for text in [&shown, &settings] {
        assert!(!text.contains("FAKE-SECRET"), "{text}");
        assert!(!text.contains("someone"), "{text}");
    }
    assert_eq!(
        ticket.arg.as_deref(),
        Some(
            format!(
                "--settings={}",
                fx.dir.path().join("live").join("tab-7.json").display()
            )
            .as_str()
        )
    );
}

#[test]
fn his_status_line_gets_every_byte_and_the_app_still_hears() {
    let fx = start(85);
    let project = fx.dir.path().join("work/.claude");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(
        project.join("settings.json"),
        json!({"statusLine": {"type": "command", "command": "my-line"}}).to_string(),
    )
    .unwrap();
    let ticket = fx
        .live
        .register(4, 1, &fx.dir.path().join("work"), fx.dir.path())
        .unwrap();

    let printed = status_line(&env_of(&ticket), FAKE_INPUT, true);
    next_view_where(&fx.views, |v| !v.meters.is_empty());
    let settings: serde_json::Value =
        serde_json::from_slice(&std::fs::read(fx.dir.path().join("live/tab-4.json")).unwrap())
            .unwrap();

    assert_eq!(printed, FAKE_INPUT.as_bytes());
    assert_eq!(
        settings["statusLine"]["command"],
        "'/app/cerberus-live' status --pass | { my-line\n}"
    );
}

#[test]
fn crossing_the_line_warns_once_and_offers_accounts_below_it() {
    let fx = start(85);
    let work = fx
        .live
        .register(7, 2, fx.dir.path(), fx.dir.path())
        .unwrap();
    let other = fx
        .live
        .register(8, 3, fx.dir.path(), fx.dir.path())
        .unwrap();
    let full = FAKE_INPUT.replace("87.2", "91");

    status_line(&env_of(&other), &full, false);
    next_view_where(&fx.views, |v| v.meters.iter().any(|m| m.account == 3));
    status_line(&env_of(&work), FAKE_INPUT, false);
    let view = next_view_where(&fx.views, |v| v.warnings.len() == 2);
    // The same window again: no second notification.
    status_line(&env_of(&work), FAKE_INPUT, false);
    next_view_where(&fx.views, |v| v.warnings.len() == 2);

    let work_warning = view.warnings.iter().find(|w| w.account == 2).unwrap();
    assert_eq!(work_warning.tab, 7);
    assert_eq!(work_warning.offers, [1], "3 is past the line too");
    let notified: Vec<_> = fx
        .warnings
        .lock()
        .unwrap()
        .iter()
        .map(|w| w.account)
        .collect();
    assert_eq!(notified, [3, 2]);

    fx.live.dismiss(2);
    let after = next_view_where(&fx.views, |v| v.warnings.len() == 1);
    assert_eq!(after.warnings[0].account, 3);
}

#[test]
fn a_wrong_token_is_ignored_and_a_closed_tab_empties_its_card() {
    let fx = start(85);
    let ticket = fx
        .live
        .register(7, 2, fx.dir.path(), fx.dir.path())
        .unwrap();
    let forged = Env {
        token: Some("0".repeat(32).into()),
        ..env_of(&ticket)
    };

    status_line(&forged, FAKE_INPUT, false);
    fx.live.closed(&ticket.token);
    let view = next_view_where(&fx.views, |v| v.tabs.is_empty());

    assert!(view.meters.is_empty(), "the forged line reported nothing");
    // A closed tab's token no longer reports; a live tab's line, sent after, proves the first was read.
    status_line(&env_of(&ticket), FAKE_INPUT, false);
    let live = fx
        .live
        .register(9, 1, fx.dir.path(), fx.dir.path())
        .unwrap();
    status_line(&env_of(&live), FAKE_INPUT, false);
    let view = next_view_where(&fx.views, |v| !v.meters.is_empty());
    let reported: Vec<_> = view.meters.iter().map(|m| m.account).collect();
    assert_eq!(reported, [1]);
}

#[test]
fn without_the_pipe_the_helper_still_passes_his_input_on() {
    let env = Env {
        pipe: Some(link::name_for("nobody-listens").into()),
        token: Some("x".into()),
    };

    assert_eq!(status_line(&env, FAKE_INPUT, true), FAKE_INPUT.as_bytes());
    assert!(status_line(&env, FAKE_INPUT, false).is_empty());
}

#[test]
fn off_registers_nothing() {
    let live = Live::off();

    assert!(
        live.register(1, 1, Path::new("."), Path::new("."))
            .is_none()
    );
    assert_eq!(live.view(), View::default());
}

fn hook(env: &Env, input: &str) {
    let mut out = Vec::new();
    helper::run(&[helper::HOOK.to_owned()], input.as_bytes(), &mut out, env);
    assert!(
        out.is_empty(),
        "a hook prints nothing, so it changes nothing in Claude Code"
    );
}

#[test]
fn hooks_fill_the_tabs_outputs_and_tasks_with_no_dropped_field_anywhere() {
    let fx = start(85);
    let ticket = fx
        .live
        .register(7, 2, fx.dir.path(), fx.dir.path())
        .unwrap();
    let written = fx.dir.path().join("out.txt");
    let write = crate::live::hooks::tests::FAKE_WRITE.replace(
        // The fixture's JSON escapes each backslash, so the real path is escaped the same way.
        r#"C:\\work\\out.txt"#,
        serde_json::to_string(&written.to_string_lossy())
            .unwrap()
            .trim_matches('"'),
    );

    hook(&env_of(&ticket), &write);
    next_view_where(&fx.views, |v| !v.tabs[0].outputs.is_empty());
    hook(&env_of(&ticket), crate::live::hooks::tests::FAKE_STOP);
    let view = next_view_where(&fx.views, |v| v.tabs[0].tasks.len() == 2);

    let tab = &view.tabs[0];
    assert_eq!(tab.outputs[0].name, "out.txt");
    assert_eq!(tab.outputs[0].folder, "");
    assert_eq!(
        tab.tasks
            .iter()
            .map(|t| t.label.as_str())
            .collect::<Vec<_>>(),
        // Seen in one report, so they share a start and sort by id.
        ["general-purpose", "shell"]
    );
    let settings = std::fs::read_to_string(fx.dir.path().join("live").join("tab-7.json")).unwrap();
    for text in [serde_json::to_string(&view).unwrap(), settings.clone()] {
        assert!(!text.contains("FAKE-SECRET"), "{text}");
    }
    let settings: serde_json::Value = serde_json::from_str(&settings).unwrap();
    assert_eq!(
        settings["hooks"]["Stop"][0]["hooks"][0]["command"],
        "'/app/cerberus-live' hook"
    );
    assert!(settings.get("statusLine").is_some());
}

#[test]
fn only_a_file_the_chat_wrote_can_be_opened_and_a_closed_tabs_tasks_end() {
    let fx = start(85);
    let ticket = fx
        .live
        .register(7, 2, fx.dir.path(), fx.dir.path())
        .unwrap();
    hook(&env_of(&ticket), crate::live::hooks::tests::FAKE_STOP);
    next_view_where(&fx.views, |v| v.tabs[0].tasks.len() == 2);

    let refused = fx
        .live
        .open_output(7, &fx.dir.path().join("secret.txt").to_string_lossy());

    assert_eq!(refused.unwrap_err().kind(), std::io::ErrorKind::NotFound);
    fx.live.closed(&ticket.token);
    let again = fx
        .live
        .register(7, 2, fx.dir.path(), fx.dir.path())
        .unwrap();
    let view = next_view_where(&fx.views, |v| !v.tabs.is_empty());
    assert!(view.tabs[0].tasks.is_empty());
    assert_eq!(view.tabs[0].finished, 2);
    assert!(again.arg.is_some());
}

#[test]
fn without_an_editor_a_written_file_is_shown_in_its_folder_never_run() {
    let script = Path::new("/work/tools/run.cmd");

    assert_eq!(shown_instead(script).unwrap(), Path::new("/work/tools"));
    assert!(shown_instead(Path::new("run.cmd")).is_err());
}
