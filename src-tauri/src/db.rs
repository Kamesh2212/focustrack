//! SQLite storage (SRS section 8). All data stays on this machine.
//!
//! Times are stored as local `YYYY-MM-DD HH:MM:SS` strings, with a separate
//! `day` column so per-day queries are simple index lookups. The tracker splits
//! sessions at midnight, so a session never spans two days.

use crate::categories::{default_category_for, DEFAULT_CATEGORIES};
use chrono::NaiveDateTime;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

pub const TIME_FMT: &str = "%Y-%m-%d %H:%M:%S";
pub const DAY_FMT: &str = "%Y-%m-%d";

pub const DEFAULT_IDLE_THRESHOLD_SECS: u64 = 300;

pub fn fmt_time(t: NaiveDateTime) -> String {
    t.format(TIME_FMT).to_string()
}

pub fn parse_time(s: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(s, TIME_FMT).ok()
}

pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA foreign_keys = ON;",
    )?;
    migrate(&conn)?;
    Ok(conn)
}

#[cfg(test)]
pub fn open_in_memory() -> rusqlite::Result<Connection> {
    let conn = Connection::open_in_memory()?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    migrate(&conn)?;
    Ok(conn)
}

/// Schema migrations, tracked with `PRAGMA user_version`.
/// Later phases (websites, limits, focus sessions) add new versions here.
fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;

    if version < 1 {
        conn.execute_batch(
            "BEGIN;
             CREATE TABLE IF NOT EXISTS categories (
                 id    INTEGER PRIMARY KEY,
                 name  TEXT NOT NULL UNIQUE,
                 color TEXT NOT NULL,
                 kind  TEXT NOT NULL DEFAULT 'neutral'
             );
             CREATE TABLE IF NOT EXISTS applications (
                 id           INTEGER PRIMARY KEY,
                 exe_name     TEXT NOT NULL UNIQUE COLLATE NOCASE,
                 display_name TEXT NOT NULL,
                 exe_path     TEXT,
                 category_id  INTEGER REFERENCES categories(id) ON DELETE SET NULL,
                 first_seen   TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS usage_sessions (
                 id            INTEGER PRIMARY KEY,
                 app_id        INTEGER NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
                 day           TEXT NOT NULL,
                 start_time    TEXT NOT NULL,
                 end_time      TEXT NOT NULL,
                 duration_secs INTEGER NOT NULL DEFAULT 0
             );
             CREATE INDEX IF NOT EXISTS idx_usage_day ON usage_sessions(day);
             CREATE INDEX IF NOT EXISTS idx_usage_app_day ON usage_sessions(app_id, day);
             CREATE TABLE IF NOT EXISTS idle_periods (
                 id            INTEGER PRIMARY KEY,
                 day           TEXT NOT NULL,
                 start_time    TEXT NOT NULL,
                 end_time      TEXT NOT NULL,
                 duration_secs INTEGER NOT NULL DEFAULT 0
             );
             CREATE INDEX IF NOT EXISTS idx_idle_day ON idle_periods(day);
             CREATE TABLE IF NOT EXISTS user_settings (
                 key   TEXT PRIMARY KEY,
                 value TEXT NOT NULL
             );
             PRAGMA user_version = 1;
             COMMIT;",
        )?;
        for (name, color, kind) in DEFAULT_CATEGORIES {
            conn.execute(
                "INSERT OR IGNORE INTO categories (name, color, kind) VALUES (?1, ?2, ?3)",
                params![name, color, kind],
            )?;
        }
    }

    if version < 2 {
        // Phase 3: websites (via browser extensions), usage limits, notifications.
        conn.execute_batch(
            "BEGIN;
             CREATE TABLE IF NOT EXISTS websites (
                 id          INTEGER PRIMARY KEY,
                 domain      TEXT NOT NULL UNIQUE COLLATE NOCASE,
                 category_id INTEGER REFERENCES categories(id) ON DELETE SET NULL,
                 first_seen  TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS website_sessions (
                 id            INTEGER PRIMARY KEY,
                 website_id    INTEGER NOT NULL REFERENCES websites(id) ON DELETE CASCADE,
                 browser       TEXT NOT NULL,
                 day           TEXT NOT NULL,
                 start_time    TEXT NOT NULL,
                 end_time      TEXT NOT NULL,
                 duration_secs INTEGER NOT NULL DEFAULT 0
             );
             CREATE INDEX IF NOT EXISTS idx_web_day ON website_sessions(day);
             CREATE INDEX IF NOT EXISTS idx_web_site_day ON website_sessions(website_id, day);
             CREATE TABLE IF NOT EXISTS usage_limits (
                 id               INTEGER PRIMARY KEY,
                 target_type      TEXT NOT NULL CHECK (target_type IN ('app', 'website')),
                 target_id        INTEGER NOT NULL,
                 daily_limit_secs INTEGER NOT NULL,
                 warn_pct         INTEGER NOT NULL DEFAULT 80,
                 block            INTEGER NOT NULL DEFAULT 0,
                 enabled          INTEGER NOT NULL DEFAULT 1,
                 UNIQUE (target_type, target_id)
             );
             CREATE TABLE IF NOT EXISTS sent_notifications (
                 key  TEXT NOT NULL,
                 day  TEXT NOT NULL,
                 PRIMARY KEY (key, day)
             );
             PRAGMA user_version = 2;
             COMMIT;",
        )?;
    }

    if version < 3 {
        // Phase 4: focus sessions and daily goals.
        conn.execute_batch(
            "BEGIN;
             CREATE TABLE IF NOT EXISTS focus_sessions (
                 id                INTEGER PRIMARY KEY,
                 label             TEXT NOT NULL,
                 kind              TEXT NOT NULL CHECK (kind IN ('focus', 'break')),
                 planned_secs      INTEGER NOT NULL,
                 day               TEXT NOT NULL,
                 start_time        TEXT NOT NULL,
                 end_time          TEXT,
                 actual_secs       INTEGER NOT NULL DEFAULT 0,
                 completed         INTEGER NOT NULL DEFAULT 0,
                 block_distracting INTEGER NOT NULL DEFAULT 0
             );
             CREATE INDEX IF NOT EXISTS idx_focus_day ON focus_sessions(day);
             CREATE TABLE IF NOT EXISTS goals (
                 id          INTEGER PRIMARY KEY,
                 kind        TEXT NOT NULL CHECK (kind IN ('max_screen', 'min_category', 'max_category', 'min_focus_sessions')),
                 category_id INTEGER REFERENCES categories(id) ON DELETE CASCADE,
                 target      INTEGER NOT NULL,
                 enabled     INTEGER NOT NULL DEFAULT 1
             );
             PRAGMA user_version = 3;
             COMMIT;",
        )?;
    }

    if version < 4 {
        migrate_v4(conn)?;
    }

    if version < 5 {
        migrate_v5(conn)?;
    }

    // A focus session left open by a crash or shutdown ends where it was last saved.
    conn.execute(
        "UPDATE focus_sessions
         SET end_time = strftime('%Y-%m-%d %H:%M:%S', start_time, '+' || actual_secs || ' seconds')
         WHERE end_time IS NULL",
        [],
    )?;
    Ok(())
}

/// Study category and mode, limit actions/overrides, custom goals, blocking
/// profiles, notes and study resources.
fn migrate_v4(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "BEGIN;
         INSERT OR IGNORE INTO categories (name, color, kind) VALUES ('Study', '#14b8a6', 'productive');

         ALTER TABLE usage_limits ADD COLUMN action TEXT NOT NULL DEFAULT 'notify';
         UPDATE usage_limits SET action = CASE WHEN block = 1 THEN 'block' ELSE 'notify' END;
         CREATE TABLE IF NOT EXISTS limit_overrides (
             limit_id   INTEGER NOT NULL,
             day        TEXT NOT NULL,
             extra_secs INTEGER NOT NULL DEFAULT 0,
             count      INTEGER NOT NULL DEFAULT 0,
             PRIMARY KEY (limit_id, day)
         );

         CREATE TABLE goals_v4 (
             id          INTEGER PRIMARY KEY,
             kind        TEXT NOT NULL CHECK (kind IN ('max_screen', 'min_category', 'max_category', 'min_focus_sessions', 'custom')),
             category_id INTEGER REFERENCES categories(id) ON DELETE CASCADE,
             target      INTEGER NOT NULL DEFAULT 0,
             title       TEXT,
             enabled     INTEGER NOT NULL DEFAULT 1
         );
         INSERT INTO goals_v4 (id, kind, category_id, target, enabled)
             SELECT id, kind, category_id, target, enabled FROM goals;
         DROP TABLE goals;
         ALTER TABLE goals_v4 RENAME TO goals;
         CREATE TABLE IF NOT EXISTS goal_checks (
             goal_id INTEGER NOT NULL REFERENCES goals(id) ON DELETE CASCADE,
             day     TEXT NOT NULL,
             PRIMARY KEY (goal_id, day)
         );

         ALTER TABLE focus_sessions ADD COLUMN mode TEXT NOT NULL DEFAULT 'focus';
         ALTER TABLE focus_sessions ADD COLUMN subject TEXT;
         ALTER TABLE focus_sessions ADD COLUMN goal_text TEXT;
         ALTER TABLE focus_sessions ADD COLUMN goal_id INTEGER;
         ALTER TABLE focus_sessions ADD COLUMN profile_id INTEGER;

         CREATE TABLE IF NOT EXISTS block_profiles (
             id                INTEGER PRIMARY KEY,
             name              TEXT NOT NULL UNIQUE,
             mode              TEXT NOT NULL CHECK (mode IN ('block', 'allow_only')),
             categories        TEXT NOT NULL DEFAULT '[]',
             apps              TEXT NOT NULL DEFAULT '[]',
             sites             TEXT NOT NULL DEFAULT '[]',
             unblock_on_breaks INTEGER NOT NULL DEFAULT 0,
             end_early         TEXT NOT NULL DEFAULT 'confirm' CHECK (end_early IN ('allow', 'confirm', 'strict')),
             builtin           INTEGER NOT NULL DEFAULT 0
         );
         INSERT OR IGNORE INTO block_profiles (name, mode, categories, apps, sites, end_early, builtin) VALUES
           ('Study Mode', 'block', '[\"Gaming\",\"Entertainment\",\"Social Media\"]', '[]', '[]', 'confirm', 1),
           ('Work Mode', 'block', '[\"Gaming\",\"Entertainment\"]', '[]',
              '[\"reddit.com\",\"instagram.com\",\"tiktok.com\",\"x.com\",\"twitter.com\",\"facebook.com\"]', 'allow', 1),
           ('Deep Focus', 'allow_only', '[\"Study\",\"Education\",\"Development\",\"Productivity\"]', '[]',
              '[\"google.com\",\"bing.com\",\"duckduckgo.com\",\"search.brave.com\",\"wikipedia.org\",\"stackoverflow.com\",\"github.com\"]', 'strict', 1),
           ('Personal', 'block', '[]', '[]', '[]', 'confirm', 1);

         CREATE TABLE IF NOT EXISTS notes (
             id               INTEGER PRIMARY KEY,
             notebook         TEXT NOT NULL DEFAULT 'General',
             title            TEXT NOT NULL DEFAULT '',
             body             TEXT NOT NULL DEFAULT '',
             focus_session_id INTEGER,
             created_at       TEXT NOT NULL,
             updated_at       TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS idx_notes_notebook ON notes(notebook, updated_at);

         CREATE TABLE IF NOT EXISTS study_resources (
             id      INTEGER PRIMARY KEY,
             title   TEXT NOT NULL,
             url     TEXT NOT NULL,
             subject TEXT
         );
         INSERT INTO study_resources (title, url) VALUES
           ('Google Scholar', 'https://scholar.google.com'),
           ('Khan Academy', 'https://www.khanacademy.org'),
           ('Coursera', 'https://www.coursera.org'),
           ('Wikipedia', 'https://www.wikipedia.org'),
           ('MDN Web Docs', 'https://developer.mozilla.org'),
           ('arXiv', 'https://arxiv.org'),
           ('Open Library', 'https://openlibrary.org');

         PRAGMA user_version = 4;
         COMMIT;",
    )?;
    // Move built-in study tools from Education to Study, unless the user changed them.
    let study: i64 = conn.query_row("SELECT id FROM categories WHERE name = 'Study'", [], |r| {
        r.get(0)
    })?;
    for exe in crate::categories::STUDY_EXES {
        conn.execute(
            "UPDATE applications SET category_id = ?1
             WHERE exe_name = ?2 AND category_id = (SELECT id FROM categories WHERE name = 'Education')",
            params![study, exe],
        )?;
    }
    for domain in crate::categories::STUDY_DOMAINS {
        conn.execute(
            "UPDATE websites SET category_id = ?1
             WHERE domain = ?2 AND category_id = (SELECT id FROM categories WHERE name = 'Education')",
            params![study, domain],
        )?;
    }
    Ok(())
}

/// Schema v5 (#5): apps get an identity key — the exe name, or the full path
/// for exe names shared by unrelated programs (javaw.exe, launcher.exe…).
/// SQLite can't drop the old UNIQUE(exe_name), so the table is rebuilt
/// (the documented 12-step procedure; ids are kept, so sessions stay linked).
fn migrate_v5(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch("PRAGMA foreign_keys = OFF;")?;
    let result = conn.execute_batch(
        "BEGIN;
         CREATE TABLE applications_v5 (
             id           INTEGER PRIMARY KEY,
             app_key      TEXT NOT NULL UNIQUE,
             exe_name     TEXT NOT NULL COLLATE NOCASE,
             display_name TEXT NOT NULL,
             exe_path     TEXT,
             category_id  INTEGER REFERENCES categories(id) ON DELETE SET NULL,
             first_seen   TEXT NOT NULL
         );
         INSERT INTO applications_v5 (id, app_key, exe_name, display_name, exe_path, category_id, first_seen)
             SELECT id, lower(exe_name), exe_name, display_name, exe_path, category_id, first_seen FROM applications;
         DROP TABLE applications;
         ALTER TABLE applications_v5 RENAME TO applications;
         CREATE INDEX IF NOT EXISTS idx_apps_exe ON applications(exe_name);
         PRAGMA user_version = 5;
         COMMIT;",
    );
    if result.is_err() {
        let _ = conn.execute_batch("ROLLBACK;");
    }
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    result?;
    // Generic names are keyed by path from now on.
    let rows: Vec<(i64, String, Option<String>)> = conn
        .prepare("SELECT id, exe_name, exe_path FROM applications")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<Result<_, _>>()?;
    for (id, exe, path) in rows {
        let key = crate::identity::app_key(&exe, path.as_deref().unwrap_or(""));
        if key != exe.to_ascii_lowercase() {
            conn.execute(
                "UPDATE OR IGNORE applications SET app_key = ?1 WHERE id = ?2",
                params![key, id],
            )?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- settings

#[derive(Debug, Clone)]
pub struct StoredSettings {
    pub paused: bool,
    pub idle_threshold_secs: u64,
    pub theme: String,
    pub notifications_enabled: bool,
    /// 0 = off
    pub break_reminder_mins: u64,
    pub website_tracking: bool,
    /// How many times per day a blocked limit may be extended (0 = never).
    pub overrides_per_day: i64,
    /// "HH:MM" for the incomplete-goals reminder, or empty for off.
    pub goal_reminder: String,
}

pub fn get_setting(conn: &Connection, key: &str) -> rusqlite::Result<Option<String>> {
    conn.query_row(
        "SELECT value FROM user_settings WHERE key = ?1",
        params![key],
        |r| r.get(0),
    )
    .optional()
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO user_settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

pub fn load_settings(conn: &Connection) -> rusqlite::Result<StoredSettings> {
    let paused = get_setting(conn, "paused")?.as_deref() == Some("1");
    let idle_threshold_secs = get_setting(conn, "idle_threshold_secs")?
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_IDLE_THRESHOLD_SECS);
    let theme = get_setting(conn, "theme")?.unwrap_or_else(|| "system".into());
    let notifications_enabled = get_setting(conn, "notifications_enabled")?.as_deref() != Some("0");
    let break_reminder_mins = get_setting(conn, "break_reminder_mins")?
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let website_tracking = get_setting(conn, "website_tracking")?.as_deref() != Some("0");
    let overrides_per_day = get_setting(conn, "overrides_per_day")?
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let goal_reminder = get_setting(conn, "goal_reminder")?.unwrap_or_else(|| "20:00".into());
    Ok(StoredSettings {
        overrides_per_day,
        goal_reminder,
        paused,
        idle_threshold_secs,
        theme,
        notifications_enabled,
        break_reminder_mins,
        website_tracking,
    })
}

// ------------------------------------------------------------ applications

/// Looks up an application by exe name, creating it (with a default
/// category) the first time it is seen. Returns `(id, display_name)`.
pub fn get_or_create_app(
    conn: &Connection,
    exe_name: &str,
    exe_path: &str,
    display_name: impl FnOnce() -> String,
    now: NaiveDateTime,
) -> rusqlite::Result<(i64, String)> {
    let key = crate::identity::app_key(exe_name, exe_path);
    if let Some((id, name, known_path)) = conn
        .query_row(
            "SELECT id, display_name, exe_path FROM applications WHERE app_key = ?1",
            params![key],
            |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get(1)?,
                    r.get::<_, Option<String>>(2)?,
                ))
            },
        )
        .optional()?
    {
        // Remember the path once we learn it (needed for launcher folders).
        if known_path.as_deref().unwrap_or("").is_empty() && !exe_path.is_empty() {
            conn.execute(
                "UPDATE applications SET exe_path = ?1 WHERE id = ?2",
                params![exe_path, id],
            )?;
        }
        return Ok((id, name));
    }
    let exe_lower = exe_name.to_ascii_lowercase();
    let name = if crate::identity::is_generic(&exe_lower) {
        // "launcher.exe" says nothing; its folder usually does (e.g. "HoYoPlay").
        crate::identity::folder_name(exe_path).unwrap_or_else(display_name)
    } else {
        display_name()
    };
    let category = match default_category_for(&exe_lower) {
        "Other" => crate::identity::category_from_path(exe_path).unwrap_or("Other"),
        c => c,
    };
    conn.execute(
        "INSERT INTO applications (app_key, exe_name, display_name, exe_path, category_id, first_seen)
         VALUES (?1, ?2, ?3, ?4, (SELECT id FROM categories WHERE name = ?5), ?6)",
        params![key, exe_name, name, exe_path, category, fmt_time(now)],
    )?;
    Ok((conn.last_insert_rowid(), name))
}

/// Looks up a website by domain, creating it with a default category.
pub fn get_or_create_website(
    conn: &Connection,
    domain: &str,
    now: NaiveDateTime,
) -> rusqlite::Result<i64> {
    if let Some(id) = conn
        .query_row(
            "SELECT id FROM websites WHERE domain = ?1",
            params![domain],
            |r| r.get(0),
        )
        .optional()?
    {
        return Ok(id);
    }
    let category = crate::categories::default_category_for_domain(domain);
    conn.execute(
        "INSERT INTO websites (domain, category_id, first_seen)
         VALUES (?1, (SELECT id FROM categories WHERE name = ?2), ?3)",
        params![domain, category, fmt_time(now)],
    )?;
    Ok(conn.last_insert_rowid())
}

// ---------------------------------------------------------------- sessions

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpanKind {
    Usage,
    Idle,
    Website,
}

impl SpanKind {
    fn table(self) -> &'static str {
        match self {
            SpanKind::Usage => "usage_sessions",
            SpanKind::Idle => "idle_periods",
            SpanKind::Website => "website_sessions",
        }
    }
}

