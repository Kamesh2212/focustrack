//! Focus Mode and Study Mode (SRS 2.7; improvements doc §5, §7).
//!
//! The timer runs in the backend (on its own thread), so it keeps going with
//! the window closed. Each focus round and break is a row in
//! `focus_sessions`. A session can use a blocking profile: its apps and
//! websites stay blocked for the whole session (breaks included, unless the
//! profile says otherwise) and open again automatically when it finishes.
//! Ending early follows the profile's rule: allowed, confirm with a phrase,
//! or strict (request, wait a minute, then confirm).

use crate::blocker::AppBlocks;
use crate::db;
use crate::installed;
use crate::limits::notify;
use crate::profiles::{self, AppRef, BlockPreview, Profile};
use crate::tracker::Shared;
use chrono::{Duration, Local, NaiveDateTime, Timelike};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread;

pub const STOP_PHRASE: &str = "I want to stop";
pub const STRICT_WAIT_SECS: i64 = 60;

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
    /// Blocking profile; None = nothing blocked.
    #[serde(default)]
    pub profile_id: Option<i64>,
    /// `focus` | `study`
    #[serde(default = "default_mode")]
    pub mode: String,
    #[serde(default)]
    pub subject: Option<String>,
    /// Free-text goal ("Finish the assignment") or a linked goal.
    #[serde(default)]
    pub goal_text: Option<String>,
    #[serde(default)]
    pub goal_id: Option<i64>,
}

fn default_mode() -> String {
    "focus".into()
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
        if self.mode != "focus" && self.mode != "study" {
            return Err("Unknown session mode".into());
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
    pub profile: Option<Profile>,
    pub preview: Option<BlockPreview>,
    pub phase: Phase,
    pub round: u32,
    pub session_id: i64,
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
    pub stop_requested: Option<NaiveDateTime>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FocusStatus {
    pub label: String,
    pub mode: String,
    pub subject: Option<String>,
    pub goal_text: Option<String>,
    pub phase: Phase,
    pub round: u32,
    pub rounds: u32,
    pub started_at: String,
    pub ends_at: String,
    /// When the whole plan (all rounds and breaks) finishes.
    pub plan_ends_at: String,
    pub remaining_secs: i64,
    pub total_secs: i64,
    pub profile_name: Option<String>,
    pub blocking: bool,
    pub preview: Option<BlockPreview>,
    /// `allow` | `confirm` | `strict`
    pub end_early: String,
    /// Strict mode: seconds until ending is allowed (after a request).
    pub stop_wait_secs: Option<i64>,
    pub plan: FocusPlan,
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
        Phase::Focus => Next::Break {
            round,
            mins: break_len(plan, round),
        },
        Phase::Break => Next::Focus { round: round + 1 },
    }
}

fn break_len(plan: &FocusPlan, round: u32) -> u32 {
    if plan.long_break_mins > 0 && round.is_multiple_of(4) {
        plan.long_break_mins
    } else {
        plan.break_mins
    }
}

/// Remaining seconds of the whole plan after the current phase ends.
fn rest_of_plan_secs(plan: &FocusPlan, phase: Phase, round: u32) -> i64 {
    let mut secs = 0i64;
    let (mut ph, mut r) = (phase, round);
    loop {
        match next_phase(plan, ph, r) {
            Next::Done => break,
            Next::Break { round, mins } => {
                secs += mins as i64 * 60;
                ph = Phase::Break;
                r = round;
            }
            Next::Focus { round } => {
                secs += plan.focus_mins as i64 * 60;
                ph = Phase::Focus;
                r = round;
            }
        }
    }
    secs
}

impl ActiveFocus {
    fn blocking_now(&self) -> bool {
        self.preview.is_some()
            && (self.phase == Phase::Focus
                || !self.profile.as_ref().is_some_and(|p| p.unblock_on_breaks))
    }

    pub fn status(&self, now: NaiveDateTime) -> FocusStatus {
        let end_early = self
            .profile
            .as_ref()
            .map_or("allow".to_string(), |p| p.end_early.clone());
        let stop_wait_secs = self
            .stop_requested
            .map(|t| (STRICT_WAIT_SECS - (now - t).num_seconds()).max(0));
        FocusStatus {
            label: self.plan.label.clone(),
            mode: self.plan.mode.clone(),
            subject: self.plan.subject.clone(),
            goal_text: self.plan.goal_text.clone(),
            phase: self.phase,
            round: self.round,
            rounds: self.plan.rounds,
            started_at: db::fmt_time(self.start),
            ends_at: db::fmt_time(self.end),
            plan_ends_at: db::fmt_time(
                self.end + Duration::seconds(rest_of_plan_secs(&self.plan, self.phase, self.round)),
            ),
            remaining_secs: (self.end - now).num_seconds().max(0),
            total_secs: (self.end - self.start).num_seconds(),
            profile_name: self.profile.as_ref().map(|p| p.name.clone()),
            blocking: self.blocking_now(),
            preview: self.preview.clone(),
            end_early,
            stop_wait_secs,
            plan: self.plan.clone(),
        }
    }
}

fn now() -> NaiveDateTime {
    let t = Local::now().naive_local();
    t.with_nanosecond(0).unwrap_or(t)
}

fn fmt_mins(m: u32) -> String {
    if m >= 60 && m.is_multiple_of(60) {
        format!("{}-hour", m / 60)
    } else {
        format!("{m}-minute")
    }
}

fn insert_row(
    conn: &Connection,
    plan: &FocusPlan,
    phase: Phase,
    secs: i64,
    start: NaiveDateTime,
    blocking: bool,
) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO focus_sessions (label, kind, planned_secs, day, start_time, block_distracting,
                                     mode, subject, goal_text, goal_id, profile_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            plan.label.trim(),
            if phase == Phase::Focus {
                "focus"
            } else {
                "break"
            },
            secs,
            start.format(db::DAY_FMT).to_string(),
            db::fmt_time(start),
            blocking,
            plan.mode,
            plan.subject
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty()),
            plan.goal_text
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty()),
            plan.goal_id,
            plan.profile_id,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

