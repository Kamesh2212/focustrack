//! Tauri commands invoked by the React frontend.

use crate::analytics::{self as an, AppUsage, CategoryUsage, DayTotal, SiteUsage};
use crate::browser;
use crate::db;
use crate::limits::{self, LimitStatus};
use crate::tracker::LiveStatus;
use crate::AppState;
#[cfg(desktop)]
use crate::TrayPause;
use chrono::{Datelike, Duration, Local, Weekday};
use rusqlite::params;
use serde::Serialize;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Manager, State};
#[cfg(desktop)]
use tauri_plugin_autostart::ManagerExt;

/// Whether FocusTrack starts with the computer (always false on phones).
fn autostart_enabled(_app: &AppHandle) -> bool {
    #[cfg(desktop)]
    return _app.autolaunch().is_enabled().unwrap_or(false);
    #[cfg(mobile)]
    false
}

type CmdResult<T> = Result<T, String>;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

macro_rules! conn {
    ($state:expr) => {
        $state.db.lock().map_err(err)?
    };
}

// ------------------------------------------------------------- dashboard

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Dashboard {
    pub today: String,
    pub screen_secs: i64,
    pub active_secs: i64,
    pub idle_secs: i64,
    pub prev_day_active_secs: i64,
    /// Average over days with activity in the last 30 days.
    pub daily_avg_secs: i64,
    /// Average per day over the last 7 days (including today).
    pub weekly_avg_secs: i64,
    pub session_count: i64,
    pub productive_secs: i64,
    pub distracting_secs: i64,
    pub top_apps: Vec<AppUsage>,
    pub top_sites: Vec<SiteUsage>,
    pub site_secs: i64,
    pub categories: Vec<CategoryUsage>,
    pub week: Vec<DayTotal>,
    pub hourly: Vec<i64>,
    pub live: LiveStatus,
    /// Active time with overlaps between devices counted once (#64).
    pub unique_active_secs: i64,
    /// Today's time per device (all devices, whatever the filter).
    pub devices: Vec<an::DeviceTotal>,
    /// Unique active time across all devices (for the devices breakdown).
    pub devices_unique_secs: i64,
}

/// `device`: `all` (default), `desktop`, `mobile` or a device ID.
#[tauri::command]
pub fn get_dashboard(state: State<'_, AppState>, device: Option<String>) -> CmdResult<Dashboard> {
    let conn = conn!(state);
    let today = Local::now().date_naive();
    let t = an::fmt_day(today);
    let devices = an::device_totals(&conn, &t, &t).map_err(err)?;
    let devices_unique_secs = an::union_secs(&an::active_spans(&conn, &t, &t).map_err(err)?);
    let ids = an::device_filter(&conn, device.as_deref()).map_err(err)?;
    let _view = db::device_view(&conn, ids.as_deref()).map_err(err)?;
    let yesterday = an::fmt_day(today - Duration::days(1));

    let (active, idle) = an::totals(&conn, &t, &t).map_err(err)?;
    let (prev_active, _) = an::totals(&conn, &yesterday, &yesterday).map_err(err)?;
    let week = an::daily_totals(&conn, today - Duration::days(6), today).map_err(err)?;
    let month = an::daily_totals(&conn, today - Duration::days(29), today).map_err(err)?;
    let apps = an::app_usage(&conn, &t, &t).map_err(err)?;
    let sites = an::site_usage(&conn, &t, &t).map_err(err)?;
    let site_secs = an::site_total(&conn, &t, &t).map_err(err)?;
    let categories = an::category_usage(&conn, &t, &t).map_err(err)?;
    let spans = an::active_spans(&conn, &t, &t).map_err(err)?;
    let (hours, _) = an::hour_buckets(&spans);
    let (productive, distracting) = an::productive_split(&categories);

    Ok(Dashboard {
        today: t,
        screen_secs: active + idle,
        active_secs: active,
        idle_secs: idle,
        prev_day_active_secs: prev_active,
        daily_avg_secs: an::average_of_active_days(&month),
        weekly_avg_secs: week.iter().map(|d| d.active_secs).sum::<i64>() / 7,
        session_count: an::count_sessions(&spans, an::SESSION_GAP_SECS),
        productive_secs: productive,
        distracting_secs: distracting,
        top_apps: apps.into_iter().filter(|a| a.secs > 0).take(8).collect(),
        top_sites: sites.into_iter().filter(|s| s.secs > 0).take(8).collect(),
        site_secs,
        categories,
        week,
        hourly: hours.to_vec(),
        live: state.shared.status(),
        unique_active_secs: an::union_secs(&spans),
        devices,
        devices_unique_secs,
    })
}

#[tauri::command]
pub fn get_live_status(state: State<'_, AppState>) -> LiveStatus {
    state.shared.status()
}

// ---------------------------------------------------------- applications

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    pub id: i64,
    pub name: String,
    pub color: String,
    pub kind: String,
}

#[tauri::command]
pub fn get_apps(state: State<'_, AppState>, from: String, to: String) -> CmdResult<Vec<AppUsage>> {
    an::parse_day(&from)?;
    an::parse_day(&to)?;
    let conn = conn!(state);
    an::app_usage(&conn, &from, &to).map_err(err)
}

#[tauri::command]
pub fn get_categories(state: State<'_, AppState>) -> CmdResult<Vec<Category>> {
    let conn = conn!(state);
    let mut stmt = conn
        .prepare("SELECT id, name, color, kind FROM categories ORDER BY id")
        .map_err(err)?;
    let rows = stmt
        .query_map([], |r| {
            Ok(Category {
                id: r.get(0)?,
                name: r.get(1)?,
                color: r.get(2)?,
                kind: r.get(3)?,
            })
        })
        .map_err(err)?;
    let out: Result<Vec<_>, _> = rows.collect();
    out.map_err(err)
}

#[tauri::command]
pub fn set_app_category(
    state: State<'_, AppState>,
    app_id: i64,
    category_id: Option<i64>,
) -> CmdResult<()> {
    let conn = conn!(state);
    conn.execute(
        "UPDATE applications SET category_id = ?1 WHERE id = ?2",
        params![category_id, app_id],
    )
    .map_err(err)?;
    Ok(())
}

