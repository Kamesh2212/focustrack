//! Background tracking engine (SRS 2.2, 2.8, FR-01..FR-04).
//!
//! Polls once a second on its own thread with its own SQLite connection:
//!   * foreground app  -> `usage_sessions`
//!   * no input for longer than the idle threshold -> `idle_periods`
//!   * locked / paused / suspended -> nothing is recorded
//!
//! Open rows are inserted when a span starts and updated in place every few
//! seconds, so a crash or power loss loses at most `FLUSH_EVERY_TICKS` seconds
//! and never produces duplicates. Sleep/hibernate is detected as a jump in
//! wall-clock time between polls; the open span is closed at the last poll
//! before the jump.

use crate::db::{self, SpanKind};
use crate::platform::{self, ForegroundApp};
use chrono::{Duration, Local, NaiveDateTime, NaiveTime, Timelike};
use rusqlite::Connection;
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

const POLL_MS: u64 = 1000;
const FLUSH_EVERY_TICKS: u64 = 5;
/// A forward jump in wall-clock time larger than this between two polls means
/// the machine was suspended (sleep / hibernate) or the clock changed.
const SUSPEND_GAP_SECS: i64 = 20;

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LiveStatus {
    /// `starting` | `active` | `idle` | `locked` | `paused`
    pub state: String,
    pub app_name: Option<String>,
    pub exe_name: Option<String>,
    /// When the current app session / idle period started.
    pub since: Option<String>,
    pub idle_secs: u64,
}

/// State shared between the tracker thread and the UI commands.
pub struct Shared {
    pub paused: AtomicBool,
    pub idle_threshold_secs: AtomicU64,
    /// Set after history is deleted so the tracker forgets its open row ids.
    pub reset_requested: AtomicBool,
    pub shutdown: AtomicBool,
    pub stopped: AtomicBool,
    pub status: Mutex<LiveStatus>,
}

impl Shared {
    pub fn new(paused: bool, idle_threshold_secs: u64) -> Self {
        Self {
            paused: AtomicBool::new(paused),
            idle_threshold_secs: AtomicU64::new(idle_threshold_secs),
            reset_requested: AtomicBool::new(false),
            shutdown: AtomicBool::new(false),
            stopped: AtomicBool::new(false),
            status: Mutex::new(LiveStatus { state: "starting".into(), ..Default::default() }),
        }
    }

    pub fn status(&self) -> LiveStatus {
        self.status.lock().map(|s| s.clone()).unwrap_or_default()
    }
}

/// What the tracker needs from the OS. Swapped for a fake in tests.
pub trait Probe {
    fn foreground(&mut self) -> Option<ForegroundApp>;
    fn idle_seconds(&mut self) -> u64;
    fn locked(&mut self) -> bool;
    fn describe(&mut self, exe_path: &str) -> Option<String>;
}

pub struct WindowsProbe;

impl Probe for WindowsProbe {
    fn foreground(&mut self) -> Option<ForegroundApp> {
        platform::foreground_app()
    }
    fn idle_seconds(&mut self) -> u64 {
        platform::idle_seconds()
    }
    fn locked(&mut self) -> bool {
        platform::is_session_locked()
    }
    fn describe(&mut self, exe_path: &str) -> Option<String> {
        platform::file_description(exe_path)
    }
}

pub fn spawn(db_path: PathBuf, shared: Arc<Shared>) {
    thread::Builder::new()
        .name("focustrack-tracker".into())
        .spawn(move || {
            let conn = match db::open(&db_path) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("[tracker] cannot open database: {e}");
                    return;
                }
            };
            let mut tracker = Tracker::new(conn, shared.clone(), WindowsProbe, now());
            loop {
                if shared.shutdown.load(Ordering::SeqCst) {
                    tracker.close_all(now());
                    shared.stopped.store(true, Ordering::SeqCst);
                    break;
                }
                tracker.tick(now());
                thread::sleep(std::time::Duration::from_millis(POLL_MS));
            }
        })
        .expect("failed to spawn tracker thread");
}

fn now() -> NaiveDateTime {
    // Whole seconds keep stored durations exact.
    let t = Local::now().naive_local();
    t.with_nanosecond(0).unwrap_or(t)
}

fn midnight(t: NaiveDateTime) -> NaiveDateTime {
    t.date().and_time(NaiveTime::MIN)
}

#[derive(Debug, Clone)]
struct Span {
    id: i64,
    start: NaiveDateTime,
    last: NaiveDateTime,
}

#[derive(Debug, Clone)]
struct ActiveSpan {
    span: Span,
    app_id: i64,
    exe: String,
    name: String,
}

