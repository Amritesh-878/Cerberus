//! One transcript read for Find a chat: its title, folder and last time, and whether a query matches.

use serde::Deserialize;

/// A line longer than this is a paste or an image, not a message worth searching; it is skipped.
pub const MOST_LINE: usize = 4 * 1024 * 1024;
const MOST_TITLE: usize = 100;
/// Characters of text kept on each side of a match.
const SNIPPET_SIDE: usize = 40;

/// The words of a search, lower case; every one must match somewhere in the chat.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Query {
    words: Vec<String>,
}

impl Query {
    pub fn new(text: &str) -> Self {
        Self {
            words: text.split_whitespace().map(str::to_lowercase).collect(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    /// Each word's runs of ASCII letters and digits, which JSON never escapes, for the raw file check.
    pub fn plain_words(&self) -> impl Iterator<Item = &str> {
        self.words.iter().flat_map(|w| {
            w.split(|c: char| !c.is_ascii_alphanumeric())
                .filter(|piece| piece.len() >= 2)
        })
    }
}

/// What the list shows for a chat before anything is typed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Meta {
    pub title: Option<String>,
    pub folder: Option<String>,
    /// Milliseconds since the Unix epoch of the newest record that has a time.
    pub last: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Read {
    pub meta: Meta,
    pub matched: bool,
    /// One line of message text around the first word's match.
    pub snippet: Option<String>,
}

// Only these names are read; tool input and results, thinking and everything else are skipped unkept.
#[derive(Deserialize)]
struct RecordIn {
    #[serde(default, rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(default, rename = "isSidechain")]
    sidechain: Option<bool>,
    #[serde(default, rename = "isMeta")]
    meta: Option<bool>,
    #[serde(default)]
    message: Option<MessageIn>,
}

#[derive(Deserialize)]
struct MessageIn {
    #[serde(default)]
    content: Option<ContentIn>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ContentIn {
    Text(String),
    Blocks(Vec<BlockIn>),
}

#[derive(Deserialize)]
struct BlockIn {
    #[serde(default, rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    text: Option<String>,
}

/// The list fields alone, from the first records and the last, without parsing the middle.
pub fn meta(bytes: &[u8]) -> Meta {
    let mut meta = Meta::default();
    let mut first_user = None;
    for record in bytes.split(|&b| b == b'\n').filter_map(record) {
        if meta.folder.is_none() {
            meta.folder = record.cwd.clone().filter(|c| !c.is_empty());
        }
        match record.kind.as_deref() {
            Some("summary") => meta.title = record.summary.as_deref().and_then(one_line),
            Some("user")
                if first_user.is_none()
                    && record.sidechain != Some(true)
                    && record.meta != Some(true) =>
            {
                first_user = texts(record.message)
                    .iter()
                    .find(|t| !is_command(t))
                    .and_then(|t| one_line(t));
            }
            _ => {}
        }
        if meta.folder.is_some() && (meta.title.is_some() || first_user.is_some()) {
            break;
        }
    }
    if meta.title.is_none() {
        meta.title = first_user;
    }
    // The newest record is near the end, so the end is read first.
    meta.last = bytes
        .rsplit(|&b| b == b'\n')
        .filter_map(record)
        .find_map(|r| r.timestamp.as_deref().and_then(epoch_ms));
    meta
}

/// Whether every plain word is somewhere in the raw file, any case; a file without one can't match.
pub fn holds_all(bytes: &[u8], query: &Query) -> bool {
    let words: Vec<&str> = query.plain_words().collect();
    if words.is_empty() {
        return true;
    }
    let lower = bytes.to_ascii_lowercase();
    // A file that isn't UTF-8 is left to the full read, which skips its broken lines.
    std::str::from_utf8(&lower).map_or(true, |text| words.iter().all(|w| text.contains(w)))
}

fn record(line: &[u8]) -> Option<RecordIn> {
    if line.len() > MOST_LINE || line.iter().all(u8::is_ascii_whitespace) {
        return None;
    }
    serde_json::from_slice(line).ok()
}

/// Reads a whole transcript; `cancelled` is asked between lines so a new keystroke stops it soon.
pub fn read(bytes: &[u8], query: &Query, cancelled: &dyn Fn() -> bool) -> Option<Read> {
    let mut meta = Meta::default();
    let mut first_user: Option<String> = None;
    let mut found = vec![false; query.words.len()];
    let mut snippet = None;
    for line in bytes.split(|&b| b == b'\n') {
        if cancelled() {
            return None;
        }
        if line.len() > MOST_LINE || line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        // A line that is not a record, or is cut short, is skipped; the rest of the chat still reads.
        let Ok(record) = serde_json::from_slice::<RecordIn>(line) else {
            continue;
        };
        if let Some(at) = record.timestamp.as_deref().and_then(epoch_ms) {
            meta.last = Some(meta.last.map_or(at, |was| was.max(at)));
        }
        if meta.folder.is_none() {
            meta.folder = record.cwd.clone().filter(|c| !c.is_empty());
        }
        match record.kind.as_deref() {
            Some("summary") => {
                if let Some(summary) = record.summary.as_deref().and_then(one_line) {
                    meta.title = Some(summary);
                }
            }
            Some(kind @ ("user" | "assistant")) => {
                if record.sidechain == Some(true) || record.meta == Some(true) {
                    continue;
                }
                for text in texts(record.message) {
                    if kind == "user" && first_user.is_none() && !is_command(&text) {
                        first_user = one_line(&text);
                    }
                    see(&text, &query.words, &mut found, &mut snippet);
                }
            }
            _ => {}
        }
    }
    if meta.title.is_none() {
        meta.title = first_user;
    }
    for field in [meta.title.clone(), meta.folder.clone()]
        .into_iter()
        .flatten()
    {
        mark(&field.to_lowercase(), &query.words, &mut found);
    }
    Some(Read {
        matched: found.iter().all(|&f| f),
        snippet: if query.is_empty() { None } else { snippet },
        meta,
    })
}

fn texts(message: Option<MessageIn>) -> Vec<String> {
    match message.and_then(|m| m.content) {
        Some(ContentIn::Text(text)) => vec![text],
        Some(ContentIn::Blocks(blocks)) => blocks
            .into_iter()
            .filter(|b| b.kind.as_deref() == Some("text"))
            .filter_map(|b| b.text)
            .collect(),
        None => Vec::new(),
    }
}

// Slash commands and their output arrive as user records wrapped in tags, not as words the user typed.
fn is_command(text: &str) -> bool {
    let start = text.trim_start();
    start.starts_with("<command-") || start.starts_with("<local-command")
}

fn see(text: &str, words: &[String], found: &mut [bool], snippet: &mut Option<String>) {
    if words.is_empty() {
        return;
    }
    let lower = text.to_lowercase();
    mark(&lower, words, found);
    if snippet.is_none()
        && let Some(at) = words.first().and_then(|w| lower.find(w.as_str()))
    {
        *snippet = Some(around(text, &lower, at));
    }
}

fn mark(lower: &str, words: &[String], found: &mut [bool]) {
    for (word, seen) in words.iter().zip(found.iter_mut()) {
        if !*seen && lower.contains(word.as_str()) {
            *seen = true;
        }
    }
}

/// About `SNIPPET_SIDE` characters each side of `at` in `lower`, cut from the original text on one line.
fn around(text: &str, lower: &str, at: usize) -> String {
    // Lower-casing can change byte lengths, so the cut is made by characters in the lower-cased copy.
    let before = lower[..at].chars().count();
    let chars: Vec<char> = text.chars().collect();
    let start = before.saturating_sub(SNIPPET_SIDE).min(chars.len());
    let end = (before + SNIPPET_SIDE * 2).min(chars.len());
    let cut: String = chars[start..end].iter().collect();
    let flat = cut.split_whitespace().collect::<Vec<_>>().join(" ");
    let lead = if start > 0 { "…" } else { "" };
    let tail = if end < chars.len() { "…" } else { "" };
    format!("{lead}{flat}{tail}")
}

fn one_line(text: &str) -> Option<String> {
    let line = text.lines().map(str::trim).find(|l| !l.is_empty())?;
    let flat = line.split_whitespace().collect::<Vec<_>>().join(" ");
    let cut: String = flat.chars().take(MOST_TITLE).collect();
    Some(if flat.chars().count() > MOST_TITLE {
        format!("{cut}…")
    } else {
        cut
    })
}

/// `2026-11-09T12:34:56.789Z` as milliseconds since the Unix epoch; any other shape is `None`.
pub fn epoch_ms(stamp: &str) -> Option<u64> {
    let b = stamp.as_bytes();
    if b.len() < 20 || b[4] != b'-' || b[7] != b'-' || b[10] != b'T' || !stamp.ends_with('Z') {
        return None;
    }
    let num = |from: usize, to: usize| stamp.get(from..to)?.parse::<u64>().ok();
    let (year, month, day) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    let (hour, minute, second) = (num(11, 13)?, num(14, 16)?, num(17, 19)?);
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }
    let millis = match stamp.get(19..stamp.len() - 1) {
        Some("") => 0,
        Some(frac) if frac.starts_with('.') && frac.len() > 1 => {
            let digits: String = frac[1..].chars().chain("000".chars()).take(3).collect();
            digits.parse::<u64>().ok()?
        }
        _ => return None,
    };
    // Howard Hinnant's days-from-civil, for the proleptic Gregorian calendar.
    let (y, m) = if month <= 2 {
        (year - 1, month + 9)
    } else {
        (year, month - 3)
    };
    let era = y / 400;
    let yoe = y - era * 400;
    let doy = (153 * m + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = (era * 146_097 + doe).checked_sub(719_468)?;
    Some(((days * 24 + hour) * 60 + minute) * 60_000 + second * 1000 + millis)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A fake chat shaped like Claude Code's records, with a fake secret in every part that is skipped.
    pub const FAKE_CHAT: &str = concat!(
        r#"{"type":"user","cwd":"C:\\code\\alpha","sessionId":"s","timestamp":"2026-11-01T10:00:00.000Z","message":{"role":"user","content":"<command-name>/clear</command-name>"}}"#,
        "\n",
        r#"{"type":"user","cwd":"C:\\code\\alpha","timestamp":"2026-11-01T10:00:01.000Z","message":{"role":"user","content":"Fix the login redirect bug\nmore detail here"}}"#,
        "\n",
        r#"{"type":"assistant","timestamp":"2026-11-01T10:00:05.000Z","message":{"role":"assistant","content":[{"type":"thinking","thinking":"FAKE-SECRET-thinking zebra"},{"type":"text","text":"I found the redirect in auth.ts and changed the callback url."},{"type":"tool_use","name":"Bash","input":{"command":"echo FAKE-SECRET-tooluse zebra"}}]}}"#,
        "\n",
        r#"{"type":"user","timestamp":"2026-11-01T10:00:06.000Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t","content":"FAKE-SECRET-toolresult sk-ant-000 zebra"}]}}"#,
        "\n",
        r#"{"type":"assistant","isSidechain":true,"timestamp":"2026-11-01T10:00:07.000Z","message":{"content":[{"type":"text","text":"FAKE-SECRET-sidechain zebra"}]}}"#,
        "\n",
        "{not json at all\n",
        r#"{"type":"assistant","timestamp":"2026-11-02T09:30:00.000Z","message":{"role":"assistant","content":[{"type":"text","text":"Done. The login page now returns to the dashboard."}]}}"#,
        "\n",
    );

    fn never() -> bool {
        false
    }

    fn read_fake(query: &str) -> Read {
        read(FAKE_CHAT.as_bytes(), &Query::new(query), &never).unwrap()
    }

    #[test]
    fn the_title_is_the_first_typed_message_and_the_folder_and_last_time_are_read() {
        let found = read_fake("");

        assert_eq!(
            found.meta.title.as_deref(),
            Some("Fix the login redirect bug")
        );
        assert_eq!(found.meta.folder.as_deref(), Some(r"C:\code\alpha"));
        assert_eq!(found.meta.last, epoch_ms("2026-11-02T09:30:00.000Z"));
        assert!(found.matched, "an empty query matches every chat");
        assert_eq!(found.snippet, None);
    }

    #[test]
    fn a_summary_record_names_the_chat() {
        let with_summary = format!(
            "{}\n{FAKE_CHAT}",
            r#"{"type":"summary","summary":"Login redirect fix","leafUuid":"x"}"#
        );

        let found = read(with_summary.as_bytes(), &Query::new(""), &never).unwrap();

        assert_eq!(found.meta.title.as_deref(), Some("Login redirect fix"));
    }

    #[test]
    fn every_word_must_match_in_any_message_title_or_folder_case_blind() {
        assert!(
            read_fake("LOGIN dashboard").matched,
            "words from two messages"
        );
        assert!(
            read_fake("alpha callback").matched,
            "a folder word and a message word"
        );
        assert!(!read_fake("login giraffe").matched);
    }

    #[test]
    fn tool_input_and_output_thinking_and_sidechains_never_match_or_show() {
        let found = read_fake("zebra");

        assert!(!found.matched);
        let with_secret = read_fake("fake-secret");
        assert!(!with_secret.matched);
        assert!(with_secret.snippet.is_none());
    }

    #[test]
    fn the_snippet_is_one_line_around_the_first_words_match() {
        let found = read_fake("callback");

        assert_eq!(
            found.snippet.as_deref(),
            Some("…the redirect in auth.ts and changed the callback url.")
        );
        let long = format!(
            r#"{{"type":"user","message":{{"content":"{}needle{}"}}}}"#,
            "a ".repeat(60),
            " b".repeat(60)
        );
        let cut = read(long.as_bytes(), &Query::new("needle"), &never)
            .unwrap()
            .snippet
            .unwrap();
        assert!(
            cut.starts_with('…') && cut.ends_with('…') && cut.contains("needle"),
            "{cut}"
        );
    }

    #[test]
    fn a_huge_line_is_skipped_and_the_rest_still_reads() {
        let huge = format!(
            "{}\n{FAKE_CHAT}",
            format_args!(
                r#"{{"type":"user","message":{{"content":"{}"}}}}"#,
                "x".repeat(MOST_LINE + 1)
            )
        );

        let found = read(huge.as_bytes(), &Query::new("dashboard"), &never).unwrap();

        assert!(found.matched);
        assert_eq!(
            found.meta.title.as_deref(),
            Some("Fix the login redirect bug")
        );
    }

    #[test]
    fn the_quick_list_fields_match_a_full_read() {
        let quick = meta(FAKE_CHAT.as_bytes());

        assert_eq!(quick, read_fake("").meta);
        let with_summary = format!(
            "{}\n{FAKE_CHAT}",
            r#"{"type":"summary","summary":"Login redirect fix","leafUuid":"x"}"#
        );
        assert_eq!(
            meta(with_summary.as_bytes()).title.as_deref(),
            Some("Login redirect fix")
        );
    }

    #[test]
    fn a_tool_result_after_the_first_message_keeps_the_title_when_the_folder_comes_late() {
        let late_folder = concat!(
            r#"{"type":"user","message":{"content":"First words here"}}"#,
            "
",
            r#"{"type":"user","message":{"content":[{"type":"tool_result","content":"x"}]}}"#,
            "
",
            r#"{"type":"assistant","cwd":"C:\\late","timestamp":"2026-11-01T10:00:00.000Z","message":{"content":"ok"}}"#,
        );

        let quick = meta(late_folder.as_bytes());

        assert_eq!(quick.title.as_deref(), Some("First words here"));
        assert_eq!(quick.folder.as_deref(), Some(r"C:\late"));
    }

    #[test]
    fn the_raw_check_needs_every_plain_word_in_any_case() {
        assert!(holds_all(
            FAKE_CHAT.as_bytes(),
            &Query::new("LOGIN Dashboard")
        ));
        assert!(!holds_all(
            FAKE_CHAT.as_bytes(),
            &Query::new("login giraffe")
        ));
        assert!(!holds_all(
            FAKE_CHAT.as_bytes(),
            &Query::new("zzzz-nothing")
        ));
        assert!(
            holds_all(FAKE_CHAT.as_bytes(), &Query::new(r"c:\code\alpha")),
            "a folder with backslashes"
        );
    }

    #[test]
    fn a_cancel_stops_the_read() {
        assert_eq!(
            read(FAKE_CHAT.as_bytes(), &Query::new("login"), &|| true),
            None
        );
    }

    #[test]
    fn times_read_as_epoch_milliseconds() {
        assert_eq!(epoch_ms("1970-01-01T00:00:00.000Z"), Some(0));
        assert_eq!(
            epoch_ms("2026-11-09T12:34:56.789Z"),
            Some(1_794_227_696_789)
        );
        assert_eq!(epoch_ms("2026-11-09T12:34:56Z"), Some(1_794_227_696_000));
        assert_eq!(epoch_ms("2026-13-09T12:34:56Z"), None);
        assert_eq!(epoch_ms("yesterday"), None);
    }

    #[test]
    fn plain_words_are_the_ones_a_raw_file_check_can_use() {
        let query = Query::new("Login c:\\code naïve 42 zzzz-nothing");

        assert_eq!(
            query.plain_words().collect::<Vec<_>>(),
            ["login", "code", "na", "ve", "42", "zzzz", "nothing"]
        );
    }
}