#[tauri::command]
pub fn rename_app(state: State<'_, AppState>, app_id: i64, name: String) -> CmdResult<()> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Name cannot be empty".into());
    }
    let conn = conn!(state);
    conn.execute(
        "UPDATE applications SET display_name = ?1 WHERE id = ?2",
        params![name, app_id],
    )
    .map_err(err)?;
    Ok(())
}

// --------------------------------------------------------------- reports

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub from: String,
    pub to: String,
    pub active_secs: i64,
    pub idle_secs: i64,
    pub daily_avg_secs: i64,
    pub prev_active_secs: i64,
    pub weekday_avg_secs: i64,
    pub weekend_avg_secs: i64,
    pub productive_secs: i64,
    pub distracting_secs: i64,
    pub peak_hour: Option<usize>,
    pub days: Vec<DayTotal>,
    pub apps: Vec<AppUsage>,
    pub sites: Vec<SiteUsage>,
    pub site_secs: i64,
    pub categories: Vec<CategoryUsage>,
    pub productivity: Vec<an::DayKinds>,
    pub focus_secs: i64,
    pub focus_sessions: i64,
    pub hourly: Vec<i64>,
    /// 7 rows (Mon..Sun) x 24 hours, active seconds.
    pub heatmap: Vec<Vec<i64>>,
    pub unique_active_secs: i64,
    pub devices: Vec<an::DeviceTotal>,
    pub devices_unique_secs: i64,
}

#[tauri::command]
pub fn get_report(
    state: State<'_, AppState>,
    from: String,
    to: String,
    device: Option<String>,
) -> CmdResult<Report> {
    let (f, t) = (an::parse_day(&from)?, an::parse_day(&to)?);
    if f > t {
        return Err("Start date is after end date".into());
    }
    let conn = conn!(state);
    let devices = an::device_totals(&conn, &from, &to).map_err(err)?;
    let devices_unique_secs = an::union_secs(&an::active_spans(&conn, &from, &to).map_err(err)?);
    let ids = an::device_filter(&conn, device.as_deref()).map_err(err)?;
    let _view = db::device_view(&conn, ids.as_deref()).map_err(err)?;
    let len = (t - f).num_days() + 1;
    let (pf, pt) = (f - Duration::days(len), f - Duration::days(1));

    let (active, idle) = an::totals(&conn, &from, &to).map_err(err)?;
    let (prev_active, _) = an::totals(&conn, &an::fmt_day(pf), &an::fmt_day(pt)).map_err(err)?;
    let days = an::daily_totals(&conn, f, t).map_err(err)?;
    let apps = an::app_usage(&conn, &from, &to).map_err(err)?;
    let sites = an::site_usage(&conn, &from, &to).map_err(err)?;
    let site_secs = an::site_total(&conn, &from, &to).map_err(err)?;
    let categories = an::category_usage(&conn, &from, &to).map_err(err)?;
    let spans = an::active_spans(&conn, &from, &to).map_err(err)?;
    let (hours, heat) = an::hour_buckets(&spans);
    let (productive, distracting) = an::productive_split(&categories);
    let productivity = an::daily_kinds(&conn, f, t).map_err(err)?;
    let (focus_secs, focus_sessions) = crate::focus::totals(&conn, &from, &to).map_err(err)?;

    let is_weekend = |d: &DayTotal| {
        an::parse_day(&d.day)
            .map(|d| matches!(d.weekday(), Weekday::Sat | Weekday::Sun))
            .unwrap_or(false)
    };
    let weekday: Vec<DayTotal> = days.iter().filter(|d| !is_weekend(d)).cloned().collect();
    let weekend: Vec<DayTotal> = days.iter().filter(|d| is_weekend(d)).cloned().collect();
    let peak_hour = hours
        .iter()
        .enumerate()
        .filter(|(_, s)| **s > 0)
        .max_by_key(|(_, s)| **s)
        .map(|(h, _)| h);

    Ok(Report {
        from,
        to,
        active_secs: active,
        idle_secs: idle,
        daily_avg_secs: an::average_of_active_days(&days),
        prev_active_secs: prev_active,
        weekday_avg_secs: an::average_of_active_days(&weekday),
        weekend_avg_secs: an::average_of_active_days(&weekend),
        productive_secs: productive,
        distracting_secs: distracting,
        peak_hour,
        days,
        apps: apps.into_iter().filter(|a| a.secs > 0).take(25).collect(),
        sites: sites.into_iter().filter(|s| s.secs > 0).take(25).collect(),
        site_secs,
        categories,
        productivity,
        focus_secs,
        focus_sessions,
        hourly: hours.to_vec(),
        heatmap: heat.iter().map(|r| r.to_vec()).collect(),
        unique_active_secs: an::union_secs(&spans),
        devices,
        devices_unique_secs,
    })
}

// --------------------------------------------------------------- history

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRow {
    pub id: i64,
    pub app_name: String,
    pub exe_name: String,
    pub category: String,
    pub color: String,
    pub day: String,
    pub start: String,
    pub end: String,
    pub secs: i64,
}

const SITE_HISTORY_SQL: &str = "
    SELECT s.id, w.domain, s.browser, COALESCE(c.name, 'Other'), COALESCE(c.color, '#94a3b8'),
           s.day, s.start_time, s.end_time, s.duration_secs
    FROM website_sessions s
    JOIN websites w ON w.id = s.website_id
    LEFT JOIN categories c ON c.id = w.category_id
    WHERE s.day BETWEEN ?1 AND ?2 AND s.duration_secs > 0
      AND (?3 = '' OR w.domain LIKE '%' || ?3 || '%' OR c.name LIKE '%' || ?3 || '%')
    ORDER BY s.start_time DESC
    LIMIT ?4";

const HISTORY_SQL: &str = "
    SELECT s.id, a.display_name, a.exe_name, COALESCE(c.name, 'Other'), COALESCE(c.color, '#94a3b8'),
           s.day, s.start_time, s.end_time, s.duration_secs
    FROM usage_sessions s
    JOIN applications a ON a.id = s.app_id
    LEFT JOIN categories c ON c.id = a.category_id
    WHERE s.day BETWEEN ?1 AND ?2 AND s.duration_secs > 0
      AND (?3 = '' OR a.display_name LIKE '%' || ?3 || '%' OR a.exe_name LIKE '%' || ?3 || '%'
           OR c.name LIKE '%' || ?3 || '%')
    ORDER BY s.start_time DESC
    LIMIT ?4";

