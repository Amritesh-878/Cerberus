//! `wait_for_tab` and `read_tab`: a chat looks at another account's tab, and nothing is typed or sent.

use std::thread;
use std::time::{Duration, Instant};

use crate::bridge::wire::{MOST_READ, MOST_WAIT_S};
use crate::bridge::{Core, State};
use crate::saved::TabId;

const POLL: Duration = Duration::from_millis(if cfg!(test) { 20 } else { 250 });
/// A message written to an inbox starts its turn within this, so until then an idle tab has not begun.
const START_GRACE: Duration = Duration::from_secs(if cfg!(test) { 2 } else { 15 });

/// A tab `list_tabs` would show the caller: linked, and on another account.
fn reachable(state: &State, token: &str, to: &str) -> Result<TabId, String> {
    let unknown =
        || format!("there is no tab \"{to}\" to reach. list_tabs shows the tabs you can reach.");
    let to = to.trim().parse::<TabId>().map_err(|_| unknown())?;
    let home = &state.links.get(token).ok_or_else(unknown)?.home;
    match state.by_tab(to) {
        Some(target) if &target.home != home => Ok(to),
        Some(_) => Err(format!(
            "tab \"{to}\" is on this chat's own account; list_tabs shows the tabs you can reach."
        )),
        None => Err(unknown()),
    }
}

/// Idle at its prompt, and not about to start the turn a delivered message begins.
fn settled(state: &State, tab: TabId) -> Option<bool> {
    let link = state.by_tab(tab)?;
    let starting = link.delivered_at.is_some_and(|landed| {
        landed.elapsed() < START_GRACE && link.busy_at.is_none_or(|busy| busy < landed)
    });
    Some(!link.busy && !starting)
}

pub(super) fn wait(core: &Core, token: &str, to: &str, timeout_s: u64) -> (String, bool) {
    let timeout_s = timeout_s.clamp(1, MOST_WAIT_S);
    let until = Instant::now() + Duration::from_secs(timeout_s);
    let tab = match reachable(&core.state(), token, to) {
        Ok(tab) => tab,
        Err(reason) => return (reason, true),
    };
    let name = || format!("Tab \"{tab}\" ({})", core.hooks.label(tab).text());
    loop {
        {
            let state = core.state();
            if !state.links.contains_key(token) {
                return ("Stopped waiting: this chat's tab closed.".to_owned(), true);
            }
            match settled(&state, tab) {
                None => {
                    drop(state);
                    return (format!("{} closed while you waited.", name()), true);
                }
                Some(true) => {
                    drop(state);
                    return (
                        format!(
                            "{} is idle at its prompt. read_tab gives its last reply.",
                            name()
                        ),
                        false,
                    );
                }
                Some(false) => {}
            }
        }
        if Instant::now() >= until {
            return (
                format!("{} is still working after {timeout_s} s.", name()),
                false,
            );
        }
        thread::sleep(POLL);
    }
}

pub(super) fn read(core: &Core, token: &str, to: &str) -> (String, bool) {
    let (tab, reader, busy) = {
        let state = core.state();
        let tab = match reachable(&state, token, to) {
            Ok(tab) => tab,
            Err(reason) => return (reason, true),
        };
        let Some(reader) = state.links.get(token).map(|l| l.tab) else {
            return ("this chat's tab is not linked.".to_owned(), true);
        };
        (tab, reader, settled(&state, tab) == Some(false))
    };
    let name = format!("Tab \"{tab}\" ({})", core.hooks.label(tab).text());
    let Some(reply) = core.hooks.last_reply(tab) else {
        return (
            format!(
                "{name} has no finished reply yet. Live cards must be on for it to report one."
            ),
            false,
        );
    };
    core.hooks.read_by(tab, reader);
    let mut text = format!("{name}'s last finished reply");
    if busy {
        text.push_str(" (it is working on a newer one)");
    }
    let (kept, cut) = capped(&reply, MOST_READ);
    text.push_str(":\n");
    text.push_str(kept);
    if cut {
        text.push_str("\n[Cut at 64 KB.]");
    }
    (text, false)
}

/// At most `most` bytes, cut on a character boundary.
pub fn capped(text: &str, most: usize) -> (&str, bool) {
    if text.len() <= most {
        return (text, false);
    }
    let mut end = most;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (&text[..end], true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_reply_is_cut_on_a_character_boundary() {
        let text = "aé";

        assert_eq!(capped(text, 2), ("a", true));
        assert_eq!(capped(text, 3), ("aé", false));
    }
}
