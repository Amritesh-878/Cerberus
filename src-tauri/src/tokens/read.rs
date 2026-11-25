//! A chat file's replies as token counts: only the id, model, time and usage of each assistant record are read.

use memchr::memmem;
use serde::Deserialize;

use crate::findchat::read::epoch_ms;
use crate::tokens::Tokens;

/// One reply's usage, once per message id.
#[derive(Debug, Clone, PartialEq)]
pub struct Reply {
    pub id: String,
    pub at_ms: u64,
    pub model: String,
    pub fast: bool,
    pub tokens: Tokens,
    /// Of `tokens.cache_write`, the part written for an hour, which costs more.
    pub written_1h: u64,
}

// Only these names are read; the content and every other field are skipped unkept.
#[derive(Deserialize)]
struct Record {
    #[serde(rename = "type", default)]
    kind: Option<String>,
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(default)]
    message: Option<Message>,
}

#[derive(Deserialize)]
struct Message {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    usage: Option<Usage>,
}

#[derive(Deserialize)]
struct Usage {
    #[serde(default)]
    input_tokens: Option<u64>,
    #[serde(default)]
    output_tokens: Option<u64>,
    #[serde(default)]
    cache_read_input_tokens: Option<u64>,
    #[serde(default)]
    cache_creation_input_tokens: Option<u64>,
    #[serde(default)]
    cache_creation: Option<Written>,
    #[serde(default)]
    speed: Option<String>,
}

#[derive(Deserialize)]
struct Written {
    #[serde(default)]
    ephemeral_1h_input_tokens: Option<u64>,
}

/// Claude Code's stand-in for replies it made itself, which no API call priced.
const SYNTHETIC: &str = "<synthetic>";

/// Each assistant reply in the file once; a message written as several records keeps its last.
pub fn replies(bytes: &[u8]) -> Vec<Reply> {
    let usage = memmem::Finder::new(b"\"usage\"");
    let mut found: Vec<Reply> = Vec::new();
    for line in bytes.split(|&b| b == b'\n') {
        if usage.find(line).is_none() {
            continue;
        }
        let Some(reply) = reply(line) else { continue };
        match found.iter_mut().rev().find(|r| r.id == reply.id) {
            Some(earlier) => *earlier = reply,
            None => found.push(reply),
        }
    }
    found
}

fn reply(line: &[u8]) -> Option<Reply> {
    let record: Record = serde_json::from_slice(line).ok()?;
    if record.kind.as_deref() != Some("assistant") {
        return None;
    }
    let message = record.message?;
    let model = message.model.filter(|m| m != SYNTHETIC && !m.is_empty())?;
    let usage = message.usage?;
    let tokens = Tokens {
        input: usage.input_tokens.unwrap_or(0),
        output: usage.output_tokens.unwrap_or(0),
        cache_read: usage.cache_read_input_tokens.unwrap_or(0),
        cache_write: usage.cache_creation_input_tokens.unwrap_or(0),
    };
    Some(Reply {
        id: message.id.filter(|id| !id.is_empty())?,
        at_ms: epoch_ms(record.timestamp.as_deref()?)?,
        model,
        fast: usage.speed.as_deref() == Some("fast"),
        written_1h: usage
            .cache_creation
            .and_then(|w| w.ephemeral_1h_input_tokens)
            .unwrap_or(0),
        tokens,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHAT: &str = concat!(
        r#"{"type":"user","timestamp":"2026-11-25T10:00:00.000Z","message":{"role":"user","content":"FAKE-SECRET-prompt"}}"#,
        "\n",
        r#"{"type":"assistant","timestamp":"2026-11-25T10:00:05.000Z","requestId":"req_1","message":{"id":"msg_1","model":"claude-opus-5-5","content":[{"type":"thinking","thinking":""}],"usage":{"input_tokens":3,"cache_creation_input_tokens":100,"cache_read_input_tokens":2000,"cache_creation":{"ephemeral_5m_input_tokens":0,"ephemeral_1h_input_tokens":100},"output_tokens":5}}}"#,
        "\n",
        r#"{"type":"assistant","timestamp":"2026-11-25T10:00:06.000Z","requestId":"req_1","message":{"id":"msg_1","model":"claude-opus-5-5","content":[{"type":"text","text":"FAKE-SECRET-reply"}],"usage":{"input_tokens":3,"cache_creation_input_tokens":100,"cache_read_input_tokens":2000,"cache_creation":{"ephemeral_5m_input_tokens":0,"ephemeral_1h_input_tokens":100},"output_tokens":42}}}"#,
        "\n",
        r#"{"type":"assistant","timestamp":"2026-11-25T10:01:00.000Z","message":{"id":"msg_2","model":"<synthetic>","content":[],"usage":{"input_tokens":0,"output_tokens":0}}}"#,
        "\n",
        r#"{"type":"assistant","timestamp":"2026-11-25T10:02:00.000Z","message":{"id":"msg_3","model":"claude-opus-5-5","usage":{"input_tokens":"lots","output_tokens":1,"speed":"fast"}}}"#,
        "\n",
        "not json with \"usage\" in it\n",
    );

    #[test]
    fn each_reply_counts_once_with_its_last_record_and_nothing_else_is_kept() {
        let found = replies(CHAT.as_bytes());

        assert_eq!(
            found,
            [Reply {
                id: "msg_1".into(),
                at_ms: epoch_ms("2026-11-25T10:00:06.000Z").unwrap(),
                model: "claude-opus-5-5".into(),
                fast: false,
                tokens: Tokens {
                    input: 3,
                    output: 42,
                    cache_read: 2000,
                    cache_write: 100,
                },
                written_1h: 100,
            }]
        );
        assert!(!format!("{found:?}").contains("FAKE-SECRET"));
    }

    #[test]
    fn a_fast_reply_says_so() {
        let line = r#"{"type":"assistant","timestamp":"2026-11-25T10:02:00Z","message":{"id":"m","model":"claude-opus-5-5","usage":{"input_tokens":1,"output_tokens":1,"speed":"fast"}}}"#;

        let found = replies(line.as_bytes());

        assert!(found[0].fast);
    }
}
