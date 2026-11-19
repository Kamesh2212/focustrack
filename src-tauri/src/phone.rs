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

/// An app leaving and coming back within this long (switching between its own
/// screens) is one session, not two.
const SAME_APP_GAP_MS: i64 = 2000;

/// Foreground spans from events (sorted by time). Time on a home screen
/// (`home` launchers) isn't app use. Also returns the start of a span still
/// open at the end, so the next import can pick it up from there.
pub fn spans(events: &[UsageEvent], home: &[String]) -> (Vec<Span>, Option<i64>) {
    let mut out = Vec::new();
    // The app in front: (package, since).
    let mut current: Option<(String, i64)> = None;
    // An app that just paused: (package, since, paused at). It may resume a
    // moment later on another of its screens.
    let mut paused: Option<(String, i64, i64)> = None;
    let push = |out: &mut Vec<Span>, package: String, start: i64, end: i64| {
        if end - start >= 1000 {
            out.push(Span {
                package,
                start,
                end,
            });
        }
    };
    let flush = |out: &mut Vec<Span>, paused: &mut Option<(String, i64, i64)>| {
        if let Some((p, start, end)) = paused.take() {
            push(out, p, start, end);
        }
    };
    for e in events {
        match e.kind {
            kind::ACTIVITY_RESUMED if !IGNORED.contains(&e.package.as_str()) => {
                if current.as_ref().is_some_and(|(p, _)| *p == e.package) {
                    continue; // another screen of the same app
                }
                // Back to the same app within a moment: carry on its session.
                if let Some((p, start, at)) = paused.take() {
                    if p == e.package && e.time - at <= SAME_APP_GAP_MS {
                        current = Some((p, start));
                        continue;
                    }
                    push(&mut out, p, start, at);
                }
                if let Some((p, start)) = current.take() {
                    push(&mut out, p, start, e.time);
                }
                if !home.contains(&e.package) {
                    current = Some((e.package.clone(), e.time));
                }
            }
            kind::ACTIVITY_PAUSED => {
                if current.as_ref().is_some_and(|(p, _)| *p == e.package) {
                    flush(&mut out, &mut paused);
                    let (p, start) = current.take().expect("checked above");
                    paused = Some((p, start, e.time));
                }
            }
            kind::SCREEN_NON_INTERACTIVE | kind::KEYGUARD_SHOWN => {
                flush(&mut out, &mut paused);
                if let Some((p, start)) = current.take() {
                    push(&mut out, p, start, e.time);
                }
            }
            _ => {}
        }
    }
    flush(&mut out, &mut paused);
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

/// Where the next import starts (Unix milliseconds).
pub fn import_since(conn: &Connection, now_ms: i64) -> rusqlite::Result<i64> {
    Ok(db::get_setting(conn, "phone_usage_since")?
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(now_ms - FIRST_IMPORT_DAYS * 86_400_000))
}

/// Records events newer than the last import, and moves the import point on.
///
/// The app and the background job may import at the same moment, from
/// separate processes. The write lock is taken first and the starting point
/// re-read inside it, so the second one only sees what the first left over:
/// no session is ever recorded twice. Returns (sessions added, next start).
pub fn record_events(
    conn: &Connection,
    events: &mut [UsageEvent],
    labels: &std::collections::HashMap<String, String>,
    home: &[String],
    now_ms: i64,
) -> rusqlite::Result<(usize, i64)> {
    conn.execute_batch("BEGIN IMMEDIATE")?;
    let result = (|| {
        let since = import_since(conn, now_ms)?;
        events.sort_by_key(|e| e.time);
        let fresh: Vec<UsageEvent> = events.iter().filter(|e| e.time >= since).cloned().collect();
        let (spans, open) = spans(&fresh, home);
        let n = record(conn, &spans, labels)?;
        // Next time, start from the app still in front (it isn't recorded yet).
        let next = open.unwrap_or(now_ms).max(since);
        db::set_setting(conn, "phone_usage_since", &next.to_string())?;
        Ok((n, next))
    })();
    match result {
        Ok(v) => {
            conn.execute_batch("COMMIT")?;
            Ok(v)
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(e)
        }
    }
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
    let since = import_since(conn, now).map_err(|e| e.to_string())?;
    let mut got = usage.events(since, now)?;
    let (n, _) = record_events(conn, &mut got.events, &got.labels, &got.home, now)
        .map_err(|e| e.to_string())?;
    Ok(Some(n))
}

/// Input from the background job: events read by Android while the app was closed.
// Used by the Android background job (and tests).
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
#[derive(serde::Deserialize)]
pub struct BackgroundInput {
    pub events: Vec<UsageEvent>,
    #[serde(default)]
    pub labels: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub home: Vec<String>,
}

/// What the background job reports back (JSON).
// Used by the Android background job (and tests).
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
#[derive(serde::Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundResult {
    pub imported: usize,
    /// Where the next import should start reading.
    pub next_since: i64,
    /// Whether a sync ran and finished.
    pub synced: bool,
    pub error: Option<String>,
}