fn close_row(
    conn: &Connection,
    id: i64,
    start: NaiveDateTime,
    end: NaiveDateTime,
    completed: bool,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE focus_sessions SET end_time = ?1, actual_secs = ?2, completed = ?3 WHERE id = ?4",
        params![
            db::fmt_time(end),
            (end - start).num_seconds().max(0),
            completed,
            id
        ],
    )?;
    Ok(())
}

/// Pushes the session's block lists to the blocker and the browser extension.
fn apply_blocks(shared: &Shared, active: Option<&ActiveFocus>) {
    let on = active.filter(|a| a.blocking_now());
    let (sites, allow_sites, apps) = match on.and_then(|a| a.preview.as_ref().map(|p| (a, p))) {
        Some((a, p)) => {
            let label = format!(
                "{} (until {})",
                a.plan.label,
                a.status(now()).plan_ends_at.get(11..16).unwrap_or("")
            );
            let blocks = if p.mode == "allow_only" {
                AppBlocks {
                    allow_only: Some(p.allowed_apps.iter().map(AppRef::target).collect()),
                    focus_label: Some(label),
                    ..Default::default()
                }
            } else {
                AppBlocks {
                    focus: p.blocked_apps.iter().map(AppRef::target).collect(),
                    focus_label: Some(label),
                    ..Default::default()
                }
            };
            let allow = (p.mode == "allow_only").then(|| p.allowed_sites.clone());
            (p.blocked_sites.clone(), allow, Some(blocks))
        }
        None => (Vec::new(), None, None),
    };
    if let Ok(mut b) = shared.focus_blocklist.lock() {
        *b = sites;
    }
    if let Ok(mut b) = shared.focus_allow_sites.lock() {
        *b = allow_sites;
    }
    if let Ok(mut b) = shared.app_blocks.lock() {
        let limit = std::mem::take(&mut b.limit);
        *b = apps.unwrap_or_default();
        b.limit = limit;
    }
}

/// Resolves what a profile would block right now (for the pre-start preview).
pub fn preview(conn: &Connection, profile_id: i64) -> Result<(Profile, BlockPreview), String> {
    let p = profiles::get(conn, profile_id)
        .map_err(|e| e.to_string())?
        .ok_or("Profile not found")?;
    let prev = profiles::resolve(conn, &p, &installed::cached()).map_err(|e| e.to_string())?;
    Ok((p, prev))
}

