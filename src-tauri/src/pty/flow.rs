use std::sync::{Condvar, Mutex, PoisonError};

use super::lock;

/// Output credit: sending stops once `high` bytes await the webview's ack, and resumes at `low`.
pub struct FlowGate {
    high: usize,
    low: usize,
    state: Mutex<State>,
    resumed: Condvar,
}

#[derive(Default)]
struct State {
    unacked: usize,
    closed: bool,
}

impl FlowGate {
    pub fn new(high: usize, low: usize) -> Self {
        Self {
            high,
            low,
            state: Mutex::default(),
            resumed: Condvar::new(),
        }
    }

    /// Blocks while the webview is too far behind, then counts `bytes` as sent.
    pub fn reserve(&self, bytes: usize) {
        let mut state = lock(&self.state);
        if state.unacked >= self.high {
            state = self
                .resumed
                .wait_while(state, |s| !s.closed && s.unacked > self.low)
                .unwrap_or_else(PoisonError::into_inner);
        }
        state.unacked += bytes;
    }

    pub fn ack(&self, bytes: usize) {
        let mut state = lock(&self.state);
        state.unacked = state.unacked.saturating_sub(bytes);
        if state.unacked <= self.low {
            self.resumed.notify_all();
        }
    }

    /// Lets every send through from now on, so a closing session never waits on a gone webview.
    pub fn close(&self) {
        lock(&self.state).closed = true;
        self.resumed.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    use super::*;

    const SHORT: Duration = Duration::from_millis(100);
    const LONG: Duration = Duration::from_secs(5);

    /// Reserves `bytes` on another thread and returns a receiver that fires once it gets through.
    fn reserve_async(gate: &Arc<FlowGate>, bytes: usize) -> mpsc::Receiver<()> {
        let (done, rx) = mpsc::channel();
        let gate = Arc::clone(gate);
        thread::spawn(move || {
            gate.reserve(bytes);
            let _ = done.send(());
        });
        rx
    }

    #[test]
    fn sends_below_the_high_mark_do_not_wait() {
        let gate = Arc::new(FlowGate::new(100, 20));

        gate.reserve(60);
        gate.reserve(39);

        assert!(reserve_async(&gate, 1).recv_timeout(LONG).is_ok());
    }

    #[test]
    fn a_send_past_the_high_mark_waits_for_acks_down_to_the_low_mark() {
        let gate = Arc::new(FlowGate::new(100, 20));
        gate.reserve(100);

        let blocked = reserve_async(&gate, 10);
        assert!(blocked.recv_timeout(SHORT).is_err());
        gate.ack(50);
        assert!(blocked.recv_timeout(SHORT).is_err());
        gate.ack(30);

        assert!(blocked.recv_timeout(LONG).is_ok());
    }

    #[test]
    fn closing_releases_a_waiting_send() {
        let gate = Arc::new(FlowGate::new(100, 20));
        gate.reserve(100);

        let blocked = reserve_async(&gate, 10);
        assert!(blocked.recv_timeout(SHORT).is_err());
        gate.close();

        assert!(blocked.recv_timeout(LONG).is_ok());
        assert!(reserve_async(&gate, 1_000).recv_timeout(LONG).is_ok());
    }

    #[test]
    fn an_ack_for_more_than_was_sent_does_not_underflow() {
        let gate = FlowGate::new(100, 20);

        gate.ack(500);
        gate.reserve(99);

        assert_eq!(lock(&gate.state).unacked, 99);
    }
}
