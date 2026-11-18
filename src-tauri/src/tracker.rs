//! Background tracking engine (SRS 2.2, 2.8, FR-01..FR-04).
//!
//! Polls once a second on its own thread with its own SQLite connection:
//!   * foreground app  -> `usage_sessions`
//!   * active tab domain while a browser is in front -> `website_sessions`
//!     (reported by the browser extension via `browser.rs`)
//!   * no input for longer than the idle threshold -> `idle_periods`
//!   * locked / paused / suspended -> nothing is recorded
//!
//! Open rows are inserted when a span starts and updated in place every few
//! seconds, so a crash or power loss loses at most `FLUSH_EVERY_TICKS` seconds
//! and never produces duplicates. Sleep/hibernate is detected as a jump in
//! wall-clock time between polls; the open span is closed at the last poll
//! before the jump.

use crate::browser::{BrowserReport, FRESH_SECS};
use crate::categories::browser_family;
use crate::db::{self, SpanKind};
use crate::platform::{self, ForegroundApp};
use chrono::{Duration, Local, NaiveDateTime, NaiveTime, Timelike};
use rusqlite::Connection;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

const POLL_MS: u64 = 1000;
const FLUSH_EVERY_TICKS: u64 = 5;
/// A forward jump in wall-clock time larger than this between two polls means
/// the machine was suspended (sleep / hibernate) or the clock changed.
const SUSPEND_GAP_SECS: i64 = 20;
/// Start checking audio/controller activity after this much keyboard/mouse silence.
const MEDIA_CHECK_AFTER_SECS: u64 = 15;

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LiveStatus {
    /// `starting` | `active` | `idle` | `locked` | `paused`
    pub state: String,
    pub app_name: Option<String>,
    pub exe_name: Option<String>,
    /// Active website when a browser is in front and its extension reports.
    pub domain: Option<String>,
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
    /// Latest report from each browser's extension, keyed by browser family.
    pub browsers: Mutex<HashMap<String, BrowserReport>>,
    /// Lower-case exe names and domains that have an enabled usage limit.
    /// These keep counting while tracking is paused (so pausing can't be
    /// used to get around a limit).
    pub limited: Mutex<HashSet<String>>,
    /// Domains the extensions should block (limit reached, blocking on).
    pub blocklist: Mutex<Vec<String>>,
    /// Distracting domains blocked while a focus round is running.
    pub focus_blocklist: Mutex<Vec<String>>,
    /// "Allow only" focus profiles: the only domains that may be visited.
    pub focus_allow_sites: Mutex<Option<Vec<String>>>,
    /// Apps to close (limits reached, focus session) — enforced by `blocker.rs`.
    pub app_blocks: Mutex<crate::blocker::AppBlocks>,
    pub focus: Mutex<Option<crate::focus::ActiveFocus>>,
    pub website_tracking: AtomicBool,
    pub server_running: AtomicBool,
    pub notifications_enabled: AtomicBool,
    pub break_reminder_mins: AtomicU64,
}

impl Shared {
    pub fn new(paused: bool, idle_threshold_secs: u64) -> Self {
        Self {
            paused: AtomicBool::new(paused),
            idle_threshold_secs: AtomicU64::new(idle_threshold_secs),
            reset_requested: AtomicBool::new(false),
            shutdown: AtomicBool::new(false),
            stopped: AtomicBool::new(false),
            status: Mutex::new(LiveStatus {
                state: "starting".into(),
                ..Default::default()
            }),
            browsers: Mutex::new(HashMap::new()),
            limited: Mutex::new(HashSet::new()),
            blocklist: Mutex::new(Vec::new()),
            focus_blocklist: Mutex::new(Vec::new()),
            focus_allow_sites: Mutex::new(None),
            app_blocks: Mutex::new(crate::blocker::AppBlocks::default()),
            focus: Mutex::new(None),
            website_tracking: AtomicBool::new(true),
            server_running: AtomicBool::new(false),
            notifications_enabled: AtomicBool::new(true),
            break_reminder_mins: AtomicU64::new(0),
        }
    }

    pub fn status(&self) -> LiveStatus {
        self.status.lock().map(|s| s.clone()).unwrap_or_default()
    }
}