#[allow(clippy::too_many_arguments)]
fn begin(
    conn: &Connection,
    plan: FocusPlan,
    profile: Option<Profile>,
    preview: Option<BlockPreview>,
    phase: Phase,
    round: u32,
    mins: u32,
    at: NaiveDateTime,
) -> rusqlite::Result<ActiveFocus> {
    let secs = mins as i64 * 60;
    let mut a = ActiveFocus {
        plan,
        profile,
        preview,
        phase,
        round,
        session_id: 0,
        start: at,
        end: at + Duration::seconds(secs),
        stop_requested: None,
    };
    a.session_id = insert_row(conn, &a.plan, phase, secs, at, a.blocking_now())?;
    Ok(a)
}

/// Starts a new plan. Fails if a session is already running.
pub fn start(conn: &Connection, shared: &Shared, plan: FocusPlan) -> Result<FocusStatus, String> {
    plan.validate()?;
    if shared.focus.lock().map(|f| f.is_some()).unwrap_or(false) {
        return Err("A session is already running. End it first.".into());
    }
    let (profile, prev) = match plan.profile_id {
        Some(id) => {
            let (p, prev) = preview(conn, id)?;
            (Some(p), Some(prev))
        }
        None => (None, None),
    };
    let at = now();
    let mins = plan.focus_mins;
    let active =
        begin(conn, plan, profile, prev, Phase::Focus, 1, mins, at).map_err(|e| e.to_string())?;
    let status = active.status(at);
    apply_blocks(shared, Some(&active));
    if let Ok(mut f) = shared.focus.lock() {
        *f = Some(active);
    }
    Ok(status)
}

/// Strict profiles: ask to end early; ending is possible after a short wait.
pub fn request_stop(shared: &Shared) -> Result<i64, String> {
    let mut f = shared.focus.lock().map_err(|e| e.to_string())?;
    let a = f.as_mut().ok_or("No session is running")?;
    let at = now();
    let t = *a.stop_requested.get_or_insert(at);
    Ok((STRICT_WAIT_SECS - (at - t).num_seconds()).max(0))
}

/// Checks the profile's early-end rule.
pub fn can_stop(a: &ActiveFocus, phrase: Option<&str>, at: NaiveDateTime) -> Result<(), String> {
    let rule = a.profile.as_ref().map_or("allow", |p| p.end_early.as_str());
    let phrase_ok = phrase.is_some_and(|p| p.trim().eq_ignore_ascii_case(STOP_PHRASE));
    match rule {
        "allow" => Ok(()),
        "confirm" if phrase_ok => Ok(()),
        "confirm" => Err(format!("Type \"{STOP_PHRASE}\" to end the session early")),
        _ => match a.stop_requested {
            None => Err("Request to end the session first".into()),
            Some(t) if (at - t).num_seconds() < STRICT_WAIT_SECS => Err(format!(
                "You can end the session in {} seconds",
                STRICT_WAIT_SECS - (at - t).num_seconds()
            )),
            Some(_) if phrase_ok => Ok(()),
            Some(_) => Err(format!("Type \"{STOP_PHRASE}\" to end the session early")),
        },
    }
}

/// Ends the current session early (recorded as not completed).
pub fn stop(conn: &Connection, shared: &Shared, phrase: Option<&str>) -> Result<(), String> {
    let at = now();
    let mut f = shared.focus.lock().map_err(|e| e.to_string())?;
    if let Some(a) = f.as_ref() {
        can_stop(a, phrase, at)?;
        close_row(conn, a.session_id, a.start, at.min(a.end), false).map_err(|e| e.to_string())?;
    }
    *f = None;
    drop(f);
    apply_blocks(shared, None);
    Ok(())
}

pub fn status(shared: &Shared) -> Option<FocusStatus> {
    let at = now();
    shared
        .focus
        .lock()
        .ok()
        .and_then(|f| f.as_ref().map(|a| a.status(at)))
}

