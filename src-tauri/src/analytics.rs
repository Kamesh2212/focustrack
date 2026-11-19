//! Read-side queries and aggregations used by the dashboard and reports.

use crate::db::{parse_time, DAY_FMT};
use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, Timelike};
use rusqlite::{params, Connection};
use serde::Serialize;
use std::collections::HashMap;

/// Gap between active spans that starts a new "computer usage session".
pub const SESSION_GAP_SECS: i64 = 5 * 60;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DayTotal {
    pub day: String,
    pub active_secs: i64,
    pub idle_secs: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUsage {
    pub id: i64,
    pub name: String,
    pub exe_name: String,
    pub category_id: Option<i64>,
    pub category: String,
    pub color: String,
    pub kind: String,
    pub secs: i64,
    pub sessions: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryUsage {
    pub name: String,
    pub color: String,
    pub kind: String,
    pub secs: i64,
}

pub fn parse_day(s: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(s, DAY_FMT).map_err(|_| format!("invalid date: {s}"))
}

pub fn fmt_day(d: NaiveDate) -> String {
    d.format(DAY_FMT).to_string()
}

pub fn days_between(from: NaiveDate, to: NaiveDate) -> Vec<NaiveDate> {
    let mut out = Vec::new();
    let mut d = from;
    while d <= to {
        out.push(d);
        d += Duration::days(1);
    }
    out
}

pub fn totals(conn: &Connection, from: &str, to: &str) -> rusqlite::Result<(i64, i64)> {
    let active: i64 = conn.query_row(
        "SELECT COALESCE(SUM(duration_secs), 0) FROM usage_sessions WHERE day BETWEEN ?1 AND ?2",
        params![from, to],
        |r| r.get(0),
    )?;
    let idle: i64 = conn.query_row(
        "SELECT COALESCE(SUM(duration_secs), 0) FROM idle_periods WHERE day BETWEEN ?1 AND ?2",
        params![from, to],
        |r| r.get(0),
    )?;
    Ok((active, idle))
}

/// One entry per calendar day in the range (zero-filled).
pub fn daily_totals(
    conn: &Connection,
    from: NaiveDate,
    to: NaiveDate,
) -> rusqlite::Result<Vec<DayTotal>> {
    let (f, t) = (fmt_day(from), fmt_day(to));
    let mut map: HashMap<String, (i64, i64)> = HashMap::new();
    {
        let mut stmt = conn.prepare(
            "SELECT day, SUM(duration_secs) FROM usage_sessions WHERE day BETWEEN ?1 AND ?2 GROUP BY day",
        )?;
        let rows = stmt.query_map(params![f, t], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })?;
        for row in rows {
            let (d, s) = row?;
            map.entry(d).or_default().0 = s;
        }
    }
    {
        let mut stmt = conn.prepare(
            "SELECT day, SUM(duration_secs) FROM idle_periods WHERE day BETWEEN ?1 AND ?2 GROUP BY day",
        )?;
        let rows = stmt.query_map(params![f, t], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })?;
        for row in rows {
            let (d, s) = row?;
            map.entry(d).or_default().1 = s;
        }
    }
    Ok(days_between(from, to)
        .into_iter()
        .map(|d| {
            let key = fmt_day(d);
            let (a, i) = map.get(&key).copied().unwrap_or((0, 0));
            DayTotal {
                day: key,
                active_secs: a,
                idle_secs: i,
            }
        })
        .collect())
}

