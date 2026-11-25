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
    let mut text = lines.join("\n");
    text.push('\n');
    fs::write(&file, text).unwrap();
    file
}

fn append(file: &Path, text: &str) {
    let mut out = fs::OpenOptions::new().append(true).open(file).unwrap();
    std::io::Write::write_all(&mut out, text.as_bytes()).unwrap();
}

fn row<'a>(rows: &'a [ModelRow], model: &str) -> &'a ModelRow {
    rows.iter()
        .find(|r| r.model == model)
        .unwrap_or_else(|| panic!("no {model} in {rows:?}"))
}

fn input_of(rows: &[ModelRow], model: &str) -> u64 {
    rows.iter()
        .find(|r| r.model == model)
        .map_or(0, |r| r.tokens.input)
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
    let mut ledger = Ledger::new(dir.path().join("usage.db"));

    let view = ledger.view(&[(7, home)], 0, NOW);

    let account = &view[0];
    assert_eq!(account.account, 7);
    let today = row(&account.today, "Opus 5.5");
    assert_eq!(today.tokens.input, 1_000_000);
    assert_eq!(today.cost, Some(4.0));
    assert_eq!(input_of(&account.week, "Opus 5.5"), 2_000_000);
    assert_eq!(input_of(&account.month, "Opus 5.5"), 3_000_000);
    let unpriced = row(&account.today, "claude-opus-4-5-20251101");
    assert_eq!(
        unpriced.cost, None,
        "a model the table doesn't know has no price"
    );
    assert_eq!(unpriced.tokens.output, 20);
    assert_eq!(account.today[0].model, "Opus 5.5", "the costliest first");
}

#[test]
fn totals_outlive_a_deleted_chat_and_new_replies_still_add_up() {
    let dir = TestDir::new();
    let home = dir.path().join("acc");
    let store = dir.path().join("usage.db");
    let first = chat(
        &home,
        "22222222-2222-4222-8222-222222222222",
        &[reply_line(
            "m1",
            "2026-11-25T09:00:00Z",
            "claude-sonnet-5-5",
            100,
            1,
        )],
    );
    Ledger::new(store.clone()).view(&[(1, home.clone())], 0, NOW);

    fs::remove_file(&first).unwrap();
    chat(
        &home,
        "33333333-3333-4333-8333-333333333333",
        &[reply_line(
            "m2",
            "2026-11-25T09:30:00Z",
            "claude-sonnet-5-5",
            40,
            1,
        )],
    );
    let view = Ledger::new(store.clone()).view(&[(1, home)], 0, NOW);

    assert_eq!(input_of(&view[0].today, "Sonnet 5.5"), 140);
    let bytes = fs::read(&store).unwrap();
    let text = String::from_utf8_lossy(&bytes);
    assert!(!text.contains("FAKE-SECRET"), "no words are kept");
}

#[test]
fn a_copied_chat_counts_once_and_stays_with_its_first_account_after_the_source_goes() {
    let dir = TestDir::new();
    let (a, b) = (dir.path().join("a"), dir.path().join("b"));
    let lines = [reply_line(
        "shared",
        "2026-11-25T09:00:00Z",
        "claude-haiku-5-5",
        1000,
        10,
    )];
    let source = chat(&a, "44444444-4444-4444-8444-444444444444", &lines);
    chat(&b, "44444444-4444-4444-8444-444444444444", &lines);
    let sub = b
        .join("projects")
        .join("C--fake")
        .join("44444444-4444-4444-8444-444444444444")
        .join("subagents");
    fs::create_dir_all(&sub).unwrap();
    fs::write(
        sub.join("agent-a1.jsonl"),
        reply_line("sub", "2026-11-25T09:05:00Z", "claude-haiku-5-5", 7, 3),
    )
    .unwrap();
    let accounts = [(1, a), (2, b)];
    let mut ledger = Ledger::new(dir.path().join("usage.db"));

    let view = ledger.view(&accounts, 0, NOW);
    assert_eq!(input_of(&view[0].today, "Haiku 5.5"), 1000);
    assert_eq!(
        input_of(&view[1].today, "Haiku 5.5"),
        7,
        "only the subagent's own reply"
    );

    fs::remove_file(&source).unwrap();
    // Touched in place, so b's copy is read whole again with the source gone.
    let copy = dir
        .path()
        .join("b/projects/C--fake/44444444-4444-4444-8444-444444444444.jsonl");
    fs::File::options()
        .write(true)
        .open(&copy)
        .unwrap()
        .set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(60))
        .unwrap();
    let mut again = Ledger::new(dir.path().join("usage.db"));
    let view = again.view(&accounts, 0, NOW);
    assert_eq!(input_of(&view[0].today, "Haiku 5.5"), 1000);
    assert_eq!(input_of(&view[1].today, "Haiku 5.5"), 7);
    assert_eq!(again.store.as_ref().unwrap().owner_of("shared"), Some(1));
}

