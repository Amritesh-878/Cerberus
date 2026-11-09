use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

use super::*;
use crate::accounts::testdir::TestDir;
use crate::findchat::read::tests::FAKE_CHAT;
use crate::sessions::slug;

const ONE: &str = "11111111-1111-4111-8111-111111111111";
const TWO: &str = "22222222-2222-4222-8222-222222222222";
const THREE: &str = "33333333-3333-4333-8333-333333333333";

fn chat(title: &str, cwd: &str, stamp: &str, text: &str) -> String {
    let cwd_json = serde_json::to_string(cwd).unwrap();
    [
        format!(
            r#"{{"type":"user","cwd":{cwd_json},"timestamp":"{stamp}","message":{{"content":{}}}}}"#,
            serde_json::to_string(title).unwrap()
        ),
        format!(
            r#"{{"type":"assistant","timestamp":"{stamp}","message":{{"content":[{{"type":"text","text":{}}}]}}}}"#,
            serde_json::to_string(text).unwrap()
        ),
        r#"{"type":"user","message":{"content":[{"type":"tool_result","content":"FAKE-SECRET-result sk-ant-000"}]}}"#.to_owned(),
    ]
    .join("\n")
}

fn put(home: &Path, cwd: &str, id: &str, body: &str) {
    let dir = home.join("projects").join(slug(cwd));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(format!("{id}.jsonl")), body).unwrap();
}

/// Runs one search to its end and returns its chats in the order they came.
fn search(
    finder: &ChatFinder,
    sources: Vec<Source>,
    query: &str,
    open: HashMap<String, TabId>,
) -> Vec<Found> {
    let (tx, rx) = mpsc::channel();
    let number = finder.search(sources, query, open, move |batch| {
        let _ = tx.send(batch);
    });
    let mut chats = Vec::new();
    loop {
        match rx.recv_timeout(Duration::from_secs(10)).unwrap() {
            Batch::Found {
                search,
                chats: more,
            } => {
                assert_eq!(search, number);
                chats.extend(more);
            }
            Batch::Done { search, .. } => {
                assert_eq!(search, number);
                return chats;
            }
        }
    }
}

struct Corpus {
    _dir: TestDir,
    work: Source,
    home: Source,
    alive: String,
}

fn corpus() -> Corpus {
    let dir = TestDir::new();
    let alive = dir.path().join("alpha").to_string_lossy().into_owned();
    fs::create_dir_all(&alive).unwrap();
    let gone = dir.path().join("gone").to_string_lossy().into_owned();
    let work = dir.path().join("work-account");
    let home = dir.path().join("home-account");
    put(
        &work,
        &alive,
        ONE,
        &chat(
            "Fix the login bug",
            &alive,
            "2026-11-01T10:00:00.000Z",
            "Changed the redirect.",
        ),
    );
    put(
        &work,
        &gone,
        TWO,
        &chat(
            "Write release notes",
            &gone,
            "2026-11-05T10:00:00.000Z",
            "Drafted notes for v0.5.",
        ),
    );
    put(
        &home,
        &alive,
        THREE,
        &chat(
            "Plan the trip",
            &alive,
            "2026-10-01T10:00:00.000Z",
            "Booked the train.",
        ),
    );
    // Not chats: a subagent's file in a session folder, and a file with another name.
    let sub = work
        .join("projects")
        .join(slug(&alive))
        .join(ONE)
        .join("subagents");
    fs::create_dir_all(&sub).unwrap();
    fs::write(
        sub.join("agent-1.jsonl"),
        chat("subagent", &alive, "2026-11-09T10:00:00.000Z", "x"),
    )
    .unwrap();
    fs::write(
        work.join("projects").join(slug(&alive)).join("notes.txt"),
        "x",
    )
    .unwrap();
    Corpus {
        work: Source {
            account: Some(1),
            config_dir: work,
        },
        home: Source {
            account: Some(2),
            config_dir: home,
        },
        alive,
        _dir: dir,
    }
}