/// Every known application with its usage in the range (unused apps get 0).
pub fn app_usage(conn: &Connection, from: &str, to: &str) -> rusqlite::Result<Vec<AppUsage>> {
    let mut stmt = conn.prepare(
        "SELECT a.id, a.display_name, a.exe_name, c.id,
                COALESCE(c.name, 'Other'), COALESCE(c.color, '#94a3b8'), COALESCE(c.kind, 'neutral'),
                COALESCE(SUM(s.duration_secs), 0) AS secs, COUNT(s.id)
         FROM applications a
         LEFT JOIN categories c ON c.id = a.category_id
         LEFT JOIN usage_sessions s ON s.app_id = a.id AND s.day BETWEEN ?1 AND ?2
         GROUP BY a.id
         ORDER BY secs DESC, a.display_name COLLATE NOCASE",
    )?;
    let rows = stmt.query_map(params![from, to], |r| {
        Ok(AppUsage {
            id: r.get(0)?,
            name: r.get(1)?,
            exe_name: r.get(2)?,
            category_id: r.get(3)?,
            category: r.get(4)?,
            color: r.get(5)?,
            kind: r.get(6)?,
            secs: r.get(7)?,
            sessions: r.get(8)?,
        })
    })?;
    let out = rows.collect();
    out
}

/// One day's time in one category.
struct CatRow {
    day: String,
    cat: CategoryUsage,
}

