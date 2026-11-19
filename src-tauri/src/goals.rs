//! Daily goals (SRS 3.2; improvements doc §3): screen-time caps, category
//! minimums/maximums, focus-session counts — measured automatically — and
//! custom goals ("Read 20 pages") that the user ticks off each day.

use crate::analytics;
use crate::categories::browser_family;
use crate::focus;
use rusqlite::{params, Connection};
use serde::Serialize;

pub const KINDS: &[&str] = &[
    "max_screen",
    "min_category",
    "max_category",
    "min_focus_sessions",
    "custom",
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalStatus {
    pub id: i64,
    pub kind: String,
    pub category_id: Option<i64>,
    pub category: Option<String>,
    pub color: String,
    /// Seconds for time goals, a count for focus sessions, unused for custom goals.
    pub target: i64,
    /// User-given name; required for custom goals.
    pub title: Option<String>,
    pub enabled: bool,
    pub current: i64,
    /// `achieved` | `in_progress` | `on_track` | `exceeded`
    pub state: String,
    pub label: String,
}

fn fmt(secs: i64) -> String {
    let (h, m) = (secs / 3600, (secs % 3600) / 60);
    match (h, m) {
        (0, m) => format!("{m}m"),
        (h, 0) => format!("{h}h"),
        (h, m) => format!("{h}h {m}m"),
    }
}

pub fn label(kind: &str, category: Option<&str>, target: i64, title: Option<&str>) -> String {
    let cat = category.unwrap_or("category");
    let title = title.map(str::trim).filter(|t| !t.is_empty());
    match (kind, title) {
        ("custom", Some(t)) => return t.to_string(),
        ("max_screen" | "max_category", Some(t)) => return format!("{t}: under {}", fmt(target)),
        ("min_category", Some(t)) => return format!("{t}: {}", fmt(target)),
        ("min_focus_sessions", Some(t)) => {
            return format!(
                "{t}: {target} session{}",
                if target == 1 { "" } else { "s" }
            )
        }
        _ => {}
    }
    match kind {
        "custom" => "Custom goal".to_string(),
        "max_screen" => format!("Keep screen time under {}", fmt(target)),
        "min_category" => format!("Spend at least {} on {}", fmt(target), cat),
        "max_category" => format!("Keep {} under {}", cat, fmt(target)),
        "min_focus_sessions" => format!(
            "Complete {} focus session{}",
            target,
            if target == 1 { "" } else { "s" }
        ),
        _ => kind.to_string(),
    }
}

pub fn state(kind: &str, current: i64, target: i64) -> &'static str {
    if kind.starts_with("max_") {
        if current > target {
            "exceeded"
        } else {
            "on_track"
        }
    } else if current >= target {
        "achieved"
    } else {
        "in_progress"
    }
}

/// Time in a category today: its (non-browser) apps plus its websites.
/// Browser time is left out of the app part so it isn't counted twice.
pub fn category_secs(conn: &Connection, day: &str, category_id: i64) -> rusqlite::Result<i64> {
    let mut stmt = conn.prepare(
        "SELECT a.exe_name, COALESCE(SUM(s.duration_secs), 0) FROM usage_sessions s
         JOIN applications a ON a.id = s.app_id
         WHERE s.day = ?1 AND a.category_id = ?2 GROUP BY a.id",
    )?;
    let rows = stmt.query_map(params![day, category_id], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
    })?;
    let mut total = 0;
    for row in rows {
        let (exe, secs) = row?;
        if browser_family(&exe).is_none() {
            total += secs;
        }
    }
    let web: i64 = conn.query_row(
        "SELECT COALESCE(SUM(s.duration_secs), 0) FROM website_sessions s
         JOIN websites w ON w.id = s.website_id WHERE s.day = ?1 AND w.category_id = ?2",
        params![day, category_id],
        |r| r.get(0),
    )?;
    Ok(total + web)
}

