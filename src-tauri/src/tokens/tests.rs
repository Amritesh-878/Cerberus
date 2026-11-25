use std::path::{Path, PathBuf};

use super::*;
use crate::accounts::testdir::TestDir;

/// 2026-11-25T10:00:00Z.
const NOW: u64 = 1_795_600_800_000;

fn reply_line(id: &str, at: &str, model: &str, input: u64, output: u64) -> String {
    format!(
        r#"{{"type":"assistant","timestamp":"{at}","message":{{"id":"{id}","model":"{model}","content":[{{"type":"text","text":"FAKE-SECRET"}}],"usage":{{"input_tokens":{input},"output_tokens":{output}}}}}}}"#
    )
}

/// Writes a chat file under `<home>/projects/C--fake/<name>.jsonl`.
fn chat(home: &Path, name: &str, lines: &[String]) -> PathBuf {
    let dir = home.join("projects").join("C--fake");
    fs::create_dir_all(&dir).unwrap();
    let file = dir.join(format!("{name}.jsonl"));
    fs::write(&file, lines.join("\n")).unwrap();
    file
}

fn row<'a>(rows: &'a [ModelRow], model: &str) -> &'a ModelRow {
    rows.iter()
        .find(|r| r.model == model)
        .unwrap_or_else(|| panic!("no {model} in {rows:?}"))
}

#[test]
fn today_week_and_month_add_up_per_model_with_a_price_or_without() {
    let dir = TestDir::new();
    let home = dir.path().join("acc");
    chat(
        &home,
        "11111111-1111-4111-8111-111111111111",
        &[
            reply_line(
                "m1",
                "2026-11-25T09:00:00Z",
                "claude-opus-5-5",
                1_000_000,
                0,
            ),
            reply_line(
                "m2",
                "2026-11-22T09:00:00Z",
                "claude-opus-5-5",
                1_000_000,
                0,
            ),
            reply_line(
                "m3",
                "2026-11-01T09:00:00Z",
                "claude-opus-5-5",
                1_000_000,
                0,
            ),
            reply_line(
                "m4",
                "2026-11-25T09:30:00Z",
                "claude-opus-4-5-20251101",
                10,
                20,
            ),
        ],
    );
    let mut ledger = Ledger::new(dir.path().join("usage.json"));

    let view = ledger.view(&[(7, home)], 0, NOW);

    let account = &view[0];
    assert_eq!(account.account, 7);
    let today = row(&account.today, "Opus 5.5");
    assert_eq!(today.tokens.input, 1_000_000);
    assert_eq!(today.cost, Some(4.0));
    assert_eq!(row(&account.week, "Opus 5.5").tokens.input, 2_000_000);
    assert_eq!(row(&account.month, "Opus 5.5").tokens.input, 3_000_000);
    let unpriced = row(&account.today, "claude-opus-4-5-20251101");
    assert_eq!(
        unpriced.cost, None,
        "a model the table doesn't know has no price"
    );
    assert_eq!(unpriced.tokens.output, 20);
    assert_eq!(account.today[0].model, "Opus 5.5", "the costliest first");
}

#[test]
fn totals_outlive_a_deleted_chat_and_the_store_holds_no_words() {
    let dir = TestDir::new();
    let home = dir.path().join("acc");
    let file = chat(
        &home,
        "22222222-2222-4222-8222-222222222222",
        &[reply_line(
            "m1",
            "2026-11-25T09:00:00Z",
            "claude-sonnet-5-5",
            500,
            100,
        )],
    );
    let store = dir.path().join("usage.json");
    Ledger::new(store.clone()).view(&[(1, home.clone())], 0, NOW);

    fs::remove_file(&file).unwrap();
    let view = Ledger::new(store.clone()).view(&[(1, home)], 0, NOW);

    let kept = row(&view[0].today, "Sonnet 5.5");
    assert_eq!(kept.tokens.input, 500);
    assert_eq!(kept.tokens.output, 100);
    let text = fs::read_to_string(&store).unwrap();
    assert!(
        !text.contains("FAKE-SECRET") && !text.contains("m1"),
        "{text}"
    );
}

#[test]
fn a_chat_copied_to_another_account_counts_once_and_a_subagent_counts_too() {
    let dir = TestDir::new();
    let (first, second) = (dir.path().join("first"), dir.path().join("second"));
    let lines = [reply_line(
        "shared",
        "2026-11-25T09:00:00Z",
        "claude-haiku-5-5",
        1000,
        10,
    )];
    chat(&first, "33333333-3333-4333-8333-333333333333", &lines);
    chat(&second, "33333333-3333-4333-8333-333333333333", &lines);
    let sub = second
        .join("projects")
        .join("C--fake")
        .join("33333333-3333-4333-8333-333333333333")
        .join("subagents");
    fs::create_dir_all(&sub).unwrap();
    fs::write(
        sub.join("agent-a1.jsonl"),
        reply_line("sub", "2026-11-25T09:05:00Z", "claude-haiku-5-5", 7, 3),
    )
    .unwrap();

    let view = Ledger::new(dir.path().join("usage.json")).view(&[(1, first), (2, second)], 0, NOW);

    assert_eq!(row(&view[0].today, "Haiku 5.5").tokens.input, 1000);
    assert_eq!(
        row(&view[1].today, "Haiku 5.5").tokens.input,
        7,
        "only the subagent's own reply"
    );
}

#[test]
fn a_store_from_a_newer_build_is_left_alone_and_a_broken_one_set_aside() {
    let dir = TestDir::new();
    let home = dir.path().join("acc");
    chat(
        &home,
        "44444444-4444-4444-8444-444444444444",
        &[reply_line(
            "m1",
            "2026-11-25T09:00:00Z",
            "claude-opus-5-5",
            1,
            1,
        )],
    );
    let store = dir.path().join("usage.json");
    fs::write(&store, r#"{"version":99,"days":{}}"#).unwrap();

    let view = Ledger::new(store.clone()).view(&[(1, home.clone())], 0, NOW);

    assert_eq!(view[0].today.len(), 1, "fresh counts still show");
    assert_eq!(
        fs::read_to_string(&store).unwrap(),
        r#"{"version":99,"days":{}}"#
    );

    fs::write(&store, "{ broken").unwrap();
    Ledger::new(store.clone()).view(&[(1, home)], 0, NOW);
    assert_eq!(
        fs::read_to_string(store.with_extension("json.unreadable")).unwrap(),
        "{ broken"
    );
    assert!(
        fs::read_to_string(&store)
            .unwrap()
            .starts_with(r#"{"version":1"#)
    );
}

#[test]
fn an_unchanged_file_is_not_read_again() {
    let dir = TestDir::new();
    let home = dir.path().join("acc");
    let file = chat(
        &home,
        "55555555-5555-4555-8555-555555555555",
        &[reply_line(
            "m1",
            "2026-11-25T09:00:00Z",
            "claude-opus-5-5",
            1,
            1,
        )],
    );
    let mut ledger = Ledger::new(dir.path().join("usage.json"));
    ledger.view(&[(1, home.clone())], 0, NOW);
    let first = Arc::clone(&ledger.read[&file].replies);

    ledger.view(&[(1, home)], 0, NOW);

    assert!(Arc::ptr_eq(&first, &ledger.read[&file].replies));
}
