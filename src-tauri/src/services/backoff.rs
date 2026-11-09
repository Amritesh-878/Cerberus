//! When a service that stopped by itself starts again, and when it has failed too often to try.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

const FIRST_WAIT: Duration = Duration::from_secs(1);
const LONGEST_WAIT: Duration = Duration::from_secs(30);
/// This many exits inside `CRASH_WINDOW` mark the service crashed.
const CRASH_EXITS: usize = 5;
const CRASH_WINDOW: Duration = Duration::from_secs(120);
/// A run this long was healthy, so the next wait starts again at one second.
const STEADY_RUN: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Next {
    RestartIn(Duration),
    Crashed,
}

/// The exits of one service; a Start or Restart by the user begins it again.
#[derive(Debug, Default)]
pub struct Backoff {
    exits: VecDeque<Instant>,
    streak: u32,
}

impl Backoff {
    /// Called for an exit Stop didn't cause, at `now`, after a run of `ran_for`.
    pub fn exited(&mut self, now: Instant, ran_for: Duration) -> Next {
        while self
            .exits
            .front()
            .is_some_and(|&at| now.saturating_duration_since(at) >= CRASH_WINDOW)
        {
            self.exits.pop_front();
        }
        self.exits.push_back(now);
        if self.exits.len() >= CRASH_EXITS {
            return Next::Crashed;
        }
        if ran_for >= STEADY_RUN {
            self.streak = 0;
        }
        let wait = FIRST_WAIT.saturating_mul(2u32.saturating_pow(self.streak));
        self.streak = self.streak.saturating_add(1);
        Next::RestartIn(wait.min(LONGEST_WAIT))
    }

    pub fn reset(&mut self) {
        self.exits.clear();
        self.streak = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secs(n: u64) -> Duration {
        Duration::from_secs(n)
    }

    #[test]
    fn quick_exits_wait_1_2_4_8_then_the_fifth_is_a_crash() {
        let start = Instant::now();
        let mut backoff = Backoff::default();

        let waits: Vec<Next> = [0, 2, 5, 10, 19]
            .into_iter()
            .map(|t| backoff.exited(start + secs(t), secs(1)))
            .collect();

        assert_eq!(
            waits,
            [
                Next::RestartIn(secs(1)),
                Next::RestartIn(secs(2)),
                Next::RestartIn(secs(4)),
                Next::RestartIn(secs(8)),
                Next::Crashed,
            ]
        );
    }

    #[test]
    fn exits_spread_past_two_minutes_keep_doubling_up_to_30_seconds_and_never_crash() {
        let start = Instant::now();
        let mut backoff = Backoff::default();

        // Each run lasts 40 s, so at most four exits ever sit inside two minutes.
        let waits: Vec<Next> = (0..8)
            .map(|i| backoff.exited(start + secs(i * 41), secs(40)))
            .collect();

        let secs_of = |n: &Next| match n {
            Next::RestartIn(d) => d.as_secs(),
            Next::Crashed => 0,
        };
        assert_eq!(
            waits.iter().map(secs_of).collect::<Vec<_>>(),
            [1, 2, 4, 8, 16, 30, 30, 30]
        );
    }

    #[test]
    fn a_steady_run_starts_the_wait_again_at_one_second() {
        let start = Instant::now();
        let mut backoff = Backoff::default();
        backoff.exited(start, secs(1));
        backoff.exited(start + secs(2), secs(1));

        let after_a_long_run = backoff.exited(start + secs(300), secs(290));

        assert_eq!(after_a_long_run, Next::RestartIn(secs(1)));
    }

    #[test]
    fn a_reset_starts_the_count_again() {
        let start = Instant::now();
        let mut backoff = Backoff::default();
        for t in 0..4 {
            backoff.exited(start + secs(t), secs(1));
        }

        backoff.reset();

        assert_eq!(
            backoff.exited(start + secs(5), secs(1)),
            Next::RestartIn(secs(1))
        );
    }
}
