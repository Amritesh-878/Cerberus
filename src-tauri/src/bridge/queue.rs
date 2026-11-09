//! Messages waiting for the user's decision, in the order they arrived. Held in memory only.

use std::collections::VecDeque;

use serde::Serialize;

use crate::saved::TabId;

pub struct Pending<T> {
    pub id: u64,
    pub from: TabId,
    pub to: TabId,
    /// The sender's MCP request id, so its cancel finds this entry.
    pub call: String,
    pub text: String,
    /// Told the outcome once the user decides.
    pub waiter: T,
}

/// What the popup shows for one waiting message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingView {
    pub id: u64,
    pub from: TabId,
    pub to: TabId,
    pub text: String,
}

/// The helper run a call id came from: helpers send `<run>:<id>`.
pub fn run_of(call: &str) -> Option<&str> {
    call.split_once(':').map(|(run, _)| run)
}

pub struct Queue<T> {
    next_id: u64,
    items: VecDeque<Pending<T>>,
}

impl<T> Default for Queue<T> {
    fn default() -> Self {
        Self {
            next_id: 1,
            items: VecDeque::new(),
        }
    }
}

impl<T> Queue<T> {
    pub fn push(&mut self, from: TabId, to: TabId, call: String, text: String, waiter: T) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.items.push_back(Pending {
            id,
            from,
            to,
            call,
            text,
            waiter,
        });
        id
    }

    pub fn take(&mut self, id: u64) -> Option<Pending<T>> {
        let at = self.items.iter().position(|p| p.id == id)?;
        self.items.remove(at)
    }

    pub fn cancel(&mut self, from: TabId, call: &str) -> Option<Pending<T>> {
        let at = self
            .items
            .iter()
            .position(|p| p.from == from && p.call == call)?;
        self.items.remove(at)
    }

    /// Takes out every message from or to a tab that closed.
    pub fn drop_tab(&mut self, tab: TabId) -> Vec<Pending<T>> {
        let (gone, kept) = self
            .items
            .drain(..)
            .partition(|p| p.from == tab || p.to == tab);
        self.items = kept;
        gone.into()
    }

    /// Takes out every message a tab's earlier helper runs sent, leaving run `run`'s and those sent to it.
    pub fn drop_from(&mut self, tab: TabId, run: &str) -> Vec<Pending<T>> {
        let (gone, kept) = self
            .items
            .drain(..)
            .partition(|p| p.from == tab && run_of(&p.call) != Some(run));
        self.items = kept;
        gone.into()
    }

    /// Takes out every message helper run `run` of a tab sent, since nobody is left to hear the outcome.
    pub fn drop_run(&mut self, tab: TabId, run: &str) -> Vec<Pending<T>> {
        let (gone, kept) = self
            .items
            .drain(..)
            .partition(|p| p.from == tab && run_of(&p.call) == Some(run));
        self.items = kept;
        gone.into()
    }

    pub fn views(&self) -> Vec<PendingView> {
        self.items
            .iter()
            .map(|p| PendingView {
                id: p.id,
                from: p.from,
                to: p.to,
                text: p.text.clone(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(queue: &Queue<()>) -> Vec<u64> {
        queue.views().iter().map(|v| v.id).collect()
    }

    #[test]
    fn messages_wait_in_arrival_order() {
        let mut queue = Queue::default();
        let first = queue.push(1, 2, "a".to_owned(), "one".to_owned(), ());
        let second = queue.push(3, 2, "a".to_owned(), "two".to_owned(), ());
        let third = queue.push(1, 3, "b".to_owned(), "three".to_owned(), ());

        assert_eq!(ids(&queue), [first, second, third]);
        assert_eq!(queue.take(second).map(|p| p.text), Some("two".to_owned()));
        assert_eq!(ids(&queue), [first, third]);
        assert!(queue.take(second).is_none());
    }

    #[test]
    fn a_cancel_takes_only_that_senders_call() {
        let mut queue = Queue::default();
        let mine = queue.push(1, 2, "7".to_owned(), "x".to_owned(), ());
        let other = queue.push(3, 2, "7".to_owned(), "y".to_owned(), ());

        assert_eq!(queue.cancel(1, "7").map(|p| p.id), Some(mine));
        assert!(queue.cancel(1, "7").is_none());
        assert_eq!(ids(&queue), [other]);
    }

    #[test]
    fn a_closed_tab_takes_its_messages_both_ways() {
        let mut queue = Queue::default();
        queue.push(1, 2, "a".to_owned(), "from 1".to_owned(), ());
        let kept = queue.push(3, 4, "b".to_owned(), "elsewhere".to_owned(), ());
        queue.push(3, 1, "c".to_owned(), "to 1".to_owned(), ());

        let gone: Vec<String> = queue.drop_tab(1).into_iter().map(|p| p.text).collect();

        assert_eq!(gone, ["from 1", "to 1"]);
        assert_eq!(ids(&queue), [kept]);
    }

    #[test]
    fn a_restarted_sender_takes_only_its_own_messages() {
        let mut queue = Queue::default();
        queue.push(1, 2, "old:1".to_owned(), "from 1".to_owned(), ());
        let to = queue.push(2, 1, "b".to_owned(), "to 1".to_owned(), ());
        let new = queue.push(1, 2, "new:1".to_owned(), "new run".to_owned(), ());

        let gone: Vec<String> = queue
            .drop_from(1, "new")
            .into_iter()
            .map(|p| p.text)
            .collect();

        assert_eq!(gone, ["from 1"]);
        assert_eq!(ids(&queue), [to, new]);
    }

    #[test]
    fn an_ended_run_takes_only_its_own_messages() {
        let mut queue = Queue::default();
        queue.push(
            1,
            2,
            "dead:1".to_owned(),
            "from the dead run".to_owned(),
            (),
        );
        let live = queue.push(
            1,
            2,
            "live:1".to_owned(),
            "from the live run".to_owned(),
            (),
        );
        let other = queue.push(3, 2, "dead:2".to_owned(), "another tab".to_owned(), ());
        let to = queue.push(2, 1, "x:1".to_owned(), "to 1".to_owned(), ());

        let gone: Vec<String> = queue
            .drop_run(1, "dead")
            .into_iter()
            .map(|p| p.text)
            .collect();

        assert_eq!(gone, ["from the dead run"]);
        assert_eq!(ids(&queue), [live, other, to]);
    }

    #[test]
    fn ids_are_not_reused() {
        let mut queue = Queue::default();
        let first = queue.push(1, 2, "a".to_owned(), "x".to_owned(), ());
        queue.take(first);

        assert_ne!(queue.push(1, 2, "a".to_owned(), "x".to_owned(), ()), first);
    }
}
