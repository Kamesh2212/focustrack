//! Usage limits, break reminders and desktop notifications (SRS 2.5, 2.6, 2.9).
//!
//! Every 15 s: compute today's usage for each enabled limit, send a Windows
//! notification when it crosses the warning threshold or the limit (once per
//! limit per day, remembered in `sent_notifications`), and refresh the list of
//! domains the browser extensions should block.

use crate::analytics;
use crate::db;
use crate::tracker::Shared;
use chrono::{Local, NaiveDateTime, Timelike};
use rusqlite::{params, Connection};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

const CHECK_EVERY: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LimitStatus {
    pub id: i64,
    /// `app` | `website`
    pub target_type: String,
    pub target_id: i64,
    pub name: String,
    pub color: String,
    pub daily_limit_secs: i64,
    pub warn_pct: i64,
    pub block: bool,
    pub enabled: bool,
    pub used_secs: i64,
}

impl LimitStatus {
    pub fn exceeded(&self) -> bool {
        self.used_secs >= self.daily_limit_secs
    }
    pub fn warned(&self) -> bool {
        self.used_secs * 100 >= self.daily_limit_secs * self.warn_pct
    }
}

/// All limits with today's usage.
pub fn limit_status(conn: &Connection, day: &str) -> rusqlite::Result<Vec<LimitStatus>> {
    let mut stmt = conn.prepare(
        "SELECT l.id, l.target_type, l.target_id,
                COALESCE(a.display_name, w.domain, '(removed)'),
                COALESCE(ca.color, cw.color, '#94a3b8'),
                l.daily_limit_secs, l.warn_pct, l.block, l.enabled,
                CASE l.target_type
                  WHEN 'app' THEN (SELECT COALESCE(SUM(duration_secs), 0) FROM usage_sessions
                                   WHERE app_id = l.target_id AND day = ?1)
                  ELSE (SELECT COALESCE(SUM(duration_secs), 0) FROM website_sessions
                        WHERE website_id = l.target_id AND day = ?1)
                END
         FROM usage_limits l
         LEFT JOIN applications a ON l.target_type = 'app' AND a.id = l.target_id
         LEFT JOIN categories ca ON ca.id = a.category_id
         LEFT JOIN websites w ON l.target_type = 'website' AND w.id = l.target_id
         LEFT JOIN categories cw ON cw.id = w.category_id
         ORDER BY l.id",
    )?;
    let rows = stmt.query_map(params![day], |r| {
        Ok(LimitStatus {
            id: r.get(0)?,
            target_type: r.get(1)?,
            target_id: r.get(2)?,
            name: r.get(3)?,
            color: r.get(4)?,
            daily_limit_secs: r.get(5)?,
            warn_pct: r.get(6)?,
            block: r.get::<_, i64>(7)? != 0,
            enabled: r.get::<_, i64>(8)? != 0,
            used_secs: r.get(9)?,
        })
    })?;
    let out = rows.collect();
    out
}

