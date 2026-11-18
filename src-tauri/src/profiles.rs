//! Blocking profiles (improvements doc §5.1, §7, §8): reusable rules for what a
//! focus or study session blocks.
//!
//! * `block` mode: block apps and websites in the chosen categories, plus any
//!   listed explicitly.
//! * `allow_only` mode: only apps/websites in the chosen categories (or listed)
//!   may be used; everything else is blocked. Browsers stay open and are
//!   filtered per website.

use crate::blocker::is_protected_name;
use crate::categories::{browser_family, default_category_for, DOMAIN_MAP};
use crate::installed::InstalledApp;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: Option<i64>,
    pub name: String,
    /// `block` | `allow_only`
    pub mode: String,
    /// Category names: blocked (block mode) or allowed (allow_only mode).
    pub categories: Vec<String>,
    /// Exe names: extra blocked / allowed apps.
    pub apps: Vec<String>,
    /// Domains: extra blocked / allowed websites.
    pub sites: Vec<String>,
    /// Open blocked things during Pomodoro breaks.
    pub unblock_on_breaks: bool,
    /// `allow` | `confirm` | `strict` — what ending a session early takes.
    pub end_early: String,
    #[serde(default)]
    pub builtin: bool,
}

impl Profile {
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() || self.name.len() > 60 {
            return Err("Give the profile a short name".into());
        }
        if !["block", "allow_only"].contains(&self.mode.as_str()) {
            return Err("Unknown profile mode".into());
        }
        if !["allow", "confirm", "strict"].contains(&self.end_early.as_str()) {
            return Err("Unknown early-end rule".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub struct AppRef {
    pub name: String,
    pub exe_name: String,
    pub category: String,
}

/// What a profile will do, shown before a session starts.
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BlockPreview {
    pub mode: String,
    pub blocked_apps: Vec<AppRef>,
    pub blocked_sites: Vec<String>,
    /// Only for `allow_only`.
    pub allowed_apps: Vec<AppRef>,
    pub allowed_sites: Vec<String>,
}

fn json_list(s: &str) -> Vec<String> {
    serde_json::from_str(s).unwrap_or_default()
}

fn row_to_profile(r: &rusqlite::Row) -> rusqlite::Result<Profile> {
    Ok(Profile {
        id: Some(r.get(0)?),
        name: r.get(1)?,
        mode: r.get(2)?,
        categories: json_list(&r.get::<_, String>(3)?),
        apps: json_list(&r.get::<_, String>(4)?),
        sites: json_list(&r.get::<_, String>(5)?),
        unblock_on_breaks: r.get::<_, i64>(6)? != 0,
        end_early: r.get(7)?,
        builtin: r.get::<_, i64>(8)? != 0,
    })
}

const COLS: &str = "id, name, mode, categories, apps, sites, unblock_on_breaks, end_early, builtin";

pub fn list(conn: &Connection) -> rusqlite::Result<Vec<Profile>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM block_profiles ORDER BY builtin DESC, id"
    ))?;
    let rows = stmt.query_map([], row_to_profile)?;
    let out = rows.collect();
    out
}

pub fn get(conn: &Connection, id: i64) -> rusqlite::Result<Option<Profile>> {
    conn.query_row(
        &format!("SELECT {COLS} FROM block_profiles WHERE id = ?1"),
        params![id],
        row_to_profile,
    )
    .optional()
}

pub fn save(conn: &Connection, p: &Profile) -> Result<i64, String> {
    p.validate()?;
    let clean = |v: &[String]| {
        let set: BTreeSet<String> = v
            .iter()
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|s| !s.is_empty())
            .collect();
        serde_json::to_string(&set.into_iter().collect::<Vec<_>>()).unwrap_or_else(|_| "[]".into())
    };
    let cats = serde_json::to_string(&p.categories).unwrap_or_else(|_| "[]".into());
    let res = match p.id {
        Some(id) => conn
            .execute(
                "UPDATE block_profiles SET name = ?1, mode = ?2, categories = ?3, apps = ?4, sites = ?5,
                 unblock_on_breaks = ?6, end_early = ?7 WHERE id = ?8",
                params![p.name.trim(), p.mode, cats, clean(&p.apps), clean(&p.sites), p.unblock_on_breaks, p.end_early, id],
            )
            .map(|_| id),
        None => conn
            .execute(
                "INSERT INTO block_profiles (name, mode, categories, apps, sites, unblock_on_breaks, end_early)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![p.name.trim(), p.mode, cats, clean(&p.apps), clean(&p.sites), p.unblock_on_breaks, p.end_early],
            )
            .map(|_| conn.last_insert_rowid()),
    };
    res.map_err(|e| {
        if e.to_string().contains("UNIQUE") {
            "A profile with that name already exists".to_string()
        } else {
            e.to_string()
        }
    })
}

pub fn delete(conn: &Connection, id: i64) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM block_profiles WHERE id = ?1 AND builtin = 0",
        params![id],
    )?;
    Ok(())
}