/// What the tracker needs from the OS. Swapped for a fake in tests.
pub trait Probe {
    fn foreground(&mut self) -> Option<ForegroundApp>;
    /// Seconds since the last keyboard/mouse input.
    fn idle_seconds(&mut self) -> u64;
    /// Seconds since the last game-controller input (`u64::MAX` if none).
    fn controller_idle_seconds(&mut self) -> u64;
    /// Lower-case exe names of apps playing sound right now.
    fn audible_exes(&mut self) -> HashSet<String>;
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
    fn controller_idle_seconds(&mut self) -> u64 {
        platform::controller_idle_seconds()
    }
    fn audible_exes(&mut self) -> HashSet<String> {
        platform::audible_exes()
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

#[derive(Debug, Clone)]
struct SiteSpan {
    span: Span,
    website_id: i64,
    domain: String,
    browser: String,
}

pub struct Tracker<P: Probe> {
    conn: Connection,
    shared: Arc<Shared>,
    probe: P,
    active: Option<ActiveSpan>,
    idle: Option<Span>,
    site: Option<SiteSpan>,
    apps: HashMap<String, (i64, String)>,
    /// When each app (lower-case exe) was last heard playing sound.
    last_audible: HashMap<String, NaiveDateTime>,
    sites: HashMap<String, i64>,
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
            site: None,
            apps: HashMap::new(),
            last_audible: HashMap::new(),
            sites: HashMap::new(),
            last_tick: start,
            ticks: 0,
        }
    }

