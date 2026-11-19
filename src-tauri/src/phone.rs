//! Phone screen time (#68): turns the operating system's foreground events
//! (Android usage access, via the `focustrack-usage` plugin) into usage
//! sessions in the same tables the Windows tracker writes. From there the
//! dashboard, reports, goals and sync treat phone usage like any other.
//!
//! Android keeps about a week of events, so FocusTrack imports whenever the
//! app runs (on start, when it comes back to the front, and every minute
//! while open) and remembers how far it got.

use crate::db;
use chrono::{Local, NaiveDateTime, TimeZone};
use rusqlite::Connection;
use tauri_plugin_focustrack_usage::{kind, UsageEvent};

/// How far back the first import looks.
const FIRST_IMPORT_DAYS: i64 = 7;
/// Ignored: the notification shade and similar system surfaces.
const IGNORED: &[&str] = &["com.android.systemui"];

/// An app in the foreground from `start` to `end` (Unix milliseconds).
#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    pub package: String,
    pub start: i64,
    pub end: i64,
}

/// Foreground spans from events (sorted by time). Also returns the start of
/// a span still open at the end, so the next import can pick it up from there.
pub fn spans(events: &[UsageEvent]) -> (Vec<Span>, Option<i64>) {
    let mut out = Vec::new();
    let mut current: Option<(String, i64)> = None;
    let mut close = |current: &mut Option<(String, i64)>, at: i64| {
        if let Some((package, start)) = current.take() {
            if at - start >= 1000 {
                out.push(Span {
                    package,
                    start,
                    end: at,
                });
            }
        }
    };
    for e in events {
        match e.kind {
            kind::ACTIVITY_RESUMED if !IGNORED.contains(&e.package.as_str()) => {
                if current.as_ref().is_some_and(|(p, _)| *p == e.package) {
                    continue; // another screen of the same app
                }
                close(&mut current, e.time);
                current = Some((e.package.clone(), e.time));
            }
            kind::ACTIVITY_PAUSED => {
                if current.as_ref().is_some_and(|(p, _)| *p == e.package) {
                    close(&mut current, e.time);
                }
            }
            kind::SCREEN_NON_INTERACTIVE | kind::KEYGUARD_SHOWN => close(&mut current, e.time),
            _ => {}
        }
    }
    let open = current.map(|(_, start)| start);
    (out, open)
}

fn local(ms: i64) -> Option<NaiveDateTime> {
    Local
        .timestamp_millis_opt(ms)
        .single()
        .map(|t| t.naive_local().with_nanosecond_zero())
}

trait NoNanos {
    fn with_nanosecond_zero(self) -> Self;
}

impl NoNanos for NaiveDateTime {
    fn with_nanosecond_zero(self) -> Self {
        use chrono::Timelike;
        self.with_nanosecond(0).unwrap_or(self)
    }
}

/// Writes spans as usage sessions, split at midnight like the desktop tracker.
pub fn record(
    conn: &Connection,
    spans: &[Span],
    labels: &std::collections::HashMap<String, String>,
) -> rusqlite::Result<usize> {
    let mut written = 0;
    for s in spans {
        let (Some(mut start), Some(end)) = (local(s.start), local(s.end)) else {
            continue;
        };
        let name = labels
            .get(&s.package)
            .cloned()
            .unwrap_or_else(|| s.package.clone());
        let (app, _) = db::get_or_create_app(conn, &s.package, "", || name, start)?;
        while start < end {
            let midnight =
                (start.date() + chrono::Duration::days(1)).and_time(chrono::NaiveTime::MIN);
            let part_end = end.min(midnight);
            let id = db::insert_usage(conn, app, start)?;
            db::update_span(conn, db::SpanKind::Usage, id, start, part_end)?;
            written += 1;
            start = part_end;
        }
    }
    Ok(written)
}

/// Imports new phone usage. Returns how many sessions were added, or `None`
/// when usage access isn't granted.
pub fn import<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    conn: &Connection,
) -> Result<Option<usize>, String> {
    use tauri_plugin_focustrack_usage::UsageExt;
    let usage = app.usage();
    if !usage.has_access()? {
        return Ok(None);
    }
    let now = Local::now().timestamp_millis();
    let since = db::get_setting(conn, "phone_usage_since")
        .map_err(|e| e.to_string())?
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(now - FIRST_IMPORT_DAYS * 86_400_000);
    let mut got = usage.events(since, now)?;
    got.events.sort_by_key(|e| e.time);
    let (spans, open) = spans(&got.events);
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    let n = record(&tx, &spans, &got.labels).map_err(|e| e.to_string())?;
    // Next time, start from the app still in front (it isn't recorded yet).
    let next = open.unwrap_or(now).max(since);
    db::set_setting(&tx, "phone_usage_since", &next.to_string()).map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(Some(n))
}