pub struct Tracker<P: Probe> {
    conn: Connection,
    shared: Arc<Shared>,
    probe: P,
    active: Option<ActiveSpan>,
    idle: Option<Span>,
    apps: HashMap<String, (i64, String)>,
    last_tick: NaiveDateTime,
    ticks: u64,
}

impl<P: Probe> Tracker<P> {
    pub fn new(conn: Connection, shared: Arc<Shared>, probe: P, start: NaiveDateTime) -> Self {
        Self {
            conn,
            shared,
            probe,
            active: None,
            idle: None,
            apps: HashMap::new(),
            last_tick: start,
            ticks: 0,
        }
    }

    pub fn tick(&mut self, now: NaiveDateTime) {
        if self.shared.reset_requested.swap(false, Ordering::SeqCst) {
            self.active = None;
            self.idle = None;
        }

        // Sleep / hibernate / clock change: close everything at the last poll.
        let gap = (now - self.last_tick).num_seconds();
        if gap > SUSPEND_GAP_SECS || gap < 0 {
            let end = self.last_tick.min(now);
            self.close_all(end);
        }
        self.last_tick = now;
        self.split_at_midnight(now);

        if self.shared.paused.load(Ordering::SeqCst) {
            self.close_all(now);
            self.set_status(LiveStatus { state: "paused".into(), ..Default::default() });
            return;
        }

        let fg = self.probe.foreground();
        let lock_screen = fg
            .as_ref()
            .map(|a| a.exe_name.eq_ignore_ascii_case("LockApp.exe"))
            .unwrap_or(false);
        if lock_screen || self.probe.locked() {
            self.close_all(now);
            self.set_status(LiveStatus { state: "locked".into(), ..Default::default() });
            return;
        }

        let idle_secs = self.probe.idle_seconds();
        let threshold = self.shared.idle_threshold_secs.load(Ordering::SeqCst);
        let last_input = (now - Duration::seconds(idle_secs as i64)).max(midnight(now));

        if idle_secs >= threshold {
            // Trim the app session back to the last input; that stretch is idle.
            if let Some(a) = self.active.take() {
                let end = last_input.max(a.span.start);
                self.finish(SpanKind::Usage, a.span, end);
            }
            match self.idle.as_mut() {
                Some(span) => span.last = now,
                None => self.idle = self.open_idle(last_input, now),
            }
            let since = self.idle.as_ref().map(|s| db::fmt_time(s.start));
            self.set_status(LiveStatus { state: "idle".into(), since, idle_secs, ..Default::default() });
            self.maybe_flush();
            return;
        }

        if let Some(span) = self.idle.take() {
            let end = last_input.max(span.start);
            self.finish(SpanKind::Idle, span, end);
        }

        match fg {
            None => {
                if let Some(a) = self.active.take() {
                    self.finish(SpanKind::Usage, a.span, now);
                }
                self.set_status(LiveStatus { state: "active".into(), idle_secs, ..Default::default() });
            }
            Some(app) => {
                let same = self
                    .active
                    .as_ref()
                    .map(|a| a.exe.eq_ignore_ascii_case(&app.exe_name))
                    .unwrap_or(false);
                if same {
                    if let Some(a) = self.active.as_mut() {
                        a.span.last = now;
                    }
                } else {
                    if let Some(a) = self.active.take() {
                        self.finish(SpanKind::Usage, a.span, now);
                    }
                    self.active = self.open_usage(&app, now);
                }
                let status = match &self.active {
                    Some(a) => LiveStatus {
                        state: "active".into(),
                        app_name: Some(a.name.clone()),
                        exe_name: Some(a.exe.clone()),
                        since: Some(db::fmt_time(a.span.start)),
                        idle_secs,
                    },
                    None => LiveStatus { state: "active".into(), idle_secs, ..Default::default() },
                };
                self.set_status(status);
            }
        }
        self.maybe_flush();
    }

    pub fn close_all(&mut self, end: NaiveDateTime) {
        if let Some(a) = self.active.take() {
            let end = end.max(a.span.start);
            self.finish(SpanKind::Usage, a.span, end);
        }
        if let Some(s) = self.idle.take() {
            let end = end.max(s.start);
            self.finish(SpanKind::Idle, s, end);
        }
    }

