//! Usage limits, break reminders, goal reminders and desktop notifications
//! (SRS 2.5, 2.6, 2.9; improvements doc §2, §3).
//!
//! Every 15 s: compute today's usage for each enabled limit and apply its action:
//!   * `notify`          — notifications at the warning level and at the limit
//!   * `block`           — block as soon as the limit is reached
//!   * `warn_then_block` — warning notification, then a 1-minute heads-up, then block
//!
//! Apps are blocked by `blocker.rs`, websites by the browser extension. A limit
//! can be extended by 15 minutes a few times a day (configurable).

use crate::analytics;
use crate::blocker;
use crate::db;
use crate::tracker::Shared;
use chrono::{Duration, Local, NaiveDateTime, NaiveTime, Timelike};
use rusqlite::{params, Connection};
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread;

const CHECK_EVERY: std::time::Duration = std::time::Duration::from_secs(15);
pub const GRACE_SECS: i64 = 60;
pub const OVERRIDE_SECS: i64 = 15 * 60;
pub const ACTIONS: &[&str] = &["notify", "block", "warn_then_block"];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LimitStatus {
    pub id: i64,
    /// `app` | `website`
    pub target_type: String,
    pub target_id: i64,
    pub name: String,
    /// Exe name (apps) or domain (websites).
    pub key: String,
    pub color: String,
    pub daily_limit_secs: i64,
    pub warn_pct: i64,
    /// `notify` | `block` | `warn_then_block`
    pub action: String,
    pub enabled: bool,
    pub used_secs: i64,
    /// Extra time granted by overrides today.
    pub extra_secs: i64,
    pub overrides_used: i64,
    /// App limits: the exe's path (empty if unknown).
    pub path: String,
    /// Launcher limits: the folder whose programs this limit also covers.
    pub scope: Option<String>,
}

impl LimitStatus {
    /// How the blocker and tracker recognise this limit's processes.
    pub fn target(&self) -> crate::identity::Target {
        crate::identity::Target::new(&self.name, &self.key, Some(&self.path))
    }
}

impl LimitStatus {
    pub fn allowed_secs(&self) -> i64 {
        self.daily_limit_secs + self.extra_secs
    }
    pub fn exceeded(&self) -> bool {
        self.used_secs >= self.allowed_secs()
    }
    pub fn warned(&self) -> bool {
        self.used_secs * 100 >= self.allowed_secs() * self.warn_pct
    }
    pub fn blocks(&self) -> bool {
        self.action != "notify"
    }
}

/// All limits with today's usage.
#[cfg(test)]
pub fn limit_status(conn: &Connection, day: &str) -> rusqlite::Result<Vec<LimitStatus>> {
    limit_status_days(conn, day, None)
}

/// Limits right now. After a clock change, usage keeps counting against the
/// real day (#11).
pub fn current_status(conn: &Connection, now: NaiveDateTime) -> rusqlite::Result<Vec<LimitStatus>> {
    let (day, extra) = crate::clock::limit_days(conn, now)?;
    limit_status_days(conn, &day, extra.as_deref())
}

/// The day limits and their "+15 min" extensions count against right now.
pub fn limit_day(conn: &Connection, now: NaiveDateTime) -> rusqlite::Result<String> {
    Ok(crate::clock::limit_days(conn, now)?.0)
}

