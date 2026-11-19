//! Read-side queries and aggregations used by the dashboard and reports.

use crate::db::{in_view, parse_time, DAY_FMT};
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
        &format!(
            "SELECT COALESCE(SUM(duration_secs), 0) FROM usage_sessions WHERE day BETWEEN ?1 AND ?2 AND {}",
            in_view("device_id")
        ),
        params![from, to],
        |r| r.get(0),
    )?;
    let idle: i64 = conn.query_row(
        &format!(
            "SELECT COALESCE(SUM(duration_secs), 0) FROM idle_periods WHERE day BETWEEN ?1 AND ?2 AND {}",
            in_view("device_id")
        ),
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
        let mut stmt = conn.prepare(&format!(
            "SELECT day, SUM(duration_secs) FROM usage_sessions WHERE day BETWEEN ?1 AND ?2 AND {} GROUP BY day",
            in_view("device_id")
        ))?;
        let rows = stmt.query_map(params![f, t], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })?;
        for row in rows {
            let (d, s) = row?;
            map.entry(d).or_default().0 = s;
        }
    }
    {
        let mut stmt = conn.prepare(&format!(
            "SELECT day, SUM(duration_secs) FROM idle_periods WHERE day BETWEEN ?1 AND ?2 AND {} GROUP BY day",
            in_view("device_id")
        ))?;
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
    let mut stmt = conn.prepare(&format!(
        "SELECT a.id, a.display_name, a.exe_name, c.id,
                COALESCE(c.name, 'Other'), COALESCE(c.color, '#94a3b8'), COALESCE(c.kind, 'neutral'),
                COALESCE(SUM(s.duration_secs), 0) AS secs, COUNT(s.id)
         FROM applications a
         LEFT JOIN categories c ON c.id = a.category_id
         LEFT JOIN usage_sessions s ON s.app_id = a.id AND s.day BETWEEN ?1 AND ?2 AND {}
         GROUP BY a.id
         ORDER BY secs DESC, a.display_name COLLATE NOCASE",
        in_view("s.device_id")
    ))?;
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
        "SELECT s.day || '|' || COALESCE(s.device_id, ''), a.exe_name, {CAT}, SUM(s.duration_secs)
         FROM usage_sessions s
         JOIN applications a ON a.id = s.app_id
         LEFT JOIN categories c ON c.id = a.category_id
         WHERE s.day BETWEEN ?1 AND ?2 AND {view}
         GROUP BY s.day, s.device_id, a.id",
        view = in_view("s.device_id")
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
        "SELECT s.day || '|' || COALESCE(s.device_id, ''), s.browser, {CAT}, SUM(s.duration_secs)
         FROM website_sessions s
         JOIN websites w ON w.id = s.website_id
         LEFT JOIN categories c ON c.id = w.category_id
         WHERE s.day BETWEEN ?1 AND ?2 AND {view}
         GROUP BY s.day, s.device_id, s.browser, 3, 4, 5",
        view = in_view("s.device_id")
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
    // Rows were keyed by day and device (to match browser and website time on
    // the same device); callers want the day.
    for r in &mut out {
        if let Some((day, _)) = r.day.split_once('|') {
            r.day = day.to_string();
        }
    }
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
    let mut stmt = conn.prepare(&format!(
        "SELECT w.id, w.domain, c.id,
                COALESCE(c.name, 'Other'), COALESCE(c.color, '#94a3b8'), COALESCE(c.kind, 'neutral'),
                COALESCE(SUM(s.duration_secs), 0) AS secs, COUNT(s.id)
         FROM websites w
         LEFT JOIN categories c ON c.id = w.category_id
         LEFT JOIN website_sessions s ON s.website_id = w.id AND s.day BETWEEN ?1 AND ?2 AND {}
         GROUP BY w.id
         ORDER BY secs DESC, w.domain",
        in_view("s.device_id")
    ))?;
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
        &format!(
            "SELECT COALESCE(SUM(duration_secs), 0) FROM website_sessions WHERE day BETWEEN ?1 AND ?2 AND {}",
            in_view("device_id")
        ),
        params![from, to],
        |r| r.get(0),
    )
}

