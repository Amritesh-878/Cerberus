//! Messages between Claude tabs on different accounts, each one held until the user approves it.

mod check;
mod deliver;
pub mod inbox;
pub mod link;
pub mod mcp;
mod queue;
mod serve;
pub mod spawn;
#[cfg(test)]
mod tests;
pub mod wire;

use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize, Serializer};

use crate::bridge::check::Sighting;
use crate::bridge::deliver::{Landed, land, prefill_into};
use crate::bridge::queue::{PendingView, Queue};
use crate::bridge::wire::{MOST_MESSAGE, Outcome, Written};
use crate::pty::SessionId;
use crate::saved::TabId;

/// A helper that has not said hello by then is not coming.
const HELLO_WAIT: Duration = Duration::from_millis(if cfg!(test) { 500 } else { 10_000 });

#[derive(Debug, thiserror::Error)]
pub enum BridgeError {
    #[error("that message is no longer waiting")]
    Gone,
    #[error("the message is {0} bytes; the most is {MOST_MESSAGE}")]
    TooLong(usize),
    #[error("the message is empty")]
    Empty,
    #[error("{0}")]
    NotTyped(String),
}

impl Serialize for BridgeError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

/// How the app names a tab to people and to other chats.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Label {
    pub account: String,
    /// "project / tab".
    pub place: String,
}

impl Label {
    pub fn text(&self) -> String {
        format!("{} · {}", self.account, self.place)
    }
}

