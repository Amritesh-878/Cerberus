//! Each account's limits from the newest report of any of its tabs, and when to warn about them.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::Serialize;

use crate::accounts::{AccountId, KeptUsage, UsageWindow};
use crate::live::event::{Status, Window};
use crate::saved::TabId;

pub const DEFAULT_WARN_AT: u8 = 90;
// Reports of one 5-hour window may round its reset differently; the next window is hours away.
const SAME_WINDOW_S: u64 = 10 * 60;
// Replies before an account with no numbers counts as one that doesn't report them, not one still loading.
const SILENT_AFTER: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowView {
    pub used: f64,
    /// Unix seconds.
    pub resets_at: u64,
    /// The window has reset since this report, so its use starts again from nothing.
    pub reset: bool,
    /// Set for numbers kept from before this start: when they were reported, in Unix milliseconds.
    pub as_of: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Meter {
    pub account: AccountId,
    pub five_hour: Option<WindowView>,
    pub seven_day: Option<WindowView>,
    /// Replied enough times since the app started with no usage numbers in any status, so its plan doesn't report them.
    pub silent: bool,
}

/// One account's share of a window's bar.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    pub account: AccountId,
    /// 0 to 100; `None` when there is no reading for the window now running.
    pub used: Option<f64>,
    /// Unix seconds.
    pub resets_at: Option<u64>,
    /// Set for a reading kept from before this start: when it came, in Unix milliseconds.
    pub as_of: Option<u64>,
    /// For a segment with no reading now: when the last one came, in Unix milliseconds.
    pub last_reading: Option<u64>,
    pub silent: bool,
}

/// Every account's use of each window, one bar per window, the soonest reset first.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Pool {
    pub five_hour: Vec<Segment>,
    pub seven_day: Vec<Segment>,
}

#[derive(Debug, Clone, Copy, Default)]
struct Kept {
    five_hour: Option<(u64, Window)>,
    seven_day: Option<(u64, Window)>,
}

#[derive(Debug, Default)]
pub struct Meters {
    by_account: BTreeMap<AccountId, Kept>,
    /// Reports older than this came before the app started, so they show with their time.
    since_ms: u64,
    /// Replies finished since the app started, per account.
    replies: BTreeMap<AccountId, u32>,
    /// Accounts with a status carrying usage numbers since the app started.
    reporting: BTreeSet<AccountId>,
}

impl Meters {
    /// Starts from the numbers kept by the last run; a report from now on replaces each window.
    pub fn restored(kept: &[KeptUsage], since_ms: u64) -> Self {
        let window = |at: u64, w: Option<UsageWindow>| {
            w.map(|w| {
                (
                    at,
                    Window {
                        used: w.used,
                        resets_at: w.resets_at,
                    },
                )
            })
        };
        Self {
            by_account: kept
                .iter()
                .map(|k| {
                    (
                        k.account,
                        Kept {
                            five_hour: window(k.at, k.five_hour),
                            seven_day: window(k.at, k.seven_day),
                        },
                    )
                })
                .collect(),
            since_ms,
            ..Self::default()
        }
    }

    /// What the profiles file keeps: each account's newest windows and the newer report time.
    pub fn kept(&self) -> Vec<KeptUsage> {
        let saved = |w: Option<(u64, Window)>| {
            w.map(|(_, w)| UsageWindow {
                used: w.used,
                resets_at: w.resets_at,
            })
        };
        self.by_account
            .iter()
            .map(|(&account, kept)| KeptUsage {
                account,
                at: kept
                    .five_hour
                    .into_iter()
                    .chain(kept.seven_day)
                    .map(|(at, _)| at)
                    .max()
                    .unwrap_or(0),
                five_hour: saved(kept.five_hour),
                seven_day: saved(kept.seven_day),
            })
            .collect()
    }