/// Domains to block right now: enabled website limits with blocking that are used up.
pub fn blocked_domains(limits: &[LimitStatus]) -> Vec<String> {
    limits
        .iter()
        .filter(|l| l.target_type == "website" && l.enabled && l.block && l.exceeded())
        .map(|l| l.name.clone())
        .collect()
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

/// The notification each limit currently calls for (exceeded wins over warn).
pub fn limit_notices(limits: &[LimitStatus]) -> Vec<Notice> {
    let mut out = Vec::new();
    for l in limits.iter().filter(|l| l.enabled && l.daily_limit_secs > 0) {
        if l.exceeded() {
            out.push(Notice {
                key: format!("limit:{}:exceeded", l.id),
                title: format!("Daily limit reached: {}", l.name),
                body: if l.block && l.target_type == "website" {
                    format!("You've used your {} for {} today. It's now blocked until tomorrow.", fmt(l.daily_limit_secs), l.name)
                } else {
                    format!("You've used your {} limit for {} today.", fmt(l.daily_limit_secs), l.name)
                },
            });
        } else if l.warned() {
            out.push(Notice {
                key: format!("limit:{}:warn", l.id),
                title: format!("{}% of your {} limit", l.warn_pct, l.name),
                body: format!(
                    "{} used of {} today — {} left.",
                    fmt(l.used_secs),
                    fmt(l.daily_limit_secs),
                    fmt(l.daily_limit_secs - l.used_secs)
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
    // Walk back from the latest span while gaps are short.
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
        body: format!("You've been using your computer for {} without a break.", fmt(length)),
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

fn check(conn: &Connection, app: &tauri::AppHandle, shared: &Shared) -> rusqlite::Result<()> {
    let now = now();
    let day = analytics::fmt_day(now.date());
    let limits = limit_status(conn, &day)?;

    if let Ok(mut b) = shared.blocklist.lock() {
        *b = blocked_domains(&limits);
    }

    if !shared.notifications_enabled.load(Ordering::SeqCst) || shared.paused.load(Ordering::SeqCst) {
        return Ok(());
    }
    let mut notices = limit_notices(&limits);
    for g in crate::goals::goal_status(conn, &day)? {
        if let Some((key, title, body)) = crate::goals::goal_notice(&g) {
            notices.push(Notice { key, title, body });
        }
    }
    let spans = analytics::active_spans(conn, &day, &day)?;
    notices.extend(break_notice(&spans, now, shared.break_reminder_mins.load(Ordering::SeqCst)));
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
            loop {
                if shared.shutdown.load(Ordering::SeqCst) {
                    break;
                }
                if let Err(e) = check(&conn, &app, &shared) {
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
        NaiveDate::from_ymd_opt(2026, 11, 18).unwrap().and_hms_opt(h, m, 0).unwrap()
    }

    fn limit(kind: &str, used: i64, limit: i64, block: bool) -> LimitStatus {
        LimitStatus {
            id: 7,
            target_type: kind.into(),
            target_id: 1,
            name: "youtube.com".into(),
            color: String::new(),
            daily_limit_secs: limit,
            warn_pct: 80,
            block,
            enabled: true,
            used_secs: used,
        }
    }

    #[test]
    fn warn_then_exceeded() {
        assert!(limit_notices(&[limit("website", 2000, 3600, false)]).is_empty());
        let warn = limit_notices(&[limit("website", 2900, 3600, false)]);
        assert_eq!(warn[0].key, "limit:7:warn");
        assert!(warn[0].body.contains("48m used of 1h"), "{}", warn[0].body);
        let over = limit_notices(&[limit("website", 3600, 3600, true)]);
        assert_eq!(over[0].key, "limit:7:exceeded");
        assert!(over[0].body.contains("blocked"));
    }

    #[test]
    fn only_exceeded_blocking_websites_are_blocked() {
        let ls = [
            limit("website", 3700, 3600, true),
            limit("website", 3700, 3600, false),
            limit("website", 100, 3600, true),
            limit("app", 3700, 3600, true),
        ];
        assert_eq!(blocked_domains(&ls), vec!["youtube.com".to_string()]);
    }

    #[test]
    fn break_reminder_after_long_continuous_use() {
        let spans = [(at(9, 0), at(10, 0)), (at(10, 2), at(11, 5))];
        let n = break_notice(&spans, at(11, 5), 120).expect("2h05m continuous");
        assert_eq!(n.key, "break:2026-11-18 09:00:00");
        assert!(break_notice(&spans, at(11, 5), 180).is_none(), "below threshold");
        assert!(break_notice(&spans, at(12, 0), 120).is_none(), "user already left");
        let with_gap = [(at(9, 0), at(10, 0)), (at(10, 30), at(11, 5))];
        assert!(break_notice(&with_gap, at(11, 5), 60).is_none(), "30m break resets it");
        assert!(break_notice(&spans, at(11, 5), 0).is_none(), "disabled");
    }

    #[test]
    fn status_query_sums_today() {
        let conn = db::open_in_memory().unwrap();
        let (app, _) = db::get_or_create_app(&conn, "steam.exe", "", || "Steam".into(), at(9, 0)).unwrap();
        let id = db::insert_usage(&conn, app, at(9, 0)).unwrap();
        db::update_span(&conn, db::SpanKind::Usage, id, at(9, 0), at(9, 30)).unwrap();
        let site = db::get_or_create_website(&conn, "youtube.com", at(9, 0)).unwrap();
        let w = db::insert_website_session(&conn, site, "chrome", at(10, 0)).unwrap();
        db::update_span(&conn, db::SpanKind::Website, w, at(10, 0), at(10, 50)).unwrap();
        conn.execute("INSERT INTO usage_limits (target_type, target_id, daily_limit_secs, block) VALUES ('app', ?1, 3600, 0)", params![app]).unwrap();
        conn.execute("INSERT INTO usage_limits (target_type, target_id, daily_limit_secs, block) VALUES ('website', ?1, 2700, 1)", params![site]).unwrap();

        let ls = limit_status(&conn, "2026-11-18").unwrap();
        assert_eq!((ls[0].name.as_str(), ls[0].used_secs), ("Steam", 1800));
        assert_eq!((ls[1].name.as_str(), ls[1].used_secs), ("youtube.com", 3000));
        assert_eq!(blocked_domains(&ls), vec!["youtube.com".to_string()]);
    }
}
