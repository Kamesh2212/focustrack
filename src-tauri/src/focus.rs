//! Focus Mode (SRS 2.7): single sessions and Pomodoro cycles.
//!
//! The timer runs here in the backend (on its own thread), so it keeps going
//! with the window closed and survives page reloads. Each focus round and
//! break is a row in `focus_sessions`. While a focus round with
//! `block_distracting` is running, distracting websites go on the extension's
//! block list.

use crate::categories::{DISTRACTING, DOMAIN_MAP};
use crate::db;
use crate::limits::notify;
use crate::tracker::Shared;
use chrono::{Duration, Local, NaiveDateTime, Timelike};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FocusPlan {
    pub label: String,
    pub focus_mins: u32,
    /// 0 = no breaks
    pub break_mins: u32,
    /// Every 4th break; 0 = same as `break_mins`
    pub long_break_mins: u32,
    pub rounds: u32,
    pub block_distracting: bool,
}

impl FocusPlan {
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=240).contains(&self.focus_mins) {
            return Err("Focus time must be between 1 and 240 minutes".into());
        }
        if self.break_mins > 60 || self.long_break_mins > 90 {
            return Err("Breaks can be at most 60 minutes (90 for long breaks)".into());
        }
        if !(1..=12).contains(&self.rounds) {
            return Err("Rounds must be between 1 and 12".into());
        }
        if self.label.trim().is_empty() || self.label.len() > 80 {
            return Err("Give the session a short name".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Focus,
    Break,
}

#[derive(Debug, Clone)]
pub struct ActiveFocus {
    pub plan: FocusPlan,
    pub phase: Phase,
    pub round: u32,
    pub session_id: i64,
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FocusStatus {
    pub label: String,
    pub phase: Phase,
    pub round: u32,
    pub rounds: u32,
    pub started_at: String,
    pub ends_at: String,
    pub remaining_secs: i64,
    pub total_secs: i64,
    pub block_distracting: bool,
    pub plan: FocusPlan,
}

impl ActiveFocus {
    pub fn status(&self, now: NaiveDateTime) -> FocusStatus {
        FocusStatus {
            label: self.plan.label.clone(),
            phase: self.phase,
            round: self.round,
            rounds: self.plan.rounds,
            started_at: db::fmt_time(self.start),
            ends_at: db::fmt_time(self.end),
            remaining_secs: (self.end - now).num_seconds().max(0),
            total_secs: (self.end - self.start).num_seconds(),
            block_distracting: self.plan.block_distracting,
            plan: self.plan.clone(),
        }
    }
}

/// What happens when a phase ends.
#[derive(Debug, Clone, PartialEq)]
pub enum Next {
    Break { round: u32, mins: u32 },
    Focus { round: u32 },
    Done,
}

/// Pomodoro sequencing: focus → break → focus … with a long break every 4th round.
pub fn next_phase(plan: &FocusPlan, phase: Phase, round: u32) -> Next {
    match phase {
        Phase::Focus if round >= plan.rounds => Next::Done,
        Phase::Focus if plan.break_mins == 0 => Next::Focus { round: round + 1 },
        Phase::Focus => {
            let long = plan.long_break_mins > 0 && round % 4 == 0;
            Next::Break { round, mins: if long { plan.long_break_mins } else { plan.break_mins } }
        }
        Phase::Break => Next::Focus { round: round + 1 },
    }
}

fn now() -> NaiveDateTime {
    let t = Local::now().naive_local();
    t.with_nanosecond(0).unwrap_or(t)
}

fn fmt_mins(m: u32) -> String {
    if m >= 60 && m % 60 == 0 {
        format!("{}-hour", m / 60)
    } else {
        format!("{m}-minute")
    }
}

fn insert_row(conn: &Connection, plan: &FocusPlan, phase: Phase, secs: i64, start: NaiveDateTime) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO focus_sessions (label, kind, planned_secs, day, start_time, block_distracting)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            plan.label.trim(),
            if phase == Phase::Focus { "focus" } else { "break" },
            secs,
            start.format(db::DAY_FMT).to_string(),
            db::fmt_time(start),
            plan.block_distracting && phase == Phase::Focus,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

fn close_row(conn: &Connection, id: i64, start: NaiveDateTime, end: NaiveDateTime, completed: bool) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE focus_sessions SET end_time = ?1, actual_secs = ?2, completed = ?3 WHERE id = ?4",
        params![db::fmt_time(end), (end - start).num_seconds().max(0), completed, id],
    )?;
    Ok(())
}