pub fn insert_usage(conn: &Connection, app_id: i64, start: NaiveDateTime) -> rusqlite::Result<i64> {
    let s = fmt_time(start);
    conn.execute(
        "INSERT INTO usage_sessions (app_id, day, start_time, end_time, duration_secs)
         VALUES (?1, ?2, ?3, ?3, 0)",
        params![app_id, start.format(DAY_FMT).to_string(), s],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn insert_website_session(
    conn: &Connection,
    website_id: i64,
    browser: &str,
    start: NaiveDateTime,
) -> rusqlite::Result<i64> {
    let s = fmt_time(start);
    conn.execute(
        "INSERT INTO website_sessions (website_id, browser, day, start_time, end_time, duration_secs)
         VALUES (?1, ?2, ?3, ?4, ?4, 0)",
        params![website_id, browser, start.format(DAY_FMT).to_string(), s],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn insert_idle(conn: &Connection, start: NaiveDateTime) -> rusqlite::Result<i64> {
    let s = fmt_time(start);
    conn.execute(
        "INSERT INTO idle_periods (day, start_time, end_time, duration_secs)
         VALUES (?1, ?2, ?2, 0)",
        params![start.format(DAY_FMT).to_string(), s],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Updates the end of an open span. Rows are updated in place by id, so
/// repeated flushes never create duplicate records.
pub fn update_span(
    conn: &Connection,
    kind: SpanKind,
    id: i64,
    start: NaiveDateTime,
    end: NaiveDateTime,
) -> rusqlite::Result<()> {
    let secs = (end - start).num_seconds().max(0);
    conn.execute(
        &format!(
            "UPDATE {} SET end_time = ?1, duration_secs = ?2 WHERE id = ?3",
            kind.table()
        ),
        params![fmt_time(end), secs, id],
    )?;
    Ok(())
}

pub fn delete_span(conn: &Connection, kind: SpanKind, id: i64) -> rusqlite::Result<()> {
    conn.execute(
        &format!("DELETE FROM {} WHERE id = ?1", kind.table()),
        params![id],
    )?;
    Ok(())
}

// ------------------------------------------------------------------ limits

/// Records that a notification was sent; returns false if it already was today.
pub fn mark_notified(conn: &Connection, key: &str, day: &str) -> rusqlite::Result<bool> {
    let n = conn.execute(
        "INSERT OR IGNORE INTO sent_notifications (key, day) VALUES (?1, ?2)",
        params![key, day],
    )?;
    Ok(n == 1)
}

pub fn clear_limit_notifications(conn: &Connection, limit_id: i64) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM sent_notifications WHERE key LIKE ?1",
        params![format!("limit:{limit_id}:%")],
    )?;
    Ok(())
}

// ---------------------------------------------------------------- deletion

pub fn delete_all_usage(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "BEGIN;
         DELETE FROM usage_sessions;
         DELETE FROM idle_periods;
         DELETE FROM website_sessions;
         DELETE FROM sent_notifications;
         DELETE FROM focus_sessions;
         DELETE FROM limit_overrides;
         DELETE FROM goal_checks;
         COMMIT;",
    )
}

pub fn delete_usage_before(conn: &Connection, day: &str) -> rusqlite::Result<usize> {
    let a = conn.execute("DELETE FROM usage_sessions WHERE day < ?1", params![day])?;
    let b = conn.execute("DELETE FROM idle_periods WHERE day < ?1", params![day])?;
    let c = conn.execute("DELETE FROM website_sessions WHERE day < ?1", params![day])?;
    conn.execute("DELETE FROM focus_sessions WHERE day < ?1", params![day])?;
    conn.execute(
        "DELETE FROM sent_notifications WHERE day < ?1",
        params![day],
    )?;
    Ok(a + b + c)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn t(h: u32, m: u32, s: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, 18)
            .unwrap()
            .and_hms_opt(h, m, s)
            .unwrap()
    }

    #[test]
    fn migration_seeds_categories_and_is_idempotent() {
        let conn = open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM categories", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n as usize, DEFAULT_CATEGORIES.len());
        let v: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v, 5);
        let profiles: i64 = conn
            .query_row("SELECT COUNT(*) FROM block_profiles", [], |r| r.get(0))
            .unwrap();
        assert_eq!(profiles, 4);
    }

    #[test]
    fn app_created_once_with_default_category() {
        let conn = open_in_memory().unwrap();
        let (a, name) = get_or_create_app(
            &conn,
            "Code.exe",
            "C:\\x\\Code.exe",
            || "VS Code".into(),
            t(9, 0, 0),
        )
        .unwrap();
        let (b, _) =
            get_or_create_app(&conn, "code.exe", "", || "ignored".into(), t(9, 0, 0)).unwrap();
        assert_eq!(a, b);
        assert_eq!(name, "VS Code");
        let cat: String = conn
            .query_row(
                "SELECT c.name FROM applications a JOIN categories c ON c.id = a.category_id WHERE a.id = ?1",
                params![a],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cat, "Development");
    }

    #[test]
    fn spans_update_in_place() {
        let conn = open_in_memory().unwrap();
        let (app, _) = get_or_create_app(&conn, "a.exe", "", || "A".into(), t(9, 0, 0)).unwrap();
        let id = insert_usage(&conn, app, t(9, 0, 0)).unwrap();
        update_span(&conn, SpanKind::Usage, id, t(9, 0, 0), t(9, 0, 30)).unwrap();
        update_span(&conn, SpanKind::Usage, id, t(9, 0, 0), t(9, 1, 0)).unwrap();
        let (count, secs): (i64, i64) = conn
            .query_row(
                "SELECT COUNT(*), SUM(duration_secs) FROM usage_sessions",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((count, secs), (1, 60));
    }

    #[test]
    fn settings_roundtrip() {
        let conn = open_in_memory().unwrap();
        assert!(!load_settings(&conn).unwrap().paused);
        set_setting(&conn, "paused", "1").unwrap();
        set_setting(&conn, "idle_threshold_secs", "120").unwrap();
        let s = load_settings(&conn).unwrap();
        assert!(s.paused);
        assert_eq!(s.idle_threshold_secs, 120);
    }

    #[test]
    fn websites_and_notifications() {
        let conn = open_in_memory().unwrap();
        let a = get_or_create_website(&conn, "youtube.com", t(9, 0, 0)).unwrap();
        let b = get_or_create_website(&conn, "YouTube.com", t(9, 0, 0)).unwrap();
        assert_eq!(a, b);
        let cat: String = conn
            .query_row(
                "SELECT c.name FROM websites w JOIN categories c ON c.id = w.category_id WHERE w.id = ?1",
                params![a],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cat, "Entertainment");
        assert!(mark_notified(&conn, "limit:1:warn", "2026-11-18").unwrap());
        assert!(!mark_notified(&conn, "limit:1:warn", "2026-11-18").unwrap());
        clear_limit_notifications(&conn, 1).unwrap();
        assert!(mark_notified(&conn, "limit:1:warn", "2026-11-18").unwrap());
    }

    #[test]
    fn generic_exes_are_separate_apps_and_games_default_to_gaming() {
        let conn = open_in_memory().unwrap();
        let (a, name_a) = get_or_create_app(
            &conn,
            "launcher.exe",
            r"C:\Program Files\HoYoPlay\launcher.exe",
            || "x".into(),
            t(9, 0, 0),
        )
        .unwrap();
        let (b, _) = get_or_create_app(
            &conn,
            "launcher.exe",
            r"C:\Games\Other\launcher.exe",
            || "x".into(),
            t(9, 0, 0),
        )
        .unwrap();
        assert_ne!(a, b, "two different launcher.exe programs");
        assert_eq!(name_a, "HoYoPlay");
        let (g, _) = get_or_create_app(
            &conn,
            "GenshinImpact.exe",
            r"C:\Program Files\HoYoPlay\games\Genshin Impact game\GenshinImpact.exe",
            || "Genshin Impact".into(),
            t(9, 0, 0),
        )
        .unwrap();
        let cat: String = conn
            .query_row(
                "SELECT c.name FROM applications a JOIN categories c ON c.id = a.category_id WHERE a.id = ?1",
                params![g],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cat, "Gaming");
    }
}