/// The background job (#68): record events read while the app was closed,
/// then sync once if this device is signed in. Runs without the app's UI or
/// runtime, in the same app process (Android WorkManager → JNI).
// Used by the Android background job (and tests).
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub fn background_run(data_dir: &std::path::Path, input_json: &str) -> BackgroundResult {
    let mut out = BackgroundResult::default();
    let mut input: BackgroundInput = match serde_json::from_str(input_json) {
        Ok(i) => i,
        Err(e) => {
            out.error = Some(format!("bad input: {e}"));
            return out;
        }
    };
    let mut conn = match db::open(&data_dir.join("focustrack.db")) {
        Ok(c) => c,
        Err(e) => {
            out.error = Some(format!("database: {e}"));
            return out;
        }
    };
    let now = Local::now().timestamp_millis();
    match record_events(&conn, &mut input.events, &input.labels, &input.home, now) {
        Ok((n, next)) => {
            out.imported = n;
            out.next_since = next;
        }
        Err(e) => {
            out.error = Some(format!("import: {e}"));
            return out;
        }
    }
    let enabled = db::get_setting(&conn, "sync_enabled")
        .ok()
        .flatten()
        .as_deref()
        == Some("1");
    if enabled {
        let handle = crate::sync::SyncHandle::new(data_dir.to_path_buf());
        match crate::sync::sync_once(&handle, &mut conn) {
            Ok(()) => out.synced = true,
            Err(e) => out.error = Some(format!("sync: {e}")),
        }
    }
    out
}

