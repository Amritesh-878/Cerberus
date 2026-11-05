//! Getting an approved message into the target tab: its own inbox first, typed into its input otherwise.

use std::io::{self, Write};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::bridge::check::{Sighting, prefill};
use crate::bridge::wire::{Order, Written};
use crate::bridge::{Core, link, lock};
use crate::pty::SessionId;
use crate::saved::TabId;

/// How long the target's helper has to write the message, and then how long the tab has to show it.
const DELIVERY_WAIT: Duration = Duration::from_millis(if cfg!(test) { 2_000 } else { 5_000 });

/// A delivery under way: the order for the target's helper and where its answers arrive.
struct Armed {
    id: u64,
    listen: Arc<Mutex<link::Stream>>,
    order: Order,
    written: Receiver<Written>,
    seen: Receiver<()>,
}

/// Where an approved message ended up.
pub enum Landed {
    /// In the target's inbox, and the tab showed it.
    Seen,
    /// Maybe in the target's inbox, not shown yet; typing it in too could send it twice.
    Unseen,
    /// Typed into the target's input box, waiting for Enter.
    Typed,
    Failed(String),
}

/// What the target's helper did with an order.
enum Reached {
    Seen,
    /// Written, or unanswered and so perhaps written.
    Unseen,
    NotWritten(String),
}

/// The inbox first; typed in only when the inbox surely didn't take it.
pub fn land(
    core: &Core,
    route: (TabId, TabId),
    from_name: &str,
    body: &str,
    type_into: impl Fn(SessionId, &[u8]) -> io::Result<()>,
) -> Landed {
    let (from, to) = route;
    match deliver(core, from, to, from_name, body) {
        Reached::Seen => Landed::Seen,
        Reached::Unseen => Landed::Unseen,
        Reached::NotWritten(why) => {
            eprintln!("bridge: typing into tab {to} instead: {why}");
            match prefill_into(core, to, body, type_into) {
                Ok(()) => Landed::Typed,
                Err(reason) => Landed::Failed(reason),
            }
        }
    }
}

/// Asks the target's helper to write into its own inbox, then watches the tab for it.
fn deliver(core: &Core, from: TabId, to: TabId, from_name: &str, body: &str) -> Reached {
    let armed = match arm(core, from, to, from_name, body) {
        Ok(armed) => armed,
        Err(why) => return Reached::NotWritten(why),
    };
    let result = match send_order(&armed.listen, &armed.order) {
        Err(e) => Reached::NotWritten(format!("could not reach the target's helper: {e}")),
        Ok(()) => match armed.written.recv_timeout(DELIVERY_WAIT) {
            Ok(Written::Yes) => seen_within(&armed.seen, DELIVERY_WAIT),
            Ok(Written::No { reason }) => Reached::NotWritten(reason),
            // The helper writes before it answers, so silence may still mean the message landed.
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => {
                seen_within(&armed.seen, Duration::ZERO)
            }
        },
    };
    let mut state = core.state();
    state.deliveries.remove(&armed.id);
    if let Some(target) = state.by_tab_mut(to) {
        target.sighting = None;
    }
    let still = state.links.values().any(|l| l.sighting.is_some());
    core.watching.store(still, Ordering::Release);
    result
}

fn arm(core: &Core, from: TabId, to: TabId, from_name: &str, body: &str) -> Result<Armed, String> {
    let mut state = core.state();
    let from_pipe = state
        .by_tab(from)
        .and_then(|l| l.hello.as_ref())
        .and_then(|h| h.pipe.clone())
        .ok_or("the sending tab has no inbox address")?;
    let target = state.by_tab(to).ok_or("the target tab is not linked")?;
    if !target.takes_inbox() {
        return Err("the target tab's inbox is not usable".to_owned());
    }
    let listen = target
        .hello
        .as_ref()
        .map(|h| Arc::clone(&h.listen))
        .ok_or("the target tab is not linked")?;
    state.next_delivery += 1;
    let id = state.next_delivery;
    let (written_tx, written) = mpsc::sync_channel(1);
    state.deliveries.insert(id, (to, written_tx));
    let (seen_tx, seen) = mpsc::sync_channel(1);
    if let Some(target) = state.by_tab_mut(to) {
        target.sighting = Some((Sighting::default(), seen_tx));
    }
    // Set before the order goes out, so output that arrives at once is still read.
    core.watching.store(true, Ordering::Release);
    Ok(Armed {
        id,
        listen,
        order: Order::Deliver {
            id,
            from_pipe: Some(from_pipe),
            from_name: from_name.to_owned(),
            body: body.to_owned(),
        },
        written,
        seen,
    })
}

fn send_order(listen: &Mutex<link::Stream>, order: &Order) -> io::Result<()> {
    let mut line = serde_json::to_vec(order)?;
    line.push(b'\n');
    let stream = lock(listen);
    let mut out = &*stream;
    out.write_all(&line)?;
    out.flush()
}

fn seen_within(seen: &Receiver<()>, wait: Duration) -> Reached {
    if seen.recv_timeout(wait).is_ok() {
        Reached::Seen
    } else {
        Reached::Unseen
    }
}

/// Types the message into the target's input box, never pressing Enter.
pub fn prefill_into(
    core: &Core,
    to: TabId,
    body: &str,
    type_into: impl Fn(SessionId, &[u8]) -> io::Result<()>,
) -> Result<(), String> {
    let pty = core
        .state()
        .links
        .values()
        .find(|l| l.tab == to)
        .and_then(|l| l.pty)
        .ok_or("the target tab is closed")?;
    type_into(pty, prefill(body).as_bytes())
        .map_err(|e| format!("could not type into the target tab: {e}"))
}