fn query_history(
    conn: &rusqlite::Connection,
    websites: bool,
    from: &str,
    to: &str,
    search: &str,
    limit: i64,
) -> rusqlite::Result<Vec<SessionRow>> {
    let mut stmt = conn.prepare(if websites {
        SITE_HISTORY_SQL
    } else {
        HISTORY_SQL
    })?;
    let rows = stmt.query_map(params![from, to, search.trim(), limit], |r| {
        Ok(SessionRow {
            id: r.get(0)?,
            app_name: r.get(1)?,
            exe_name: r.get(2)?,
            category: r.get(3)?,
            color: r.get(4)?,
            day: r.get(5)?,
            start: r.get(6)?,
            end: r.get(7)?,
            secs: r.get(8)?,
        })
    })?;
    let out = rows.collect();
    out
}

#[tauri::command]
pub fn get_history(
    state: State<'_, AppState>,
    from: String,
    to: String,
    search: Option<String>,
    limit: Option<i64>,
    kind: Option<String>,
) -> CmdResult<Vec<SessionRow>> {
    an::parse_day(&from)?;
    an::parse_day(&to)?;
    let conn = conn!(state);
    let websites = kind.as_deref() == Some("websites");
    query_history(
        &conn,
        websites,
        &from,
        &to,
        search.as_deref().unwrap_or(""),
        limit.unwrap_or(500).clamp(1, 5000),
    )
    .map_err(err)
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Writes a CSV of app or website sessions to the Downloads folder; returns its path.
#[tauri::command]
pub fn export_csv(
    app: AppHandle,
    state: State<'_, AppState>,
    from: String,
    to: String,
    search: Option<String>,
    kind: Option<String>,
) -> CmdResult<String> {
    an::parse_day(&from)?;
    an::parse_day(&to)?;
    let websites = kind.as_deref() == Some("websites");
    let rows = {
        let conn = conn!(state);
        query_history(
            &conn,
            websites,
            &from,
            &to,
            search.as_deref().unwrap_or(""),
            i64::MAX,
        )
        .map_err(err)?
    };
    let mut out = String::from(if websites {
        "website,browser,category,date,start,end,duration_seconds,duration_minutes\n"
    } else {
        "application,executable,category,date,start,end,duration_seconds,duration_minutes\n"
    });
    for r in rows.iter().rev() {
        out.push_str(&format!(
            "{},{},{},{},{},{},{},{:.1}\n",
            csv_field(&r.app_name),
            csv_field(&r.exe_name),
            csv_field(&r.category),
            r.day,
            r.start,
            r.end,
            r.secs,
            r.secs as f64 / 60.0
        ));
    }
    let dir = app
        .path()
        .download_dir()
        .or_else(|_| app.path().document_dir())
        .map_err(err)?;
    let label = if websites { "websites" } else { "apps" };
    let path = dir.join(format!("focustrack-{label}-{from}-to-{to}.csv"));
    std::fs::write(&path, out).map_err(err)?;
    Ok(path.to_string_lossy().into_owned())
}

// -------------------------------------------------------------- settings

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub paused: bool,
    pub idle_threshold_secs: u64,
    pub autostart: bool,
    pub theme: String,
    pub data_dir: String,
    pub notifications_enabled: bool,
    pub break_reminder_mins: u64,
    pub website_tracking: bool,
    pub overrides_per_day: i64,
    pub goal_reminder: String,
    /// JSON: [{"id": "...", "visible": true}, ...] — empty means default.
    pub dashboard_layout: String,
}

#[tauri::command]
pub fn get_settings(app: AppHandle, state: State<'_, AppState>) -> CmdResult<Settings> {
    let conn = conn!(state);
    let s = db::load_settings(&conn).map_err(err)?;
    Ok(Settings {
        paused: state.shared.paused.load(Ordering::SeqCst),
        idle_threshold_secs: s.idle_threshold_secs,
        autostart: autostart_enabled(&app),
        theme: s.theme,
        data_dir: state.data_dir.to_string_lossy().into_owned(),
        notifications_enabled: s.notifications_enabled,
        break_reminder_mins: s.break_reminder_mins,
        website_tracking: s.website_tracking,
        overrides_per_day: s.overrides_per_day,
        goal_reminder: s.goal_reminder,
        dashboard_layout: db::get_setting(&conn, "dashboard_layout")
            .map_err(err)?
            .unwrap_or_default(),
    })
}

#[tauri::command]
pub fn set_overrides_per_day(state: State<'_, AppState>, count: i64) -> CmdResult<()> {
    if !(0..=5).contains(&count) {
        return Err("Choose between 0 and 5".into());
    }
    let conn = conn!(state);
    db::set_setting(&conn, "overrides_per_day", &count.to_string()).map_err(err)
}

#[tauri::command]
pub fn set_goal_reminder(state: State<'_, AppState>, time: String) -> CmdResult<()> {
    let t = time.trim();
    if !t.is_empty() && chrono::NaiveTime::parse_from_str(t, "%H:%M").is_err() {
        return Err("Use a time like 20:00".into());
    }
    let conn = conn!(state);
    db::set_setting(&conn, "goal_reminder", t).map_err(err)
}

#[tauri::command]
pub fn set_dashboard_layout(state: State<'_, AppState>, layout: String) -> CmdResult<()> {
    if layout.len() > 4000
        || serde_json::from_str::<serde_json::Value>(&layout)
            .map(|v| !v.is_array())
            .unwrap_or(true)
    {
        return Err("invalid layout".into());
    }
    let conn = conn!(state);
    db::set_setting(&conn, "dashboard_layout", &layout).map_err(err)
}

/// Shared by the settings screen and the tray menu.
pub fn apply_paused(app: &AppHandle, paused: bool) -> CmdResult<()> {
    let state = app.state::<AppState>();
    state.shared.paused.store(paused, Ordering::SeqCst);
    {
        let conn = conn!(state);
        db::set_setting(&conn, "paused", if paused { "1" } else { "0" }).map_err(err)?;
    }
    #[cfg(desktop)]
    if let Some(item) = app.try_state::<TrayPause>() {
        let _ = item.0.set_text(if paused {
            "Resume tracking"
        } else {
            "Pause tracking (limits still apply)"
        });
    }
    Ok(())
}