/// JNI entry point for the WorkManager job in `plugins/usage`
/// (`com.focustrack.usage.BackgroundImport.run`).
#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_com_focustrack_usage_BackgroundImport_run<'l>(
    mut env: jni::JNIEnv<'l>,
    _class: jni::objects::JClass<'l>,
    data_dir: jni::objects::JString<'l>,
    input: jni::objects::JString<'l>,
) -> jni::objects::JString<'l> {
    let dir: String = env
        .get_string(&data_dir)
        .map(Into::into)
        .unwrap_or_default();
    let json: String = env.get_string(&input).map(Into::into).unwrap_or_default();
    let result = std::panic::catch_unwind(|| background_run(std::path::Path::new(&dir), &json))
        .unwrap_or_else(|_| BackgroundResult {
            error: Some("background import crashed".into()),
            ..Default::default()
        });
    let text = serde_json::to_string(&result).unwrap_or_default();
    env.new_string(text)
        .unwrap_or_else(|_| jni::objects::JObject::null().into())
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
            // Keep importing (and syncing) every ~15 minutes while the app is closed.
            if let Some(dir) = db_path.parent().and_then(|d| d.to_str()) {
                use tauri_plugin_focustrack_usage::UsageExt;
                if let Err(e) = app.usage().enable_background(dir) {
                    eprintln!("[phone] couldn't schedule background import: {e}");
                }
            }
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
        let (spans, open) = spans(&events, &[]);
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
    fn overlapping_imports_never_record_twice() {
        let conn = db::open_in_memory().unwrap();
        let labels = Default::default();
        let base = 1_800_000_000_000;
        let now = base + 3_600_000;
        db::set_setting(&conn, "phone_usage_since", &base.to_string()).unwrap();
        let mut batch = vec![
            ev("com.whatsapp", kind::ACTIVITY_RESUMED, 0),
            ev("com.whatsapp", kind::ACTIVITY_PAUSED, 60),
            ev("com.google.android.youtube", kind::ACTIVITY_RESUMED, 61),
            ev("com.google.android.youtube", kind::ACTIVITY_PAUSED, 300),
            ev("com.duolingo", kind::ACTIVITY_RESUMED, 400), // still open
        ];
        // The app imports first...
        let (n, next) = record_events(&conn, &mut batch.clone(), &labels, &[], now).unwrap();
        assert_eq!(n, 2);
        assert_eq!(next, base + 400_000, "resumes at the app still in front");
        // ...then the background job, which read the same events and more.
        batch.push(ev("com.duolingo", kind::ACTIVITY_PAUSED, 500));
        let (n, _) = record_events(&conn, &mut batch, &labels, &[], now + 1000).unwrap();
        assert_eq!(n, 1, "only Duolingo is new");
        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM usage_sessions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 3);
    }

    #[test]
    fn background_run_imports_without_the_app() {
        let dir = std::env::temp_dir().join(format!("ft-bg-{}", focustrack_sync_proto::new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let now = Local::now().timestamp_millis();
        let input = serde_json::json!({
            "events": [
                { "package": "com.instagram.android", "kind": kind::ACTIVITY_RESUMED, "time": now - 600_000 },
                { "package": "com.instagram.android", "kind": kind::ACTIVITY_PAUSED, "time": now - 300_000 },
            ],
            "labels": { "com.instagram.android": "Instagram" }
        });
        let r = background_run(&dir, &input.to_string());
        assert_eq!(
            (r.imported, r.synced, r.error.as_deref()),
            (1, false, None),
            "not signed in: no sync"
        );
        let conn = db::open(&dir.join("focustrack.db")).unwrap();
        let name: String = conn
            .query_row("SELECT a.display_name FROM usage_sessions s JOIN applications a ON a.id = s.app_id", [], |r| r.get(0))
            .unwrap();
        assert_eq!(name, "Instagram");
        drop(conn);
        let bad = background_run(&dir, "not json");
        assert!(bad.error.unwrap().starts_with("bad input"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn home_screen_isnt_app_use_and_brief_gaps_join_up() {
        let home = vec!["com.google.android.apps.nexuslauncher".to_string()];
        let events = [
            // YouTube's splash screen, then its main screen a second later.
            ev("com.google.android.youtube", kind::ACTIVITY_RESUMED, 0),
            ev("com.google.android.youtube", kind::ACTIVITY_PAUSED, 3),
            ev("com.google.android.youtube", kind::ACTIVITY_RESUMED, 4),
            ev("com.google.android.youtube", kind::ACTIVITY_PAUSED, 60),
            // Home screen for 10 s.
            ev(
                "com.google.android.apps.nexuslauncher",
                kind::ACTIVITY_RESUMED,
                60,
            ),
            ev(
                "com.google.android.apps.nexuslauncher",
                kind::ACTIVITY_PAUSED,
                70,
            ),
            ev("com.android.chrome", kind::ACTIVITY_RESUMED, 70),
            ev("com.android.chrome", kind::ACTIVITY_PAUSED, 100),
            ev(
                "com.google.android.apps.nexuslauncher",
                kind::ACTIVITY_RESUMED,
                100,
            ), // back home, stays
        ];
        let (spans, open) = spans(&events, &home);
        assert_eq!(
            spans,
            vec![
                span("com.google.android.youtube", 0, 60),
                span("com.android.chrome", 70, 100)
            ],
            "one YouTube session, no home-screen sessions"
        );
        assert_eq!(open, None, "the home screen isn't left open");
    }

    #[test]
    fn switching_without_a_pause_event_closes_the_previous_app() {
        let events = [
            ev("com.whatsapp", kind::ACTIVITY_RESUMED, 0),
            ev("com.spotify.music", kind::ACTIVITY_RESUMED, 120),
            ev("com.spotify.music", kind::ACTIVITY_PAUSED, 180),
        ];
        let (spans, open) = spans(&events, &[]);
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
