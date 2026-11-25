//! Calendar days at the user's offset from UTC, which the webview supplies.

const DAY_MS: i64 = 86_400_000;

/// The day number since 1970-01-01 at `offset_minutes` east of UTC.
pub fn day_of(at_ms: u64, offset_minutes: i32) -> i64 {
    let local = i64::try_from(at_ms).unwrap_or(i64::MAX) + i64::from(offset_minutes) * 60_000;
    local.div_euclid(DAY_MS)
}

/// When a local day starts, in Unix milliseconds; before the epoch reads as the epoch.
pub fn start_ms(day: i64, offset_minutes: i32) -> u64 {
    u64::try_from(day * DAY_MS - i64::from(offset_minutes) * 60_000).unwrap_or(0)
}

/// `YYYY-MM-DD` for a day number (Howard Hinnant's civil-from-days), to check day numbers against the calendar.
#[cfg(test)]
pub fn date_of(day: i64) -> String {
    let z = day + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_follow_the_offset_and_the_calendar() {
        // 2026-11-25T20:00:00Z is already the 26th in India, still the 25th in New York.
        let at = 1_795_636_800_000;

        assert_eq!(date_of(day_of(at, 0)), "2026-11-25");
        assert_eq!(date_of(day_of(at, 330)), "2026-11-26");
        assert_eq!(date_of(day_of(at, -300)), "2026-11-25");
        assert_eq!(date_of(0), "1970-01-01");
        assert_eq!(date_of(day_of(951_782_400_000, 0)), "2000-02-29");
        let india = day_of(at, 330);
        assert_eq!(
            start_ms(india, 330),
            1_795_631_400_000,
            "midnight in India is 18:30 UTC"
        );
    }
}
