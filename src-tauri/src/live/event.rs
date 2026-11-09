//! What leaves the helper: a few named fields of Claude Code's status line and hook input, nothing else.

use serde::de::{Deserializer, IgnoredAny};
use serde::{Deserialize, Serialize};

pub use crate::live::hooks::{Subagent, Task, TaskKind, Tasks, Wrote};

/// A status line input is a few KiB; anything far larger is not one.
pub const MOST_INPUT: usize = 1024 * 1024;
/// One event line from the helper, token and fields included.
pub const MOST_LINE: u64 = 4 * 1024;
const MOST_SESSION: usize = 128;
const MOST_MODEL: usize = 64;
// Unix seconds past this are milliseconds or garbage, not a reset time.
const LATEST_RESET_S: f64 = 10_000_000_000.0;

pub const PIPE_VAR: &str = "CERBERUS_LIVE_PIPE";
pub const TOKEN_VAR: &str = "CERBERUS_LIVE_TOKEN";

/// The one line a helper writes per update; the token names its tab.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Line {
    pub token: String,
    pub event: Event,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Event {
    Status(Status),
    Wrote(Wrote),
    Tasks(Tasks),
    Subagent(Subagent),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// Milliseconds since the Unix epoch, when the helper ran.
    pub at: u64,
    pub session: Option<String>,
    pub model: Option<String>,
    /// Share of the context window in use, 0 to 100.
    pub context: Option<f64>,
    pub five_hour: Option<Window>,
    pub seven_day: Option<Window>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Window {
    /// 0 to 100.
    pub used: f64,
    /// Unix seconds.
    pub resets_at: u64,
}

// Only these names are read; serde skips every other field without keeping it.
#[derive(Deserialize)]
struct StatusIn {
    #[serde(default, deserialize_with = "text")]
    session_id: Option<String>,
    #[serde(default)]
    model: Option<ModelIn>,
    #[serde(default)]
    context_window: Option<ContextIn>,
    #[serde(default)]
    rate_limits: Option<LimitsIn>,
}

#[derive(Deserialize)]
struct ModelIn {
    #[serde(default, deserialize_with = "text")]
    display_name: Option<String>,
    #[serde(default, deserialize_with = "text")]
    id: Option<String>,
}

#[derive(Deserialize)]
struct ContextIn {
    #[serde(default, deserialize_with = "number")]
    used_percentage: Option<f64>,
}

#[derive(Deserialize)]
struct LimitsIn {
    #[serde(default)]
    five_hour: Option<WindowIn>,
    #[serde(default)]
    seven_day: Option<WindowIn>,
}

#[derive(Deserialize)]
struct WindowIn {
    #[serde(default, deserialize_with = "number")]
    used_percentage: Option<f64>,
    #[serde(default, deserialize_with = "number")]
    resets_at: Option<f64>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Lenient<T> {
    Value(T),
    Other(IgnoredAny),
}

// A field of an unexpected type reads as missing instead of failing the whole update.
fn number<'de, D: Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
    Ok(match Lenient::<f64>::deserialize(d)? {
        Lenient::Value(n) if n.is_finite() => Some(n),
        _ => None,
    })
}

fn text<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    Ok(match Lenient::<String>::deserialize(d)? {
        Lenient::Value(s) => Some(s),
        Lenient::Other(_) => None,
    })
}

/// The kept fields of one status line input, or `None` when it is not one.
pub fn status_from(input: &[u8], at: u64) -> Option<Status> {
    if input.len() > MOST_INPUT {
        return None;
    }
    let parsed: StatusIn = serde_json::from_slice(input).ok()?;
    let limits = parsed.rate_limits;
    let model = parsed.model.and_then(|m| m.display_name.or(m.id));
    Some(Status {
        at,
        session: parsed.session_id.filter(|s| session_like(s)),
        model: model.as_deref().and_then(model_name),
        context: parsed
            .context_window
            .and_then(|c| c.used_percentage)
            .map(percent),
        five_hour: limits.as_ref().and_then(|l| window(l.five_hour.as_ref())),
        seven_day: limits.as_ref().and_then(|l| window(l.seven_day.as_ref())),
    })
}

impl Status {
    /// The same checks again on the app's side, since anything holding the token could send a line.
    pub fn checked(self) -> Self {
        let window = |w: Option<Window>| {
            w.and_then(|w| {
                window(Some(&WindowIn {
                    used_percentage: Some(w.used),
                    resets_at: Some(w.resets_at as f64),
                }))
            })
        };
        Self {
            at: self.at,
            session: self.session.filter(|s| session_like(s)),
            model: self.model.as_deref().and_then(model_name),
            context: self.context.filter(|c| c.is_finite()).map(percent),
            five_hour: window(self.five_hour),
            seven_day: window(self.seven_day),
        }
    }
}

fn window(found: Option<&WindowIn>) -> Option<Window> {
    let found = found?;
    let resets = found.resets_at?;
    if !(1.0..LATEST_RESET_S).contains(&resets) {
        return None;
    }
    Some(Window {
        used: percent(found.used_percentage?),
        // In range by the check above, so the cast neither wraps nor saturates.
        resets_at: resets as u64,
    })
}

