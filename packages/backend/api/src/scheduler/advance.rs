use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use cron::Schedule;
use std::str::FromStr;

/// Return the next datetime strictly after `from` that matches `cron_expr` in `tz`.
pub fn next_after(cron_expr: &str, tz: &str, from: DateTime<Utc>) -> Result<DateTime<Utc>, String> {
    let schedule = Schedule::from_str(cron_expr).map_err(|e| format!("cron parse: {e}"))?;
    let zone: Tz = tz.parse().map_err(|_| format!("bad tz: {tz}"))?;
    let from_local = from.with_timezone(&zone);
    schedule
        .after(&from_local)
        .next()
        .map(|dt| dt.with_timezone(&Utc))
        .ok_or_else(|| "no future occurrence".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn advances_weekly_monday_9am_america_new_york() {
        // 2026-04-18 is a Saturday; next Monday is 2026-04-20.
        let now = Utc.with_ymd_and_hms(2026, 4, 18, 12, 0, 0).unwrap();
        // cron syntax: sec min hour dom mon dow year
        let next = next_after("0 0 9 * * MON *", "America/New_York", now).unwrap();
        // 9am ET on Monday 2026-04-20 == 13:00 UTC (EDT).
        assert_eq!(next, Utc.with_ymd_and_hms(2026, 4, 20, 13, 0, 0).unwrap());
    }

    #[test]
    fn bad_tz_returns_error() {
        let now = Utc::now();
        let err = next_after("0 0 9 * * MON *", "Not/A/Zone", now).unwrap_err();
        assert!(err.contains("bad tz"));
    }
}