/// Advances the timer; returns the notification to show, if any.
pub fn tick(
    conn: &Connection,
    shared: &Shared,
    at: NaiveDateTime,
    save_progress: bool,
) -> rusqlite::Result<Option<(String, String)>> {
    let mut guard = match shared.focus.lock() {
        Ok(g) => g,
        Err(_) => return Ok(None),
    };
    let Some(active) = guard.as_ref() else {
        return Ok(None);
    };

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
            let unblocked = if a.preview.is_some() {
                " Blocked apps and websites are available again."
            } else {
                ""
            };
            let goal = a
                .plan
                .goal_text
                .as_deref()
                .map(|g| format!(" Did you finish \"{g}\"?"))
                .unwrap_or_default();
            let body = if rounds > 1 {
                format!(
                    "All {rounds} rounds of \"{}\" are done.{unblocked}{goal}",
                    a.plan.label
                )
            } else {
                format!(
                    "You completed a {} session: {}.{unblocked}{goal}",
                    fmt_mins(a.plan.focus_mins),
                    a.plan.label
                )
            };
            (
                if a.plan.mode == "study" {
                    "Study session complete"
                } else {
                    "Focus session complete"
                }
                .to_string(),
                body,
            )
        }
        Next::Break { round, mins } => {
            *guard = Some(begin(
                conn,
                a.plan.clone(),
                a.profile.clone(),
                a.preview.clone(),
                Phase::Break,
                round,
                mins,
                a.end,
            )?);
            (
                format!("Round {round} of {rounds} done"),
                format!("Time for a {} break.", fmt_mins(mins)),
            )
        }
        Next::Focus { round } => {
            let mins = a.plan.focus_mins;
            *guard = Some(begin(
                conn,
                a.plan.clone(),
                a.profile.clone(),
                a.preview.clone(),
                Phase::Focus,
                round,
                mins,
                a.end,
            )?);
            (
                "Back to focus".to_string(),
                format!(
                    "Round {round} of {rounds} of \"{}\" has started.",
                    a.plan.label
                ),
            )
        }
    };
    let current = guard.clone();
    drop(guard);
    apply_blocks(shared, current.as_ref());
    Ok(Some(note))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FocusRecord {
    pub id: i64,
    pub label: String,
    pub mode: String,
    pub subject: Option<String>,
    pub goal_text: Option<String>,
    pub goal_id: Option<i64>,
    pub day: String,
    pub start: String,
    pub end: Option<String>,
    pub planned_secs: i64,
    pub actual_secs: i64,
    pub completed: bool,
}

pub fn history(
    conn: &Connection,
    mode: Option<&str>,
    limit: i64,
) -> rusqlite::Result<Vec<FocusRecord>> {
    let mut stmt = conn.prepare(
        "SELECT id, label, mode, subject, goal_text, goal_id, day, start_time, end_time, planned_secs, actual_secs, completed
         FROM focus_sessions WHERE kind = 'focus' AND end_time IS NOT NULL AND (?1 IS NULL OR mode = ?1)
         ORDER BY start_time DESC LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![mode, limit], |r| {
        Ok(FocusRecord {
            id: r.get(0)?,
            label: r.get(1)?,
            mode: r.get(2)?,
            subject: r.get(3)?,
            goal_text: r.get(4)?,
            goal_id: r.get(5)?,
            day: r.get(6)?,
            start: r.get(7)?,
            end: r.get(8)?,
            planned_secs: r.get(9)?,
            actual_secs: r.get(10)?,
            completed: r.get::<_, i64>(11)? != 0,
        })
    })?;
    let out = rows.collect();
    out
}