    /// Keeps every row within a single calendar day.
    fn split_at_midnight(&mut self, now: NaiveDateTime) {
        let mid = midnight(now);
        if let Some(a) = self.active.as_ref() {
            if a.span.start < mid {
                let a = self.active.take().unwrap();
                self.finish(SpanKind::Usage, a.span.clone(), mid);
                if let Ok(id) = db::insert_usage(&self.conn, a.app_id, mid) {
                    self.active = Some(ActiveSpan { span: Span { id, start: mid, last: now }, ..a });
                }
            }
        }
        if let Some(s) = self.idle.as_ref() {
            if s.start < mid {
                let s = self.idle.take().unwrap();
                self.finish(SpanKind::Idle, s, mid);
                self.idle = self.open_idle(mid, now);
            }
        }
    }

    fn open_usage(&mut self, app: &ForegroundApp, now: NaiveDateTime) -> Option<ActiveSpan> {
        let key = app.exe_name.to_ascii_lowercase();
        let (app_id, name) = match self.apps.get(&key) {
            Some(v) => v.clone(),
            None => {
                let probe = &mut self.probe;
                let found = db::get_or_create_app(
                    &self.conn,
                    &app.exe_name,
                    &app.exe_path,
                    || {
                        probe
                            .describe(&app.exe_path)
                            .unwrap_or_else(|| platform::pretty_exe_name(&app.exe_name))
                    },
                    now,
                );
                match found {
                    Ok(v) => {
                        self.apps.insert(key, v.clone());
                        v
                    }
                    Err(e) => {
                        eprintln!("[tracker] app lookup failed: {e}");
                        return None;
                    }
                }
            }
        };
        match db::insert_usage(&self.conn, app_id, now) {
            Ok(id) => Some(ActiveSpan {
                span: Span { id, start: now, last: now },
                app_id,
                exe: app.exe_name.clone(),
                name,
            }),
            Err(e) => {
                eprintln!("[tracker] insert failed: {e}");
                None
            }
        }
    }

    fn open_idle(&mut self, start: NaiveDateTime, now: NaiveDateTime) -> Option<Span> {
        match db::insert_idle(&self.conn, start) {
            Ok(id) => Some(Span { id, start, last: now }),
            Err(e) => {
                eprintln!("[tracker] insert idle failed: {e}");
                None
            }
        }
    }

    fn finish(&mut self, kind: SpanKind, span: Span, end: NaiveDateTime) {
        let result = if end <= span.start {
            db::delete_span(&self.conn, kind, span.id)
        } else {
            db::update_span(&self.conn, kind, span.id, span.start, end)
        };
        if let Err(e) = result {
            eprintln!("[tracker] closing span failed: {e}");
        }
    }

    fn maybe_flush(&mut self) {
        self.ticks += 1;
        if self.ticks % FLUSH_EVERY_TICKS != 0 {
            return;
        }
        if let Some(a) = &self.active {
            let _ = db::update_span(&self.conn, SpanKind::Usage, a.span.id, a.span.start, a.span.last);
        }
        if let Some(s) = &self.idle {
            let _ = db::update_span(&self.conn, SpanKind::Idle, s.id, s.start, s.last);
        }
    }