/// Every app we know of (used or installed) with its category.
fn candidate_apps(
    conn: &Connection,
    installed: &[InstalledApp],
) -> rusqlite::Result<BTreeMap<String, AppRef>> {
    let mut out = BTreeMap::new();
    let mut stmt = conn.prepare(
        "SELECT a.exe_name, a.display_name, COALESCE(c.name, 'Other') FROM applications a
         LEFT JOIN categories c ON c.id = a.category_id",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })?;
    for row in rows {
        let (exe, name, category) = row?;
        out.insert(
            exe.to_ascii_lowercase(),
            AppRef {
                name,
                exe_name: exe,
                category,
            },
        );
    }
    for a in installed {
        out.entry(a.exe_name.to_ascii_lowercase())
            .or_insert_with(|| AppRef {
                name: a.name.clone(),
                exe_name: a.exe_name.clone(),
                category: default_category_for(&a.exe_name.to_ascii_lowercase()).to_string(),
            });
    }
    // Browsers are filtered per website; core Windows tools are never blocked.
    out.retain(|k, _| browser_family(k).is_none() && !is_protected_name(k));
    Ok(out)
}

/// Domains in the given categories: built-in defaults, overridden by the user's own categorisation.
fn domains_in(conn: &Connection, categories: &[String]) -> rusqlite::Result<BTreeSet<String>> {
    let mut set: BTreeSet<String> = DOMAIN_MAP
        .iter()
        .filter(|(_, c)| categories.iter().any(|x| x == c))
        .map(|(d, _)| d.to_string())
        .collect();
    let mut stmt = conn.prepare("SELECT w.domain, COALESCE(c.name, 'Other') FROM websites w LEFT JOIN categories c ON c.id = w.category_id")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    for row in rows {
        let (domain, cat) = row?;
        if categories.contains(&cat) {
            set.insert(domain);
        } else {
            set.remove(&domain);
        }
    }
    Ok(set)
}

pub fn resolve(
    conn: &Connection,
    p: &Profile,
    installed: &[InstalledApp],
) -> rusqlite::Result<BlockPreview> {
    let apps = candidate_apps(conn, installed)?;
    let listed: BTreeSet<String> = p.apps.iter().map(|a| a.to_ascii_lowercase()).collect();
    let selected =
        |exe: &String, a: &AppRef| p.categories.contains(&a.category) || listed.contains(exe);
    let mut sites = domains_in(conn, &p.categories)?;
    sites.extend(p.sites.iter().map(|s| s.to_ascii_lowercase()));

    let (chosen, rest): (Vec<_>, Vec<_>) = apps.iter().partition(|(exe, a)| selected(exe, a));
    let refs = |v: Vec<(&String, &AppRef)>| {
        let mut out: Vec<AppRef> = v.into_iter().map(|(_, a)| a.clone()).collect();
        out.sort_by_key(|a| a.name.to_lowercase());
        out
    };
    Ok(if p.mode == "allow_only" {
        BlockPreview {
            mode: p.mode.clone(),
            blocked_apps: refs(rest),
            blocked_sites: Vec::new(),
            allowed_apps: refs(chosen),
            allowed_sites: sites.into_iter().collect(),
        }
    } else {
        BlockPreview {
            mode: p.mode.clone(),
            blocked_apps: refs(chosen),
            blocked_sites: sites.into_iter().collect(),
            allowed_apps: Vec::new(),
            allowed_sites: Vec::new(),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use chrono::NaiveDate;

    fn installed(exe: &str, name: &str) -> InstalledApp {
        InstalledApp {
            exe_name: exe.into(),
            name: name.into(),
            path: String::new(),
        }
    }

    #[test]
    fn builtin_profiles_resolve() {
        let conn = db::open_in_memory().unwrap();
        let t = NaiveDate::from_ymd_opt(2026, 11, 18)
            .unwrap()
            .and_hms_opt(9, 0, 0)
            .unwrap();
        db::get_or_create_app(&conn, "Code.exe", "", || "VS Code".into(), t).unwrap();
        db::get_or_create_app(&conn, "chrome.exe", "", || "Chrome".into(), t).unwrap();
        let inst = [
            installed("steam.exe", "Steam"),
            installed("Discord.exe", "Discord"),
            installed("explorer.exe", "Explorer"),
        ];

        let all = list(&conn).unwrap();
        let study = all.iter().find(|p| p.name == "Study Mode").unwrap();
        let prev = resolve(&conn, study, &inst).unwrap();
        let names: Vec<&str> = prev.blocked_apps.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["Steam"],
            "games blocked; Discord (Communication), browsers and Explorer are not"
        );
        assert!(prev.blocked_sites.contains(&"youtube.com".to_string()));
        assert!(prev.blocked_sites.contains(&"instagram.com".to_string()));

        let deep = all.iter().find(|p| p.name == "Deep Focus").unwrap();
        let prev = resolve(&conn, deep, &inst).unwrap();
        assert_eq!(
            prev.allowed_apps
                .iter()
                .map(|a| a.name.as_str())
                .collect::<Vec<_>>(),
            vec!["VS Code"]
        );
        assert!(prev.blocked_apps.iter().any(|a| a.name == "Discord"));
        assert!(prev.allowed_sites.contains(&"github.com".to_string()));
        assert!(
            prev.allowed_sites.contains(&"coursera.org".to_string()),
            "Study category sites are allowed"
        );
    }

    #[test]
    fn save_and_validate() {
        let conn = db::open_in_memory().unwrap();
        let mut p = Profile {
            id: None,
            name: "Exam week".into(),
            mode: "block".into(),
            categories: vec!["Gaming".into()],
            apps: vec!["Discord.exe".into()],
            sites: vec![" Reddit.com ".into()],
            unblock_on_breaks: true,
            end_early: "strict".into(),
            builtin: false,
        };
        let id = save(&conn, &p).unwrap();
        let back = get(&conn, id).unwrap().unwrap();
        assert_eq!(back.apps, vec!["discord.exe".to_string()]);
        assert_eq!(back.sites, vec!["reddit.com".to_string()]);
        assert!(save(&conn, &p).unwrap_err().contains("already exists"));
        p.mode = "nope".into();
        assert!(save(&conn, &p).is_err());
        // Built-ins can't be deleted.
        let builtin = list(&conn)
            .unwrap()
            .into_iter()
            .find(|p| p.builtin)
            .unwrap();
        delete(&conn, builtin.id.unwrap()).unwrap();
        assert!(get(&conn, builtin.id.unwrap()).unwrap().is_some());
    }
}
