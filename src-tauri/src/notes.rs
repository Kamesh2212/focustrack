//! Notes (improvements doc §5.4, §6) and study resources (§5.2).
//!
//! Notes live in notebooks (e.g. one per subject). A note created while a
//! focus/study session is running is linked to that session.

use crate::db;
use chrono::NaiveDateTime;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

pub const QUICK_NOTEBOOK: &str = "Quick notes";
const MAX_BODY: usize = 200_000;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub id: i64,
    pub notebook: String,
    pub title: String,
    pub body: String,
    pub focus_session_id: Option<i64>,
    pub session_label: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Notebook {
    pub name: String,
    pub count: i64,
    pub updated_at: String,
}

fn clean_notebook(name: &str) -> String {
    let n = name.trim();
    if n.is_empty() { "General".into() } else { n.chars().take(60).collect() }
}

fn check_len(title: &str, body: &str) -> Result<(), String> {
    if title.len() > 200 || body.len() > MAX_BODY {
        return Err("That note is too long".into());
    }
    Ok(())
}

fn row_to_note(r: &rusqlite::Row) -> rusqlite::Result<Note> {
    Ok(Note {
        id: r.get(0)?,
        notebook: r.get(1)?,
        title: r.get(2)?,
        body: r.get(3)?,
        focus_session_id: r.get(4)?,
        session_label: r.get(5)?,
        created_at: r.get(6)?,
        updated_at: r.get(7)?,
    })
}

const SELECT: &str = "SELECT n.id, n.notebook, n.title, n.body, n.focus_session_id, f.label, n.created_at, n.updated_at
     FROM notes n LEFT JOIN focus_sessions f ON f.id = n.focus_session_id";

pub fn notebooks(conn: &Connection) -> rusqlite::Result<Vec<Notebook>> {
    let mut stmt = conn.prepare(
        "SELECT notebook, COUNT(*), MAX(updated_at) FROM notes GROUP BY notebook ORDER BY MAX(updated_at) DESC",
    )?;
    let rows = stmt.query_map([], |r| Ok(Notebook { name: r.get(0)?, count: r.get(1)?, updated_at: r.get(2)? }))?;
    let out = rows.collect();
    out
}

/// Notes, newest first, optionally filtered by notebook and/or text.
pub fn list(conn: &Connection, notebook: Option<&str>, search: Option<&str>) -> rusqlite::Result<Vec<Note>> {
    let mut stmt = conn.prepare(&format!(
        "{SELECT} WHERE (?1 IS NULL OR n.notebook = ?1)
           AND (?2 IS NULL OR n.title LIKE '%' || ?2 || '%' OR n.body LIKE '%' || ?2 || '%')
         ORDER BY n.updated_at DESC, n.id DESC LIMIT 500"
    ))?;
    let search = search.map(str::trim).filter(|s| !s.is_empty());
    let rows = stmt.query_map(params![notebook, search], row_to_note)?;
    let out = rows.collect();
    out
}

pub fn get(conn: &Connection, id: i64) -> rusqlite::Result<Option<Note>> {
    conn.query_row(&format!("{SELECT} WHERE n.id = ?1"), params![id], row_to_note).optional()
}

pub fn create(
    conn: &Connection,
    notebook: &str,
    title: &str,
    body: &str,
    focus_session_id: Option<i64>,
    now: NaiveDateTime,
) -> Result<Note, String> {
    check_len(title, body)?;
    let t = db::fmt_time(now);
    conn.execute(
        "INSERT INTO notes (notebook, title, body, focus_session_id, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
        params![clean_notebook(notebook), title.trim(), body, focus_session_id, t],
    )
    .map_err(|e| e.to_string())?;
    get(conn, conn.last_insert_rowid()).map_err(|e| e.to_string())?.ok_or_else(|| "note not saved".into())
}