    pub fn tick(&mut self, now: NaiveDateTime) {
        if self.shared.reset_requested.swap(false, Ordering::SeqCst) {
            self.active = None;
            self.idle = None;
            self.site = None;
        }

        // Sleep / hibernate / clock change: close everything at the last poll.
        let gap = (now - self.last_tick).num_seconds();
        if !(0..=SUSPEND_GAP_SECS).contains(&gap) {
            let end = self.last_tick.min(now);
            self.close_all(end);
        }
        self.last_tick = now;
        self.split_at_midnight(now);

        // While paused, only apps and websites with a usage limit are recorded.
        let paused = self.shared.paused.load(Ordering::SeqCst);
        let fg = self.probe.foreground();
        let lock_screen = fg
            .as_ref()
            .map(|a| a.exe_name.eq_ignore_ascii_case("LockApp.exe"))
            .unwrap_or(false);
        if lock_screen || self.probe.locked() {
            self.close_all(now);
            self.set_status(LiveStatus {
                state: "locked".into(),
                ..Default::default()
            });
            return;
        }

        let idle_secs = self.effective_idle(fg.as_ref(), now);
        let threshold = self.shared.idle_threshold_secs.load(Ordering::SeqCst);
        let last_input = (now - Duration::seconds(idle_secs as i64)).max(midnight(now));

        if idle_secs >= threshold || (paused && fg.is_none()) {
            // Trim the app session back to the last input; that stretch is idle.
            if let Some(a) = self.active.take() {
                let end = last_input.max(a.span.start);
                self.finish(SpanKind::Usage, a.span, end);
            }
            if let Some(w) = self.site.take() {
                let end = last_input.max(w.span.start);
                self.finish(SpanKind::Website, w.span, end);
            }
            if paused {
                // Paused: idle time isn't recorded.
                if let Some(span) = self.idle.take() {
                    let end = last_input.max(span.start);
                    self.finish(SpanKind::Idle, span, end);
                }
                self.set_status(LiveStatus {
                    state: "paused".into(),
                    ..Default::default()
                });
                self.maybe_flush();
                return;
            }
            match self.idle.as_mut() {
                Some(span) => span.last = now,
                None => self.idle = self.open_idle(last_input, now),
            }
            let since = self.idle.as_ref().map(|s| db::fmt_time(s.start));
            self.set_status(LiveStatus {
                state: "idle".into(),
                since,
                idle_secs,
                ..Default::default()
            });
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
                self.update_site(None, now);
                self.set_status(LiveStatus {
                    state: "active".into(),
                    idle_secs,
                    ..Default::default()
                });
            }
            Some(app) => {
                let record_app = !paused || self.is_limited_app(&app.exe_name);
                let same = self
                    .active
                    .as_ref()
                    .map(|a| a.exe.eq_ignore_ascii_case(&app.exe_name))
                    .unwrap_or(false);
                if !record_app {
                    if let Some(a) = self.active.take() {
                        self.finish(SpanKind::Usage, a.span, now);
                    }
                } else if same {
                    if let Some(a) = self.active.as_mut() {
                        a.span.last = now;
                    }
                } else {
                    if let Some(a) = self.active.take() {
                        self.finish(SpanKind::Usage, a.span, now);
                    }
                    self.active = self.open_usage(&app, now);
                }
                let site = self
                    .current_site(&app.exe_name, now)
                    .filter(|(_, d)| !paused || self.is_limited_site(d));
                self.update_site(site, now);
                if paused {
                    // Say what is still being counted, if anything.
                    let counting = self
                        .active
                        .as_ref()
                        .map(|a| a.name.clone())
                        .or_else(|| self.site.as_ref().map(|w| w.domain.clone()));
                    self.set_status(LiveStatus {
                        state: "paused".into(),
                        app_name: counting,
                        domain: self.site.as_ref().map(|w| w.domain.clone()),
                        ..Default::default()
                    });
                    self.maybe_flush();
                    return;
                }
                let status = match &self.active {
                    Some(a) => LiveStatus {
                        state: "active".into(),
                        app_name: Some(a.name.clone()),
                        exe_name: Some(a.exe.clone()),
                        domain: self.site.as_ref().map(|w| w.domain.clone()),
                        since: Some(db::fmt_time(a.span.start)),
                        idle_secs,
                    },
                    None => LiveStatus {
                        state: "active".into(),
                        idle_secs,
                        ..Default::default()
                    },
                };
                self.set_status(status);
            }
        }
        self.maybe_flush();
    }

    fn is_limited_app(&self, exe_name: &str) -> bool {
        let exe = exe_name.to_ascii_lowercase();
        self.shared
            .limited
            .lock()
            .map(|l| l.contains(&exe))
            .unwrap_or(false)
    }

    /// A limit on `youtube.com` also covers `m.youtube.com`.
    fn is_limited_site(&self, domain: &str) -> bool {
        self.shared
            .limited
            .lock()
            .map(|l| {
                l.iter()
                    .any(|k| domain == k || domain.ends_with(&format!(".{k}")))
            })
            .unwrap_or(false)
    }

    /// Seconds without "activity": keyboard/mouse input, game-controller input,
    /// or the foreground app playing sound (watching a video, playing a game).
    /// The extra checks only run once keyboard/mouse have been quiet for a while.
    fn effective_idle(&mut self, fg: Option<&ForegroundApp>, now: NaiveDateTime) -> u64 {
        let input = self.probe.idle_seconds();
        if input < MEDIA_CHECK_AFTER_SECS {
            return input;
        }
        let controller = self.probe.controller_idle_seconds();
        let media = match fg {
            Some(app) => {
                let exe = app.exe_name.to_ascii_lowercase();
                if self.probe.audible_exes().contains(&exe) {
                    self.last_audible.insert(exe.clone(), now);
                }
                self.last_audible
                    .get(&exe)
                    .map_or(u64::MAX, |t| (now - *t).num_seconds().max(0) as u64)
            }
            None => u64::MAX,
        };
        input.min(controller).min(media)
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
        if let Some(w) = self.site.take() {
            let end = end.max(w.span.start);
            self.finish(SpanKind::Website, w.span, end);
        }
    }

    /// The domain the browser's extension reports for the foreground browser.
    fn current_site(&self, exe_name: &str, now: NaiveDateTime) -> Option<(String, String)> {
        if !self.shared.website_tracking.load(Ordering::SeqCst) {
            return None;
        }
        let family = browser_family(exe_name)?;
        let reports = self.shared.browsers.lock().ok()?;
        let fresh = |r: &BrowserReport| (now - r.at).num_seconds() <= FRESH_SECS;
        // Prefer the matching browser; otherwise (e.g. an unrecognised
        // Chromium build) fall back to the most recently focused report.
        let (browser, report) = match reports.get(family) {
            Some(r) => (family.to_string(), r),
            None => reports
                .iter()
                .filter(|(_, r)| r.focused && fresh(r))
                .max_by_key(|(_, r)| r.at)
                .map(|(k, r)| (k.clone(), r))?,
        };
        if !fresh(report) {
            return None;
        }
        report.domain.clone().map(|d| (browser, d))
    }

    fn update_site(&mut self, site: Option<(String, String)>, now: NaiveDateTime) {
        let same = matches!((&self.site, &site), (Some(cur), Some((b, d))) if cur.domain == *d && cur.browser == *b);
        if same {
            if let Some(w) = self.site.as_mut() {
                w.span.last = now;
            }
            return;
        }
        if let Some(w) = self.site.take() {
            self.finish(SpanKind::Website, w.span, now);
        }
        if let Some((browser, domain)) = site {
            self.site = self.open_site(&browser, &domain, now);
        }
    }

    fn open_site(&mut self, browser: &str, domain: &str, now: NaiveDateTime) -> Option<SiteSpan> {
        let website_id = match self.sites.get(domain) {
            Some(id) => *id,
            None => match db::get_or_create_website(&self.conn, domain, now) {
                Ok(id) => {
                    self.sites.insert(domain.to_string(), id);
                    id
                }
                Err(e) => {
                    eprintln!("[tracker] website lookup failed: {e}");
                    return None;
                }
            },
        };
        match db::insert_website_session(&self.conn, website_id, browser, now) {
            Ok(id) => Some(SiteSpan {
                span: Span {
                    id,
                    start: now,
                    last: now,
                },
                website_id,
                domain: domain.to_string(),
                browser: browser.to_string(),
            }),
            Err(e) => {
                eprintln!("[tracker] insert website session failed: {e}");
                None
            }
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
                    self.active = Some(ActiveSpan {
                        span: Span {
                            id,
                            start: mid,
                            last: now,
                        },
                        ..a
                    });
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
        if let Some(w) = self.site.as_ref() {
            if w.span.start < mid {
                let w = self.site.take().unwrap();
                self.finish(SpanKind::Website, w.span.clone(), mid);
                if let Ok(id) =
                    db::insert_website_session(&self.conn, w.website_id, &w.browser, mid)
                {
                    self.site = Some(SiteSpan {
                        span: Span {
                            id,
                            start: mid,
                            last: now,
                        },
                        ..w
                    });
                }
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
                span: Span {
                    id,
                    start: now,
                    last: now,
                },
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
            Ok(id) => Some(Span {
                id,
                start,
                last: now,
            }),
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
        if !self.ticks.is_multiple_of(FLUSH_EVERY_TICKS) {
            return;
        }
        if let Some(a) = &self.active {
            let _ = db::update_span(
                &self.conn,
                SpanKind::Usage,
                a.span.id,
                a.span.start,
                a.span.last,
            );
        }
        if let Some(s) = &self.idle {
            let _ = db::update_span(&self.conn, SpanKind::Idle, s.id, s.start, s.last);
        }
        if let Some(w) = &self.site {
            let _ = db::update_span(
                &self.conn,
                SpanKind::Website,
                w.span.id,
                w.span.start,
                w.span.last,
            );
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

    struct Fake {
        app: Option<&'static str>,
        idle: u64,
        controller_idle: u64,
        audible: Vec<&'static str>,
        locked: bool,
    }

    impl Default for Fake {
        fn default() -> Self {
            Fake {
                app: None,
                idle: 0,
                controller_idle: u64::MAX,
                audible: Vec::new(),
                locked: false,
            }
        }
    }

    impl Probe for Fake {
        fn foreground(&mut self) -> Option<ForegroundApp> {
            self.app.map(|n| ForegroundApp {
                exe_name: n.into(),
                exe_path: format!("C:\\{n}"),
            })
        }
        fn idle_seconds(&mut self) -> u64 {
            self.idle
        }
        fn controller_idle_seconds(&mut self) -> u64 {
            self.controller_idle
        }
        fn audible_exes(&mut self) -> HashSet<String> {
            self.audible.iter().map(|s| s.to_string()).collect()
        }
        fn locked(&mut self) -> bool {
            self.locked
        }
        fn describe(&mut self, _: &str) -> Option<String> {
            None
        }
    }

    fn at(d: u32, h: u32, m: u32, s: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, d)
            .unwrap()
            .and_hms_opt(h, m, s)
            .unwrap()
    }

    fn setup(start: NaiveDateTime) -> Tracker<Fake> {
        let conn = db::open_in_memory().unwrap();
        Tracker::new(
            conn,
            Arc::new(Shared::new(false, 300)),
            Fake::default(),
            start,
        )
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
            .query_row(
                "SELECT COALESCE(SUM(duration_secs),0) FROM idle_periods",
                [],
                |r| r.get(0),
            )
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
    fn pausing_doesnt_stop_limited_apps_and_sites_from_counting() {
        // #4: pausing used to stop everything, so limits could never be reached.
        let mut t = setup(at(18, 9, 0, 0));
        t.shared
            .limited
            .lock()
            .unwrap()
            .extend(["steam.exe".to_string(), "youtube.com".to_string()]);
        t.shared.paused.store(true, Ordering::SeqCst);

        t.probe.app = Some("steam.exe"); // limited app: counts
        run(&mut t, at(18, 9, 0, 0), at(18, 9, 10, 0));
        assert_eq!(t.shared.status().state, "paused");
        assert_eq!(t.shared.status().app_name.as_deref(), Some("Steam"));

        t.probe.app = Some("code.exe"); // not limited: private while paused
        run(&mut t, at(18, 9, 10, 1), at(18, 9, 20, 0));

        t.probe.app = Some("chrome.exe"); // browser on a limited site (subdomain)
        let mut cur = at(18, 9, 20, 1);
        while cur <= at(18, 9, 30, 0) {
            report(&t, "chrome", Some("m.youtube.com"), cur);
            t.tick(cur);
            cur += Duration::seconds(1);
        }
        t.close_all(at(18, 9, 30, 0));

        let apps = usage(&t);
        assert_eq!(apps.len(), 1, "only Steam is recorded as an app: {apps:?}");
        assert_eq!((apps[0].0.as_str(), apps[0].2), ("steam.exe", 601));
        assert_eq!(
            sites(&t),
            vec![("m.youtube.com".to_string(), "chrome".to_string(), 599)]
        );
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

    fn report(t: &Tracker<Fake>, browser: &str, domain: Option<&str>, at: NaiveDateTime) {
        t.shared.browsers.lock().unwrap().insert(
            browser.into(),
            BrowserReport {
                domain: domain.map(String::from),
                focused: true,
                at,
            },
        );
    }

    fn sites(t: &Tracker<Fake>) -> Vec<(String, String, i64)> {
        let mut stmt = t
            .conn
            .prepare(
                "SELECT w.domain, s.browser, s.duration_secs FROM website_sessions s
                 JOIN websites w ON w.id = s.website_id ORDER BY s.id",
            )
            .unwrap();
        let rows: Vec<(String, String, i64)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        rows
    }

    #[test]
    fn browser_time_is_attributed_to_reported_domain() {
        let mut t = setup(at(18, 9, 0, 0));
        t.probe.app = Some("brave.exe");
        report(&t, "brave", Some("github.com"), at(18, 9, 0, 0));
        run(&mut t, at(18, 9, 0, 0), at(18, 9, 1, 0));
        report(&t, "brave", Some("youtube.com"), at(18, 9, 1, 1));
        run(&mut t, at(18, 9, 1, 1), at(18, 9, 2, 0));
        assert_eq!(t.shared.status().domain.as_deref(), Some("youtube.com"));
        // Switch to a non-browser app: website time stops.
        t.probe.app = Some("code.exe");
        run(&mut t, at(18, 9, 2, 1), at(18, 9, 3, 0));
        t.close_all(at(18, 9, 3, 0));

        let rows = sites(&t);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0], ("github.com".into(), "brave".into(), 61));
        assert_eq!(rows[1], ("youtube.com".into(), "brave".into(), 60));
    }

    #[test]
    fn stale_or_disabled_reports_are_ignored() {
        let mut t = setup(at(18, 9, 0, 0));
        t.probe.app = Some("chrome.exe");
        report(&t, "chrome", Some("reddit.com"), at(18, 8, 0, 0)); // an hour old
        run(&mut t, at(18, 9, 0, 0), at(18, 9, 0, 30));
        assert!(sites(&t).is_empty());

        report(&t, "chrome", Some("reddit.com"), at(18, 9, 0, 31));
        t.shared.website_tracking.store(false, Ordering::SeqCst);
        run(&mut t, at(18, 9, 0, 31), at(18, 9, 1, 0));
        assert!(sites(&t).is_empty());
        let total: i64 = usage(&t).iter().map(|r| r.2).sum();
        assert!(total > 0, "app time is still tracked");
    }

    #[test]
    fn website_time_stops_when_idle() {
        let mut t = setup(at(18, 9, 0, 0));
        t.probe.app = Some("msedge.exe");
        let mut cur = at(18, 9, 0, 0);
        while cur <= at(18, 9, 10, 0) {
            report(&t, "edge", Some("netflix.com"), cur);
            t.probe.idle = ((cur - at(18, 9, 0, 0)).num_seconds() - 120).max(0) as u64;
            t.tick(cur);
            cur += Duration::seconds(1);
        }
        t.close_all(at(18, 9, 10, 0));
        // Last input at 09:02:00; the idle threshold is reached at 09:07:00
        // and website time is trimmed back to the last input.
        assert_eq!(sites(&t)[0].2, 120);
    }

    /// Runs `from..=to`, with keyboard/mouse quiet since `quiet_from`; `audible`
    /// decides each second whether the foreground app is playing sound.
    fn run_quiet(
        t: &mut Tracker<Fake>,
        from: NaiveDateTime,
        to: NaiveDateTime,
        quiet_from: NaiveDateTime,
        audible: impl Fn(NaiveDateTime) -> bool,
    ) {
        let mut cur = from;
        while cur <= to {
            t.probe.idle = (cur - quiet_from).num_seconds().max(0) as u64;
            let app = t.probe.app.unwrap_or("");
            t.probe.audible = if audible(cur) { vec![app] } else { vec![] };
            report(t, "edge", Some("netflix.com"), cur);
            t.tick(cur);
            cur += Duration::seconds(1);
        }
    }

    #[test]
    fn watching_a_video_counts_as_active() {
        // #1: a 30-minute video with no keyboard/mouse input is all active time.
        let mut t = setup(at(18, 20, 0, 0));
        t.probe.app = Some("msedge.exe");
        run_quiet(
            &mut t,
            at(18, 20, 0, 0),
            at(18, 20, 30, 0),
            at(18, 20, 0, 0),
            |_| true,
        );
        t.close_all(at(18, 20, 30, 0));
        assert_eq!(usage(&t)[0].2, 30 * 60);
        assert_eq!(sites(&t)[0].2, 30 * 60);
        assert_eq!(idle_total(&t), 0);
    }

    #[test]
    fn quiet_scenes_dont_end_a_video_but_stopping_does() {
        let mut t = setup(at(18, 20, 0, 0));
        t.probe.app = Some("msedge.exe");
        // Sound until 20:10 with a 2-minute silent scene at 20:05; then the video ends.
        let playing = |c: NaiveDateTime| {
            c < at(18, 20, 10, 0) && !(c >= at(18, 20, 5, 0) && c < at(18, 20, 7, 0))
        };
        run_quiet(
            &mut t,
            at(18, 20, 0, 0),
            at(18, 20, 20, 0),
            at(18, 20, 0, 0),
            playing,
        );
        t.close_all(at(18, 20, 20, 0));
        // Active until the sound stopped at 20:10 (last heard 20:09:59), idle after.
        let active: i64 = usage(&t).iter().map(|r| r.2).sum();
        assert_eq!(active, 10 * 60 - 1);
        assert_eq!(idle_total(&t), 10 * 60 + 1);
    }

    #[test]
    fn background_music_from_another_app_doesnt_count() {
        let mut t = setup(at(18, 20, 0, 0));
        t.probe.app = Some("code.exe");
        let mut cur = at(18, 20, 0, 0);
        while cur <= at(18, 20, 10, 0) {
            t.probe.idle = (cur - at(18, 20, 0, 0)).num_seconds() as u64;
            t.probe.audible = vec!["spotify.exe"];
            t.tick(cur);
            cur += Duration::seconds(1);
        }
        t.close_all(at(18, 20, 10, 0));
        let active: i64 = usage(&t).iter().map(|r| r.2).sum();
        assert_eq!(active, 0, "trimmed back to the last input");
        assert!(idle_total(&t) > 0);
    }

    #[test]
    fn controller_input_counts_as_active() {
        // #3: playing with a gamepad (keyboard/mouse untouched, no sound).
        let mut t = setup(at(18, 21, 0, 0));
        t.probe.app = Some("game.exe");
        let mut cur = at(18, 21, 0, 0);
        while cur <= at(18, 21, 20, 0) {
            t.probe.idle = (cur - at(18, 21, 0, 0)).num_seconds() as u64;
            t.probe.controller_idle = 2;
            t.tick(cur);
            cur += Duration::seconds(1);
        }
        t.close_all(at(18, 21, 20, 0));
        assert_eq!(usage(&t)[0].2, 20 * 60);
        assert_eq!(idle_total(&t), 0);
    }

    #[test]
    fn flushes_open_session_periodically() {
        let mut t = setup(at(18, 9, 0, 0));
        t.probe.app = Some("code.exe");
        run(&mut t, at(18, 9, 0, 0), at(18, 9, 0, 59));
        // No close_all: simulates a crash. At most FLUSH_EVERY_TICKS lost.
        let secs = usage(&t)[0].2;
        assert!(
            secs >= 59 - FLUSH_EVERY_TICKS as i64 && secs <= 59,
            "got {secs}"
        );
    }
}