    /// Each window keeps the newest report that had it, so a tab with no reply yet clears nothing.
    pub fn report(&mut self, account: AccountId, status: &Status) {
        if status.five_hour.is_some() || status.seven_day.is_some() {
            self.reporting.insert(account);
        }
        let kept = self.by_account.entry(account).or_default();
        newer(&mut kept.five_hour, status.at, status.five_hour);
        newer(&mut kept.seven_day, status.at, status.seven_day);
    }

    /// A reply on the account finished, which a plan that reports usage follows with its numbers.
    pub fn replied(&mut self, account: AccountId) {
        *self.replies.entry(account).or_default() += 1;
    }

    fn silent(&self, account: AccountId) -> bool {
        !self.reporting.contains(&account)
            && self.replies.get(&account).copied().unwrap_or(0) >= SILENT_AFTER
    }

    /// The 5-hour window still running at `now_s`, as Claude Code drops one once it resets.
    pub fn five_hour(&self, account: AccountId, now_s: u64) -> Option<Window> {
        live_window(self.by_account.get(&account)?.five_hour, now_s)
    }

    pub fn view(&self, now_s: u64) -> Vec<Meter> {
        let shown = |kept: Option<(u64, Window)>| {
            kept.map(|(at, w)| WindowView {
                used: w.used,
                resets_at: w.resets_at,
                reset: w.resets_at <= now_s,
                as_of: (at < self.since_ms).then_some(at),
            })
        };
        let accounts: BTreeSet<AccountId> = self
            .by_account
            .keys()
            .chain(self.replies.keys())
            .copied()
            .collect();
        accounts
            .into_iter()
            .map(|account| {
                let kept = self.by_account.get(&account).copied().unwrap_or_default();
                Meter {
                    account,
                    five_hour: shown(kept.five_hour),
                    seven_day: shown(kept.seven_day),
                    silent: self.silent(account),
                }
            })
            .collect()
    }
}

impl Meters {
    /// Each account in `accounts` gets a segment; readings come first by soonest reset, then the ones without one.
    pub fn pool(&self, accounts: &[AccountId], now_s: u64) -> Pool {
        let bar = |pick: fn(&Kept) -> Option<(u64, Window)>| {
            let mut segments: Vec<Segment> = accounts
                .iter()
                .map(|&account| {
                    let kept = self.by_account.get(&account).and_then(pick);
                    let running = kept.filter(|(_, w)| w.resets_at > now_s);
                    Segment {
                        account,
                        used: running.map(|(_, w)| w.used),
                        resets_at: running.map(|(_, w)| w.resets_at),
                        as_of: running.map(|(at, _)| at).filter(|&at| at < self.since_ms),
                        last_reading: kept.filter(|_| running.is_none()).map(|(at, _)| at),
                        silent: self.silent(account),
                    }
                })
                .collect();
            // Stable, so accounts with no reading keep the app's order.
            segments.sort_by_key(|s| (s.resets_at.is_none(), s.resets_at));
            segments
        };
        Pool {
            five_hour: bar(|k| k.five_hour),
            seven_day: bar(|k| k.seven_day),
        }
    }
}

fn newer(kept: &mut Option<(u64, Window)>, at: u64, found: Option<Window>) {
    if let Some(found) = found
        && kept.is_none_or(|(was, _)| at >= was)
    {
        *kept = Some((at, found));
    }
}