#[test]
fn an_empty_search_lists_every_accounts_chats_newest_first() {
    let fx = corpus();
    let finder = ChatFinder::default();

    let mut chats = search(
        &finder,
        vec![fx.work.clone(), fx.home.clone()],
        "",
        HashMap::new(),
    );
    chats.sort_by_key(|c| std::cmp::Reverse(c.last));

    let rows: Vec<(&str, Option<u32>, bool)> = chats
        .iter()
        .map(|c| (c.title.as_str(), c.account, c.folder_exists))
        .collect();
    assert_eq!(
        rows,
        [
            ("Write release notes", Some(1), false),
            ("Fix the login bug", Some(1), true),
            ("Plan the trip", Some(2), true),
        ]
    );
    assert_eq!(chats[1].folder.as_deref(), Some(fx.alive.as_str()));
    assert!(chats.iter().all(|c| c.snippet.is_none()));
}

#[test]
fn words_find_a_chat_on_any_account_with_a_line_around_the_match() {
    let fx = corpus();
    let finder = ChatFinder::default();
    let sources = vec![fx.work.clone(), fx.home.clone()];
    search(&finder, sources.clone(), "", HashMap::new());

    let train = search(&finder, sources.clone(), "TRAIN booked", HashMap::new());
    let both = search(&finder, sources.clone(), "alpha", HashMap::new());
    let none = search(&finder, sources, "giraffe zebu", HashMap::new());

    assert_eq!(train.len(), 1);
    assert_eq!(train[0].id, THREE);
    assert_eq!(train[0].snippet.as_deref(), Some("Booked the train."));
    assert_eq!(both.len(), 2, "the folder name matches both chats in it");
    assert!(none.is_empty());
}

/// Runs one search and returns its chats in the list's order, as the dialog shows them.
fn ranked(finder: &ChatFinder, sources: Vec<Source>, query: &str) -> Vec<(String, u32, u32)> {
    let mut chats = search(finder, sources, query, HashMap::new());
    chats.sort_by(Found::order);
    chats.into_iter().map(|c| (c.id, c.hits, c.exact)).collect()
}

#[test]
fn a_chat_matching_more_words_comes_first_and_ties_go_newest_first() {
    let fx = corpus();
    let finder = ChatFinder::default();
    let sources = vec![fx.work.clone(), fx.home.clone()];

    let more = ranked(&finder, sources.clone(), "notes login redirect");
    let tie = ranked(&finder, sources, "train login");

    assert_eq!(
        more,
        [(ONE.to_owned(), 2, 2), (TWO.to_owned(), 1, 1)],
        "the older chat with two words before the newer one with one"
    );
    assert_eq!(
        tie,
        [(ONE.to_owned(), 1, 1), (THREE.to_owned(), 1, 1)],
        "one word each, so the newest first"
    );
}

#[test]
fn a_misremembered_word_keeps_the_chat_below_an_exact_one() {
    let fx = corpus();
    let finder = ChatFinder::default();
    let sources = vec![fx.work.clone(), fx.home.clone()];

    let wrong = ranked(&finder, sources.clone(), "fix login giraffe");
    let typo = ranked(&finder, sources, "relaese booked");

    assert_eq!(wrong, [(ONE.to_owned(), 2, 2)]);
    assert_eq!(
        typo,
        [(THREE.to_owned(), 1, 1), (TWO.to_owned(), 1, 0)],
        "the older exact match before the newer typo"
    );
}

#[test]
fn a_tool_result_secret_never_matches_or_reaches_a_result() {
    let fx = corpus();
    let finder = ChatFinder::default();
    let sources = vec![fx.work.clone(), fx.home.clone()];

    let listed = search(&finder, sources.clone(), "", HashMap::new());
    let secret = search(
        &finder,
        sources.clone(),
        "fake-secret-result",
        HashMap::new(),
    );
    let typo = search(&finder, sources, "secrett resullt", HashMap::new());

    assert!(secret.is_empty());
    assert!(typo.is_empty(), "a word one edit from the secret");
    assert!(
        !serde_json::to_string(&listed)
            .unwrap()
            .contains("FAKE-SECRET")
    );
}

#[test]
fn a_chat_already_in_a_tab_says_which() {
    let fx = corpus();
    let finder = ChatFinder::default();
    let open = HashMap::from([(ONE.to_owned(), 7)]);

    let chats = search(&finder, vec![fx.work.clone()], "login", open);

    assert_eq!(chats[0].open_in, Some(7));
}

