//! Tauri commands invoked by the React frontend.

use crate::analytics::{self as an, AppUsage, CategoryUsage, DayTotal};
use crate::db;
use crate::tracker::LiveStatus;
use crate::{AppState, TrayPause};
use chrono::{Datelike, Duration, Local, Weekday};
use rusqlite::params;
use serde::Serialize;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_autostart::ManagerExt;

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
    pub categories: Vec<CategoryUsage>,
    pub week: Vec<DayTotal>,
    pub hourly: Vec<i64>,
    pub live: LiveStatus,
}

#[tauri::command]
pub fn get_dashboard(state: State<'_, AppState>) -> CmdResult<Dashboard> {
    let conn = conn!(state);
    let today = Local::now().date_naive();
    let t = an::fmt_day(today);
    let yesterday = an::fmt_day(today - Duration::days(1));

    let (active, idle) = an::totals(&conn, &t, &t).map_err(err)?;
    let (prev_active, _) = an::totals(&conn, &yesterday, &yesterday).map_err(err)?;
    let week = an::daily_totals(&conn, today - Duration::days(6), today).map_err(err)?;
    let month = an::daily_totals(&conn, today - Duration::days(29), today).map_err(err)?;
    let apps = an::app_usage(&conn, &t, &t).map_err(err)?;
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
        categories,
        week,
        hourly: hours.to_vec(),
        live: state.shared.status(),
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
            Ok(Category { id: r.get(0)?, name: r.get(1)?, color: r.get(2)?, kind: r.get(3)? })
        })
        .map_err(err)?;
    let out: Result<Vec<_>, _> = rows.collect();
    out.map_err(err)
}

#[tauri::command]
pub fn set_app_category(state: State<'_, AppState>, app_id: i64, category_id: Option<i64>) -> CmdResult<()> {
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
    conn.execute("UPDATE applications SET display_name = ?1 WHERE id = ?2", params![name, app_id])
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
    pub categories: Vec<CategoryUsage>,
    pub hourly: Vec<i64>,
    /// 7 rows (Mon..Sun) x 24 hours, active seconds.
    pub heatmap: Vec<Vec<i64>>,
}

#[tauri::command]
pub fn get_report(state: State<'_, AppState>, from: String, to: String) -> CmdResult<Report> {
    let (f, t) = (an::parse_day(&from)?, an::parse_day(&to)?);
    if f > t {
        return Err("Start date is after end date".into());
    }
    let conn = conn!(state);
    let len = (t - f).num_days() + 1;
    let (pf, pt) = (f - Duration::days(len), f - Duration::days(1));

    let (active, idle) = an::totals(&conn, &from, &to).map_err(err)?;
    let (prev_active, _) = an::totals(&conn, &an::fmt_day(pf), &an::fmt_day(pt)).map_err(err)?;
    let days = an::daily_totals(&conn, f, t).map_err(err)?;
    let apps = an::app_usage(&conn, &from, &to).map_err(err)?;
    let categories = an::category_usage(&conn, &from, &to).map_err(err)?;
    let spans = an::active_spans(&conn, &from, &to).map_err(err)?;
    let (hours, heat) = an::hour_buckets(&spans);
    let (productive, distracting) = an::productive_split(&categories);

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
        categories,
        hourly: hours.to_vec(),
        heatmap: heat.iter().map(|r| r.to_vec()).collect(),
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
    from: &str,
    to: &str,
    search: &str,
    limit: i64,
) -> rusqlite::Result<Vec<SessionRow>> {
    let mut stmt = conn.prepare(HISTORY_SQL)?;
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
) -> CmdResult<Vec<SessionRow>> {
    an::parse_day(&from)?;
    an::parse_day(&to)?;
    let conn = conn!(state);
    query_history(&conn, &from, &to, search.as_deref().unwrap_or(""), limit.unwrap_or(500).clamp(1, 5000))
        .map_err(err)
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Writes a CSV of app sessions to the Downloads folder; returns its path.
#[tauri::command]
pub fn export_csv(
    app: AppHandle,
    state: State<'_, AppState>,
    from: String,
    to: String,
    search: Option<String>,
) -> CmdResult<String> {
    an::parse_day(&from)?;
    an::parse_day(&to)?;
    let rows = {
        let conn = conn!(state);
        query_history(&conn, &from, &to, search.as_deref().unwrap_or(""), i64::MAX).map_err(err)?
    };
    let mut out = String::from("application,executable,category,date,start,end,duration_seconds,duration_minutes\n");
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
    let path = dir.join(format!("focustrack-{from}-to-{to}.csv"));
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
}

#[tauri::command]
pub fn get_settings(app: AppHandle, state: State<'_, AppState>) -> CmdResult<Settings> {
    let conn = conn!(state);
    let s = db::load_settings(&conn).map_err(err)?;
    Ok(Settings {
        paused: state.shared.paused.load(Ordering::SeqCst),
        idle_threshold_secs: s.idle_threshold_secs,
        autostart: app.autolaunch().is_enabled().unwrap_or(false),
        theme: s.theme,
        data_dir: state.data_dir.to_string_lossy().into_owned(),
    })
}

/// Shared by the settings screen and the tray menu.
pub fn apply_paused(app: &AppHandle, paused: bool) -> CmdResult<()> {
    let state = app.state::<AppState>();
    state.shared.paused.store(paused, Ordering::SeqCst);
    {
        let conn = conn!(state);
        db::set_setting(&conn, "paused", if paused { "1" } else { "0" }).map_err(err)?;
    }
    if let Some(item) = app.try_state::<TrayPause>() {
        let _ = item.0.set_text(if paused { "Resume tracking" } else { "Pause tracking" });
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
    state.shared.idle_threshold_secs.store(secs, Ordering::SeqCst);
    let conn = conn!(state);
    db::set_setting(&conn, "idle_threshold_secs", &secs.to_string()).map_err(err)
}

#[tauri::command]
pub fn set_autostart(app: AppHandle, enabled: bool) -> CmdResult<()> {
    let launcher = app.autolaunch();
    let result = if enabled { launcher.enable() } else { launcher.disable() };
    result.map_err(err)
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