/// Limit status for `day`; usage recorded under `extra` counts too.
fn limit_status_days(
    conn: &Connection,
    day: &str,
    extra: Option<&str>,
) -> rusqlite::Result<Vec<LimitStatus>> {
    let also = extra.unwrap_or(day);
    let mut stmt = conn.prepare(
        "SELECT l.id, l.target_type, l.target_id,
                COALESCE(a.display_name, w.domain, '(removed)'),
                COALESCE(a.exe_name, w.domain, ''),
                COALESCE(ca.color, cw.color, '#94a3b8'),
                l.daily_limit_secs, l.warn_pct, l.action, l.enabled,
                CASE l.target_type
                  WHEN 'app' THEN (SELECT COALESCE(SUM(duration_secs), 0) FROM usage_sessions
                                   WHERE app_id = l.target_id AND day IN (?1, ?2)
                                     AND device_id = (SELECT value FROM user_settings WHERE key = 'device_id'))
                  -- A limit on youtube.com also counts m.youtube.com (#8).
                  ELSE (SELECT COALESCE(SUM(s.duration_secs), 0) FROM website_sessions s
                        JOIN websites w2 ON w2.id = s.website_id
                        WHERE s.day IN (?1, ?2) AND s.device_id = (SELECT value FROM user_settings WHERE key = 'device_id')
                          AND (w2.domain = w.domain OR w2.domain LIKE '%.' || w.domain))
                END,
                COALESCE(o.extra_secs, 0), COALESCE(o.count, 0), COALESCE(a.exe_path, '')
         FROM usage_limits l
         LEFT JOIN applications a ON l.target_type = 'app' AND a.id = l.target_id
         LEFT JOIN categories ca ON ca.id = a.category_id
         LEFT JOIN websites w ON l.target_type = 'website' AND w.id = l.target_id
         LEFT JOIN categories cw ON cw.id = w.category_id
         LEFT JOIN limit_overrides o ON o.limit_id = l.id AND o.day = ?1
         ORDER BY l.id",
    )?;
    let rows = stmt.query_map(params![day, also], |r| {
        Ok(LimitStatus {
            id: r.get(0)?,
            target_type: r.get(1)?,
            target_id: r.get(2)?,
            name: r.get(3)?,
            key: r.get(4)?,
            color: r.get(5)?,
            daily_limit_secs: r.get(6)?,
            warn_pct: r.get(7)?,
            action: r.get(8)?,
            enabled: r.get::<_, i64>(9)? != 0,
            used_secs: r.get(10)?,
            extra_secs: r.get(11)?,
            overrides_used: r.get(12)?,
            path: r.get(13)?,
            scope: None,
        })
    })?;
    let mut out: Vec<LimitStatus> = rows.collect::<Result<_, _>>()?;
    // Launcher limits also count every program installed under the launcher's
    // folder (HoYoPlay's launcher.exe → GenshinImpact.exe) (#5).
    for l in out.iter_mut().filter(|l| l.target_type == "app") {
        l.scope = crate::identity::scope_dir(&l.key, &l.path);
        if let Some(scope) = &l.scope {
            l.used_secs = conn.query_row(
                "SELECT COALESCE(SUM(s.duration_secs), 0) FROM usage_sessions s
                 JOIN applications a ON a.id = s.app_id
                 WHERE s.day IN (?1, ?4) AND s.device_id = (SELECT value FROM user_settings WHERE key = 'device_id')
                   AND (s.app_id = ?2 OR instr(lower(COALESCE(a.exe_path, '')), ?3) = 1)",
                params![day, l.target_id, scope, also],
                |r| r.get(0),
            )?;
        }
    }
    Ok(out)
}

/// Extends a limit by 15 minutes for today, if overrides remain.
pub fn override_limit(conn: &Connection, id: i64, day: &str, per_day: i64) -> Result<(), String> {
    let used: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(count), 0) FROM limit_overrides WHERE day = ?1",
            params![day],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if used >= per_day {
        return Err(if per_day == 0 {
            "Extending limits is turned off in Settings".into()
        } else {
            format!(
                "You've used all {per_day} extension{} for today",
                if per_day == 1 { "" } else { "s" }
            )
        });
    }
    conn.execute(
        "INSERT INTO limit_overrides (limit_id, day, extra_secs, count) VALUES (?1, ?2, ?3, 1)
         ON CONFLICT(limit_id, day) DO UPDATE SET extra_secs = extra_secs + ?3, count = count + 1",
        params![id, day, OVERRIDE_SECS],
    )
    .map_err(|e| e.to_string())?;
    // Allow the "reached" notification and grace period to happen again.
    conn.execute(
        "DELETE FROM sent_notifications WHERE key LIKE ?1 AND day = ?2",
        params![format!("limit:{id}:%"), day],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// When each blocking limit starts being enforced: immediately for `block`,
/// after a one-minute heads-up for `warn_then_block`.
pub fn enforcement_times(
    limits: &[LimitStatus],
    first_reached: &mut HashMap<(i64, i64), NaiveDateTime>,
    now: NaiveDateTime,
) -> Vec<(LimitStatus, NaiveDateTime)> {
    let mut out = Vec::new();
    for l in limits.iter().filter(|l| l.enabled && l.blocks()) {
        let key = (l.id, l.extra_secs);
        if !l.exceeded() {
            first_reached.remove(&key);
            continue;
        }
        let reached = *first_reached.entry(key).or_insert(now);
        let from = if l.action == "warn_then_block" {
            reached + Duration::seconds(GRACE_SECS)
        } else {
            reached
        };
        out.push((l.clone(), from));
    }
    out
}

#[derive(Debug, Clone, PartialEq)]
pub struct Notice {
    pub key: String,
    pub title: String,
    pub body: String,
}

fn fmt(secs: i64) -> String {
    let (h, m) = (secs / 3600, (secs % 3600) / 60);
    match (h, m) {
        (0, m) => format!("{m}m"),
        (h, 0) => format!("{h}h"),
        (h, m) => format!("{h}h {m}m"),
    }
}

/// The notification each limit currently calls for.
pub fn limit_notices(limits: &[LimitStatus]) -> Vec<Notice> {
    let mut out = Vec::new();
    for l in limits
        .iter()
        .filter(|l| l.enabled && l.daily_limit_secs > 0)
    {
        let kind = if l.target_type == "app" {
            "app"
        } else {
            "site"
        };
        if l.exceeded() {
            let (title, body) = match l.action.as_str() {
                "notify" => (
                    format!("Daily limit reached: {}", l.name),
                    format!("You've used your {} limit for {} today.", fmt(l.allowed_secs()), l.name),
                ),
                "warn_then_block" => (
                    format!("{} closes in 1 minute", l.name),
                    format!("You've reached your {} limit. Save your work — the {kind} will be blocked until tomorrow.", fmt(l.allowed_secs())),
                ),
                _ => (
                    format!("{} is blocked for today", l.name),
                    format!("You've reached your {} limit for {}.", fmt(l.allowed_secs()), l.name),
                ),
            };
            out.push(Notice {
                key: format!("limit:{}:exceeded", l.id),
                title,
                body,
            });
        } else if l.warned() {
            out.push(Notice {
                key: format!("limit:{}:warn", l.id),
                title: format!("{}% of your {} limit", l.warn_pct, l.name),
                body: format!(
                    "{} used of {} today — {} left.",
                    fmt(l.used_secs),
                    fmt(l.allowed_secs()),
                    fmt(l.allowed_secs() - l.used_secs)
                ),
            });
        }
    }
    out
}

/// "You've been using your computer for 2h" once per continuous session.
pub fn break_notice(
    spans: &[(NaiveDateTime, NaiveDateTime)],
    now: NaiveDateTime,
    reminder_mins: u64,
) -> Option<Notice> {
    if reminder_mins == 0 || spans.is_empty() {
        return None;
    }
    let mut start = spans.last()?.0;
    let end = spans.iter().map(|s| s.1).max()?;
    for w in spans.windows(2).rev() {
        if (w[1].0 - w[0].1).num_seconds() > analytics::SESSION_GAP_SECS {
            break;
        }
        start = w[0].0;
    }
    let ongoing = (now - end).num_seconds() <= 90;
    let length = (end - start).num_seconds();
    if !ongoing || length < reminder_mins as i64 * 60 {
        return None;
    }
    Some(Notice {
        key: format!("break:{}", db::fmt_time(start)),
        title: "Time for a break?".into(),
        body: format!(
            "You've been using your computer for {} without a break.",
            fmt(length)
        ),
    })
}

/// Evening reminder listing goals that aren't done yet.
pub fn goal_reminder(
    goals: &[crate::goals::GoalStatus],
    now: NaiveDateTime,
    at: &str,
) -> Option<Notice> {
    let time = NaiveTime::parse_from_str(at, "%H:%M").ok()?;
    if now.time() < time {
        return None;
    }
    let pending: Vec<&str> = goals
        .iter()
        .filter(|g| g.enabled && g.state == "in_progress")
        .map(|g| g.label.as_str())
        .collect();
    if pending.is_empty() {
        return None;
    }
    let shown = pending
        .iter()
        .take(3)
        .copied()
        .collect::<Vec<_>>()
        .join(" · ");
    let more = if pending.len() > 3 {
        format!(" and {} more", pending.len() - 3)
    } else {
        String::new()
    };
    Some(Notice {
        key: "goals:reminder".into(),
        title: format!(
            "{} goal{} left today",
            pending.len(),
            if pending.len() == 1 { "" } else { "s" }
        ),
        body: format!("{shown}{more}"),
    })
}

fn now() -> NaiveDateTime {
    let t = Local::now().naive_local();
    t.with_nanosecond(0).unwrap_or(t)
}

pub fn notify(app: &tauri::AppHandle, title: &str, body: &str) {
    use tauri_plugin_notification::NotificationExt;
    if let Err(e) = app.notification().builder().title(title).body(body).show() {
        eprintln!("[limits] notification failed: {e}");
    }
}

fn check(
    conn: &Connection,
    app: &tauri::AppHandle,
    shared: &Shared,
    first_reached: &mut HashMap<(i64, i64), NaiveDateTime>,
) -> rusqlite::Result<()> {
    let now = now();
    let (day, extra) = crate::clock::limit_days(conn, now)?;
    let limits = limit_status_days(conn, &day, extra.as_deref())?;

    if let Ok(mut l) = shared.limited.lock() {
        *l = limits
            .iter()
            .filter(|l| l.enabled && l.target_type == "website" && !l.key.is_empty())
            .map(|l| l.key.to_ascii_lowercase())
            .collect();
    }
    if let Ok(mut l) = shared.limited_apps.lock() {
        *l = limits
            .iter()
            .filter(|l| l.enabled && l.target_type == "app" && !l.key.is_empty())
            .map(LimitStatus::target)
            .collect();
    }

    // Blocking applies even when notifications are off.
    let enforced = enforcement_times(&limits, first_reached, now);
    if let Ok(mut b) = shared.blocklist.lock() {
        *b = enforced
            .iter()
            .filter(|(l, from)| l.target_type == "website" && now >= *from)
            .map(|(l, _)| l.key.clone())
            .collect();
    }
    if let Ok(mut b) = shared.app_blocks.lock() {
        b.limit = enforced
            .iter()
            .filter(|(l, _)| {
                l.target_type == "app" && !blocker::is_protected_name(&l.key.to_ascii_lowercase())
            })
            .map(|(l, from)| (l.target(), *from))
            .collect();
    }

    if !shared.notifications_enabled.load(Ordering::SeqCst) {
        return Ok(());
    }
    let mut notices = limit_notices(&limits);
    // Paused: limits still apply (and notify); other reminders wait.
    if shared.paused.load(Ordering::SeqCst) {
        for n in notices {
            if db::mark_notified(conn, &n.key, &day)? {
                notify(app, &n.title, &n.body);
            }
        }
        return Ok(());
    }
    let goals = crate::goals::goal_status(conn, &day)?;
    for g in &goals {
        if let Some((key, title, body)) = crate::goals::goal_notice(g) {
            notices.push(Notice { key, title, body });
        }
    }
    let settings = db::load_settings(conn)?;
    notices.extend(goal_reminder(&goals, now, &settings.goal_reminder));
    // Break reminders are about this computer, not the phone.
    let spans = analytics::own_active_spans(conn, &day, &day)?;
    notices.extend(break_notice(
        &spans,
        now,
        shared.break_reminder_mins.load(Ordering::SeqCst),
    ));
    for n in notices {
        if db::mark_notified(conn, &n.key, &day)? {
            notify(app, &n.title, &n.body);
        }
    }
    Ok(())
}

pub fn spawn(app: tauri::AppHandle, db_path: PathBuf, shared: Arc<Shared>) {
    thread::Builder::new()
        .name("focustrack-limits".into())
        .spawn(move || {
            let conn = match db::open(&db_path) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("[limits] cannot open database: {e}");
                    return;
                }
            };
            let mut first_reached = HashMap::new();
            let mut day = String::new();
            let mut watch = crate::clock::Watch::default();
            loop {
                if shared.shutdown.load(Ordering::SeqCst) {
                    break;
                }
                if let Some(secs) = watch.observe(chrono::Utc::now(), crate::platform::uptime_ms())
                {
                    eprintln!("[limits] system clock moved by {secs} s");
                    if let Err(e) = crate::clock::record(&conn, now(), secs) {
                        eprintln!("[limits] could not record the clock change: {e}");
                    }
                }
                match limit_day(&conn, now()) {
                    Ok(d) if d != day => {
                        day = d;
                        first_reached.clear();
                    }
                    _ => {}
                }
                if let Err(e) = check(&conn, &app, &shared, &mut first_reached) {
                    eprintln!("[limits] check failed: {e}");
                }
                thread::sleep(CHECK_EVERY);
            }
        })
        .expect("failed to spawn limits thread");
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn at(h: u32, m: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, 18)
            .unwrap()
            .and_hms_opt(h, m, 0)
            .unwrap()
    }

    fn limit(kind: &str, used: i64, limit: i64, action: &str) -> LimitStatus {
        LimitStatus {
            id: 7,
            target_type: kind.into(),
            target_id: 1,
            name: "youtube.com".into(),
            key: "youtube.com".into(),
            color: String::new(),
            daily_limit_secs: limit,
            warn_pct: 80,
            action: action.into(),
            enabled: true,
            used_secs: used,
            extra_secs: 0,
            overrides_used: 0,
            path: String::new(),
            scope: None,
        }
    }

    #[test]
    fn notices_follow_the_action() {
        assert!(limit_notices(&[limit("website", 2000, 3600, "notify")]).is_empty());
        let warn = limit_notices(&[limit("website", 2900, 3600, "notify")]);
        assert_eq!(warn[0].key, "limit:7:warn");
        assert!(warn[0].body.contains("48m used of 1h"), "{}", warn[0].body);
        // "Warn me at" applies to every action, including "block at the limit".
        assert_eq!(
            limit_notices(&[limit("website", 2900, 3600, "block")])[0].key,
            "limit:7:warn"
        );
        assert_eq!(
            limit_notices(&[limit("app", 2900, 3600, "warn_then_block")])[0].key,
            "limit:7:warn"
        );
        let over = limit_notices(&[limit("app", 3600, 3600, "warn_then_block")]);
        assert!(over[0].title.contains("closes in 1 minute"));
        assert!(limit_notices(&[limit("app", 3600, 3600, "block")])[0]
            .title
            .contains("blocked"));
    }

    #[test]
    fn enforcement_waits_for_grace_and_respects_overrides() {
        let mut first = HashMap::new();
        let ls = [
            limit("app", 3700, 3600, "warn_then_block"),
            limit("app", 3700, 3600, "notify"),
        ];
        let e = enforcement_times(&ls, &mut first, at(10, 0));
        assert_eq!(e.len(), 1, "notify-only limits never block");
        assert_eq!(e[0].1, at(10, 1));
        // Next check keeps the original reach time.
        assert_eq!(
            enforcement_times(&ls, &mut first, at(10, 0) + Duration::seconds(15))[0].1,
            at(10, 1)
        );
        // An override (+15 min) un-blocks until the new allowance is used.
        let extended = LimitStatus {
            extra_secs: 900,
            ..ls[0].clone()
        };
        assert!(enforcement_times(&[extended], &mut first, at(10, 2)).is_empty());
        assert_eq!(
            enforcement_times(
                &[limit("app", 3600, 3600, "block")],
                &mut HashMap::new(),
                at(11, 0)
            )[0]
            .1,
            at(11, 0)
        );
    }

    #[test]
    fn overrides_are_capped_per_day() {
        let conn = db::open_in_memory().unwrap();
        override_limit(&conn, 1, "2026-11-18", 2).unwrap();
        override_limit(&conn, 2, "2026-11-18", 2).unwrap();
        assert!(override_limit(&conn, 1, "2026-11-18", 2)
            .unwrap_err()
            .contains("all 2"));
        assert!(override_limit(&conn, 1, "2026-11-18", 0)
            .unwrap_err()
            .contains("turned off"));
        let extra: i64 = conn
            .query_row(
                "SELECT extra_secs FROM limit_overrides WHERE limit_id = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(extra, 900);
    }

    #[test]
    fn break_reminder_after_long_continuous_use() {
        let spans = [(at(9, 0), at(10, 0)), (at(10, 2), at(11, 5))];
        let n = break_notice(&spans, at(11, 5), 120).expect("2h05m continuous");
        assert_eq!(n.key, "break:2026-11-18 09:00:00");
        assert!(break_notice(&spans, at(11, 5), 180).is_none());
        assert!(break_notice(&spans, at(12, 0), 120).is_none());
        let with_gap = [(at(9, 0), at(10, 0)), (at(10, 30), at(11, 5))];
        assert!(break_notice(&with_gap, at(11, 5), 60).is_none());
        assert!(break_notice(&spans, at(11, 5), 0).is_none());
    }

    #[test]
    fn moving_the_clock_forward_does_not_reset_limits() {
        let conn = db::open_in_memory().unwrap();
        let (app, _) =
            db::get_or_create_app(&conn, "steam.exe", "", || "Steam".into(), at(9, 0)).unwrap();
        conn.execute(
            "INSERT INTO usage_limits (target_type, target_id, daily_limit_secs, action) VALUES ('app', ?1, 1800, 'block')",
            params![app],
        )
        .unwrap();
        // 20 minutes on the 18th, then the clock is moved forward a day.
        let id = db::insert_usage(&conn, app, at(9, 0)).unwrap();
        db::update_span(&conn, db::SpanKind::Usage, id, at(9, 0), at(9, 20)).unwrap();
        let tomorrow = |h, m| at(h, m) + chrono::Duration::days(1);
        crate::clock::record(&conn, tomorrow(9, 20), 86_400).unwrap();
        // 15 more minutes, recorded under the 19th.
        let id = db::insert_usage(&conn, app, tomorrow(9, 20)).unwrap();
        db::update_span(
            &conn,
            db::SpanKind::Usage,
            id,
            tomorrow(9, 20),
            tomorrow(9, 35),
        )
        .unwrap();

        let ls = current_status(&conn, tomorrow(9, 35)).unwrap();
        assert_eq!(ls[0].used_secs, 35 * 60);
        assert!(ls[0].exceeded(), "still the real 18th");
        assert_eq!(limit_day(&conn, tomorrow(9, 35)).unwrap(), "2026-11-18");

        // Real midnight (the clock shows the 20th): a new day, as usual.
        let next = at(0, 5) + chrono::Duration::days(2);
        assert_eq!(current_status(&conn, next).unwrap()[0].used_secs, 0);
    }

    #[test]
    fn status_query_sums_today_with_overrides() {
        let conn = db::open_in_memory().unwrap();
        let (app, _) =
            db::get_or_create_app(&conn, "steam.exe", "", || "Steam".into(), at(9, 0)).unwrap();
        let id = db::insert_usage(&conn, app, at(9, 0)).unwrap();
        db::update_span(&conn, db::SpanKind::Usage, id, at(9, 0), at(9, 30)).unwrap();
        conn.execute(
            "INSERT INTO usage_limits (target_type, target_id, daily_limit_secs, action) VALUES ('app', ?1, 1800, 'block')",
            params![app],
        )
        .unwrap();
        let ls = limit_status(&conn, "2026-11-18").unwrap();
        assert_eq!(
            (ls[0].name.as_str(), ls[0].key.as_str(), ls[0].used_secs),
            ("Steam", "steam.exe", 1800)
        );
        assert!(ls[0].exceeded());
        override_limit(&conn, ls[0].id, "2026-11-18", 1).unwrap();
        let ls = limit_status(&conn, "2026-11-18").unwrap();
        assert_eq!((ls[0].extra_secs, ls[0].overrides_used), (900, 1));
        assert!(!ls[0].exceeded());
    }

    #[test]
    fn goal_reminder_lists_pending_goals_after_set_time() {
        let g = |label: &str, state: &str| crate::goals::GoalStatus {
            id: 1,
            kind: "custom".into(),
            category_id: None,
            category: None,
            color: String::new(),
            target: 0,
            title: Some(label.into()),
            enabled: true,
            current: 0,
            state: state.into(),
            label: label.into(),
        };
        let goals = [g("Read 20 pages", "in_progress"), g("Exercise", "achieved")];
        assert!(goal_reminder(&goals, at(19, 0), "20:00").is_none());
        let n = goal_reminder(&goals, at(20, 5), "20:00").unwrap();
        assert_eq!(n.title, "1 goal left today");
        assert_eq!(n.body, "Read 20 pages");
        assert!(goal_reminder(&goals, at(21, 0), "").is_none(), "off");
    }

    #[test]
    fn launcher_limits_count_their_games_and_site_limits_their_subdomains() {
        let conn = db::open_in_memory().unwrap();
        let (launcher, _) = db::get_or_create_app(
            &conn,
            "launcher.exe",
            r"C:\Program Files\HoYoPlay\launcher.exe",
            || "x".into(),
            at(9, 0),
        )
        .unwrap();
        let (game, _) = db::get_or_create_app(
            &conn,
            "GenshinImpact.exe",
            r"C:\Program Files\HoYoPlay\games\Genshin Impact game\GenshinImpact.exe",
            || "Genshin Impact".into(),
            at(9, 0),
        )
        .unwrap();
        for (app, h, mins) in [(launcher, 9, 1), (game, 10, 40)] {
            let id = db::insert_usage(&conn, app, at(h, 0)).unwrap();
            db::update_span(&conn, db::SpanKind::Usage, id, at(h, 0), at(h, mins)).unwrap();
        }
        let yt = db::get_or_create_website(&conn, "youtube.com", at(9, 0)).unwrap();
        let m = db::get_or_create_website(&conn, "m.youtube.com", at(9, 0)).unwrap();
        for (site, h) in [(yt, 11), (m, 12)] {
            let id = db::insert_website_session(&conn, site, "chrome", at(h, 0)).unwrap();
            db::update_span(&conn, db::SpanKind::Website, id, at(h, 0), at(h, 10)).unwrap();
        }
        conn.execute("INSERT INTO usage_limits (target_type, target_id, daily_limit_secs, action) VALUES ('app', ?1, 1800, 'block')", params![launcher]).unwrap();
        conn.execute("INSERT INTO usage_limits (target_type, target_id, daily_limit_secs, action) VALUES ('website', ?1, 3600, 'notify')", params![yt]).unwrap();

        let ls = limit_status(&conn, "2026-11-18").unwrap();
        assert_eq!(ls[0].used_secs, 41 * 60, "launcher + game");
        assert!(ls[0].scope.as_deref().unwrap().ends_with(r"\hoyoplay\"));
        assert!(ls[0].exceeded());
        assert!(ls[0].target().matches(
            "genshinimpact.exe",
            Some(r"c:\program files\hoyoplay\games\genshin impact game\genshinimpact.exe"),
            None
        ));
        assert_eq!(ls[1].used_secs, 20 * 60, "youtube.com + m.youtube.com");
    }
}