/// What the bridge needs from the rest of the app.
pub trait Hooks: Send + Sync + 'static {
    fn label(&self, tab: TabId) -> Label;
    /// The waiting messages or a tab's note changed.
    fn changed(&self, snapshot: Snapshot);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Note {
    /// Approved messages reach this tab by typing them into its input, for the user to send.
    PrefillOnly,
    /// This tab's helper never started, so it can neither send nor be reached.
    Off,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TabNote {
    pub tab: TabId,
    pub note: Note,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Snapshot {
    pub pending: Vec<PendingView>,
    pub notes: Vec<TabNote>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Decision {
    Approve { text: String },
    Reject,
}

/// What the popup tells the user after a decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "how", rename_all = "camelCase")]
pub enum Decided {
    Delivered {
        from: TabId,
        to: TabId,
    },
    /// In the target's input box, waiting for Enter.
    Prefilled {
        from: TabId,
        to: TabId,
    },
    /// In the target's inbox but not shown yet; `id` lets the user type it in instead.
    Unconfirmed {
        id: u64,
        from: TabId,
        to: TabId,
    },
    Rejected,
    Failed {
        reason: String,
    },
}

/// What a tab's `claude` gets so its helper can reach the bridge; the token names the tab.
pub struct Ticket {
    pub token: String,
    pub env: Vec<(&'static str, OsString)>,
}

struct Hello {
    messaging: bool,
    pipe: Option<String>,
    listen: Arc<Mutex<link::Stream>>,
}

struct TabLink {
    tab: TabId,
    /// The account's config dir, lower-cased: the wall Claude Code's own messages stop at.
    home: String,
    pty: Option<SessionId>,
    born: Instant,
    hello: Option<Hello>,
    facts: Option<(Option<String>, Option<u64>)>,
    busy: bool,
    sighting: Option<(Sighting, SyncSender<()>)>,
}

impl TabLink {
    fn note(&self) -> Option<Note> {
        let Some(hello) = &self.hello else {
            return (self.born.elapsed() >= HELLO_WAIT).then_some(Note::Off);
        };
        let gate_failed = self
            .facts
            .as_ref()
            .is_some_and(|(version, protocol)| !inbox::passes_gate(version.as_deref(), *protocol));
        (!cfg!(windows) || !hello.messaging || gate_failed).then_some(Note::PrefillOnly)
    }

    /// Whether the helper can write into this tab's inbox; unknown facts mean no.
    fn takes_inbox(&self) -> bool {
        let facts_pass = self
            .facts
            .as_ref()
            .is_some_and(|(version, protocol)| inbox::passes_gate(version.as_deref(), *protocol));
        cfg!(windows) && self.hello.as_ref().is_some_and(|h| h.messaging) && facts_pass
    }
}

#[derive(Default)]
struct State {
    links: HashMap<String, TabLink>,
    queue: Queue<SyncSender<Outcome>>,
    /// Each delivery's target tab, so only that tab's helper can answer it.
    deliveries: HashMap<u64, (TabId, SyncSender<Written>)>,
    next_delivery: u64,
    /// Cancels that came before their send, since the two travel on separate connections; a call id carries its helper run's tag, so a restart can't reuse one.
    early_cancels: HashSet<(TabId, String)>,
    /// Messages written to an inbox that did not show, kept so the user can type one in instead.
    unseen: HashMap<u64, (TabId, String)>,
}

impl State {
    fn by_tab(&self, tab: TabId) -> Option<&TabLink> {
        self.links
            .values()
            .find(|l| l.tab == tab && l.hello.is_some())
    }

    fn by_tab_mut(&mut self, tab: TabId) -> Option<&mut TabLink> {
        self.links.values_mut().find(|l| l.tab == tab)
    }

    fn snapshot(&self) -> Snapshot {
        let mut notes: Vec<TabNote> = self
            .links
            .values()
            .filter_map(|l| l.note().map(|note| TabNote { tab: l.tab, note }))
            .collect();
        notes.sort_by_key(|n| n.tab);
        Snapshot {
            pending: self.queue.views(),
            notes,
        }
    }
}

struct Core {
    name: String,
    hooks: Box<dyn Hooks>,
    state: Mutex<State>,
    /// Set while a delivery waits to show, so other output skips the lock.
    watching: AtomicBool,
}

// A panicked holder leaves the maps usable, so poisoning is not a reason to fail.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Core {
    fn state(&self) -> MutexGuard<'_, State> {
        lock(&self.state)
    }

    /// Called with no lock held, since the hooks reach other parts of the app.
    fn changed(&self) {
        let snapshot = self.state().snapshot();
        self.hooks.changed(snapshot);
    }
}

/// The app's end of the bridge; off when its pipe could not be made.
#[derive(Clone)]
pub struct Bridge(Option<Arc<Core>>);

impl Bridge {
    pub fn start(hooks: impl Hooks) -> io::Result<Self> {
        let name = link::new_name()?;
        let core = Arc::new(Core {
            name,
            hooks: Box::new(hooks),
            state: Mutex::new(State::default()),
            watching: AtomicBool::new(false),
        });
        serve::listen(&core)?;
        Ok(Self(Some(core)))
    }

    pub fn off() -> Self {
        Self(None)
    }

    /// Links a `claude` tab about to start under the account whose config dir is `home`.
    pub fn register(&self, tab: TabId, home: &Path) -> Option<Ticket> {
        let core = self.0.as_ref()?;
        let token = match link::random_hex(32) {
            Ok(token) => token,
            Err(e) => {
                eprintln!("bridge: no token for tab {tab}: {e}");
                return None;
            }
        };
        core.state().links.insert(
            token.clone(),
            TabLink {
                tab,
                home: home.to_string_lossy().to_lowercase(),
                pty: None,
                born: Instant::now(),
                hello: None,
                facts: None,
                busy: false,
                sighting: None,
            },
        );
        let weak = Arc::downgrade(core);
        // Shows "messages off" if the helper never says hello.
        let _ = thread::Builder::new().spawn(move || {
            thread::sleep(HELLO_WAIT);
            if let Some(core) = Weak::upgrade(&weak) {
                core.changed();
            }
        });
        Some(Ticket {
            env: vec![
                (wire::BRIDGE_VAR, OsString::from(&core.name)),
                (wire::TOKEN_VAR, OsString::from(&token)),
            ],
            token,
        })
    }

    pub fn attach(&self, token: &str, pty: SessionId) {
        if let Some(core) = &self.0
            && let Some(link) = core.state().links.get_mut(token)
        {
            link.pty = Some(pty);
        }
    }

    /// The tab's program ended: its messages, both ways, end with it.
    pub fn closed(&self, token: &str) {
        let Some(core) = &self.0 else { return };
        let gone = {
            let mut state = core.state();
            let Some(link) = state.links.remove(token) else {
                return;
            };
            state.early_cancels.retain(|(tab, _)| *tab != link.tab);
            state.unseen.retain(|_, (to, _)| *to != link.tab);
            state
                .queue
                .drop_tab(link.tab)
                .into_iter()
                .map(|p| (p.from == link.tab, p.waiter))
                .collect::<Vec<_>>()
        };
        for (was_sender, waiter) in gone {
            let outcome = if was_sender {
                Outcome::Cancelled
            } else {
                Outcome::Failed {
                    reason: "the target tab closed before the user approved it.".to_owned(),
                }
            };
            let _ = waiter.try_send(outcome);
        }
        core.changed();
    }

    /// Ends every waiting call plainly, for an app that is closing.
    pub fn shutdown(&self) {
        let Some(core) = &self.0 else { return };
        let waiters: Vec<_> = {
            let mut state = core.state();
            let tabs: Vec<TabId> = state.links.values().map(|l| l.tab).collect();
            tabs.into_iter()
                .flat_map(|tab| state.queue.drop_tab(tab))
                .map(|p| p.waiter)
                .collect()
        };
        for waiter in waiters {
            let _ = waiter.try_send(Outcome::Failed {
                reason: "Cerberus closed before the user decided.".to_owned(),
            });
        }
    }

    /// Fed every chunk of a `claude` tab's output; cheap unless a delivery is waiting to show.
    pub fn output(&self, token: &str, bytes: &[u8]) {
        let Some(core) = &self.0 else { return };
        if !core.watching.load(Ordering::Acquire) {
            return;
        }
        let mut state = core.state();
        if let Some(link) = state.links.get_mut(token)
            && let Some((sighting, seen)) = &mut link.sighting
            && sighting.feed(bytes)
        {
            let _ = seen.try_send(());
        }
    }

    /// The tab started or finished a turn; not proof a delivery landed, since the user may have started it.
    pub fn working(&self, token: &str, busy: bool) {
        let Some(core) = &self.0 else { return };
        if let Some(link) = core.state().links.get_mut(token) {
            link.busy = busy;
        }
    }

    /// Types a message that reached an inbox but never showed into the target's input, once.
    pub fn type_in(
        &self,
        id: u64,
        type_into: impl Fn(SessionId, &[u8]) -> io::Result<()>,
    ) -> Result<(), BridgeError> {
        let core = self.0.as_ref().ok_or(BridgeError::Gone)?;
        let (to, body) = core.state().unseen.remove(&id).ok_or(BridgeError::Gone)?;
        prefill_into(core, to, &body, type_into).map_err(BridgeError::NotTyped)
    }

    /// The user closed the notice, so the text kept for typing it in goes.
    pub fn discard(&self, id: u64) {
        if let Some(core) = &self.0 {
            core.state().unseen.remove(&id);
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        self.0
            .as_ref()
            .map(|core| core.state().snapshot())
            .unwrap_or_default()
    }

    /// Acts on the user's choice; blocks while an approved message is delivered, ~10 s at most.
    pub fn decide(
        &self,
        id: u64,
        decision: Decision,
        type_into: impl Fn(SessionId, &[u8]) -> io::Result<()>,
    ) -> Result<Decided, BridgeError> {
        let Some(core) = &self.0 else {
            return Err(BridgeError::Gone);
        };
        if let Decision::Approve { text } = &decision {
            if text.trim().is_empty() {
                return Err(BridgeError::Empty);
            }
            if text.len() > MOST_MESSAGE {
                return Err(BridgeError::TooLong(text.len()));
            }
        }
        let pending = core.state().queue.take(id).ok_or(BridgeError::Gone)?;
        core.changed();
        let to = core.hooks.label(pending.to).text();
        let Decision::Approve { text } = decision else {
            let _ = pending.waiter.try_send(Outcome::Rejected { to });
            return Ok(Decided::Rejected);
        };
        let edited = (text != pending.text).then(|| text.clone());
        let from_label = core.hooks.label(pending.from).text();
        let body = format!(
            "[Cerberus: a message from tab \"{id}\" ({from_label}), approved by the user. Reply with send_to_tab to tab \"{id}\".]\n{text}",
            id = pending.from
        );
        let (from, target) = (pending.from, pending.to);
        let (outcome, decided) = match land(core, (from, target), &from_label, &body, type_into) {
            Landed::Seen => (
                Outcome::Delivered { to, edited },
                Decided::Delivered { from, to: target },
            ),
            Landed::Unseen => {
                core.state().unseen.insert(id, (target, body));
                (
                    Outcome::Unconfirmed { to, edited },
                    Decided::Unconfirmed {
                        id,
                        from,
                        to: target,
                    },
                )
            }
            Landed::Typed => (
                Outcome::Prefilled { to, edited },
                Decided::Prefilled { from, to: target },
            ),
            Landed::Failed(reason) => (
                Outcome::Failed {
                    reason: reason.clone(),
                },
                Decided::Failed { reason },
            ),
        };
        let _ = pending.waiter.try_send(outcome);
        Ok(decided)
    }
}