#[tauri::command]
pub fn set_paused(app: AppHandle, paused: bool) -> CmdResult<()> {
    apply_paused(&app, paused)
}

#[tauri::command]
pub fn set_idle_threshold(state: State<'_, AppState>, secs: u64) -> CmdResult<()> {
    if !(30..=3600).contains(&secs) {
        return Err("Idle threshold must be between 30 seconds and 60 minutes".into());
    }
    state
        .shared
        .idle_threshold_secs
        .store(secs, Ordering::SeqCst);
    let conn = conn!(state);
    db::set_setting(&conn, "idle_threshold_secs", &secs.to_string()).map_err(err)
}

#[tauri::command]
pub fn set_autostart(app: AppHandle, enabled: bool) -> CmdResult<()> {
    #[cfg(desktop)]
    {
        let launcher = app.autolaunch();
        let result = if enabled {
            launcher.enable()
        } else {
            launcher.disable()
        };
        result.map_err(err)
    }
    #[cfg(mobile)]
    {
        let _ = (app, enabled);
        Err("Not available on this device".into())
    }
}

#[tauri::command]
pub fn set_theme(state: State<'_, AppState>, theme: String) -> CmdResult<()> {
    if !["system", "light", "dark"].contains(&theme.as_str()) {
        return Err(format!("unknown theme: {theme}"));
    }
    let conn = conn!(state);
    db::set_setting(&conn, "theme", &theme).map_err(err)
}

#[tauri::command]
pub fn delete_all_data(state: State<'_, AppState>) -> CmdResult<()> {
    let conn = conn!(state);
    db::delete_all_usage(&conn).map_err(err)?;
    state.shared.reset_requested.store(true, Ordering::SeqCst);
    Ok(())
}

/// Deletes usage older than `days` days. Returns rows removed.
#[tauri::command]
pub fn delete_data_older_than(state: State<'_, AppState>, days: i64) -> CmdResult<usize> {
    if days < 1 {
        return Err("days must be at least 1".into());
    }
    let cutoff = an::fmt_day(Local::now().date_naive() - Duration::days(days));
    let conn = conn!(state);
    db::delete_usage_before(&conn, &cutoff).map_err(err)
}

#[tauri::command]
pub fn set_notifications_enabled(state: State<'_, AppState>, enabled: bool) -> CmdResult<()> {
    state
        .shared
        .notifications_enabled
        .store(enabled, Ordering::SeqCst);
    let conn = conn!(state);
    db::set_setting(
        &conn,
        "notifications_enabled",
        if enabled { "1" } else { "0" },
    )
    .map_err(err)
}

#[tauri::command]
pub fn set_break_reminder(state: State<'_, AppState>, mins: u64) -> CmdResult<()> {
    if mins > 8 * 60 {
        return Err("Break reminder must be at most 8 hours".into());
    }
    state
        .shared
        .break_reminder_mins
        .store(mins, Ordering::SeqCst);
    let conn = conn!(state);
    db::set_setting(&conn, "break_reminder_mins", &mins.to_string()).map_err(err)
}

#[tauri::command]
pub fn set_website_tracking(state: State<'_, AppState>, enabled: bool) -> CmdResult<()> {
    state
        .shared
        .website_tracking
        .store(enabled, Ordering::SeqCst);
    let conn = conn!(state);
    db::set_setting(&conn, "website_tracking", if enabled { "1" } else { "0" }).map_err(err)
}

#[tauri::command]
pub fn send_test_notification(app: AppHandle) {
    limits::notify(
        &app,
        "FocusTrack notifications are on",
        "This is how limit warnings and break reminders will look.",
    );
}

// -------------------------------------------------------------- websites

#[tauri::command]
pub fn get_websites(
    state: State<'_, AppState>,
    from: String,
    to: String,
) -> CmdResult<Vec<SiteUsage>> {
    an::parse_day(&from)?;
    an::parse_day(&to)?;
    let conn = conn!(state);
    an::site_usage(&conn, &from, &to).map_err(err)
}