/// Active time with overlaps counted once: an hour on the laptop while a
/// video plays on the phone is one hour of unique time, two of combined (#64).
pub fn union_secs(spans: &[(NaiveDateTime, NaiveDateTime)]) -> i64 {
    let mut sorted: Vec<_> = spans.iter().filter(|(s, e)| e > s).copied().collect();
    sorted.sort();
    let mut total = 0;
    let mut current: Option<(NaiveDateTime, NaiveDateTime)> = None;
    for (s, e) in sorted {
        match current {
            Some((cs, ce)) if s <= ce => current = Some((cs, ce.max(e))),
            Some((cs, ce)) => {
                total += (ce - cs).num_seconds();
                current = Some((s, e));
            }
            None => current = Some((s, e)),
        }
    }
    if let Some((cs, ce)) = current {
        total += (ce - cs).num_seconds();
    }
    total
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DeviceTotal {
    pub id: String,
    pub name: String,
    /// `windows` | `android` | … (empty when unknown)
    pub platform: String,
    pub active_secs: i64,
    pub idle_secs: i64,
    /// When this device's data ends (its last recorded activity in the range).
    pub last_activity: Option<String>,
    pub current: bool,
}

/// Screen time per device in the range, this device first (ignores the device view).
pub fn device_totals(
    conn: &Connection,
    from: &str,
    to: &str,
) -> rusqlite::Result<Vec<DeviceTotal>> {
    let me: String = crate::db::get_setting(conn, "device_id")?.unwrap_or_default();
    let mut map: HashMap<String, DeviceTotal> = HashMap::new();
    fn entry<'m>(
        map: &'m mut HashMap<String, DeviceTotal>,
        me: &str,
        id: String,
    ) -> &'m mut DeviceTotal {
        map.entry(id.clone()).or_insert_with(|| DeviceTotal {
            current: id == me,
            id,
            name: String::new(),
            platform: String::new(),
            active_secs: 0,
            idle_secs: 0,
            last_activity: None,
        })
    }
    entry(&mut map, &me, me.clone());
    let mut stmt = conn.prepare(
        "SELECT device_id, SUM(duration_secs), MAX(end_time) FROM usage_sessions
         WHERE day BETWEEN ?1 AND ?2 AND device_id IS NOT NULL GROUP BY device_id",
    )?;
    for row in stmt.query_map(params![from, to], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, Option<String>>(2)?,
        ))
    })? {
        let (id, secs, last) = row?;
        let e = entry(&mut map, &me, id);
        e.active_secs = secs;
        e.last_activity = last;
    }
    let mut stmt = conn.prepare(
        "SELECT device_id, SUM(duration_secs) FROM idle_periods
         WHERE day BETWEEN ?1 AND ?2 AND device_id IS NOT NULL GROUP BY device_id",
    )?;
    for row in stmt.query_map(params![from, to], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
    })? {
        let (id, secs) = row?;
        entry(&mut map, &me, id).idle_secs = secs;
    }
    let mut stmt = conn.prepare("SELECT id, name, platform FROM devices")?;
    let known: HashMap<String, (String, String)> = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                (r.get::<_, String>(1)?, r.get::<_, String>(2)?),
            ))
        })?
        .collect::<rusqlite::Result<_>>()?;
    let mut out: Vec<DeviceTotal> = map
        .into_values()
        .map(|mut d| {
            if d.current {
                d.name = crate::sync::device_name(conn);
                d.platform = "windows".into();
            } else if let Some((name, platform)) = known.get(&d.id) {
                d.name = name.clone();
                d.platform = platform.clone();
            }
            if d.name.is_empty() {
                d.name = "Other device".into();
            }
            d
        })
        .collect();
    out.sort_by(|a, b| {
        b.current
            .cmp(&a.current)
            .then(b.active_secs.cmp(&a.active_secs))
    });
    Ok(out)
}