/// Imports every minute while the app runs (and right away on start).
#[cfg(mobile)]
pub fn spawn<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    db_path: std::path::PathBuf,
    sync: std::sync::Arc<crate::sync::SyncHandle>,
) {
    std::thread::Builder::new()
        .name("focustrack-phone-usage".into())
        .spawn(move || {
            let conn = match db::open(&db_path) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("[phone] cannot open database: {e}");
                    return;
                }
            };
            loop {
                match import(&app, &conn) {
                    Ok(Some(n)) if n > 0 => sync.wake(),
                    Ok(_) => {}
                    Err(e) => eprintln!("[phone] usage import failed: {e}"),
                }
                std::thread::sleep(std::time::Duration::from_secs(60));
            }
        })
        .expect("failed to spawn phone usage thread");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(package: &str, kind: i32, secs: i64) -> UsageEvent {
        UsageEvent {
            package: package.into(),
            kind,
            time: 1_800_000_000_000 + secs * 1000,
        }
    }

    fn span(package: &str, from: i64, to: i64) -> Span {
        Span {
            package: package.into(),
            start: 1_800_000_000_000 + from * 1000,
            end: 1_800_000_000_000 + to * 1000,
        }
    }

    #[test]
    fn foreground_spans_from_events() {
        let events = [
            ev("com.whatsapp", kind::ACTIVITY_RESUMED, 0),
            ev("com.whatsapp", kind::ACTIVITY_RESUMED, 30), // another chat screen
            ev("com.whatsapp", kind::ACTIVITY_PAUSED, 60),
            ev("com.google.android.youtube", kind::ACTIVITY_RESUMED, 61),
            ev("com.android.systemui", kind::ACTIVITY_RESUMED, 100), // pulled the shade down
            ev("com.google.android.youtube", kind::ACTIVITY_PAUSED, 600),
            ev("com.instagram.android", kind::ACTIVITY_RESUMED, 700),
            ev("android", kind::SCREEN_NON_INTERACTIVE, 900), // screen off
            ev("com.instagram.android", kind::ACTIVITY_PAUSED, 901),
            ev("com.duolingo", kind::ACTIVITY_RESUMED, 2000), // still open
        ];
        let (spans, open) = spans(&events);
        assert_eq!(
            spans,
            vec![
                span("com.whatsapp", 0, 60),
                span("com.google.android.youtube", 61, 600),
                span("com.instagram.android", 700, 900),
            ]
        );
        assert_eq!(open, Some(1_800_000_000_000 + 2000 * 1000));
    }

    #[test]
    fn switching_without_a_pause_event_closes_the_previous_app() {
        let events = [
            ev("com.whatsapp", kind::ACTIVITY_RESUMED, 0),
            ev("com.spotify.music", kind::ACTIVITY_RESUMED, 120),
            ev("com.spotify.music", kind::ACTIVITY_PAUSED, 180),
        ];
        let (spans, open) = spans(&events);
        assert_eq!(
            spans,
            vec![
                span("com.whatsapp", 0, 120),
                span("com.spotify.music", 120, 180)
            ]
        );
        assert_eq!(open, None);
    }

    #[test]
    fn recorded_sessions_get_names_categories_and_split_at_midnight() {
        let conn = db::open_in_memory().unwrap();
        let start = Local
            .with_ymd_and_hms(2026, 11, 18, 23, 50, 0)
            .unwrap()
            .timestamp_millis();
        let s = Span {
            package: "com.google.android.youtube".into(),
            start,
            end: start + 20 * 60_000, // until 00:10
        };
        let labels = [(
            "com.google.android.youtube".to_string(),
            "YouTube".to_string(),
        )]
        .into();
        assert_eq!(record(&conn, &[s], &labels).unwrap(), 2);
        let rows: Vec<(String, i64, String, String)> = conn
            .prepare(
                "SELECT s.day, s.duration_secs, a.display_name, c.name FROM usage_sessions s
                 JOIN applications a ON a.id = s.app_id JOIN categories c ON c.id = a.category_id ORDER BY s.id",
            )
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert_eq!(
            rows,
            vec![
                (
                    "2026-11-18".into(),
                    600,
                    "YouTube".into(),
                    "Entertainment".into()
                ),
                (
                    "2026-11-19".into(),
                    600,
                    "YouTube".into(),
                    "Entertainment".into()
                ),
            ]
        );
    }
}