pub fn goal_status(conn: &Connection, day: &str) -> rusqlite::Result<Vec<GoalStatus>> {
    let mut stmt = conn.prepare(
        "SELECT g.id, g.kind, g.category_id, c.name, COALESCE(c.color, '#6366f1'), g.target, g.enabled, g.title,
                EXISTS (SELECT 1 FROM goal_checks k WHERE k.goal_id = g.id AND k.day = ?1)
         FROM goals g LEFT JOIN categories c ON c.id = g.category_id ORDER BY g.id",
    )?;
    type Row = (
        i64,
        String,
        Option<i64>,
        Option<String>,
        String,
        i64,
        bool,
        Option<String>,
        bool,
    );
    let rows: Vec<Row> = stmt
        .query_map(params![day], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get::<_, i64>(6)? != 0,
                r.get(7)?,
                r.get::<_, i64>(8)? != 0,
            ))
        })?
        .collect::<Result<_, _>>()?;

    let (active, idle) = analytics::totals(conn, day, day)?;
    let (_, focus_done) = focus::totals(conn, day, day)?;
    let mut out = Vec::new();
    for (id, kind, category_id, category, color, target, enabled, title, checked) in rows {
        let (current, target) = match kind.as_str() {
            "custom" => (checked as i64, 1),
            _ => (0, target),
        };
        let current = match kind.as_str() {
            "custom" => current,
            "max_screen" => active + idle,
            "min_focus_sessions" => focus_done,
            _ => match category_id {
                Some(c) => category_secs(conn, day, c)?,
                None => 0,
            },
        };
        out.push(GoalStatus {
            label: label(&kind, category.as_deref(), target, title.as_deref()),
            title,
            state: state(&kind, current, target).to_string(),
            id,
            kind,
            category_id,
            category,
            color,
            target,
            enabled,
            current,
        });
    }
    Ok(out)
}

/// Notification for a goal that was just achieved or exceeded (sent once a day).
pub fn goal_notice(g: &GoalStatus) -> Option<(String, String, String)> {
    if !g.enabled {
        return None;
    }
    match g.state.as_str() {
        // You ticked it yourself; no need to tell you.
        "achieved" if g.kind == "custom" => None,
        "achieved" => Some((
            format!("goal:{}:achieved", g.id),
            "Goal achieved".into(),
            format!("{} — done for today.", g.label),
        )),
        "exceeded" => Some((
            format!("goal:{}:exceeded", g.id),
            "Daily goal missed".into(),
            format!("{} — you're at {} today.", g.label, fmt(g.current)),
        )),
        _ => None,
    }
}

/// Ticks or unticks a custom goal for `day`. Returns the new state.
pub fn toggle_check(conn: &Connection, id: i64, day: &str) -> rusqlite::Result<bool> {
    let removed = conn.execute(
        "DELETE FROM goal_checks WHERE goal_id = ?1 AND day = ?2",
        params![id, day],
    )?;
    if removed > 0 {
        return Ok(false);
    }
    // The same goal ticked on the same day has the same ID on every device.
    let goal_uuid: Option<String> =
        conn.query_row("SELECT uuid FROM goals WHERE id = ?1", params![id], |r| {
            r.get(0)
        })?;
    conn.execute(
        "INSERT INTO goal_checks (goal_id, day, uuid) VALUES (?1, ?2, ?3)",
        params![id, day, goal_uuid.map(|g| check_id(&g, day))],
    )?;
    Ok(true)
}