#[test]
fn a_new_search_stops_the_last_and_close_forgets_what_was_read() {
    let fx = corpus();
    let finder = ChatFinder::default();
    let (tx, rx) = mpsc::channel();
    let first = finder.search(vec![fx.work.clone()], "", HashMap::new(), move |b| {
        let _ = tx.send(b);
    });

    let second = search(&finder, vec![fx.work.clone()], "notes", HashMap::new());

    assert_eq!(second.len(), 1);
    // The first search ended early or finished first; it never reports after the second began.
    while let Ok(batch) = rx.try_recv() {
        let number = match batch {
            Batch::Found { search, .. } | Batch::Done { search, .. } => search,
        };
        assert_eq!(number, first);
    }
    finder.close();
    assert!(finder.cache().is_empty());
}

#[test]
fn a_full_read_matches_the_reader_on_a_whole_chat() {
    let dir = TestDir::new();
    let home = dir.path().join("acct");
    put(&home, r"C:\code\alpha", ONE, FAKE_CHAT);
    let finder = ChatFinder::default();

    let chats = search(
        &finder,
        vec![Source {
            account: None,
            config_dir: home,
        }],
        "dashboard",
        HashMap::new(),
    );

    assert_eq!(chats.len(), 1);
    assert_eq!(chats[0].title, "Fix the login redirect bug");
    assert_eq!(chats[0].account, None);
}

#[test]
fn a_lone_early_match_shows_while_the_rest_are_still_read() {
    let dir = TestDir::new();
    let home = dir.path().join("acct");
    let cwd = dir.path().join("w").to_string_lossy().into_owned();
    // The match is the newest chat, so it is read first, and the rest outlast the wait by far.
    let filler = format!(
        "{}
{}",
        chat("filler", &cwd, "2026-01-01T00:00:00.000Z", "nothing here"),
        format_args!(
            r#"{{"type":"user","message":{{"content":[{{"type":"tool_result","content":"{}"}}]}}}}"#,
            "quokka ".repeat(2000)
        )
    );
    for i in 0..1500u32 {
        let id = format!("{i:08x}-0000-4000-8000-000000000000");
        put(&home, &cwd, &id, &filler);
    }
    put(
        &home,
        &cwd,
        ONE,
        &chat(
            "the one",
            &cwd,
            "2026-11-01T00:00:00.000Z",
            "a quokka appears",
        ),
    );
    let newest = std::time::SystemTime::now() + Duration::from_secs(60);
    let file = home
        .join("projects")
        .join(slug(&cwd))
        .join(format!("{ONE}.jsonl"));
    fs::File::options()
        .write(true)
        .open(&file)
        .unwrap()
        .set_modified(newest)
        .unwrap();
    // A tool result's word gets past the raw check, so every filler is parsed in full.
    let finder = ChatFinder::with_wait(Duration::from_millis(20));
    let (tx, rx) = mpsc::channel();

    finder.search(
        vec![Source {
            account: None,
            config_dir: home,
        }],
        "quokka",
        HashMap::new(),
        move |b| {
            let _ = tx.send(b);
        },
    );
    let first = rx.recv_timeout(Duration::from_secs(10)).unwrap();
    finder.close();

    assert!(matches!(first, Batch::Found { ref chats, .. } if chats[0].id == ONE));
    // Sent mid-scan, so the close stopped the search before it could finish.
    assert!(rx.recv_timeout(Duration::from_millis(500)).is_err());
}

#[test]
fn a_stopped_search_adds_nothing_to_the_cache() {
    let fx = corpus();
    let finder = ChatFinder::default();
    let file = fx
        .work
        .config_dir
        .join("projects")
        .join(slug(&fx.alive))
        .join(format!("{ONE}.jsonl"));
    let stamp = Stamp {
        modified: 1,
        len: 1,
    };
    finder.current.store(5, Ordering::Release);

    let stale = finder.check(4, &file, stamp, &Query::new(""), &|| false);
    assert!(stale.is_some(), "it still answers its own caller");
    assert!(finder.cache().is_empty());
    finder.check(5, &file, stamp, &Query::new(""), &|| false);
    assert_eq!(finder.cache().len(), 1);
}