/// (focused seconds, completed focus rounds) for days in range; `mode` filters focus/study.
pub fn totals_mode(
    conn: &Connection,
    from: &str,
    to: &str,
    mode: Option<&str>,
) -> rusqlite::Result<(i64, i64)> {
    conn.query_row(
        "SELECT COALESCE(SUM(actual_secs), 0), COALESCE(SUM(completed), 0)
         FROM focus_sessions WHERE kind = 'focus' AND day BETWEEN ?1 AND ?2 AND (?3 IS NULL OR mode = ?3)",
        params![from, to, mode],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
}

pub fn totals(conn: &Connection, from: &str, to: &str) -> rusqlite::Result<(i64, i64)> {
    totals_mode(conn, from, to, None)
}

/// Consecutive days, ending today (or yesterday), with a completed focus session.
pub fn streak(conn: &Connection, today: chrono::NaiveDate) -> rusqlite::Result<i64> {
    let mut count = 0;
    let mut day = today;
    let has = |d: chrono::NaiveDate| -> rusqlite::Result<bool> {
        conn.query_row(
            "SELECT 1 FROM focus_sessions WHERE kind = 'focus' AND completed = 1 AND day = ?1 LIMIT 1",
            params![d.format(db::DAY_FMT).to_string()],
            |_| Ok(()),
        )
        .optional()
        .map(|o| o.is_some())
    };
    // Today without a session yet doesn't break the streak.
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
                if shared.show_requested.swap(false, Ordering::SeqCst) {
                    use tauri::Manager;
                    if let Some(w) = app.get_webview_window("main") {
                        let _ = w.unminimize();
                        let _ = w.show();
                        let _ = w.set_focus();
                    }
                }
                match tick(&conn, &shared, now(), ticks.is_multiple_of(15)) {
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
            profile_id: None,
            mode: "focus".into(),
            subject: None,
            goal_text: None,
            goal_id: None,
        }
    }

    fn profile_id(conn: &Connection, name: &str) -> i64 {
        conn.query_row(
            "SELECT id FROM block_profiles WHERE name = ?1",
            params![name],
            |r| r.get(0),
        )
        .unwrap()
    }

    #[test]
    fn pomodoro_sequence() {
        let p = plan(4, 5, 15);
        assert_eq!(
            next_phase(&p, Phase::Focus, 1),
            Next::Break { round: 1, mins: 5 }
        );
        assert_eq!(next_phase(&p, Phase::Break, 1), Next::Focus { round: 2 });
        assert_eq!(next_phase(&p, Phase::Focus, 4), Next::Done);
        assert_eq!(
            next_phase(&plan(8, 5, 15), Phase::Focus, 4),
            Next::Break { round: 4, mins: 15 }
        );
        assert_eq!(next_phase(&plan(1, 0, 0), Phase::Focus, 1), Next::Done);
        assert_eq!(
            next_phase(&plan(3, 0, 0), Phase::Focus, 1),
            Next::Focus { round: 2 }
        );
        // 4 × 25 with 5-minute breaks: after round 1 there are 3 breaks and 3 rounds left.
        assert_eq!(
            rest_of_plan_secs(&p, Phase::Focus, 1),
            (3 * 5 + 3 * 25) * 60
        );
    }

    #[test]
    fn validation() {
        assert!(plan(4, 5, 15).validate().is_ok());
        assert!(FocusPlan {
            focus_mins: 0,
            ..plan(1, 0, 0)
        }
        .validate()
        .is_err());
        assert!(FocusPlan {
            rounds: 0,
            ..plan(1, 0, 0)
        }
        .validate()
        .is_err());
        assert!(FocusPlan {
            label: " ".into(),
            ..plan(1, 0, 0)
        }
        .validate()
        .is_err());
        assert!(FocusPlan {
            mode: "nap".into(),
            ..plan(1, 0, 0)
        }
        .validate()
        .is_err());
    }

    #[test]
    fn profile_blocks_whole_session_and_restores_access() {
        let conn = db::open_in_memory().unwrap();
        let shared = Shared::new(false, 300);
        let t = now();
        db::get_or_create_app(&conn, "steam.exe", "", || "Steam".into(), t).unwrap();
        let p = FocusPlan {
            focus_mins: 1,
            break_mins: 1,
            profile_id: Some(profile_id(&conn, "Study Mode")),
            ..plan(2, 1, 0)
        };

        let st = start(&conn, &shared, p.clone()).unwrap();
        assert!(st.blocking);
        assert!(start(&conn, &shared, p).is_err(), "one session at a time");
        assert!(shared
            .focus_blocklist
            .lock()
            .unwrap()
            .contains(&"youtube.com".to_string()));
        assert!(shared
            .app_blocks
            .lock()
            .unwrap()
            .focus
            .iter()
            .any(|t| t.exe == "steam.exe"));

        let t0 = shared.focus.lock().unwrap().as_ref().unwrap().start;
        let n = tick(&conn, &shared, t0 + Duration::seconds(60), false)
            .unwrap()
            .unwrap();
        assert_eq!(n.0, "Round 1 of 2 done");
        assert!(
            shared
                .app_blocks
                .lock()
                .unwrap()
                .focus
                .iter()
                .any(|t| t.exe == "steam.exe"),
            "still blocked during the break"
        );

        tick(&conn, &shared, t0 + Duration::seconds(120), false)
            .unwrap()
            .unwrap();
        let done = tick(&conn, &shared, t0 + Duration::seconds(180), false)
            .unwrap()
            .unwrap();
        assert_eq!(done.0, "Focus session complete");
        assert!(done.1.contains("available again"));
        assert!(status(&shared).is_none());
        assert!(shared.focus_blocklist.lock().unwrap().is_empty());
        assert!(shared.app_blocks.lock().unwrap().focus.is_empty());

        let day = t0.format(db::DAY_FMT).to_string();
        assert_eq!(totals(&conn, &day, &day).unwrap(), (120, 2));
    }

    #[test]
    fn early_end_rules() {
        let conn = db::open_in_memory().unwrap();
        let shared = Shared::new(false, 300);
        // Study Mode = confirm with a phrase.
        start(
            &conn,
            &shared,
            FocusPlan {
                profile_id: Some(profile_id(&conn, "Study Mode")),
                ..plan(1, 0, 0)
            },
        )
        .unwrap();
        assert!(stop(&conn, &shared, None).is_err());
        assert!(stop(&conn, &shared, Some("nope")).is_err());
        stop(&conn, &shared, Some(" i want to stop ")).unwrap();
        assert!(status(&shared).is_none());

        // Deep Focus = strict: request, wait, then confirm.
        start(
            &conn,
            &shared,
            FocusPlan {
                profile_id: Some(profile_id(&conn, "Deep Focus")),
                ..plan(1, 0, 0)
            },
        )
        .unwrap();
        assert!(shared.app_blocks.lock().unwrap().allow_only.is_some());
        let a = shared.focus.lock().unwrap().clone().unwrap();
        assert!(
            can_stop(&a, Some(STOP_PHRASE), a.start).is_err(),
            "must request first"
        );
        let requested = ActiveFocus {
            stop_requested: Some(a.start),
            ..a
        };
        assert!(can_stop(
            &requested,
            Some(STOP_PHRASE),
            requested.start + Duration::seconds(30)
        )
        .is_err());
        assert!(can_stop(
            &requested,
            Some(STOP_PHRASE),
            requested.start + Duration::seconds(61)
        )
        .is_ok());

        let h = history(&conn, None, 10).unwrap();
        assert_eq!(h.len(), 1);
        assert!(!h[0].completed);
    }

    #[test]
    fn study_sessions_are_counted_separately() {
        let conn = db::open_in_memory().unwrap();
        let shared = Shared::new(false, 300);
        start(
            &conn,
            &shared,
            FocusPlan {
                mode: "study".into(),
                subject: Some("Maths".into()),
                focus_mins: 1,
                ..plan(1, 0, 0)
            },
        )
        .unwrap();
        let t0 = shared.focus.lock().unwrap().as_ref().unwrap().start;
        let n = tick(&conn, &shared, t0 + Duration::seconds(60), false)
            .unwrap()
            .unwrap();
        assert_eq!(n.0, "Study session complete");
        let day = t0.format(db::DAY_FMT).to_string();
        assert_eq!(
            totals_mode(&conn, &day, &day, Some("study")).unwrap(),
            (60, 1)
        );
        assert_eq!(
            totals_mode(&conn, &day, &day, Some("focus")).unwrap(),
            (0, 0)
        );
        assert_eq!(
            history(&conn, Some("study"), 5).unwrap()[0]
                .subject
                .as_deref(),
            Some("Maths")
        );
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
        assert_eq!(
            streak(&conn, today).unwrap(),
            2,
            "today not done yet keeps the streak"
        );
    }
}