/// Stable record ID for "goal `goal_uuid` ticked on `day`".
pub fn check_id(goal_uuid: &str, day: &str) -> String {
    focustrack_sync_proto::name_id("goal_check", &format!("{goal_uuid}|{day}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use chrono::NaiveDate;

    #[test]
    fn labels_and_states() {
        assert_eq!(
            label("max_screen", None, 6 * 3600, None),
            "Keep screen time under 6h"
        );
        assert_eq!(
            label("min_category", Some("Study"), 7200, None),
            "Spend at least 2h on Study"
        );
        assert_eq!(
            label("min_category", Some("Development"), 7200, Some("Coding")),
            "Coding: 2h"
        );
        assert_eq!(
            label("max_category", Some("Social Media"), 1800, None),
            "Keep Social Media under 30m"
        );
        assert_eq!(
            label("min_focus_sessions", None, 3, None),
            "Complete 3 focus sessions"
        );
        assert_eq!(
            label("custom", None, 0, Some("Read 20 pages")),
            "Read 20 pages"
        );
        assert_eq!(state("max_screen", 100, 200), "on_track");
        assert_eq!(state("max_screen", 300, 200), "exceeded");
        assert_eq!(state("min_category", 100, 200), "in_progress");
        assert_eq!(state("min_focus_sessions", 3, 3), "achieved");
    }

    #[test]
    fn category_time_combines_apps_and_websites_without_double_counting() {
        let conn = db::open_in_memory().unwrap();
        let t = |h| {
            NaiveDate::from_ymd_opt(2026, 11, 18)
                .unwrap()
                .and_hms_opt(h, 0, 0)
                .unwrap()
        };
        let edu: i64 = conn
            .query_row("SELECT id FROM categories WHERE name = 'Study'", [], |r| {
                r.get(0)
            })
            .unwrap();
        let (anki, _) =
            db::get_or_create_app(&conn, "anki.exe", "", || "Anki".into(), t(9)).unwrap();
        let (chrome, _) =
            db::get_or_create_app(&conn, "chrome.exe", "", || "Chrome".into(), t(9)).unwrap();
        conn.execute(
            "UPDATE applications SET category_id = ?1 WHERE id = ?2",
            params![edu, chrome],
        )
        .unwrap();
        for (app, h) in [(anki, 9), (chrome, 10)] {
            let id = db::insert_usage(&conn, app, t(h)).unwrap();
            db::update_span(
                &conn,
                db::SpanKind::Usage,
                id,
                t(h),
                t(h) + chrono::Duration::minutes(30),
            )
            .unwrap();
        }
        let site = db::get_or_create_website(&conn, "coursera.org", t(10)).unwrap();
        let w = db::insert_website_session(&conn, site, "chrome", t(10)).unwrap();
        db::update_span(
            &conn,
            db::SpanKind::Website,
            w,
            t(10),
            t(10) + chrono::Duration::minutes(20),
        )
        .unwrap();

        // Anki 30m + coursera 20m; the browser's own 30m is not added again.
        assert_eq!(category_secs(&conn, "2026-11-18", edu).unwrap(), 50 * 60);

        conn.execute(
            "INSERT INTO goals (kind, category_id, target) VALUES ('min_category', ?1, 3600)",
            params![edu],
        )
        .unwrap();
        let g = &goal_status(&conn, "2026-11-18").unwrap()[0];
        assert_eq!((g.current, g.state.as_str()), (3000, "in_progress"));
        assert!(goal_notice(g).is_none());
    }

    #[test]
    fn custom_goals_are_ticked_per_day() {
        let conn = db::open_in_memory().unwrap();
        conn.execute(
            "INSERT INTO goals (kind, title) VALUES ('custom', 'Exercise for 30 minutes')",
            [],
        )
        .unwrap();
        let g = &goal_status(&conn, "2026-11-18").unwrap()[0];
        assert_eq!(
            (g.label.as_str(), g.state.as_str()),
            ("Exercise for 30 minutes", "in_progress")
        );
        assert!(toggle_check(&conn, g.id, "2026-11-18").unwrap());
        let g = &goal_status(&conn, "2026-11-18").unwrap()[0];
        assert_eq!((g.current, g.state.as_str()), (1, "achieved"));
        assert!(
            goal_notice(g).is_none(),
            "no notification for goals you tick yourself"
        );
        assert_eq!(
            goal_status(&conn, "2026-11-19").unwrap()[0].state,
            "in_progress",
            "resets the next day"
        );
        assert!(!toggle_check(&conn, g.id, "2026-11-18").unwrap());
    }
}