/// Device IDs for a filter: `all` (None), `desktop`, `mobile`, or one device ID.
pub fn device_filter(
    conn: &Connection,
    filter: Option<&str>,
) -> rusqlite::Result<Option<Vec<String>>> {
    let f = match filter {
        None | Some("") | Some("all") => return Ok(None),
        Some(f) => f,
    };
    if f != "desktop" && f != "mobile" {
        return Ok(Some(vec![f.to_string()]));
    }
    let me: String = crate::db::get_setting(conn, "device_id")?.unwrap_or_default();
    let mut stmt = conn.prepare(
        "SELECT DISTINCT device_id FROM usage_sessions WHERE device_id IS NOT NULL
         UNION SELECT id FROM devices",
    )?;
    let ids: Vec<String> = stmt
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    let mobile: std::collections::HashSet<String> = conn
        .prepare("SELECT id FROM devices WHERE platform IN ('android', 'ios')")?
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    let mut out: Vec<String> = ids
        .into_iter()
        .chain(std::iter::once(me))
        .filter(|id| mobile.contains(id) == (f == "mobile"))
        .collect();
    out.sort();
    out.dedup();
    Ok(Some(out))
}

/// Active spans in the range, ordered by start.
pub fn active_spans(
    conn: &Connection,
    from: &str,
    to: &str,
) -> rusqlite::Result<Vec<(NaiveDateTime, NaiveDateTime)>> {
    spans(conn, from, to, false)
}

/// Active spans recorded on this device only (break reminders, live checks).
pub fn own_active_spans(
    conn: &Connection,
    from: &str,
    to: &str,
) -> rusqlite::Result<Vec<(NaiveDateTime, NaiveDateTime)>> {
    spans(conn, from, to, true)
}

