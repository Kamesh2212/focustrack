//! System clock changes (#11).
//!
//! Limits reset at midnight, so moving the clock forward a day used to give a
//! fresh set of limits. FocusTrack compares the wall clock with Windows' uptime
//! counter (which keeps running through sleep and ignores clock changes): when
//! they drift apart by more than [`JUMP_SECS`], the clock was changed by hand.
//! Until the real day is over, limits keep counting against the real day and
//! also include the usage recorded under the day the clock now shows. Every
//! change is logged.
//!
//! Small corrections (time sync), daylight saving and time-zone changes are not
//! clock changes: the first are under the threshold and the others don't move
//! UTC.

use crate::analytics::fmt_day;
use crate::db;
use chrono::{DateTime, Duration, NaiveDateTime, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// Clock moves larger than this (either way) count as a change.
pub const JUMP_SECS: i64 = 10 * 60;
const SHIFT_KEY: &str = "clock_shift";
const LOG_KEY: &str = "clock_changes";
const LOG_MAX: usize = 50;

/// Notices clock changes between two observations.
#[derive(Default)]
pub struct Watch {
    last: Option<(DateTime<Utc>, u64)>,
}

impl Watch {
    /// Returns how far the clock was moved since the last call, in seconds
    /// (negative = moved back), if it moved more than [`JUMP_SECS`].
    pub fn observe(&mut self, utc: DateTime<Utc>, uptime_ms: u64) -> Option<i64> {
        let (prev_utc, prev_ms) = self.last.replace((utc, uptime_ms))?;
        if uptime_ms < prev_ms {
            return None;
        }
        let real = ((uptime_ms - prev_ms) / 1000) as i64;
        let moved = (utc - prev_utc).num_seconds() - real;
        (moved.abs() > JUMP_SECS).then_some(moved)
    }
}

/// The clock is off from real time by `skew_secs` since `real_day`.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct Shift {
    real_day: String,
    skew_secs: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ClockChange {
    /// Local time just after the change.
    pub at: String,
    /// How far the clock moved (negative = back).
    pub secs: i64,
}

fn load(conn: &Connection) -> rusqlite::Result<Option<Shift>> {
    Ok(db::get_setting(conn, SHIFT_KEY)?
        .filter(|s| !s.is_empty())
        .and_then(|s| serde_json::from_str(&s).ok()))
}

fn save(conn: &Connection, shift: Option<&Shift>) -> rusqlite::Result<()> {
    let v = shift
        .and_then(|s| serde_json::to_string(s).ok())
        .unwrap_or_default();
    db::set_setting(conn, SHIFT_KEY, &v)
}

/// Records that the clock was moved by `secs`; `now` is local time after the move.
pub fn record(conn: &Connection, now: NaiveDateTime, secs: i64) -> rusqlite::Result<()> {
    let mut shift = load(conn)?.unwrap_or(Shift {
        real_day: fmt_day((now - Duration::seconds(secs)).date()),
        skew_secs: 0,
    });
    shift.skew_secs += secs;
    // Moved back to (about) the right time: nothing to correct.
    save(conn, (shift.skew_secs.abs() > JUMP_SECS).then_some(&shift))?;

    let mut log = changes(conn)?;
    log.push(ClockChange {
        at: db::fmt_time(now),
        secs,
    });
    let skip = log.len().saturating_sub(LOG_MAX);
    let v = serde_json::to_string(&log[skip..]).unwrap_or_default();
    db::set_setting(conn, LOG_KEY, &v)
}

/// Clock changes FocusTrack has noticed, oldest first.
pub fn changes(conn: &Connection) -> rusqlite::Result<Vec<ClockChange>> {
    Ok(db::get_setting(conn, LOG_KEY)?
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default())
}

/// The day limits count against at local time `now`, and the calendar day whose
/// usage also counts because the clock was changed today.
pub fn limit_days(
    conn: &Connection,
    now: NaiveDateTime,
) -> rusqlite::Result<(String, Option<String>)> {
    let today = fmt_day(now.date());
    let Some(shift) = load(conn)? else {
        return Ok((today, None));
    };
    let real_now = now - Duration::seconds(shift.skew_secs);
    if fmt_day(real_now.date()) != shift.real_day {
        // The real day is over: back to normal.
        save(conn, None)?;
        return Ok((today, None));
    }
    let extra = (today != shift.real_day).then_some(today);
    Ok((shift.real_day, extra))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveDate, TimeZone};

    fn local(d: u32, h: u32, m: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, d)
            .unwrap()
            .and_hms_opt(h, m, 0)
            .unwrap()
    }

    fn utc(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(1_800_000_000 + secs, 0).unwrap()
    }

    #[test]
    fn watch_tells_clock_changes_from_sleep() {
        let mut w = Watch::default();
        assert_eq!(w.observe(utc(0), 1_000), None);
        // Normal 5 s tick.
        assert_eq!(w.observe(utc(5), 6_000), None);
        // Asleep for 2 hours: wall clock and uptime both move.
        assert_eq!(w.observe(utc(7_205), 7_206_000), None);
        // A time sync nudging the clock 30 s is ignored.
        assert_eq!(w.observe(utc(7_240), 7_211_000), None);
        // Clock moved forward a day.
        assert_eq!(w.observe(utc(7_245 + 86_400), 7_216_000), Some(86_400));
        // And back again.
        assert_eq!(w.observe(utc(7_250), 7_221_000), Some(-86_400));
    }

    #[test]
    fn moving_the_clock_forward_keeps_counting_today() {
        let conn = db::open_in_memory().unwrap();
        assert_eq!(
            limit_days(&conn, local(18, 10, 0)).unwrap(),
            ("2026-11-18".into(), None)
        );

        // Mon 18th 10:00 → clock set to Tue 19th 10:00.
        record(&conn, local(19, 10, 0), 86_400).unwrap();
        assert_eq!(
            limit_days(&conn, local(19, 10, 5)).unwrap(),
            ("2026-11-18".into(), Some("2026-11-19".into()))
        );
        // Still the real 18th at 23:59 (clock shows the 19th 23:59).
        assert_eq!(
            limit_days(&conn, local(19, 23, 59)).unwrap().0,
            "2026-11-18"
        );
        // Real midnight: limits reset as usual.
        assert_eq!(
            limit_days(&conn, local(20, 0, 1)).unwrap(),
            ("2026-11-20".into(), None)
        );
        assert_eq!(changes(&conn).unwrap().len(), 1);
    }

    #[test]
    fn moving_the_clock_back_again_clears_the_shift() {
        let conn = db::open_in_memory().unwrap();
        record(&conn, local(19, 10, 0), 86_400).unwrap();
        record(&conn, local(18, 10, 1), -86_400).unwrap();
        assert_eq!(
            limit_days(&conn, local(18, 10, 2)).unwrap(),
            ("2026-11-18".into(), None)
        );
        assert_eq!(changes(&conn).unwrap().len(), 2);
    }

    #[test]
    fn moving_the_clock_back_a_day_still_counts_today() {
        let conn = db::open_in_memory().unwrap();
        record(&conn, local(17, 10, 0), -86_400).unwrap();
        assert_eq!(
            limit_days(&conn, local(17, 12, 0)).unwrap(),
            ("2026-11-18".into(), Some("2026-11-17".into()))
        );
    }
}
