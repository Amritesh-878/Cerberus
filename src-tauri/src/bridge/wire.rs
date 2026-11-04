//! The lines a tab's helper and the app exchange over the bridge pipe, one JSON object each.

use serde::{Deserialize, Serialize};

/// The longest message a tab may send; it must fit Claude Code's 1 MiB inbox line once escaped.
pub const MOST_MESSAGE: usize = 64 * 1024;
/// A request line holds at most one message, escaped, plus a few names.
pub const MOST_LINE: u64 = 1024 * 1024;

pub const BRIDGE_VAR: &str = "CERBERUS_BRIDGE";
pub const TOKEN_VAR: &str = "CERBERUS_BRIDGE_TOKEN";
pub const SOCKET_VAR: &str = "CLAUDE_CODE_MESSAGING_SOCKET";
pub const CHILD_TOKEN_VAR: &str = "CLAUDE_CODE_MESSAGING_TOKEN";

/// The first and only line a helper writes on a connection; the token names its tab.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ask {
    pub token: String,
    #[serde(flatten)]
    pub kind: AskKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "ask", rename_all = "camelCase")]
pub enum AskKind {
    /// The helper is up; the app answers `Order::Ready`, then sends orders on this connection.
    #[serde(rename_all = "camelCase")]
    Listen {
        /// Whether Claude Code gave the helper its own inbox's socket and token.
        messaging: bool,
        /// That inbox's address, sent as `from` when this tab's messages are delivered.
        pipe: Option<String>,
    },
    /// What the tab's own session file says, once the helper finds it.
    #[serde(rename_all = "camelCase")]
    Facts {
        version: Option<String>,
        peer_protocol: Option<u64>,
    },
    List,
    /// Waits until the user decides; `call` is the MCP request id, so a cancel can find it.
    Send {
        call: String,
        to: String,
        message: String,
    },
    Cancel {
        call: String,
    },
    /// The answer to `Order::Deliver` with the same id.
    Delivered {
        id: u64,
        written: Written,
    },
}

/// What the app tells a listening helper.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "order", rename_all = "camelCase")]
pub enum Order {
    Ready,
    /// Write this into the tab's own Claude Code inbox.
    #[serde(rename_all = "camelCase")]
    Deliver {
        id: u64,
        from_pipe: Option<String>,
        from_name: String,
        body: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "written", rename_all = "camelCase")]
pub enum Written {
    Yes,
    No { reason: String },
}

/// The app's one-line answer to every ask but `Listen`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reply", rename_all = "camelCase")]
pub enum Reply {
    Ok,
    Tabs { tabs: Vec<TabEntry> },
    Outcome { outcome: Outcome },
    Refused { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabEntry {
    pub id: String,
    /// "account · project / tab".
    pub label: String,
    pub busy: bool,
}

/// What happened to a sent message, told to the sender as its tool result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "camelCase")]
pub enum Outcome {
    /// `edited` holds the text the user changed it to, when they did.
    Delivered {
        to: String,
        edited: Option<String>,
    },
    /// In the target's input box; it goes when the user presses Enter there.
    Prefilled {
        to: String,
        edited: Option<String>,
    },
    /// Written to the target's inbox, but the tab had not shown it after a few seconds.
    Unconfirmed {
        to: String,
        edited: Option<String>,
    },
    Rejected {
        to: String,
    },
    Failed {
        reason: String,
    },
    /// The sender stopped waiting; nobody reads this.
    Cancelled,
}

impl Outcome {
    /// The sender's tool result, and whether it reads as an error.
    pub fn tool_text(&self) -> (String, bool) {
        let with_edit = |head: String, edited: &Option<String>| match edited {
            Some(text) => {
                format!("{head} The user edited it before sending; it now reads:\n{text}")
            }
            None => head,
        };
        match self {
            Self::Delivered { to, edited } => (
                with_edit(
                    format!("Delivered to {to}. A reply, if any, arrives as a new message."),
                    edited,
                ),
                false,
            ),
            Self::Prefilled { to, edited } => (
                with_edit(
                    format!(
                        "Approved and placed in the input box of {to}; it is sent when the user presses Enter there."
                    ),
                    edited,
                ),
                false,
            ),
            Self::Unconfirmed { to, edited } => (
                with_edit(
                    format!(
                        "Written to the inbox of {to}, but that tab had not shown it after 5 seconds. It may still arrive there."
                    ),
                    edited,
                ),
                false,
            ),
            Self::Rejected { to } => (
                format!("Not sent: the user rejected this message to {to}."),
                true,
            ),
            Self::Failed { reason } => (format!("Not sent: {reason}"), true),
            Self::Cancelled => ("Not sent: the call was cancelled.".to_owned(), true),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ask_is_one_flat_object() {
        let ask = Ask {
            token: "t".to_owned(),
            kind: AskKind::Send {
                call: "3".to_owned(),
                to: "12".to_owned(),
                message: "hi".to_owned(),
            },
        };

        let line = serde_json::to_string(&ask).unwrap();

        assert_eq!(
            line,
            r#"{"token":"t","ask":"send","call":"3","to":"12","message":"hi"}"#
        );
        assert_eq!(serde_json::from_str::<Ask>(&line).unwrap(), ask);
    }

    #[test]
    fn each_outcome_tells_the_sender_what_happened() {
        let to = "Work · Plans / planner".to_owned();
        let edited = Some("shorter".to_owned());

        let (delivered, _) = Outcome::Delivered {
            to: to.clone(),
            edited: None,
        }
        .tool_text();
        let (after_edit, _) = Outcome::Delivered {
            to: to.clone(),
            edited: edited.clone(),
        }
        .tool_text();
        let (prefilled, _) = Outcome::Prefilled {
            to: to.clone(),
            edited,
        }
        .tool_text();
        let (rejected, is_error) = Outcome::Rejected { to }.tool_text();

        assert!(delivered.starts_with("Delivered to Work · Plans / planner."));
        assert!(after_edit.ends_with("it now reads:\nshorter"));
        assert!(prefilled.contains("presses Enter") && prefilled.ends_with("shorter"));
        assert!(rejected.starts_with("Not sent: the user rejected"));
        assert!(is_error);
    }
}