#[tauri::command]
pub fn set_website_category(
    state: State<'_, AppState>,
    website_id: i64,
    category_id: Option<i64>,
) -> CmdResult<()> {
    let conn = conn!(state);
    conn.execute(
        "UPDATE websites SET category_id = ?1 WHERE id = ?2",
        params![category_id, website_id],
    )
    .map_err(err)?;
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserConn {
    pub browser: String,
    pub last_seen_secs: i64,
    pub connected: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserStatus {
    pub server_running: bool,
    pub port: u16,
    pub browsers: Vec<BrowserConn>,
}

#[tauri::command]
pub fn get_browser_status(state: State<'_, AppState>) -> BrowserStatus {
    let now = Local::now().naive_local();
    let mut browsers: Vec<BrowserConn> = state
        .shared
        .browsers
        .lock()
        .map(|m| {
            m.iter()
                .map(|(b, r)| {
                    let age = (now - r.at).num_seconds().max(0);
                    BrowserConn {
                        browser: b.clone(),
                        last_seen_secs: age,
                        connected: age <= browser::FRESH_SECS,
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    browsers.sort_by_key(|b| b.last_seen_secs);
    BrowserStatus {
        server_running: state.shared.server_running.load(Ordering::SeqCst),
        port: crate::profile::api_port(),
        browsers,
    }
}

// ---------------------------------------------------------------- limits

#[tauri::command]
pub fn get_limits(state: State<'_, AppState>) -> CmdResult<Vec<LimitStatus>> {
    let conn = conn!(state);
    limits::current_status(&conn, Local::now().naive_local()).map_err(err)
}

/// Creates or updates the limit for an app or website.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn set_limit(
    state: State<'_, AppState>,
    target_type: String,
    target_id: i64,
    daily_limit_secs: i64,
    warn_pct: i64,
    action: String,
    enabled: bool,
    reach: Option<String>,
) -> CmdResult<()> {
    let reach = reach.unwrap_or_else(|| "device".into());
    if reach != "device" && reach != "all" {
        return Err("A limit applies to this device or all devices".into());
    }
    if !crate::limits::ACTIONS.contains(&action.as_str()) {
        return Err(format!("unknown limit action: {action}"));
    }
    if target_type != "app" && target_type != "website" {
        return Err("target must be an app or a website".into());
    }
    if !(60..=24 * 3600).contains(&daily_limit_secs) {
        return Err("Limit must be between 1 minute and 24 hours".into());
    }
    if !(10..=99).contains(&warn_pct) {
        return Err("Warning must be between 10% and 99%".into());
    }
    let conn = conn!(state);
    conn.execute(
        "INSERT INTO usage_limits (target_type, target_id, daily_limit_secs, warn_pct, block, action, enabled, reach)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(target_type, target_id) DO UPDATE SET
           daily_limit_secs = excluded.daily_limit_secs, warn_pct = excluded.warn_pct,
           block = excluded.block, action = excluded.action, enabled = excluded.enabled,
           reach = excluded.reach",
        params![target_type, target_id, daily_limit_secs, warn_pct, action != "notify", action, enabled, reach],
    )
    .map_err(err)?;
    let id: i64 = conn
        .query_row(
            "SELECT id FROM usage_limits WHERE target_type = ?1 AND target_id = ?2",
            params![target_type, target_id],
            |r| r.get(0),
        )
        .map_err(err)?;
    // A changed limit may need to warn again today.
    db::clear_limit_notifications(&conn, id).map_err(err)
}

/// Adds 15 minutes to a limit for today (limited number per day).
#[tauri::command]
pub fn override_limit(state: State<'_, AppState>, id: i64) -> CmdResult<()> {
    let conn = conn!(state);
    let day = limits::limit_day(&conn, Local::now().naive_local()).map_err(err)?;
    let per_day = db::load_settings(&conn).map_err(err)?.overrides_per_day;
    crate::limits::override_limit(&conn, id, &day, per_day)?;
    // Unblock right away rather than waiting for the next limits check.
    let key: Option<String> = conn
        .query_row(
            "SELECT COALESCE(a.exe_name, w.domain) FROM usage_limits l
             LEFT JOIN applications a ON l.target_type = 'app' AND a.id = l.target_id
             LEFT JOIN websites w ON l.target_type = 'website' AND w.id = l.target_id WHERE l.id = ?1",
            params![id],
            |r| r.get(0),
        )
        .map_err(err)?;
    if let Some(k) = key {
        if let Ok(mut b) = state.shared.app_blocks.lock() {
            let k = k.to_ascii_lowercase();
            b.limit.retain(|(t, _)| t.exe != k);
        }
        if let Ok(mut b) = state.shared.blocklist.lock() {
            b.retain(|d| *d != k);
        }
    }
    Ok(())
}

#[tauri::command]
pub fn delete_limit(state: State<'_, AppState>, id: i64) -> CmdResult<()> {
    let conn = conn!(state);
    conn.execute("DELETE FROM usage_limits WHERE id = ?1", params![id])
        .map_err(err)?;
    db::clear_limit_notifications(&conn, id).map_err(err)
}

// -------------------------------------------------------- installed apps

/// Apps installed on this PC (cached for 10 minutes; the scan reads the
/// Start Menu and registry, which takes a moment).
#[tauri::command]
pub async fn get_installed_apps() -> CmdResult<Vec<crate::installed::InstalledApp>> {
    tauri::async_runtime::spawn_blocking(crate::installed::cached)
        .await
        .map_err(err)
}

/// Returns the id for an app, registering it if it has never been used
/// (so limits can be set for installed-but-unused apps).
#[tauri::command]
pub fn ensure_app(
    state: State<'_, AppState>,
    exe_name: String,
    name: String,
    path: String,
) -> CmdResult<i64> {
    let exe = exe_name.trim();
    if exe.is_empty() || !exe.to_ascii_lowercase().ends_with(".exe") || exe.contains(['\\', '/']) {
        return Err("invalid executable name".into());
    }
    let conn = conn!(state);
    let now = Local::now().naive_local();
    db::get_or_create_app(&conn, exe, &path, || name.trim().to_string(), now)
        .map(|(id, _)| id)
        .map_err(err)
}

// ------------------------------------------------------------ focus mode

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FocusOverview {
    pub active: Option<crate::focus::FocusStatus>,
    pub today_secs: i64,
    pub today_sessions: i64,
    pub week_secs: i64,
    pub week_sessions: i64,
    pub streak_days: i64,
    pub history: Vec<crate::focus::FocusRecord>,
    pub stop_phrase: String,
}

/// Focus overview; `mode` = "study" for the Study page's numbers and history.
#[tauri::command]
pub fn get_focus(state: State<'_, AppState>, mode: Option<String>) -> CmdResult<FocusOverview> {
    let conn = conn!(state);
    let today = Local::now().date_naive();
    let t = an::fmt_day(today);
    let m = mode.as_deref();
    let (today_secs, today_sessions) = crate::focus::totals_mode(&conn, &t, &t, m).map_err(err)?;
    let (week_secs, week_sessions) =
        crate::focus::totals_mode(&conn, &an::fmt_day(today - Duration::days(6)), &t, m)
            .map_err(err)?;
    Ok(FocusOverview {
        active: crate::focus::status(&state.shared),
        today_secs,
        today_sessions,
        week_secs,
        week_sessions,
        streak_days: crate::focus::streak(&conn, today).map_err(err)?,
        history: crate::focus::history(&conn, m, 30).map_err(err)?,
        stop_phrase: crate::focus::STOP_PHRASE.into(),
    })
}

/// What a profile would block if a session started now.
#[tauri::command]
pub async fn preview_profile(
    state: State<'_, AppState>,
    profile_id: i64,
) -> CmdResult<crate::profiles::BlockPreview> {
    // Warm the installed-apps cache off the main thread first.
    tauri::async_runtime::spawn_blocking(crate::installed::cached)
        .await
        .map_err(err)?;
    let conn = conn!(state);
    crate::focus::preview(&conn, profile_id).map(|(_, p)| p)
}

#[tauri::command]
pub fn request_stop_focus(state: State<'_, AppState>) -> CmdResult<i64> {
    crate::focus::request_stop(&state.shared)
}

// -------------------------------------------------------------- profiles

#[tauri::command]
pub fn get_profiles(state: State<'_, AppState>) -> CmdResult<Vec<crate::profiles::Profile>> {
    let conn = conn!(state);
    crate::profiles::list(&conn).map_err(err)
}

#[tauri::command]
pub fn save_profile(
    state: State<'_, AppState>,
    profile: crate::profiles::Profile,
) -> CmdResult<i64> {
    let conn = conn!(state);
    crate::profiles::save(&conn, &profile)
}

#[tauri::command]
pub fn delete_profile(state: State<'_, AppState>, id: i64) -> CmdResult<()> {
    let conn = conn!(state);
    crate::profiles::delete(&conn, id).map_err(err)
}

/// Starts a session, then pops up the focus timer and a notification.
/// Async: creating a window from a synchronous command can deadlock on Windows.
#[tauri::command]
pub async fn start_focus(
    app: AppHandle,
    state: State<'_, AppState>,
    plan: crate::focus::FocusPlan,
    devices: Option<Vec<String>>,
) -> CmdResult<crate::focus::FocusStatus> {
    let devices = devices.unwrap_or_default();
    if devices.len() > 50 || devices.iter().any(|d| d.len() > 64) {
        return Err("Too many devices".into());
    }
    let status = {
        let conn = conn!(state);
        crate::focus::start(&conn, &state.shared, plan, devices)?
    };
    if status.run.is_some() {
        state.sync.wake();
    }
    crate::popup::open(&app);
    if state.shared.notifications_enabled.load(Ordering::SeqCst) {
        let what = if status.mode == "study" {
            "Study session started"
        } else {
            "Focus session started"
        };
        let blocked = status
            .preview
            .as_ref()
            .map(|p| {
                format!(
                    "{} until {}.",
                    crate::popup::blocked_summary(p),
                    &status.plan_ends_at[11..16]
                )
            })
            .unwrap_or_else(|| {
                format!(
                    "Until {}. Nothing is blocked.",
                    &status.plan_ends_at[11..16]
                )
            });
        crate::limits::notify(
            &app,
            what,
            &format!(
                "{}: {blocked}",
                status.subject.as_deref().unwrap_or(&status.label)
            ),
        );
    }
    Ok(status)
}

#[tauri::command]
pub fn show_main_window(app: AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        #[cfg(desktop)]
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

#[tauri::command]
pub fn stop_focus(
    app: AppHandle,
    state: State<'_, AppState>,
    phrase: Option<String>,
) -> CmdResult<()> {
    let conn = conn!(state);
    crate::focus::stop(&conn, &state.shared, phrase.as_deref())?;
    crate::popup::close(&app);
    state.sync.wake();
    Ok(())
}

// ----------------------------------------------------------------- goals

#[tauri::command]
pub fn get_goals(state: State<'_, AppState>) -> CmdResult<Vec<crate::goals::GoalStatus>> {
    let conn = conn!(state);
    crate::goals::goal_status(&conn, &an::fmt_day(Local::now().date_naive())).map_err(err)
}

/// Creates (no `id`) or updates a goal.
#[tauri::command]
pub fn set_goal(
    state: State<'_, AppState>,
    id: Option<i64>,
    kind: String,
    category_id: Option<i64>,
    target: i64,
    enabled: bool,
    title: Option<String>,
) -> CmdResult<()> {
    if !crate::goals::KINDS.contains(&kind.as_str()) {
        return Err(format!("unknown goal type: {kind}"));
    }
    let title = title
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty());
    if title.as_ref().is_some_and(|t| t.len() > 120) {
        return Err("Keep the goal name under 120 characters".into());
    }
    if kind == "custom" && title.is_none() {
        return Err("Describe the goal, e.g. \"Read 20 pages\"".into());
    }
    let needs_category = kind.ends_with("_category");
    if needs_category != category_id.is_some() {
        return Err(if needs_category {
            "Choose a category".into()
        } else {
            "This goal has no category".into()
        });
    }
    let valid = match kind.as_str() {
        "custom" => true,
        "min_focus_sessions" => (1..=20).contains(&target),
        _ => (60..=24 * 3600).contains(&target),
    };
    let target = if kind == "custom" { 0 } else { target };
    if !valid {
        return Err("That target is out of range".into());
    }
    let conn = conn!(state);
    match id {
        Some(id) => {
            conn.execute(
                "UPDATE goals SET kind = ?1, category_id = ?2, target = ?3, enabled = ?4, title = ?5 WHERE id = ?6",
                params![kind, category_id, target, enabled, title, id],
            )
            .map_err(err)?;
            conn.execute(
                "DELETE FROM sent_notifications WHERE key LIKE ?1",
                params![format!("goal:{id}:%")],
            )
            .map_err(err)?;
            // The changed goal may be achieved again, on any device.
            conn.execute(
                "DELETE FROM shared_notices WHERE key LIKE 'goal:' || (SELECT uuid FROM goals WHERE id = ?1) || ':%'",
                params![id],
            )
            .map_err(err)?;
        }
        None => {
            conn.execute(
                "INSERT INTO goals (kind, category_id, target, enabled, title) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![kind, category_id, target, enabled, title],
            )
            .map_err(err)?;
        }
    }
    Ok(())
}

/// Ticks or unticks a custom goal for today.
#[tauri::command]
pub fn toggle_goal(state: State<'_, AppState>, id: i64) -> CmdResult<bool> {
    let conn = conn!(state);
    crate::goals::toggle_check(&conn, id, &an::fmt_day(Local::now().date_naive())).map_err(err)
}

#[tauri::command]
pub fn delete_goal(state: State<'_, AppState>, id: i64) -> CmdResult<()> {
    let conn = conn!(state);
    conn.execute("DELETE FROM goals WHERE id = ?1", params![id])
        .map_err(err)?;
    conn.execute(
        "DELETE FROM sent_notifications WHERE key LIKE ?1",
        params![format!("goal:{id}:%")],
    )
    .map_err(err)?;
    Ok(())
}

// -------------------------------------------------------------- insights

#[tauri::command]
pub fn get_insights(state: State<'_, AppState>) -> CmdResult<Vec<crate::insights::Insight>> {
    let conn = conn!(state);
    crate::insights::insights(&conn, Local::now().date_naive()).map_err(err)
}

// ----------------------------------------------------------------- notes

fn current_session_id(state: &AppState) -> Option<i64> {
    state
        .shared
        .focus
        .lock()
        .ok()
        .and_then(|f| f.as_ref().map(|a| a.session_id))
}

#[tauri::command]
pub fn get_notebooks(state: State<'_, AppState>) -> CmdResult<Vec<crate::notes::Notebook>> {
    let conn = conn!(state);
    crate::notes::notebooks(&conn).map_err(err)
}

#[tauri::command]
pub fn get_notes(
    state: State<'_, AppState>,
    notebook: Option<String>,
    search: Option<String>,
) -> CmdResult<Vec<crate::notes::Note>> {
    let conn = conn!(state);
    crate::notes::list(&conn, notebook.as_deref(), search.as_deref()).map_err(err)
}

/// New note; linked to the running focus/study session, if any.
#[tauri::command]
pub fn create_note(
    state: State<'_, AppState>,
    notebook: String,
    title: String,
    body: String,
) -> CmdResult<crate::notes::Note> {
    let session = current_session_id(&state);
    let conn = conn!(state);
    crate::notes::create(
        &conn,
        &notebook,
        &title,
        &body,
        session,
        Local::now().naive_local(),
    )
}

#[tauri::command]
pub fn update_note(
    state: State<'_, AppState>,
    id: i64,
    notebook: String,
    title: String,
    body: String,
) -> CmdResult<crate::notes::Note> {
    let conn = conn!(state);
    crate::notes::update(
        &conn,
        id,
        &notebook,
        &title,
        &body,
        Local::now().naive_local(),
    )
}

#[tauri::command]
pub fn delete_note(state: State<'_, AppState>, id: i64) -> CmdResult<()> {
    let conn = conn!(state);
    crate::notes::delete(&conn, id).map_err(err)
}

// ------------------------------------------------------ study resources

#[tauri::command]
pub fn get_resources(state: State<'_, AppState>) -> CmdResult<Vec<crate::notes::Resource>> {
    let conn = conn!(state);
    crate::notes::resources(&conn).map_err(err)
}

#[tauri::command]
pub fn add_resource(
    state: State<'_, AppState>,
    title: String,
    url: String,
    subject: Option<String>,
) -> CmdResult<()> {
    let conn = conn!(state);
    crate::notes::add_resource(&conn, &title, &url, subject.as_deref())
}

#[tauri::command]
pub fn delete_resource(state: State<'_, AppState>, id: i64) -> CmdResult<()> {
    let conn = conn!(state);
    crate::notes::delete_resource(&conn, id).map_err(err)
}

/// Opens an http(s) link in the default browser.
#[tauri::command]
pub fn open_url(app: AppHandle, url: String) -> CmdResult<()> {
    use tauri_plugin_opener::OpenerExt;
    if !crate::notes::valid_url(&url) {
        return Err("Only web links can be opened".into());
    }
    app.opener().open_url(url.trim(), None::<&str>).map_err(err)
}

/// Study time today and this week: Study-category apps and websites.
#[tauri::command]
pub fn get_study_time(state: State<'_, AppState>) -> CmdResult<(i64, i64)> {
    let conn = conn!(state);
    let study: i64 = conn
        .query_row("SELECT id FROM categories WHERE name = 'Study'", [], |r| {
            r.get(0)
        })
        .map_err(err)?;
    let today = Local::now().date_naive();
    let mut week = 0;
    for i in 0..7 {
        week += crate::goals::category_secs(&conn, &an::fmt_day(today - Duration::days(i)), study)
            .map_err(err)?;
    }
    let t = crate::goals::category_secs(&conn, &an::fmt_day(today), study).map_err(err)?;
    Ok((t, week))
}

// ------------------------------------------------------- administrator mode

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminMode {
    /// FocusTrack is running with administrator rights right now.
    pub elevated: bool,
    /// The start-up task is set up.
    pub enabled: bool,
    /// Apps FocusTrack tried to close but couldn't (they run as administrator).
    pub needs_admin: Vec<String>,
}

#[tauri::command]
pub async fn get_admin_mode(state: State<'_, AppState>) -> CmdResult<AdminMode> {
    let enabled = tauri::async_runtime::spawn_blocking(crate::admin::task_installed)
        .await
        .map_err(err)?;
    let mut needs: Vec<String> = state
        .shared
        .needs_admin
        .lock()
        .map(|s| s.iter().cloned().collect())
        .unwrap_or_default();
    needs.sort();
    Ok(AdminMode {
        elevated: crate::platform::is_elevated(),
        enabled,
        needs_admin: needs,
    })
}

/// Shows the Windows administrator prompt, sets up the start-up task and
/// restarts FocusTrack with administrator rights.
#[tauri::command]
pub async fn enable_admin_mode(app: AppHandle) -> CmdResult<()> {
    tauri::async_runtime::spawn_blocking(crate::admin::enable)
        .await
        .map_err(err)??;
    // The task starts FocusTrack at sign-in now; avoid a second, normal start.
    #[cfg(desktop)]
    let _ = app.autolaunch().disable();
    crate::admin::restart_elevated();
    crate::quit(&app);
    Ok(())
}

#[tauri::command]
pub async fn disable_admin_mode(app: AppHandle) -> CmdResult<()> {
    tauri::async_runtime::spawn_blocking(crate::admin::disable)
        .await
        .map_err(err)??;
    #[cfg(desktop)]
    let _ = app.autolaunch().enable();
    if crate::platform::is_elevated() {
        crate::admin::restart_normal();
        crate::quit(&app);
    }
    Ok(())
}

// ------------------------------------------------------------------ sync

/// Runs `f` on a fresh database connection off the main thread (network calls
/// and key derivation take a moment; the app stays responsive).
async fn with_sync<T: Send + 'static>(
    state: &State<'_, AppState>,
    f: impl FnOnce(&crate::sync::SyncHandle, &rusqlite::Connection) -> Result<T, String>
        + Send
        + 'static,
) -> CmdResult<T> {
    let handle = state.sync.clone();
    let path = state.data_dir.join("focustrack.db");
    tauri::async_runtime::spawn_blocking(move || {
        let conn = db::open(&path).map_err(|e| e.to_string())?;
        let out = f(&handle, &conn);
        crate::sync::refresh_status(&handle, &conn);
        out
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn sync_status(state: State<'_, AppState>) -> crate::sync::SyncStatus {
    state.sync.status()
}

/// Creates an account; returns the recovery key (shown once).
#[tauri::command]
pub async fn sync_sign_up(
    state: State<'_, AppState>,
    server: String,
    email: String,
    password: String,
    device_name: String,
) -> CmdResult<String> {
    with_sync(&state, move |h, c| {
        crate::sync::sign_up(
            h,
            c,
            &server,
            &email,
            &password,
            &device_name,
            focustrack_sync_proto::crypto::KdfParams::standard(),
        )
    })
    .await
}

#[tauri::command]
pub async fn sync_sign_in(
    state: State<'_, AppState>,
    server: String,
    email: String,
    password: String,
    device_name: String,
) -> CmdResult<()> {
    with_sync(&state, move |h, c| {
        crate::sync::sign_in(h, c, &server, &email, &password, &device_name)
    })
    .await
}

#[tauri::command]
pub async fn sync_sign_out(state: State<'_, AppState>) -> CmdResult<()> {
    with_sync(&state, crate::sync::sign_out).await
}

#[tauri::command]
pub fn sync_now(state: State<'_, AppState>) {
    state.sync.wake();
}

#[tauri::command]
pub async fn sync_set_enabled(state: State<'_, AppState>, enabled: bool) -> CmdResult<()> {
    with_sync(&state, move |h, c| crate::sync::set_enabled(h, c, enabled)).await
}

#[tauri::command]
pub async fn sync_devices(state: State<'_, AppState>) -> CmdResult<Vec<crate::sync::DeviceRow>> {
    with_sync(&state, crate::sync::devices).await
}

#[tauri::command]
pub async fn sync_remove_device(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    with_sync(&state, move |h, c| crate::sync::remove_device(h, c, &id)).await
}

#[tauri::command]
pub async fn sync_rename_device(state: State<'_, AppState>, name: String) -> CmdResult<()> {
    with_sync(&state, move |h, c| crate::sync::rename_device(h, c, &name)).await
}

// ------------------------------------------------------------------ phone

/// `windows`, `android`, … so the UI can show what applies on this device.
#[tauri::command]
pub fn get_platform() -> &'static str {
    crate::sync::PLATFORM
}

/// Whether the phone lets FocusTrack read screen time (Android usage access).
#[tauri::command]
pub fn phone_usage_access(app: AppHandle) -> CmdResult<bool> {
    use tauri_plugin_focustrack_usage::UsageExt;
    app.usage().has_access()
}

/// Status- and navigation-bar heights on phones (zero elsewhere).
#[tauri::command]
pub fn get_insets(app: AppHandle) -> CmdResult<tauri_plugin_focustrack_usage::Insets> {
    use tauri_plugin_focustrack_usage::UsageExt;
    app.usage().insets()
}

#[tauri::command]
pub fn open_usage_access(app: AppHandle) -> CmdResult<()> {
    use tauri_plugin_focustrack_usage::UsageExt;
    app.usage().open_access_settings()
}

/// Phones: whether app blocking (an accessibility service) is on (#69).
#[tauri::command]
pub fn phone_blocking(app: AppHandle) -> CmdResult<bool> {
    use tauri_plugin_focustrack_usage::UsageExt;
    app.usage().blocking_enabled()
}

/// Opens Android's accessibility settings, after FocusTrack's explanation.
#[tauri::command]
pub fn open_phone_blocking(app: AppHandle) -> CmdResult<()> {
    use tauri_plugin_focustrack_usage::UsageExt;
    app.usage().open_blocking_settings()
}

/// Imports new phone usage now (when the app comes to the front). Returns how
/// many sessions were added, or null without usage access.
#[tauri::command]
pub async fn import_phone_usage(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<Option<usize>> {
    let path = state.data_dir.join("focustrack.db");
    let sync = state.sync.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let conn = db::open(&path).map_err(err)?;
        let n = crate::phone::import(&app, &conn)?;
        if n.is_some_and(|n| n > 0) {
            sync.wake();
        }
        Ok(n)
    })
    .await
    .map_err(err)?
}

// ------------------------------------------------------- cloud data (#67)

/// Deletes synced history everywhere: all (`before` = null) or before a day,
/// for all devices or one. Returns how many records the server removed.
#[tauri::command]
pub async fn sync_delete_history(
    state: State<'_, AppState>,
    before: Option<String>,
    device: Option<String>,
) -> CmdResult<u64> {
    with_sync(&state, move |h, c| {
        crate::sync::delete_history(h, c, before.as_deref(), device.as_deref())
    })
    .await
}

#[tauri::command]
pub async fn sync_delete_account(state: State<'_, AppState>, password: String) -> CmdResult<()> {
    with_sync(&state, move |h, c| {
        crate::sync::delete_account(h, c, &password)
    })
    .await
}

#[tauri::command]
pub async fn sync_retention(state: State<'_, AppState>) -> CmdResult<Option<u32>> {
    with_sync(&state, crate::sync::retention).await
}

#[tauri::command]
pub async fn sync_set_retention(
    state: State<'_, AppState>,
    days: Option<u32>,
) -> CmdResult<Option<u32>> {
    with_sync(&state, move |h, c| crate::sync::set_retention(h, c, days)).await
}

/// Exports everything in the account (decrypted) to a JSON file in Downloads.
/// Returns the file's path.
#[tauri::command]
pub async fn sync_export(app: AppHandle, state: State<'_, AppState>) -> CmdResult<String> {
    let dir = app
        .path()
        .download_dir()
        .unwrap_or_else(|_| state.data_dir.clone());
    let file = dir.join(format!(
        "FocusTrack-export-{}.json",
        Local::now().format("%Y-%m-%d-%H%M")
    ));
    let out = file.clone();
    with_sync(&state, move |h, c| crate::sync::export(h, c, &out)).await?;
    Ok(file.to_string_lossy().into_owned())
}

/// Pause tracking or turn sync off on a device (#66).
#[tauri::command]
pub async fn sync_device_control(
    state: State<'_, AppState>,
    id: String,
    pause_tracking: Option<bool>,
    sync_enabled: Option<bool>,
) -> CmdResult<()> {
    with_sync(&state, move |h, c| {
        crate::sync::set_device_control(h, c, &id, pause_tracking, sync_enabled)
    })
    .await
}
