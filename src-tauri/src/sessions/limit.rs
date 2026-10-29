use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

use serde::Deserialize;

// A limit is the last assistant record, a few KB; tool output after it would be a new turn anyway.
const TAIL_BYTES: u64 = 64 * 1024;

/// A usage limit the chat's last turn hit; `resets_at` is in seconds since 1970, when known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limit {
    pub resets_at: Option<u64>,
}

// Only these fields are kept; serde skips the message and every other field without storing them.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Record {
    #[serde(rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    is_api_error_message: bool,
    error: Option<String>,
    quota_limits: Option<QuotaLimits>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct QuotaLimits {
    resets_at: Option<u64>,
}

/// The limit the transcript's last assistant record reports, unless its reset time has passed.
pub fn limit_in(transcript: &Path, now: u64) -> io::Result<Option<Limit>> {
    Ok(last_limit(&tail(transcript)?, now))
}

fn tail(path: &Path) -> io::Result<Vec<u8>> {
    let mut file = File::open(path)?;
    let len = file.metadata()?.len();
    let start = len.saturating_sub(TAIL_BYTES);
    file.seek(SeekFrom::Start(start))?;
    let mut bytes = Vec::new();
    file.take(TAIL_BYTES).read_to_end(&mut bytes)?;
    // A cut-off first line is not a record.
    if start > 0 {
        let first_break = bytes
            .iter()
            .position(|&b| b == b'\n')
            .map_or(bytes.len(), |i| i + 1);
        bytes.drain(..first_break);
    }
    Ok(bytes)
}

fn last_limit(tail: &[u8], now: u64) -> Option<Limit> {
    let last = tail
        .split(|&b| b == b'\n')
        .rev()
        .filter_map(|line| serde_json::from_slice::<Record>(line).ok())
        .find(|record| record.kind.as_deref() == Some("assistant"))?;
    if !last.is_api_error_message || last.error.as_deref() != Some("rate_limit") {
        return None;
    }
    let resets_at = last.quota_limits.and_then(|q| q.resets_at);
    match resets_at {
        Some(at) if at <= now => None,
        _ => Some(Limit { resets_at }),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::accounts::testdir::TestDir;

    const NOW: u64 = 1_800_000_000;
    const RESETS: u64 = NOW + 3_600;
    const FAKE_SECRET: &str = "sk-fake-0000-not-a-real-key";

    fn user(text: &str) -> String {
        format!(
            r#"{{"type":"user","message":{{"role":"user","content":"{text}"}},"sessionId":"s"}}"#
        )
    }

    fn reply(text: &str) -> String {
        format!(
            r#"{{"type":"assistant","message":{{"role":"assistant","content":[{{"type":"text","text":"{text}"}}]}}}}"#
        )
    }

    fn limit_record(resets_at: Option<u64>) -> String {
        let quota = resets_at.map_or(String::new(), |at| {
            format!(
                r#""quotaLimits":{{"status":"rejected","resetsAt":{at},"rateLimitType":"five_hour"}},"#
            )
        });
        format!(
            r#"{{"type":"assistant","message":{{"model":"<synthetic>","role":"assistant","content":[{{"type":"text","text":"You've hit your session limit · resets 5:50am (Etc/Fake) {FAKE_SECRET}"}}]}},{quota}"error":"rate_limit","isApiErrorMessage":true}}"#
        )
    }

    fn lines(records: &[String]) -> Vec<u8> {
        records
            .iter()
            .map(|r| format!("{r}\n"))
            .collect::<String>()
            .into_bytes()
    }

    #[test]
    fn a_turn_that_hit_the_limit_gives_only_its_reset_time() {
        let tail = lines(&[
            user(&format!("my key is {FAKE_SECRET}")),
            limit_record(Some(RESETS)),
            r#"{"type":"system","subtype":"fake"}"#.to_owned(),
        ]);

        let found = last_limit(&tail, NOW);

        assert_eq!(
            found,
            Some(Limit {
                resets_at: Some(RESETS)
            })
        );
        // The result type holds a number and nothing else, so no transcript text can travel with it.
        assert!(!format!("{found:?}").contains("sk-fake"));
    }

    #[test]
    fn a_later_answer_clears_the_limit() {
        let tail = lines(&[limit_record(Some(RESETS)), user("again"), reply("done")]);

        assert_eq!(last_limit(&tail, NOW), None);
    }

    #[test]
    fn a_new_prompt_after_the_limit_keeps_it_until_an_answer_comes() {
        let tail = lines(&[limit_record(Some(RESETS)), user("again")]);

        assert_eq!(
            last_limit(&tail, NOW),
            Some(Limit {
                resets_at: Some(RESETS)
            })
        );
    }

    #[test]
    fn a_limit_whose_reset_has_passed_is_over() {
        let tail = lines(&[limit_record(Some(NOW - 1))]);

        assert_eq!(last_limit(&tail, NOW), None);
    }

    #[test]
    fn a_limit_without_quota_fields_has_no_reset_time() {
        let tail = lines(&[limit_record(None)]);

        assert_eq!(last_limit(&tail, NOW), Some(Limit { resets_at: None }));
    }

    #[test]
    fn other_api_errors_are_not_limits() {
        let tail = lines(&[
            r#"{"type":"assistant","error":"overloaded","isApiErrorMessage":true}"#.to_owned(),
        ]);

        assert_eq!(last_limit(&tail, NOW), None);
    }

    #[test]
    fn broken_lines_and_an_empty_tail_find_nothing() {
        assert_eq!(last_limit(b"", NOW), None);
        assert_eq!(last_limit(b"{not json\n\n{\"type\":", NOW), None);
    }

    #[test]
    fn only_the_tail_of_a_long_transcript_is_read() {
        let dir = TestDir::new();
        let path = dir.path().join("chat.jsonl");
        let padding = reply(&"x".repeat(1_000));
        let mut records = vec![padding; 200];
        records.push(limit_record(Some(RESETS)));
        fs::write(&path, lines(&records)).unwrap();

        let read = tail(&path).unwrap();

        assert!(read.len() as u64 <= TAIL_BYTES);
        assert!(read.starts_with(b"{\"type\":\"assistant\""));
        assert_eq!(
            limit_in(&path, NOW).unwrap(),
            Some(Limit {
                resets_at: Some(RESETS)
            })
        );
    }
}
