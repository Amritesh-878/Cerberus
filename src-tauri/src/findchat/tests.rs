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
const FOUR: &str = "44444444-4444-4444-8444-444444444444";

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
fn a_pasted_id_finds_that_chat_on_whichever_account_has_it() {
    let fx = corpus();
    let finder = ChatFinder::default();
    let sources = || vec![fx.work.clone(), fx.home.clone()];

    for typed in [
        THREE.to_owned(),
        format!("claude --resume {THREE}"),
        format!("  '{}' ", THREE.to_uppercase()),
    ] {
        let chats = search(&finder, sources(), &typed, HashMap::new());
        let found: Vec<(&str, Option<u32>)> =
            chats.iter().map(|c| (c.id.as_str(), c.account)).collect();
        assert_eq!(found, [(THREE, Some(2))], "{typed}");
    }
    let missing = "44444444-4444-4444-8444-444444444444";
    assert!(search(&finder, sources(), missing, HashMap::new()).is_empty());
}

#[test]
fn only_an_id_shaped_word_counts_as_an_id() {
    assert_eq!(
        session_id_in(&format!("claude --resume {ONE}")).as_deref(),
        Some(ONE)
    );
    assert_eq!(session_id_in("login bug"), None);
    assert_eq!(session_id_in("11111111-1111-4111-8111"), None);
    assert_eq!(session_id_in("1111111g-1111-4111-8111-111111111111"), None);
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
    assert_eq!(
        train[0].marks,
        [[0, 6], [11, 5]],
        "both words marked in the line"
    );
    assert_eq!(both.len(), 2, "the folder name matches both chats in it");
    assert!(both.iter().all(|c| c.place == Place::Folder));
    assert!(none.is_empty());
}

/// Runs one search and returns its chats in the list's order, as the dialog shows them.
fn ranked(finder: &ChatFinder, sources: Vec<Source>, query: &str) -> Vec<(String, Group, Place)> {
    let mut chats = search(finder, sources, query, HashMap::new());
    chats.sort_by(Found::order);
    chats
        .into_iter()
        .map(|c| (c.id, c.group, c.place))
        .collect()
}

#[test]
fn letters_in_order_find_a_title_and_mark_them() {
    let fx = corpus();
    let finder = ChatFinder::default();
    let sources = vec![fx.work.clone(), fx.home.clone()];

    let found = search(&finder, sources, "fxlog", HashMap::new());

    assert_eq!(found.len(), 1);
    assert_eq!(found[0].id, ONE);
    assert_eq!(found[0].group, Group::Name);
    assert_eq!(found[0].place, Place::Title);
    assert_eq!(found[0].marks, [[0, 1], [2, 1], [8, 3]]);
}

#[test]
fn a_chat_lists_only_when_it_holds_every_word() {
    let fx = corpus();
    let finder = ChatFinder::default();
    let sources = vec![fx.work.clone(), fx.home.clone()];

    let all = ranked(&finder, sources.clone(), "booked train");
    let red = ranked(&finder, sources, "changed redirect");

    assert_eq!(all, [(THREE.to_owned(), Group::Text, Place::Text)]);
    assert_eq!(red, [(ONE.to_owned(), Group::Text, Place::Text)]);
}

#[test]
fn close_matches_come_only_when_no_chat_holds_every_word() {
    let fx = corpus();
    let finder = ChatFinder::default();
    let sources = vec![fx.work.clone(), fx.home.clone()];

    let most = ranked(&finder, sources.clone(), "notes login redirect");
    let tie = ranked(&finder, sources.clone(), "train login");
    let found = ranked(&finder, sources, "login redirect");

    assert_eq!(
        most,
        [(ONE.to_owned(), Group::Close, Place::Text)],
        "two of three words; one of three is not most"
    );
    assert_eq!(
        tie,
        [
            (ONE.to_owned(), Group::Close, Place::Text),
            (THREE.to_owned(), Group::Close, Place::Text)
        ],
        "one word each, so the newest first"
    );
    assert_eq!(
        found,
        [(ONE.to_owned(), Group::Text, Place::Text)],
        "a full match leaves no close ones"
    );
}

#[test]
fn a_misremembered_word_keeps_the_chat_below_an_exact_one() {
    let fx = corpus();
    let finder = ChatFinder::default();
    let sources = vec![fx.work.clone(), fx.home.clone()];

    let typo = ranked(&finder, sources.clone(), "relaese booked");
    let title = ranked(&finder, sources, "lgoin");

    assert_eq!(
        typo,
        [
            (THREE.to_owned(), Group::Close, Place::Text),
            (TWO.to_owned(), Group::Close, Place::Text)
        ],
        "the older exact match before the newer typo"
    );
    assert_eq!(
        title,
        [(ONE.to_owned(), Group::Text, Place::Title)],
        "a typo in the title is a title match"
    );
}

#[test]
fn a_name_match_comes_before_a_chat_whose_words_are_only_in_its_messages() {
    let fx = corpus();
    put(
        &fx.home.config_dir,
        &fx.alive,
        FOUR,
        &chat(
            "Pack for the coast",
            &fx.alive,
            "2026-11-08T10:00:00.000Z",
            "Write the release notes later.",
        ),
    );
    let finder = ChatFinder::default();
    let sources = vec![fx.work.clone(), fx.home.clone()];

    let notes = ranked(&finder, sources, "release notes");

    assert_eq!(
        notes,
        [
            (TWO.to_owned(), Group::Name, Place::Title),
            (FOUR.to_owned(), Group::Text, Place::Text)
        ],
        "the older title match first"
    );
}

#[test]
fn one_letter_lists_names_only() {
    let fx = corpus();
    let finder = ChatFinder::default();
    let sources = vec![fx.work.clone(), fx.home.clone()];

    let found = search(&finder, sources, "w", HashMap::new());

    assert!(found.iter().all(|c| c.group == Group::Name));
    assert!(found.iter().any(|c| c.id == TWO), "Write release notes");
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
    // The first search ended early or finished first, and it never reports after the second began.
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

    let stale = finder.check(4, &file, stamp, &Query::new(""), Pass::Main, &|| false);
    assert!(stale.is_some(), "it still answers its own caller");
    assert!(finder.cache().is_empty());
    finder.check(5, &file, stamp, &Query::new(""), Pass::Main, &|| false);
    assert_eq!(finder.cache().len(), 1);
}