fn percent(n: f64) -> f64 {
    n.clamp(0.0, 100.0)
}

fn session_like(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= MOST_SESSION
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn model_name(s: &str) -> Option<String> {
    let clean: String = s
        .chars()
        .filter(|c| !c.is_control())
        .take(MOST_MODEL)
        .collect();
    let clean = clean.trim();
    (!clean.is_empty()).then(|| clean.to_owned())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    // A fake status line input shaped like the real one, with fake secrets in every dropped field.
    pub const FAKE_INPUT: &str = r#"{
        "session_id": "b1fe1f2e-f7e7-4e25-922d-0596af0123fb",
        "session_name": "FAKE-SECRET-name-sk-ant-000",
        "transcript_path": "C:\\Users\\someone\\FAKE-SECRET-transcript.jsonl",
        "cwd": "C:\\FAKE-SECRET-cwd",
        "prompt_id": "FAKE-SECRET-prompt",
        "model": {"id": "claude-opus-5-5", "display_name": "Opus 5.5"},
        "workspace": {"current_dir": "C:\\FAKE-SECRET-dir", "project_dir": "C:\\FAKE-SECRET-dir", "added_dirs": []},
        "version": "2.1.319",
        "output_style": {"name": "FAKE-SECRET-style"},
        "cost": {"total_cost_usd": 1.5, "total_lines_added": 3},
        "context_window": {"total_input_tokens": 120, "context_window_size": 200000,
            "current_usage": {"input_tokens": 10}, "used_percentage": 42.5, "remaining_percentage": 57.5},
        "prompt_cache": {"last_miss_cause": "FAKE-SECRET-cause"},
        "rate_limits": {
            "five_hour": {"used_percentage": 87.2, "resets_at": 4102444800},
            "seven_day": {"used_percentage": 41, "resets_at": 4102531200},
            "spend_limit": {"used_usd": 9.99, "period": "FAKE-SECRET-period"}
        }
    }"#;

    #[test]
    fn only_the_named_fields_are_kept() {
        let status = status_from(FAKE_INPUT.as_bytes(), 7).unwrap();

        assert_eq!(
            status,
            Status {
                at: 7,
                session: Some("b1fe1f2e-f7e7-4e25-922d-0596af0123fb".into()),
                model: Some("Opus 5.5".into()),
                context: Some(42.5),
                five_hour: Some(Window {
                    used: 87.2,
                    resets_at: 4_102_444_800
                }),
                seven_day: Some(Window {
                    used: 41.0,
                    resets_at: 4_102_531_200
                }),
            }
        );
        let line = serde_json::to_string(&Line {
            token: "t".into(),
            event: Event::Status(status),
        })
        .unwrap();
        assert!(!line.contains("FAKE-SECRET"), "{line}");
        assert!(!line.contains("someone"), "{line}");
    }

    #[test]
    fn an_early_input_has_no_limits_and_no_context_yet() {
        let early = r#"{"session_id":"abc","model":{"id":"claude-x"},
            "context_window":{"used_percentage":null,"current_usage":null}}"#;

        let status = status_from(early.as_bytes(), 1).unwrap();

        assert_eq!(status.context, None);
        assert_eq!(status.model.as_deref(), Some("claude-x"));
        assert_eq!(status.five_hour, None);
        assert_eq!(status.seven_day, None);
    }

    #[test]
    fn odd_values_read_as_missing_or_are_held_in_range() {
        let odd = r#"{"session_id":"../../etc","model":{"display_name":"\u001b[31mRed\u0007"},
            "context_window":{"used_percentage":"lots"},
            "rate_limits":{"five_hour":{"used_percentage":250,"resets_at":1762700400},
                           "seven_day":{"used_percentage":5,"resets_at":1762700400000}}}"#;

        let status = status_from(odd.as_bytes(), 1).unwrap();

        assert_eq!(status.session, None);
        assert_eq!(status.model.as_deref(), Some("[31mRed"));
        assert_eq!(status.context, None);
        assert_eq!(status.five_hour.map(|w| w.used), Some(100.0));
        assert_eq!(status.seven_day, None);
    }

    #[test]
    fn what_is_not_a_status_input_is_nothing() {
        assert_eq!(status_from(b"not json", 1), None);
        assert_eq!(status_from(b"[1,2]", 1), None);
        assert_eq!(status_from(&vec![b' '; MOST_INPUT + 1], 1), None);
    }

    #[test]
    fn the_app_checks_a_line_again() {
        let forged = Status {
            at: 1,
            session: Some("x y".into()),
            model: Some(String::new()),
            context: Some(f64::NAN),
            five_hour: Some(Window {
                used: -5.0,
                resets_at: 1_762_700_400,
            }),
            seven_day: Some(Window {
                used: 5.0,
                resets_at: 0,
            }),
        };

        let checked = forged.checked();

        assert_eq!(checked.session, None);
        assert_eq!(checked.model, None);
        assert_eq!(checked.context, None);
        assert_eq!(checked.five_hour.map(|w| w.used), Some(0.0));
        assert_eq!(checked.seven_day, None);
    }
}