    fn set_status(&self, status: LiveStatus) {
        if let Ok(mut s) = self.shared.status.lock() {
            *s = status;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[derive(Default)]
    struct Fake {
        app: Option<&'static str>,
        idle: u64,
        locked: bool,
    }

    impl Probe for Fake {
        fn foreground(&mut self) -> Option<ForegroundApp> {
            self.app.map(|n| ForegroundApp { exe_name: n.into(), exe_path: format!("C:\\{n}") })
        }
        fn idle_seconds(&mut self) -> u64 {
            self.idle
        }
        fn locked(&mut self) -> bool {
            self.locked
        }
        fn describe(&mut self, _: &str) -> Option<String> {
            None
        }
    }

    fn at(d: u32, h: u32, m: u32, s: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, d).unwrap().and_hms_opt(h, m, s).unwrap()
    }

    fn setup(start: NaiveDateTime) -> Tracker<Fake> {
        let conn = db::open_in_memory().unwrap();
        Tracker::new(conn, Arc::new(Shared::new(false, 300)), Fake::default(), start)
    }

    /// Runs one tick per second from `from` to `to` inclusive.
    fn run(t: &mut Tracker<Fake>, from: NaiveDateTime, to: NaiveDateTime) {
        let mut cur = from;
        while cur <= to {
            t.tick(cur);
            cur += Duration::seconds(1);
        }
    }

    fn usage(t: &Tracker<Fake>) -> Vec<(String, String, i64)> {
        let mut stmt = t
            .conn
            .prepare(
                "SELECT a.exe_name, s.day, s.duration_secs FROM usage_sessions s
                 JOIN applications a ON a.id = s.app_id ORDER BY s.id",
            )
            .unwrap();
        let rows: Vec<(String, String, i64)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        rows
    }

    fn idle_total(t: &Tracker<Fake>) -> i64 {
        t.conn
            .query_row("SELECT COALESCE(SUM(duration_secs),0) FROM idle_periods", [], |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn records_app_switches() {
        let mut t = setup(at(18, 9, 0, 0));
        t.probe.app = Some("code.exe");
        run(&mut t, at(18, 9, 0, 0), at(18, 9, 10, 0));
        t.probe.app = Some("chrome.exe");
        run(&mut t, at(18, 9, 10, 1), at(18, 9, 15, 0));
        t.close_all(at(18, 9, 15, 0));
        let rows = usage(&t);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].0, "code.exe");
        assert_eq!(rows[0].2, 601);
        assert_eq!(rows[1].0, "chrome.exe");
        assert_eq!(rows[1].2, 299);
    }

    #[test]
    fn idle_time_is_trimmed_from_active_usage() {
        let mut t = setup(at(18, 9, 0, 0));
        t.probe.app = Some("code.exe");
        run(&mut t, at(18, 9, 0, 0), at(18, 9, 10, 0));
        // User walked away at 09:10:00; threshold (300s) hit at 09:15:00.
        let mut cur = at(18, 9, 10, 1);
        while cur <= at(18, 9, 20, 0) {
            t.probe.idle = (cur - at(18, 9, 10, 0)).num_seconds() as u64;
            t.tick(cur);
            cur += Duration::seconds(1);
        }
        // Back at 09:20:01.
        t.probe.idle = 0;
        run(&mut t, at(18, 9, 20, 1), at(18, 9, 21, 0));
        t.close_all(at(18, 9, 21, 0));

        let rows = usage(&t);
        assert_eq!(rows[0].2, 600, "active usage stops at last input");
        assert_eq!(idle_total(&t), 601, "09:10:00 -> 09:20:01 is idle");
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn locked_and_paused_time_is_not_counted() {
        let mut t = setup(at(18, 9, 0, 0));
        t.probe.app = Some("code.exe");
        run(&mut t, at(18, 9, 0, 0), at(18, 9, 1, 0));
        t.probe.locked = true;
        run(&mut t, at(18, 9, 1, 1), at(18, 9, 30, 0));
        t.probe.locked = false;
        t.shared.paused.store(true, Ordering::SeqCst);
        run(&mut t, at(18, 9, 30, 1), at(18, 9, 40, 0));
        assert_eq!(t.shared.status().state, "paused");
        t.shared.paused.store(false, Ordering::SeqCst);
        run(&mut t, at(18, 9, 40, 1), at(18, 9, 41, 0));
        t.close_all(at(18, 9, 41, 0));

        let total: i64 = usage(&t).iter().map(|r| r.2).sum();
        assert_eq!(total, 61 + 59);
        assert_eq!(idle_total(&t), 0);
    }

    #[test]
    fn suspend_gap_closes_session_at_last_poll() {
        let mut t = setup(at(18, 9, 0, 0));
        t.probe.app = Some("code.exe");
        run(&mut t, at(18, 9, 0, 0), at(18, 9, 5, 0));
        // Laptop lid closed for two hours.
        run(&mut t, at(18, 11, 5, 0), at(18, 11, 6, 0));
        t.close_all(at(18, 11, 6, 0));
        let rows = usage(&t);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].2, 300);
        assert_eq!(rows[1].2, 60);
    }

    #[test]
    fn sessions_split_at_midnight() {
        let mut t = setup(at(18, 23, 59, 0));
        t.probe.app = Some("code.exe");
        run(&mut t, at(18, 23, 59, 0), at(19, 0, 1, 0));
        t.close_all(at(19, 0, 1, 0));
        let rows = usage(&t);
        assert_eq!(rows.len(), 2);
        assert_eq!((rows[0].1.as_str(), rows[0].2), ("2026-11-18", 60));
        assert_eq!((rows[1].1.as_str(), rows[1].2), ("2026-11-19", 60));
    }

    #[test]
    fn flushes_open_session_periodically() {
        let mut t = setup(at(18, 9, 0, 0));
        t.probe.app = Some("code.exe");
        run(&mut t, at(18, 9, 0, 0), at(18, 9, 0, 59));
        // No close_all: simulates a crash. At most FLUSH_EVERY_TICKS lost.
        let secs = usage(&t)[0].2;
        assert!(secs >= 59 - FLUSH_EVERY_TICKS as i64 && secs <= 59, "got {secs}");
    }
}