#[test]
fn a_new_offset_moves_a_reply_to_its_new_day_without_counting_it_twice() {
    let dir = TestDir::new();
    let home = dir.path().join("acc");
    // 19:00 UTC on the 24th: the 24th in London, already the 25th in India.
    chat(
        &home,
        "55555555-5555-4555-8555-555555555555",
        &[reply_line(
            "late",
            "2026-11-24T19:00:00Z",
            "claude-opus-5-5",
            50,
            1,
        )],
    );
    let mut ledger = Ledger::new(dir.path().join("usage.db"));

    let london = ledger.view(&[(1, home.clone())], 0, NOW);
    let india = ledger.view(&[(1, home)], 330, NOW);

    assert_eq!(input_of(&london[0].today, "Opus 5.5"), 0);
    assert_eq!(input_of(&india[0].today, "Opus 5.5"), 50);
    assert_eq!(input_of(&london[0].week, "Opus 5.5"), 50);
    assert_eq!(input_of(&india[0].week, "Opus 5.5"), 50);
}

#[test]
fn a_failed_write_is_tried_again_on_the_next_look() {
    let dir = TestDir::new();
    let home = dir.path().join("acc");
    chat(
        &home,
        "66666666-6666-4666-8666-666666666666",
        &[reply_line(
            "m1",
            "2026-11-25T09:00:00Z",
            "claude-opus-5-5",
            9,
            1,
        )],
    );
    let mut ledger = Ledger::new(dir.path().join("usage.db"));
    ledger.view(&[], 0, NOW);
    ledger.store.as_ref().unwrap().break_writes();

    ledger.view(&[(1, home.clone())], 0, NOW);
    assert!(
        ledger.recorded.is_empty(),
        "nothing recorded, so the file's place stays"
    );
    ledger.store.as_ref().unwrap().mend_writes();
    let view = ledger.view(&[(1, home)], 0, NOW);

    assert_eq!(input_of(&view[0].today, "Opus 5.5"), 9);
}

#[test]
fn a_store_from_a_newer_build_is_left_alone_and_a_broken_one_set_aside() {
    let dir = TestDir::new();
    let home = dir.path().join("acc");
    chat(
        &home,
        "77777777-7777-4777-8777-777777777777",
        &[reply_line(
            "m1",
            "2026-11-25T09:00:00Z",
            "claude-opus-5-5",
            1,
            1,
        )],
    );
    let file = dir.path().join("usage.db");
    {
        let newer = rusqlite::Connection::open(&file).unwrap();
        newer.execute_batch("CREATE TABLE later (x)").unwrap();
        newer.pragma_update(None, "user_version", 2).unwrap();
    }

    let view = Ledger::new(file.clone()).view(&[(1, home.clone())], 0, NOW);

    assert_eq!(view[0].today.len(), 1, "fresh counts still show");
    let newer = rusqlite::Connection::open(&file).unwrap();
    let version: i64 = newer
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    let tables: i64 = newer
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE name = 'replies'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!((version, tables), (2, 0), "the newer file is untouched");
    drop(newer);

    fs::remove_file(&file).unwrap();
    fs::write(
        &file,
        "not a database at all, just bytes that are long enough to look like a header",
    )
    .unwrap();
    let view = Ledger::new(file.clone()).view(&[(1, home)], 0, NOW);
    assert_eq!(view[0].today.len(), 1);
    assert!(file.with_extension("db.unreadable").is_file());
}

#[test]
fn an_appended_tail_is_read_alone_and_a_rewritten_file_whole() {
    let dir = TestDir::new();
    let home = dir.path().join("acc");
    let file = chat(
        &home,
        "88888888-8888-4888-8888-888888888888",
        &[reply_line(
            "m1",
            "2026-11-25T09:00:00Z",
            "claude-opus-5-5",
            100,
            1,
        )],
    );
    let mut ledger = Ledger::new(dir.path().join("usage.db"));
    ledger.view(&[(1, home.clone())], 0, NOW);
    let first_offset = ledger.recorded[&file].offset;

    // Appended as Claude Code does, with a third line still half written.
    let second = reply_line("m2", "2026-11-25T09:01:00Z", "claude-opus-5-5", 200, 2);
    let third = reply_line("m3", "2026-11-25T09:02:00Z", "claude-opus-5-5", 400, 4);
    append(&file, &format!("{second}\n{}", &third[..20]));
    let view = ledger.view(&[(1, home.clone())], 0, NOW);

    assert_eq!(input_of(&view[0].today, "Opus 5.5"), 300);
    let offset = ledger.recorded[&file].offset;
    assert!(offset > first_offset);
    assert_eq!(
        offset as usize,
        fs::read(&file).unwrap().len() - 20,
        "the half line is read again"
    );

    append(&file, &format!("{}\n", &third[20..]));
    let view = ledger.view(&[(1, home.clone())], 0, NOW);
    assert_eq!(input_of(&view[0].today, "Opus 5.5"), 700);

    // Rewritten shorter with a reply already counted and a new one: only the new one adds.
    fs::write(
        &file,
        format!(
            "{}\n{}\n",
            reply_line("m1", "2026-11-25T09:00:00Z", "claude-opus-5-5", 100, 1),
            reply_line("m9", "2026-11-25T09:05:00Z", "claude-opus-5-5", 5, 0)
        ),
    )
    .unwrap();
    let view = ledger.view(&[(1, home)], 0, NOW);
    assert_eq!(input_of(&view[0].today, "Opus 5.5"), 705);
}