pub fn update(conn: &Connection, id: i64, notebook: &str, title: &str, body: &str, now: NaiveDateTime) -> Result<Note, String> {
    check_len(title, body)?;
    let n = conn
        .execute(
            "UPDATE notes SET notebook = ?1, title = ?2, body = ?3, updated_at = ?4 WHERE id = ?5",
            params![clean_notebook(notebook), title.trim(), body, db::fmt_time(now), id],
        )
        .map_err(|e| e.to_string())?;
    if n == 0 {
        return Err("That note no longer exists".into());
    }
    get(conn, id).map_err(|e| e.to_string())?.ok_or_else(|| "note not found".into())
}

pub fn delete(conn: &Connection, id: i64) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM notes WHERE id = ?1", params![id])?;
    Ok(())
}

// ------------------------------------------------------- study resources

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Resource {
    pub id: i64,
    pub title: String,
    pub url: String,
    pub subject: Option<String>,
}

pub fn valid_url(url: &str) -> bool {
    let u = url.trim();
    (u.starts_with("https://") || u.starts_with("http://")) && u.len() > 10 && u.len() < 2000 && !u.contains(char::is_whitespace)
}

pub fn resources(conn: &Connection) -> rusqlite::Result<Vec<Resource>> {
    let mut stmt = conn.prepare("SELECT id, title, url, subject FROM study_resources ORDER BY subject IS NOT NULL, subject, title")?;
    let rows = stmt.query_map([], |r| Ok(Resource { id: r.get(0)?, title: r.get(1)?, url: r.get(2)?, subject: r.get(3)? }))?;
    let out = rows.collect();
    out
}

pub fn add_resource(conn: &Connection, title: &str, url: &str, subject: Option<&str>) -> Result<(), String> {
    if title.trim().is_empty() || title.len() > 120 {
        return Err("Give the link a short name".into());
    }
    if !valid_url(url) {
        return Err("Enter a full web address starting with https://".into());
    }
    conn.execute(
        "INSERT INTO study_resources (title, url, subject) VALUES (?1, ?2, ?3)",
        params![title.trim(), url.trim(), subject.map(str::trim).filter(|s| !s.is_empty())],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn delete_resource(conn: &Connection, id: i64) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM study_resources WHERE id = ?1", params![id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn at(m: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, 18).unwrap().and_hms_opt(9, m, 0).unwrap()
    }

    #[test]
    fn notes_crud_and_notebooks() {
        let conn = db::open_in_memory().unwrap();
        let a = create(&conn, "Maths", "Limits", "lim x→0 sin x / x = 1", None, at(0)).unwrap();
        create(&conn, " ", "", "untitled", None, at(1)).unwrap();
        assert_eq!(list(&conn, None, None).unwrap().len(), 2);
        assert_eq!(list(&conn, Some("General"), None).unwrap().len(), 1, "blank notebook becomes General");
        assert_eq!(list(&conn, None, Some("sin x")).unwrap()[0].id, a.id);

        let b = update(&conn, a.id, "Maths", "Limits (revised)", "…", at(5)).unwrap();
        assert_eq!((b.title.as_str(), b.updated_at.as_str()), ("Limits (revised)", "2026-11-18 09:05:00"));
        assert_eq!(notebooks(&conn).unwrap()[0].name, "Maths", "most recently edited first");
        delete(&conn, a.id).unwrap();
        assert!(update(&conn, a.id, "x", "y", "z", at(6)).is_err());
        assert!(create(&conn, "x", "y", &"z".repeat(MAX_BODY + 1), None, at(7)).is_err());
    }

    #[test]
    fn resources_validate_urls() {
        let conn = db::open_in_memory().unwrap();
        let seeded = resources(&conn).unwrap().len();
        assert!(seeded >= 5);
        add_resource(&conn, "Lecture notes", "https://example.edu/notes", Some("Maths")).unwrap();
        assert!(add_resource(&conn, "Bad", "javascript:alert(1)", None).is_err());
        assert!(add_resource(&conn, "Bad", "file:///C:/x", None).is_err());
        assert_eq!(resources(&conn).unwrap().len(), seeded + 1);
    }
}