/// Domains in a distracting category: the built-in list, adjusted by the
/// user's own website categories.
pub fn distracting_domains(conn: &Connection) -> rusqlite::Result<Vec<String>> {
    let mut set: BTreeSet<String> = DOMAIN_MAP
        .iter()
        .filter(|(_, c)| DISTRACTING.contains(c))
        .map(|(d, _)| d.to_string())
        .collect();
    let mut stmt = conn.prepare(
        "SELECT w.domain, COALESCE(c.kind, 'neutral') FROM websites w LEFT JOIN categories c ON c.id = w.category_id",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    for row in rows {
        let (domain, kind) = row?;
        if kind == "distracting" {
            set.insert(domain);
        } else {
            set.remove(&domain);
        }
    }
    Ok(set.into_iter().collect())
}

fn set_focus_block(conn: &Connection, shared: &Shared, on: bool) {
    let list = if on { distracting_domains(conn).unwrap_or_default() } else { Vec::new() };
    if let Ok(mut b) = shared.focus_blocklist.lock() {
        *b = list;
    }
}

fn begin(conn: &Connection, shared: &Shared, plan: FocusPlan, phase: Phase, round: u32, mins: u32, at: NaiveDateTime) -> rusqlite::Result<ActiveFocus> {
    let secs = mins as i64 * 60;
    let id = insert_row(conn, &plan, phase, secs, at)?;
    set_focus_block(conn, shared, plan.block_distracting && phase == Phase::Focus);
    Ok(ActiveFocus { plan, phase, round, session_id: id, start: at, end: at + Duration::seconds(secs) })
}

/// Starts a new plan, ending any session in progress.
pub fn start(conn: &Connection, shared: &Shared, plan: FocusPlan) -> Result<FocusStatus, String> {
    plan.validate()?;
    let at = now();
    stop(conn, shared).map_err(|e| e.to_string())?;
    let mins = plan.focus_mins;
    let active = begin(conn, shared, plan, Phase::Focus, 1, mins, at).map_err(|e| e.to_string())?;
    let status = active.status(at);
    if let Ok(mut f) = shared.focus.lock() {
        *f = Some(active);
    }
    Ok(status)
}

/// Ends the current session early (recorded as not completed).
pub fn stop(conn: &Connection, shared: &Shared) -> rusqlite::Result<()> {
    let at = now();
    let current = shared.focus.lock().ok().and_then(|mut f| f.take());
    if let Some(a) = current {
        close_row(conn, a.session_id, a.start, at.min(a.end), false)?;
    }
    set_focus_block(conn, shared, false);
    Ok(())
}

pub fn status(shared: &Shared) -> Option<FocusStatus> {
    let at = now();
    shared.focus.lock().ok().and_then(|f| f.as_ref().map(|a| a.status(at)))
}

/// Advances the timer; returns the notification to show, if any.
pub fn tick(conn: &Connection, shared: &Shared, at: NaiveDateTime, save_progress: bool) -> rusqlite::Result<Option<(String, String)>> {
    let mut guard = match shared.focus.lock() {
        Ok(g) => g,
        Err(_) => return Ok(None),
    };
    let Some(active) = guard.as_ref() else { return Ok(None) };

    if at < active.end {
        if save_progress {
            conn.execute(
                "UPDATE focus_sessions SET actual_secs = ?1 WHERE id = ?2",
                params![(at - active.start).num_seconds(), active.session_id],
            )?;
        }
        return Ok(None);
    }

    let a = guard.take().expect("checked above");
    close_row(conn, a.session_id, a.start, a.end, true)?;
    let rounds = a.plan.rounds;
    let note = match next_phase(&a.plan, a.phase, a.round) {
        Next::Done => {
            set_focus_block(conn, shared, false);
            let body = if rounds > 1 {
                format!("All {rounds} rounds of \"{}\" are done. Nice work!", a.plan.label)
            } else {
                format!("You completed a {} focus session: {}.", fmt_mins(a.plan.focus_mins), a.plan.label)
            };
            ("Focus session complete".to_string(), body)
        }
        Next::Break { round, mins } => {
            *guard = Some(begin(conn, shared, a.plan.clone(), Phase::Break, round, mins, a.end)?);
            (
                format!("Round {round} of {rounds} done"),
                format!("Time for a {} break.", fmt_mins(mins)),
            )
        }
        Next::Focus { round } => {
            let mins = a.plan.focus_mins;
            *guard = Some(begin(conn, shared, a.plan.clone(), Phase::Focus, round, mins, a.end)?);
            (
                "Back to focus".to_string(),
                format!("Round {round} of {rounds} of \"{}\" has started.", a.plan.label),
            )
        }
    };
    Ok(Some(note))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FocusRecord {
    pub id: i64,
    pub label: String,
    pub day: String,
    pub start: String,
    pub end: Option<String>,
    pub planned_secs: i64,
    pub actual_secs: i64,
    pub completed: bool,
}

pub fn history(conn: &Connection, limit: i64) -> rusqlite::Result<Vec<FocusRecord>> {
    let mut stmt = conn.prepare(
        "SELECT id, label, day, start_time, end_time, planned_secs, actual_secs, completed
         FROM focus_sessions WHERE kind = 'focus' AND end_time IS NOT NULL
         ORDER BY start_time DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit], |r| {
        Ok(FocusRecord {
            id: r.get(0)?,
            label: r.get(1)?,
            day: r.get(2)?,
            start: r.get(3)?,
            end: r.get(4)?,
            planned_secs: r.get(5)?,
            actual_secs: r.get(6)?,
            completed: r.get::<_, i64>(7)? != 0,
        })
    })?;
    let out = rows.collect();
    out
}

/// (focused seconds, completed focus sessions) for days in range.
pub fn totals(conn: &Connection, from: &str, to: &str) -> rusqlite::Result<(i64, i64)> {
    conn.query_row(
        "SELECT COALESCE(SUM(actual_secs), 0), COALESCE(SUM(completed), 0)
         FROM focus_sessions WHERE kind = 'focus' AND day BETWEEN ?1 AND ?2",
        params![from, to],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
}

/// Consecutive days, ending today (or yesterday), with a completed focus session.
pub fn streak(conn: &Connection, today: chrono::NaiveDate) -> rusqlite::Result<i64> {
    let mut count = 0;
    let mut day = today;
    // Today without a session yet doesn't break the streak.
    let has = |d: chrono::NaiveDate| -> rusqlite::Result<bool> {
        conn.query_row(
            "SELECT 1 FROM focus_sessions WHERE kind = 'focus' AND completed = 1 AND day = ?1 LIMIT 1",
            params![d.format(db::DAY_FMT).to_string()],
            |_| Ok(()),
        )
        .optional()
        .map(|o| o.is_some())
    };
    if !has(day)? {
        day -= Duration::days(1);
    }
    while has(day)? {
        count += 1;
        day -= Duration::days(1);
    }
    Ok(count)
}

pub fn spawn(app: tauri::AppHandle, db_path: PathBuf, shared: Arc<Shared>) {
    thread::Builder::new()
        .name("focustrack-focus".into())
        .spawn(move || {
            let conn = match db::open(&db_path) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("[focus] cannot open database: {e}");
                    return;
                }
            };
            let mut ticks: u64 = 0;
            loop {
                if shared.shutdown.load(Ordering::SeqCst) {
                    break;
                }
                ticks += 1;
                match tick(&conn, &shared, now(), ticks % 15 == 0) {
                    Ok(Some((title, body))) => {
                        if shared.notifications_enabled.load(Ordering::SeqCst) {
                            notify(&app, &title, &body);
                        }
                    }
                    Ok(None) => {}
                    Err(e) => eprintln!("[focus] tick failed: {e}"),
                }
                thread::sleep(std::time::Duration::from_secs(1));
            }
        })
        .expect("failed to spawn focus thread");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(rounds: u32, brk: u32, long: u32) -> FocusPlan {
        FocusPlan {
            label: "Study".into(),
            focus_mins: 25,
            break_mins: brk,
            long_break_mins: long,
            rounds,
            block_distracting: true,
        }
    }

    #[test]
    fn pomodoro_sequence() {
        let p = plan(4, 5, 15);
        assert_eq!(next_phase(&p, Phase::Focus, 1), Next::Break { round: 1, mins: 5 });
        assert_eq!(next_phase(&p, Phase::Break, 1), Next::Focus { round: 2 });
        assert_eq!(next_phase(&p, Phase::Focus, 4), Next::Done);
        let p8 = plan(8, 5, 15);
        assert_eq!(next_phase(&p8, Phase::Focus, 4), Next::Break { round: 4, mins: 15 });
        assert_eq!(next_phase(&plan(1, 0, 0), Phase::Focus, 1), Next::Done);
        assert_eq!(next_phase(&plan(3, 0, 0), Phase::Focus, 1), Next::Focus { round: 2 });
    }

    #[test]
    fn validation() {
        assert!(plan(4, 5, 15).validate().is_ok());
        assert!(FocusPlan { focus_mins: 0, ..plan(1, 0, 0) }.validate().is_err());
        assert!(FocusPlan { rounds: 0, ..plan(1, 0, 0) }.validate().is_err());
        assert!(FocusPlan { label: " ".into(), ..plan(1, 0, 0) }.validate().is_err());
    }

    #[test]
    fn runs_rounds_and_blocks_only_during_focus() {
        let conn = db::open_in_memory().unwrap();
        let shared = Shared::new(false, 300);
        // A user who re-categorised YouTube as Education doesn't get it blocked.
        let yt = db::get_or_create_website(&conn, "youtube.com", now()).unwrap();
        conn.execute("UPDATE websites SET category_id = (SELECT id FROM categories WHERE name = 'Education') WHERE id = ?1", params![yt]).unwrap();

        let st = start(&conn, &shared, FocusPlan { focus_mins: 1, break_mins: 1, ..plan(2, 1, 0) }).unwrap();
        assert_eq!((st.phase, st.round), (Phase::Focus, 1));
        let blocked = shared.focus_blocklist.lock().unwrap().clone();
        assert!(blocked.contains(&"instagram.com".to_string()));
        assert!(!blocked.contains(&"youtube.com".to_string()));

        let t0 = shared.focus.lock().unwrap().as_ref().unwrap().start;
        assert!(tick(&conn, &shared, t0 + Duration::seconds(30), true).unwrap().is_none());
        let n = tick(&conn, &shared, t0 + Duration::seconds(60), false).unwrap().unwrap();
        assert_eq!(n.0, "Round 1 of 2 done");
        assert_eq!(status(&shared).unwrap().phase, Phase::Break);
        assert!(shared.focus_blocklist.lock().unwrap().is_empty(), "no blocking during breaks");

        tick(&conn, &shared, t0 + Duration::seconds(120), false).unwrap().unwrap();
        assert_eq!(status(&shared).unwrap().round, 2);
        let done = tick(&conn, &shared, t0 + Duration::seconds(180), false).unwrap().unwrap();
        assert_eq!(done.0, "Focus session complete");
        assert!(status(&shared).is_none());

        let day = t0.format(db::DAY_FMT).to_string();
        assert_eq!(totals(&conn, &day, &day).unwrap(), (120, 2));
        assert_eq!(history(&conn, 10).unwrap().len(), 2);
    }

    #[test]
    fn stopping_early_is_not_completed() {
        let conn = db::open_in_memory().unwrap();
        let shared = Shared::new(false, 300);
        start(&conn, &shared, plan(1, 0, 0)).unwrap();
        stop(&conn, &shared).unwrap();
        assert!(status(&shared).is_none());
        let h = history(&conn, 10).unwrap();
        assert_eq!(h.len(), 1);
        assert!(!h[0].completed);
    }

    #[test]
    fn streak_counts_consecutive_days() {
        let conn = db::open_in_memory().unwrap();
        let today = chrono::NaiveDate::from_ymd_opt(2026, 11, 18).unwrap();
        for d in [16, 17] {
            conn.execute(
                "INSERT INTO focus_sessions (label, kind, planned_secs, day, start_time, end_time, actual_secs, completed)
                 VALUES ('x', 'focus', 60, ?1, ?1 || ' 09:00:00', ?1 || ' 09:01:00', 60, 1)",
                params![format!("2026-11-{d}")],
            )
            .unwrap();
        }
        assert_eq!(streak(&conn, today).unwrap(), 2, "today not done yet keeps the streak");
    }
}