fn live_window(kept: Option<(u64, Window)>, now_s: u64) -> Option<Window> {
    kept.map(|(_, w)| w).filter(|w| w.resets_at > now_s)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Warning {
    pub account: AccountId,
    /// The tab whose report crossed the line; its Continue on is what the banner offers.
    pub tab: TabId,
    pub used: f64,
    pub resets_at: u64,
    /// Other accounts not known to be past the line, in the order given.
    pub offers: Vec<AccountId>,
}

/// One warning per account per 5-hour window, kept on show until dismissed or the window resets.
#[derive(Debug, Default)]
pub struct Warner {
    warned: HashMap<AccountId, u64>,
    open: BTreeMap<AccountId, (TabId, Window)>,
}

impl Warner {
    /// True the first time this account's window reaches `warn_at`; the window's reset time keys it.
    pub fn check(&mut self, account: AccountId, tab: TabId, window: Window, warn_at: u8) -> bool {
        if window.used < f64::from(warn_at) {
            return false;
        }
        if self
            .warned
            .get(&account)
            .is_some_and(|&warned| warned.abs_diff(window.resets_at) <= SAME_WINDOW_S)
        {
            // Same window: a banner still showing follows the newer use, a dismissed one stays gone.
            if let Some((_, shown)) = self.open.get_mut(&account) {
                shown.used = window.used;
            }
            return false;
        }
        self.warned.insert(account, window.resets_at);
        self.open.insert(account, (tab, window));
        true
    }

    pub fn dismiss(&mut self, account: AccountId) {
        self.open.remove(&account);
    }

    /// A closed tab's warning stays, since the account is still near its limit.
    pub fn view(
        &self,
        meters: &Meters,
        accounts: &[AccountId],
        warn_at: u8,
        now_s: u64,
    ) -> Vec<Warning> {
        self.open
            .iter()
            .filter(|(_, (_, w))| w.resets_at > now_s)
            .map(|(&account, &(tab, w))| Warning {
                account,
                tab,
                used: w.used,
                resets_at: w.resets_at,
                offers: offers(meters, accounts, account, warn_at, now_s),
            })
            .collect()
    }
}

/// Accounts other than `from` below the line, the most left in the 5-hour window first; one with nothing reported yet counts as below, after the known ones.
pub fn offers(
    meters: &Meters,
    accounts: &[AccountId],
    from: AccountId,
    warn_at: u8,
    now_s: u64,
) -> Vec<AccountId> {
    let mut below: Vec<(AccountId, Option<f64>)> = accounts
        .iter()
        .copied()
        .filter(|&a| a != from)
        .map(|a| (a, meters.five_hour(a, now_s).map(|w| w.used)))
        .filter(|(_, used)| used.is_none_or(|u| u < f64::from(warn_at)))
        .collect();
    // Stable, so equal readings and unknown accounts keep the app's order.
    below.sort_by(|(_, a), (_, b)| match (a, b) {
        (Some(a), Some(b)) => a.total_cmp(b),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });
    below.into_iter().map(|(a, _)| a).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_762_700_000;
    const RESET: u64 = NOW + 3_600;

    fn status(at: u64, five: Option<f64>, seven: Option<f64>) -> Status {
        Status {
            at,
            session: None,
            model: None,
            context: None,
            context_size: None,
            cache_expires_at: None,
            five_hour: five.map(|used| Window {
                used,
                resets_at: RESET,
            }),
            seven_day: seven.map(|used| Window {
                used,
                resets_at: NOW + 86_400,
            }),
        }
    }

    #[test]
    fn an_account_is_silent_only_after_three_replies_with_no_numbers_since_start() {
        let silent = |meters: &Meters| meters.view(NOW).iter().any(|m| m.account == 2 && m.silent);
        let mut meters = Meters::restored(
            &[KeptUsage {
                account: 2,
                at: 5,
                five_hour: Some(UsageWindow {
                    used: 40.0,
                    resets_at: RESET,
                }),
                seven_day: None,
            }],
            10,
        );
        meters.report(2, &status(20, None, None));
        meters.replied(2);
        meters.replied(2);
        assert!(!silent(&meters), "two replies may still be loading");

        meters.replied(2);
        assert!(
            silent(&meters),
            "numbers kept from an earlier run don't count"
        );

        meters.report(2, &status(30, Some(12.0), None));
        assert!(!silent(&meters), "one status with numbers and it reports");
    }

    #[test]
    fn the_newest_report_of_any_tab_wins_per_window() {
        let mut meters = Meters::default();
        meters.report(1, &status(10, Some(40.0), Some(20.0)));
        meters.report(1, &status(30, Some(55.0), None));
        // An older report that arrives late changes nothing.
        meters.report(1, &status(20, Some(45.0), Some(21.0)));

        let view = meters.view(NOW);

        assert_eq!(view.len(), 1);
        assert_eq!(view[0].five_hour.map(|w| w.used), Some(55.0));
        assert_eq!(view[0].seven_day.map(|w| w.used), Some(21.0));
        assert_eq!(view[0].five_hour.map(|w| w.resets_at), Some(RESET));
    }

    #[test]
    fn accounts_stay_apart_and_unreported_ones_are_absent() {
        let mut meters = Meters::default();
        meters.report(1, &status(10, Some(40.0), None));
        meters.report(2, &status(5, Some(90.0), None));
        meters.report(3, &status(5, None, None));

        let view = meters.view(NOW);

        let five: Vec<_> = view
            .iter()
            .map(|m| (m.account, m.five_hour.map(|w| w.used)))
            .collect();
        assert_eq!(five, [(1, Some(40.0)), (2, Some(90.0)), (3, None)]);
    }

    #[test]
    fn a_window_past_its_reset_shows_as_reset_and_no_longer_warns() {
        let mut meters = Meters::default();
        meters.report(1, &status(10, Some(99.0), None));

        assert_eq!(meters.view(RESET)[0].five_hour.map(|w| w.reset), Some(true));
        assert_eq!(
            meters.view(RESET - 1)[0].five_hour.map(|w| w.reset),
            Some(false)
        );
        assert_eq!(meters.five_hour(1, RESET), None);
        assert!(meters.five_hour(1, RESET - 1).is_some());
    }

    #[test]
    fn kept_numbers_show_with_their_time_until_a_new_report_replaces_them() {
        let mut meters = Meters::default();
        meters.report(1, &status(5_000, Some(40.0), Some(12.0)));
        let kept = meters.kept();
        assert_eq!(kept[0].at, 5_000);
        assert_eq!(kept[0].five_hour.map(|w| w.used), Some(40.0));

        let mut restarted = Meters::restored(&kept, 9_000);
        let first = restarted.view(NOW)[0].clone();
        assert_eq!(first.five_hour.and_then(|w| w.as_of), Some(5_000));
        assert_eq!(first.seven_day.and_then(|w| w.as_of), Some(5_000));

        restarted.report(1, &status(9_500, Some(55.0), None));
        let after = restarted.view(NOW)[0].clone();
        assert_eq!(
            after.five_hour.map(|w| (w.used, w.as_of)),
            Some((55.0, None))
        );
        // A window the new report left out keeps its old time.
        assert_eq!(after.seven_day.and_then(|w| w.as_of), Some(5_000));
        assert_eq!(restarted.kept()[0].at, 9_500);
    }

    #[test]
    fn the_pool_shows_every_account_soonest_reset_first_and_the_unread_ones_last() {
        let mut meters = Meters::default();
        let mut later = status(1, Some(30.0), None);
        later.five_hour = Some(Window {
            used: 30.0,
            resets_at: RESET + 600,
        });
        meters.report(1, &later);
        meters.report(2, &status(2, Some(70.0), Some(10.0)));
        meters.report(
            4,
            &Status {
                five_hour: Some(Window {
                    used: 50.0,
                    resets_at: NOW - 1,
                }),
                ..status(3, None, None)
            },
        );

        let pool = meters.pool(&[1, 2, 3, 4], NOW);

        let order: Vec<_> = pool.five_hour.iter().map(|s| (s.account, s.used)).collect();
        assert_eq!(
            order,
            [(2, Some(70.0)), (1, Some(30.0)), (3, None), (4, None)]
        );
        assert_eq!(
            pool.five_hour[3].last_reading,
            Some(3),
            "its window has reset since"
        );
        assert_eq!(pool.five_hour[2].last_reading, None, "never read");
        let week: Vec<_> = pool.seven_day.iter().map(|s| s.account).collect();
        assert_eq!(week, [2, 1, 3, 4]);
    }

    #[test]
    fn offers_put_the_most_left_first_and_the_unread_after() {
        let mut meters = Meters::default();
        meters.report(1, &status(1, Some(60.0), None));
        meters.report(2, &status(2, Some(20.0), None));
        meters.report(3, &status(3, Some(95.0), None));

        let order = offers(&meters, &[0, 1, 2, 3, 4, 5], 1, 90, NOW);

        assert_eq!(
            order,
            [2, 0, 4, 5],
            "3 is past the line, 1 is the one moving away"
        );
        assert_eq!(offers(&meters, &[1, 2, 3], 2, 90, NOW), [1]);
    }

    #[test]
    fn the_warning_comes_once_per_reset_window() {
        let mut warner = Warner::default();
        let at = |used, resets_at| Window { used, resets_at };

        assert!(!warner.check(1, 7, at(84.9, RESET), 85));
        assert!(warner.check(1, 7, at(85.0, RESET), 85));
        assert!(!warner.check(1, 8, at(90.0, RESET), 85));
        warner.dismiss(1);
        assert!(!warner.check(1, 7, at(95.0, RESET), 85));
        // The next window warns again.
        assert!(warner.check(1, 7, at(86.0, RESET + 18_000), 85));
        // Another account has its own.
        assert!(warner.check(2, 9, at(86.0, RESET), 85));
    }

    #[test]
    fn a_reset_time_that_drifts_a_little_is_still_the_same_window() {
        let mut warner = Warner::default();
        let at = |used, resets_at| Window { used, resets_at };

        assert!(warner.check(1, 7, at(86.0, RESET), 85));
        assert!(!warner.check(1, 7, at(88.0, RESET + 90), 85));
        assert!(!warner.check(1, 7, at(91.0, RESET - SAME_WINDOW_S), 85));
        let shown = warner.view(&Meters::default(), &[1], 85, NOW);
        assert_eq!(shown[0].used, 91.0, "the banner follows the newer use");
        assert!(warner.check(1, 7, at(86.0, RESET + SAME_WINDOW_S + 1), 85));
    }

    #[test]
    fn a_dismissed_warning_stays_gone_while_use_climbs() {
        let mut warner = Warner::default();
        let at = |used| Window {
            used,
            resets_at: RESET,
        };
        warner.check(1, 7, at(86.0), 85);
        warner.dismiss(1);

        assert!(!warner.check(1, 7, at(97.0), 85));
        assert!(warner.view(&Meters::default(), &[1], 85, NOW).is_empty());
    }

    #[test]
    fn only_accounts_below_the_line_are_offered() {
        let mut meters = Meters::default();
        meters.report(1, &status(10, Some(88.0), None));
        meters.report(2, &status(10, Some(85.0), None));
        meters.report(3, &status(10, Some(20.0), None));
        let mut warner = Warner::default();
        warner.check(
            1,
            7,
            Window {
                used: 88.0,
                resets_at: RESET,
            },
            85,
        );

        let warnings = warner.view(&meters, &[1, 2, 3, 4], 85, NOW);

        assert_eq!(
            warnings,
            [Warning {
                account: 1,
                tab: 7,
                used: 88.0,
                resets_at: RESET,
                // 2 is at the line, 4 has not reported.
                offers: vec![3, 4],
            }]
        );
    }

    #[test]
    fn a_warning_leaves_once_dismissed_or_its_window_resets() {
        let meters = Meters::default();
        let mut warner = Warner::default();
        let w = Window {
            used: 90.0,
            resets_at: RESET,
        };
        warner.check(1, 7, w, 85);
        warner.check(2, 8, w, 85);

        assert_eq!(warner.view(&meters, &[1, 2], 85, RESET).len(), 0);
        warner.dismiss(1);
        let left = warner.view(&meters, &[1, 2], 85, NOW);
        assert_eq!(left.iter().map(|w| w.account).collect::<Vec<_>>(), [2]);
    }
}
