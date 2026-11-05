//! The bridge pipe: each helper connection carries one ask, and a `Listen` stays open for orders.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;

use crate::bridge::wire::{Ask, AskKind, MOST_LINE, MOST_MESSAGE, Order, Outcome, Reply, TabEntry};
use crate::bridge::{Core, Hello, link};
use crate::saved::TabId;

// Every tab holds one connection open and one per waiting message; more than this is not Cerberus.
const MOST_CONNECTIONS: usize = 256;

pub fn listen(core: &Arc<Core>) -> io::Result<()> {
    let mut listener = link::Listener::bind(&core.name)?;
    let core = Arc::clone(core);
    let open = Arc::new(AtomicUsize::new(0));
    thread::Builder::new().spawn(move || {
        loop {
            let stream = match listener.accept() {
                Ok(Ok(stream)) => stream,
                Ok(Err(e)) => {
                    eprintln!("bridge: a helper left early: {e}");
                    continue;
                }
                Err(e) => {
                    eprintln!("bridge: stopped listening: {e}");
                    return;
                }
            };
            if open.fetch_add(1, Ordering::AcqRel) >= MOST_CONNECTIONS {
                open.fetch_sub(1, Ordering::AcqRel);
                continue;
            }
            let core = Arc::clone(&core);
            let open = Arc::clone(&open);
            let started = thread::Builder::new().spawn(move || {
                if let Err(e) = answer(&core, stream) {
                    eprintln!("bridge: could not answer a helper: {e}");
                }
                open.fetch_sub(1, Ordering::AcqRel);
            });
            if started.is_err() {
                eprintln!("bridge: no thread for a helper");
            }
        }
    })?;
    Ok(())
}

fn answer(core: &Core, stream: link::Stream) -> io::Result<()> {
    let mut line = String::new();
    BufReader::new((&stream).take(MOST_LINE)).read_line(&mut line)?;
    let ask: Ask = serde_json::from_str(&line)?;
    let Some(tab) = core.state().links.get(&ask.token).map(|l| l.tab) else {
        // A wrong token learns nothing, not even that the pipe is Cerberus.
        return Ok(());
    };
    let reply = match ask.kind {
        AskKind::Listen {
            messaging,
            pipe,
            run,
        } => {
            return listening(core, &ask.token, stream, (messaging, pipe), &run);
        }
        AskKind::Facts {
            version,
            peer_protocol,
        } => {
            if let Some(link) = core.state().links.get_mut(&ask.token) {
                link.facts = Some((version, peer_protocol));
            }
            core.changed();
            Reply::Ok
        }
        AskKind::List => Reply::Tabs {
            tabs: reachable(core, &ask.token),
        },
        AskKind::Send { call, to, message } => Reply::Outcome {
            outcome: send(core, tab, &ask.token, call, &to, message),
        },
        AskKind::Cancel { call } => {
            let cancelled = {
                let mut state = core.state();
                let found = state.queue.cancel(tab, &call);
                if found.is_none() {
                    state.early_cancels.insert((tab, call));
                }
                found
            };
            if let Some(pending) = cancelled {
                let _ = pending.waiter.try_send(Outcome::Cancelled);
                core.changed();
            }
            Reply::Ok
        }
        AskKind::Delivered { id, written } => {
            let mut state = core.state();
            // Another tab's helper could guess an id; only the target's answer counts.
            if state.deliveries.get(&id).is_some_and(|(to, _)| *to == tab)
                && let Some((_, waiting)) = state.deliveries.remove(&id)
            {
                let _ = waiting.try_send(written);
            }
            Reply::Ok
        }
    };
    write_line(&stream, &reply)?;
    link::finish(&stream)
}

fn listening(
    core: &Core,
    token: &str,
    stream: link::Stream,
    (messaging, pipe): (bool, Option<String>),
    run: &str,
) -> io::Result<()> {
    write_line(&stream, &Order::Ready)?;
    let ended = {
        let mut state = core.state();
        let Some(link) = state.links.get_mut(token) else {
            return Ok(());
        };
        let again = link.hello.is_some();
        let tab = link.tab;
        link.hello = Some(Hello {
            messaging,
            pipe,
            listen: Arc::new(Mutex::new(stream)),
        });
        // A second hello is a restarted helper, whose earlier calls ended with it; its own may already be queued.
        if again {
            state.queue.drop_from(tab, run)
        } else {
            Vec::new()
        }
    };
    for pending in ended {
        let _ = pending.waiter.try_send(Outcome::Cancelled);
    }
    core.changed();
    Ok(())
}

fn write_line(stream: &link::Stream, value: &impl serde::Serialize) -> io::Result<()> {
    let mut line = serde_json::to_vec(value)?;
    line.push(b'\n');
    let mut out = stream;
    out.write_all(&line)?;
    out.flush()
}

/// The linked tabs on other accounts, labelled with no lock held.
fn reachable(core: &Core, token: &str) -> Vec<TabEntry> {
    let tabs: Vec<(TabId, bool)> = {
        let state = core.state();
        let Some(home) = state.links.get(token).map(|l| l.home.clone()) else {
            return Vec::new();
        };
        let mut tabs: Vec<_> = state
            .links
            .values()
            .filter(|l| l.hello.is_some() && l.home != home)
            .map(|l| (l.tab, l.busy))
            .collect();
        tabs.sort_unstable();
        tabs
    };
    tabs.into_iter()
        .map(|(tab, busy)| TabEntry {
            id: tab.to_string(),
            label: core.hooks.label(tab).text(),
            busy,
        })
        .collect()
}

/// Queues the message for the popup and waits, as long as it takes, for the user's decision.
fn send(core: &Core, from: TabId, token: &str, call: String, to: &str, message: String) -> Outcome {
    let refuse = |reason: String| Outcome::Failed { reason };
    if message.trim().is_empty() {
        return refuse("the message is empty.".to_owned());
    }
    if message.len() > MOST_MESSAGE {
        return refuse(format!(
            "the message is {} bytes and the most is {MOST_MESSAGE}. Shorten it or split it.",
            message.len()
        ));
    }
    let Ok(to) = to.trim().parse::<TabId>() else {
        return refuse(format!(
            "there is no tab \"{to}\". list_tabs shows the tabs you can reach."
        ));
    };
    let (tx, rx) = mpsc::sync_channel(1);
    {
        let mut state = core.state();
        if state.early_cancels.remove(&(from, call.clone())) {
            return Outcome::Cancelled;
        }
        let home = state.links.get(token).map(|l| l.home.clone());
        let Some(target) = state.by_tab(to) else {
            return refuse(format!(
                "tab \"{to}\" is not a Claude tab Cerberus can reach. list_tabs shows the tabs you can reach."
            ));
        };
        if home.as_deref() == Some(target.home.as_str()) {
            return refuse(format!(
                "tab \"{to}\" is on this chat's own account; use SendMessage for chats on the same account."
            ));
        }
        state.queue.push(from, to, call, message, tx);
    }
    core.changed();
    rx.recv().unwrap_or_else(|_| Outcome::Failed {
        reason: "Cerberus closed before the user decided.".to_owned(),
    })
}