#[test]
fn a_restart_resumes_each_file_where_it_was_recorded() {
    let dir = TestDir::new();
    let home = dir.path().join("acc");
    let file = chat(
        &home,
        "abababab-abab-4bab-8bab-abababababab",
        &[reply_line(
            "m1",
            "2026-11-25T09:00:00Z",
            "claude-opus-5-5",
            3,
            1,
        )],
    );
    let store = dir.path().join("usage.db");
    let mut first = Ledger::new(store.clone());
    first.view(&[(1, home.clone())], 0, NOW);
    let place = first.recorded[&file];
    drop(first);

    let mut again = Ledger::new(store);
    let view = again.view(&[(1, home)], 0, NOW);

    assert_eq!(again.recorded[&file], place);
    assert_eq!(input_of(&view[0].today, "Opus 5.5"), 3);
}

#[test]
fn a_file_untouched_for_90_days_is_not_read() {
    let dir = TestDir::new();
    let home = dir.path().join("acc");
    let file = chat(
        &home,
        "99999999-9999-4999-8999-999999999999",
        &[reply_line(
            "m1",
            "2026-08-01T09:00:00Z",
            "claude-opus-5-5",
            1,
            1,
        )],
    );
    let old = std::time::SystemTime::now() - std::time::Duration::from_secs(100 * 86_400);
    fs::File::options()
        .write(true)
        .open(&file)
        .unwrap()
        .set_modified(old)
        .unwrap();
    let now_ms = u64::try_from(
        std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    )
    .unwrap();
    let mut ledger = Ledger::new(dir.path().join("usage.db"));

    ledger.view(&[(1, home)], 0, now_ms);

    assert!(!ledger.recorded.contains_key(&file));
}

/// Times the first scan, a refresh after an append, and a refresh with nothing new; prints them.
#[test]
#[ignore = "benchmark: TOKENS_BENCH_DIR=<folder of acc-*/projects> cargo test --release --lib bench_ -- --ignored --nocapture"]
fn bench_first_scan_then_refreshes() {
    let Some(root) = std::env::var_os("TOKENS_BENCH_DIR").map(PathBuf::from) else {
        return;
    };
    let mut homes: Vec<PathBuf> = fs::read_dir(&root)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.join("projects").is_dir())
        .collect();
    homes.sort();
    let accounts: Vec<(AccountId, PathBuf)> = (1..).zip(homes).collect();
    let largest = accounts
        .iter()
        .flat_map(|(_, home)| chat_files(home))
        .max_by_key(|(_, len, _)| *len)
        .map(|(path, _, _)| path)
        .unwrap();
    let dir = TestDir::new();
    let mut ledger = Ledger::new(dir.path().join("usage.db"));
    let now_ms = u64::try_from(
        std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    )
    .unwrap();

    let started = std::time::Instant::now();
    ledger.view(&accounts, 0, now_ms);
    let first = started.elapsed();

    let mut appended = 0;
    for n in 0..400 {
        let line = format!(
            "{}\n",
            reply_line(
                &format!("bench_{n}_{now_ms}"),
                "2026-11-25T09:00:00Z",
                "claude-opus-5-5",
                1,
                1
            )
        );
        appended += line.len();
        append(&largest, &line);
    }
    let started = std::time::Instant::now();
    ledger.view(&accounts, 0, now_ms);
    let refresh = started.elapsed();

    let started = std::time::Instant::now();
    ledger.view(&accounts, 0, now_ms);
    let idle = started.elapsed();
    let files = ledger.recorded.len();
    drop(ledger);

    let started = std::time::Instant::now();
    Ledger::new(dir.path().join("usage.db")).view(&accounts, 0, now_ms);
    let restart = started.elapsed();

    println!(
        "files {files}, first scan {first:?}, refresh after {appended} bytes appended {refresh:?}, refresh with nothing new {idle:?}, first look after a restart {restart:?}, store {} bytes",
        fs::metadata(dir.path().join("usage.db")).map_or(0, |m| m.len())
    );
}