fn spans(
    conn: &Connection,
    from: &str,
    to: &str,
    own: bool,
) -> rusqlite::Result<Vec<(NaiveDateTime, NaiveDateTime)>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT start_time, end_time FROM usage_sessions
         WHERE day BETWEEN ?1 AND ?2 AND duration_secs > 0
           AND ((?3 = 1 AND device_id = {own}) OR (?3 = 0 AND {view})) ORDER BY start_time",
        own = crate::db::OWN_DEVICE,
        view = in_view("device_id")
    ))?;
    let rows = stmt.query_map(params![from, to, own], |r| {
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
    fn device_view_unique_time_and_per_device_totals() {
        use crate::db;
        let conn = db::open_in_memory().unwrap();
        let me = db::get_setting(&conn, "device_id").unwrap().unwrap();
        let phone = "01a1f48e-0000-7000-8000-000000000002";
        conn.execute(
            "INSERT INTO devices (id, name, platform) VALUES (?1, 'Pixel', 'android')",
            params![phone],
        )
        .unwrap();
        let (app, _) =
            db::get_or_create_app(&conn, "code.exe", "", || "Code".into(), at(18, 9, 0)).unwrap();
        // This PC: 09:00–10:00. The phone: 09:30–10:30 (30 min overlap).
        let id = db::insert_usage(&conn, app, at(18, 9, 0)).unwrap();
        db::update_span(&conn, db::SpanKind::Usage, id, at(18, 9, 0), at(18, 10, 0)).unwrap();
        let id = db::insert_usage(&conn, app, at(18, 9, 30)).unwrap();
        db::update_span(
            &conn,
            db::SpanKind::Usage,
            id,
            at(18, 9, 30),
            at(18, 10, 30),
        )
        .unwrap();
        conn.execute(
            "UPDATE usage_sessions SET device_id = ?1 WHERE id = ?2",
            params![phone, id],
        )
        .unwrap();
        let day = "2026-11-18";

        // All devices: combined 2 h, unique 1.5 h.
        assert_eq!(totals(&conn, day, day).unwrap().0, 7200);
        assert_eq!(union_secs(&active_spans(&conn, day, day).unwrap()), 5400);

        // One device / desktops / phones.
        for (filter, secs) in [
            (Some("mobile"), 3600),
            (Some("desktop"), 3600),
            (Some(phone), 3600),
            (None, 7200),
        ] {
            let ids = device_filter(&conn, filter).unwrap();
            let _view = db::device_view(&conn, ids.as_deref()).unwrap();
            assert_eq!(totals(&conn, day, day).unwrap().0, secs, "{filter:?}");
            assert_eq!(
                app_usage(&conn, day, day).unwrap()[0].secs,
                secs,
                "{filter:?}"
            );
        }
        // The view ends with the guard.
        assert_eq!(totals(&conn, day, day).unwrap().0, 7200);

        let devs = device_totals(&conn, day, day).unwrap();
        assert_eq!(devs.len(), 2);
        assert!(devs[0].current && devs[0].id == me, "this device first");
        assert_eq!(
            (
                devs[1].name.as_str(),
                devs[1].platform.as_str(),
                devs[1].active_secs
            ),
            ("Pixel", "android", 3600)
        );
        assert_eq!(
            devs[1].last_activity.as_deref(),
            Some("2026-11-18 10:30:00")
        );
    }

    /// Real data: `FT_DB_COPY=<copy of a synced profile db> cargo test devices_real -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn devices_real_database_copy() {
        use crate::db;
        let conn = db::open(std::path::Path::new(&std::env::var("FT_DB_COPY").unwrap())).unwrap();
        let day: String = conn
            .query_row("SELECT MAX(day) FROM usage_sessions", [], |r| r.get(0))
            .unwrap();
        for d in device_totals(&conn, &day, &day).unwrap() {
            println!(
                "{} ({}) current={} active={}s last={:?}",
                d.name, d.platform, d.current, d.active_secs, d.last_activity
            );
        }
        println!(
            "combined {}s, unique {}s",
            totals(&conn, &day, &day).unwrap().0,
            union_secs(&active_spans(&conn, &day, &day).unwrap())
        );
        for f in ["desktop", "mobile"] {
            let ids = device_filter(&conn, Some(f)).unwrap();
            let _v = db::device_view(&conn, ids.as_deref()).unwrap();
            println!("{f}: {}s", totals(&conn, &day, &day).unwrap().0);
        }
    }

    #[test]
    fn an_empty_selection_shows_nothing_not_everything() {
        use crate::db;
        let conn = db::open_in_memory().unwrap();
        let (app, _) =
            db::get_or_create_app(&conn, "code.exe", "", || "Code".into(), at(18, 9, 0)).unwrap();
        let id = db::insert_usage(&conn, app, at(18, 9, 0)).unwrap();
        db::update_span(&conn, db::SpanKind::Usage, id, at(18, 9, 0), at(18, 10, 0)).unwrap();
        let ids = device_filter(&conn, Some("mobile")).unwrap();
        assert_eq!(ids, Some(vec![]), "no phones");
        let _view = db::device_view(&conn, ids.as_deref()).unwrap();
        assert_eq!(totals(&conn, "2026-11-18", "2026-11-18").unwrap().0, 0);
    }

    #[test]
    fn union_merges_overlaps_and_touching_spans() {
        let spans = [
            (at(18, 9, 0), at(18, 10, 0)),
            (at(18, 9, 30), at(18, 9, 45)),  // inside
            (at(18, 10, 0), at(18, 10, 15)), // touching
            (at(18, 12, 0), at(18, 12, 30)),
        ];
        assert_eq!(union_secs(&spans), 75 * 60 + 30 * 60);
        assert_eq!(union_secs(&[]), 0);
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