/// Screen time per day and category. Time in a browser is split by the
/// categories of the websites visited in it (#14), so YouTube counts as
/// Entertainment and GitHub as Development rather than both as "Other".
/// Browser time without a recorded website keeps the browser's own category.
fn category_rows(conn: &Connection, from: &str, to: &str) -> rusqlite::Result<Vec<CatRow>> {
    let cat = |r: &rusqlite::Row, i: usize| -> rusqlite::Result<CategoryUsage> {
        Ok(CategoryUsage {
            name: r.get(i)?,
            color: r.get(i + 1)?,
            kind: r.get(i + 2)?,
            secs: r.get(i + 3)?,
        })
    };
    const CAT: &str =
        "COALESCE(c.name, 'Other'), COALESCE(c.color, '#94a3b8'), COALESCE(c.kind, 'neutral')";

    let mut stmt = conn.prepare(&format!(
        "SELECT s.day, a.exe_name, {CAT}, SUM(s.duration_secs)
         FROM usage_sessions s
         JOIN applications a ON a.id = s.app_id
         LEFT JOIN categories c ON c.id = a.category_id
         WHERE s.day BETWEEN ?1 AND ?2
         GROUP BY s.day, a.id"
    ))?;
    let apps = stmt
        .query_map(params![from, to], |r| {
            let exe: String = r.get(1)?;
            Ok((
                crate::categories::browser_family(&exe),
                CatRow {
                    day: r.get(0)?,
                    cat: cat(r, 2)?,
                },
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut stmt = conn.prepare(&format!(
        "SELECT s.day, s.browser, {CAT}, SUM(s.duration_secs)
         FROM website_sessions s
         JOIN websites w ON w.id = s.website_id
         LEFT JOIN categories c ON c.id = w.category_id
         WHERE s.day BETWEEN ?1 AND ?2
         GROUP BY s.day, s.browser, 3, 4, 5"
    ))?;
    let sites = stmt
        .query_map(params![from, to], |r| {
            Ok((
                r.get::<_, String>(1)?,
                CatRow {
                    day: r.get(0)?,
                    cat: cat(r, 2)?,
                },
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    // Browser and website time per (day, browser family).
    let mut app_total: HashMap<(String, String), i64> = HashMap::new();
    for (family, row) in &apps {
        if let Some(f) = family {
            *app_total
                .entry((row.day.clone(), f.to_string()))
                .or_default() += row.cat.secs;
        }
    }
    let mut web_total: HashMap<(String, String), i64> = HashMap::new();
    for (browser, row) in &sites {
        *web_total
            .entry((row.day.clone(), browser.clone()))
            .or_default() += row.cat.secs;
    }
    // (seconds of browser time that move to website categories, browser total, website total)
    let moved = |day: &str, family: &str| -> (i64, i64, i64) {
        let key = (day.to_string(), family.to_string());
        let a = app_total.get(&key).copied().unwrap_or(0);
        let w = web_total.get(&key).copied().unwrap_or(0);
        (a.min(w), a, w)
    };
    let scale = |secs: i64, num: i64, den: i64| -> i64 {
        if den == 0 {
            0
        } else {
            ((secs as i128 * num as i128 + den as i128 / 2) / den as i128) as i64
        }
    };

    let mut out = Vec::with_capacity(apps.len() + sites.len());
    for (family, mut row) in apps {
        if let Some(f) = family {
            let (credit, a, _) = moved(&row.day, f);
            row.cat.secs = scale(row.cat.secs, a - credit, a);
        }
        out.push(row);
    }
    for (browser, mut row) in sites {
        // Only time spent with the browser in front counts (it always should be).
        let (credit, _, w) = moved(&row.day, &browser);
        row.cat.secs = scale(row.cat.secs, credit, w);
        out.push(row);
    }
    out.retain(|r| r.cat.secs > 0);
    Ok(out)
}

pub fn category_usage(
    conn: &Connection,
    from: &str,
    to: &str,
) -> rusqlite::Result<Vec<CategoryUsage>> {
    let mut merged: Vec<CategoryUsage> = Vec::new();
    for CatRow { cat, .. } in category_rows(conn, from, to)? {
        match merged
            .iter_mut()
            .find(|c| c.name == cat.name && c.color == cat.color && c.kind == cat.kind)
        {
            Some(c) => c.secs += cat.secs,
            None => merged.push(cat),
        }
    }
    merged.sort_by(|a, b| b.secs.cmp(&a.secs).then_with(|| a.name.cmp(&b.name)));
    Ok(merged)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SiteUsage {
    pub id: i64,
    pub domain: String,
    pub category_id: Option<i64>,
    pub category: String,
    pub color: String,
    pub kind: String,
    pub secs: i64,
    pub sessions: i64,
}

/// Every known website with its usage in the range (unused sites get 0).
pub fn site_usage(conn: &Connection, from: &str, to: &str) -> rusqlite::Result<Vec<SiteUsage>> {
    let mut stmt = conn.prepare(
        "SELECT w.id, w.domain, c.id,
                COALESCE(c.name, 'Other'), COALESCE(c.color, '#94a3b8'), COALESCE(c.kind, 'neutral'),
                COALESCE(SUM(s.duration_secs), 0) AS secs, COUNT(s.id)
         FROM websites w
         LEFT JOIN categories c ON c.id = w.category_id
         LEFT JOIN website_sessions s ON s.website_id = w.id AND s.day BETWEEN ?1 AND ?2
         GROUP BY w.id
         ORDER BY secs DESC, w.domain",
    )?;
    let rows = stmt.query_map(params![from, to], |r| {
        Ok(SiteUsage {
            id: r.get(0)?,
            domain: r.get(1)?,
            category_id: r.get(2)?,
            category: r.get(3)?,
            color: r.get(4)?,
            kind: r.get(5)?,
            secs: r.get(6)?,
            sessions: r.get(7)?,
        })
    })?;
    let out = rows.collect();
    out
}

pub fn site_total(conn: &Connection, from: &str, to: &str) -> rusqlite::Result<i64> {
    conn.query_row(
        "SELECT COALESCE(SUM(duration_secs), 0) FROM website_sessions WHERE day BETWEEN ?1 AND ?2",
        params![from, to],
        |r| r.get(0),
    )
}

/// Active spans in the range, ordered by start.
pub fn active_spans(
    conn: &Connection,
    from: &str,
    to: &str,
) -> rusqlite::Result<Vec<(NaiveDateTime, NaiveDateTime)>> {
    let mut stmt = conn.prepare(
        "SELECT start_time, end_time FROM usage_sessions
         WHERE day BETWEEN ?1 AND ?2 AND duration_secs > 0 ORDER BY start_time",
    )?;
    let rows = stmt.query_map(params![from, to], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (s, e) = row?;
        if let (Some(s), Some(e)) = (parse_time(&s), parse_time(&e)) {
            out.push((s, e));
        }
    }
    Ok(out)
}

/// Splits spans on hour boundaries. Returns (seconds per hour of day,
/// seconds per [weekday Mon=0][hour]).
pub fn hour_buckets(spans: &[(NaiveDateTime, NaiveDateTime)]) -> ([i64; 24], [[i64; 24]; 7]) {
    let mut hours = [0i64; 24];
    let mut heat = [[0i64; 24]; 7];
    for &(start, end) in spans {
        let mut cur = start;
        while cur < end {
            let hour_start = cur.date().and_hms_opt(cur.hour(), 0, 0).unwrap();
            let seg_end = (hour_start + Duration::hours(1)).min(end);
            let secs = (seg_end - cur).num_seconds();
            let h = cur.hour() as usize;
            hours[h] += secs;
            heat[cur.weekday().num_days_from_monday() as usize][h] += secs;
            cur = seg_end;
        }
    }
    (hours, heat)
}

/// Number of distinct usage sessions: active spans separated by more than
/// `gap_secs` of inactivity (idle, locked, asleep) start a new session.
pub fn count_sessions(spans: &[(NaiveDateTime, NaiveDateTime)], gap_secs: i64) -> i64 {
    let mut count = 0;
    let mut last_end: Option<NaiveDateTime> = None;
    for &(start, end) in spans {
        match last_end {
            Some(prev) if (start - prev).num_seconds() <= gap_secs => {}
            _ => count += 1,
        }
        last_end = Some(last_end.map_or(end, |p| p.max(end)));
    }
    count
}

pub fn productive_split(categories: &[CategoryUsage]) -> (i64, i64) {
    let productive = categories
        .iter()
        .filter(|c| c.kind == "productive")
        .map(|c| c.secs)
        .sum();
    let distracting = categories
        .iter()
        .filter(|c| c.kind == "distracting")
        .map(|c| c.secs)
        .sum();
    (productive, distracting)
}

/// Average active seconds over days that have any recorded activity.
pub fn average_of_active_days(days: &[DayTotal]) -> i64 {
    let used: Vec<i64> = days
        .iter()
        .map(|d| d.active_secs)
        .filter(|s| *s > 0)
        .collect();
    if used.is_empty() {
        0
    } else {
        used.iter().sum::<i64>() / used.len() as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(d: u32, h: u32, m: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, d)
            .unwrap()
            .and_hms_opt(h, m, 0)
            .unwrap()
    }

    #[test]
    fn browser_time_counts_by_website_category() {
        use crate::db::{self, SpanKind};
        let conn = db::open_in_memory().unwrap();
        let t = |m: i64| at(18, 9, 0) + Duration::minutes(m);
        let span = |kind, id, from, to| db::update_span(&conn, kind, id, t(from), t(to)).unwrap();
        // 60 minutes in Brave: 30 on YouTube, 20 on GitHub, 10 with no site recorded.
        let (brave, _) =
            db::get_or_create_app(&conn, "brave.exe", "", || "Brave".into(), t(0)).unwrap();
        let id = db::insert_usage(&conn, brave, t(0)).unwrap();
        span(SpanKind::Usage, id, 0, 60);
        let yt = db::get_or_create_website(&conn, "youtube.com", t(0)).unwrap();
        let id = db::insert_website_session(&conn, yt, "brave", t(0)).unwrap();
        span(SpanKind::Website, id, 0, 30);
        let gh = db::get_or_create_website(&conn, "github.com", t(30)).unwrap();
        let id = db::insert_website_session(&conn, gh, "brave", t(30)).unwrap();
        span(SpanKind::Website, id, 30, 50);
        // 15 minutes in a game.
        let (game, _) =
            db::get_or_create_app(&conn, "game.exe", "", || "Game".into(), t(60)).unwrap();
        conn.execute("UPDATE applications SET category_id = (SELECT id FROM categories WHERE name = 'Gaming') WHERE id = ?1", [game]).unwrap();
        let id = db::insert_usage(&conn, game, t(60)).unwrap();
        span(SpanKind::Usage, id, 60, 75);

        let cats = category_usage(&conn, "2026-11-18", "2026-11-18").unwrap();
        let secs = |name: &str| cats.iter().find(|c| c.name == name).map_or(0, |c| c.secs);
        assert_eq!(secs("Entertainment"), 30 * 60);
        assert_eq!(secs("Development"), 20 * 60);
        assert_eq!(secs("Gaming"), 15 * 60);
        assert_eq!(
            cats.iter().map(|c| c.secs).sum::<i64>(),
            75 * 60,
            "total screen time unchanged"
        );
        // Gaming + YouTube are distracting; GitHub is productive.
        assert_eq!(productive_split(&cats), (20 * 60, 45 * 60));

        let day = &daily_kinds(&conn, at(18, 0, 0).date(), at(18, 0, 0).date()).unwrap()[0];
        assert_eq!(
            (day.productive_secs, day.distracting_secs, day.neutral_secs),
            (20 * 60, 45 * 60, 10 * 60)
        );
    }

    #[test]
    fn hour_buckets_split_across_hours() {
        // Wed 18 Nov 2026, 09:30 -> 11:15
        let (hours, heat) = hour_buckets(&[(at(18, 9, 30), at(18, 11, 15))]);
        assert_eq!(hours[9], 30 * 60);
        assert_eq!(hours[10], 60 * 60);
        assert_eq!(hours[11], 15 * 60);
        assert_eq!(heat[2][10], 3600, "Wednesday is index 2");
        assert_eq!(hours.iter().sum::<i64>(), 105 * 60);
    }

    #[test]
    fn sessions_split_on_long_gaps() {
        let spans = [
            (at(18, 9, 0), at(18, 9, 30)),
            (at(18, 9, 32), at(18, 10, 0)), // 2 min gap: same session
            (at(18, 13, 0), at(18, 14, 0)), // lunch: new session
        ];
        assert_eq!(count_sessions(&spans, SESSION_GAP_SECS), 2);
        assert_eq!(count_sessions(&[], SESSION_GAP_SECS), 0);
    }

    #[test]
    fn days_between_is_inclusive() {
        let d = |n| NaiveDate::from_ymd_opt(2026, 11, n).unwrap();
        assert_eq!(days_between(d(16), d(18)).len(), 3);
        assert!(days_between(d(18), d(16)).is_empty());
    }

    #[test]
    fn average_ignores_empty_days() {
        let mk = |s| DayTotal {
            day: String::new(),
            active_secs: s,
            idle_secs: 0,
        };
        assert_eq!(average_of_active_days(&[mk(0), mk(100), mk(300)]), 200);
        assert_eq!(average_of_active_days(&[mk(0)]), 0);
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DayKinds {
    pub day: String,
    pub productive_secs: i64,
    pub neutral_secs: i64,
    pub distracting_secs: i64,
}

/// Screen time per day split by category kind, with browser time split by
/// website (zero-filled).
pub fn daily_kinds(
    conn: &Connection,
    from: NaiveDate,
    to: NaiveDate,
) -> rusqlite::Result<Vec<DayKinds>> {
    let mut map: HashMap<String, DayKinds> = HashMap::new();
    for CatRow { day, cat } in category_rows(conn, &fmt_day(from), &fmt_day(to))? {
        let (kind, secs) = (cat.kind, cat.secs);
        let e = map.entry(day.clone()).or_insert(DayKinds {
            day,
            productive_secs: 0,
            neutral_secs: 0,
            distracting_secs: 0,
        });
        match kind.as_str() {
            "productive" => e.productive_secs += secs,
            "distracting" => e.distracting_secs += secs,
            _ => e.neutral_secs += secs,
        }
    }
    Ok(days_between(from, to)
        .into_iter()
        .map(|d| {
            let key = fmt_day(d);
            map.remove(&key).unwrap_or(DayKinds {
                day: key,
                productive_secs: 0,
                neutral_secs: 0,
                distracting_secs: 0,
            })
        })
        .collect())
}
