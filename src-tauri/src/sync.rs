//! Multi-device sync engine (#61, design `docs/SYNC-DESIGN.md`).
//!
//! Off until the user signs in. Then, on its own thread, about once a minute
//! (or right away when asked) it:
//!
//! 1. pushes this device's changed rows as encrypted records and marks them
//!    synced only once the server acknowledges them;
//! 2. pulls other devices' records since the last cursor and applies them to
//!    the same local tables, tagged with their device ID.
//!
//! Tracking never waits for sync, and nothing is lost while offline: rows
//! simply stay pending until a push succeeds.

use crate::db;
use chrono::{Local, NaiveDateTime};
use focustrack_sync_proto::api::{DeviceInfo, NewDevice};
use focustrack_sync_proto::client::{self, Client, ClientError};
use focustrack_sync_proto::crypto::{AccountKey, KdfParams};
use focustrack_sync_proto::{hlc, kind, recovery, Envelope};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const SYNC_EVERY: Duration = Duration::from_secs(60);
const BATCH: usize = 500;
/// Detailed history a new device downloads (design D7).
const FIRST_SYNC_DAYS: u32 = 35;
/// This device's platform as other devices see it.
#[cfg(target_os = "android")]
pub const PLATFORM: &str = "android";
#[cfg(target_os = "ios")]
pub const PLATFORM: &str = "ios";
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub const PLATFORM: &str = "windows";

// ------------------------------------------------------------------ secrets

/// What lets this device sync. Never stored in the database.
pub struct Secrets {
    pub refresh_token: String,
    pub account_key: AccountKey,
}

#[derive(Serialize, Deserialize)]
struct StoredSecrets {
    refresh_token: String,
    account_key: String,
}

/// Device secrets on disk, encrypted for the signed-in Windows user (DPAPI).
mod vault {
    use super::*;
    use base64::Engine;
    const FILE: &str = "sync-secrets.bin";
    fn b64() -> base64::engine::GeneralPurpose {
        base64::engine::general_purpose::STANDARD
    }

    pub fn save(dir: &Path, s: &Secrets) -> std::io::Result<()> {
        let plain = serde_json::to_vec(&StoredSecrets {
            refresh_token: s.refresh_token.clone(),
            account_key: b64().encode(s.account_key.bytes()),
        })?;
        std::fs::write(dir.join(FILE), protect(&plain)?)
    }

    pub fn load(dir: &Path) -> Option<Secrets> {
        let data = std::fs::read(dir.join(FILE)).ok()?;
        let plain = unprotect(&data).ok()?;
        let s: StoredSecrets = serde_json::from_slice(&plain).ok()?;
        let key: [u8; 32] = b64().decode(s.account_key).ok()?.try_into().ok()?;
        Some(Secrets {
            refresh_token: s.refresh_token,
            account_key: AccountKey::from_bytes(key),
        })
    }

    pub fn clear(dir: &Path) {
        let _ = std::fs::remove_file(dir.join(FILE));
    }

    #[cfg(windows)]
    fn dpapi(data: &[u8], encrypt: bool) -> std::io::Result<Vec<u8>> {
        use windows_sys::Win32::Foundation::LocalFree;
        use windows_sys::Win32::Security::Cryptography::{
            CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
        };
        let input = CRYPT_INTEGER_BLOB {
            cbData: data.len() as u32,
            pbData: data.as_ptr() as *mut u8,
        };
        let mut out = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: std::ptr::null_mut(),
        };
        let ok = unsafe {
            if encrypt {
                CryptProtectData(
                    &input,
                    std::ptr::null(),
                    std::ptr::null(),
                    std::ptr::null(),
                    std::ptr::null(),
                    CRYPTPROTECT_UI_FORBIDDEN,
                    &mut out,
                )
            } else {
                CryptUnprotectData(
                    &input,
                    std::ptr::null_mut(),
                    std::ptr::null(),
                    std::ptr::null(),
                    std::ptr::null(),
                    CRYPTPROTECT_UI_FORBIDDEN,
                    &mut out,
                )
            }
        };
        if ok == 0 || out.pbData.is_null() {
            return Err(std::io::Error::last_os_error());
        }
        let bytes = unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec() };
        unsafe {
            LocalFree(out.pbData as _);
        }
        Ok(bytes)
    }

    #[cfg(windows)]
    fn protect(d: &[u8]) -> std::io::Result<Vec<u8>> {
        dpapi(d, true)
    }
    #[cfg(windows)]
    fn unprotect(d: &[u8]) -> std::io::Result<Vec<u8>> {
        dpapi(d, false)
    }
    // Other platforms use their own key stores (Keystore/Keychain) in their apps.
    #[cfg(not(windows))]
    fn protect(d: &[u8]) -> std::io::Result<Vec<u8>> {
        Ok(d.to_vec())
    }
    #[cfg(not(windows))]
    fn unprotect(d: &[u8]) -> std::io::Result<Vec<u8>> {
        Ok(d.to_vec())
    }
}

// ------------------------------------------------------------------- status

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    pub signed_in: bool,
    pub enabled: bool,
    pub email: Option<String>,
    pub server: Option<String>,
    pub device_id: String,
    pub device_name: String,
    /// `off` | `idle` | `syncing` | `error` | `signed_out`
    pub state: String,
    pub last_sync: Option<String>,
    pub last_error: Option<String>,
    /// This device's changes not yet uploaded.
    pub pending: i64,
    /// Other devices this one has data from.
    pub other_devices: i64,
}

pub struct SyncHandle {
    data_dir: PathBuf,
    status: Mutex<SyncStatus>,
    secrets: Mutex<Option<Secrets>>,
    access: Mutex<Option<String>>,
    wake: AtomicBool,
    /// Held while a sync runs, so sign-in/out never overlap with one.
    running: Mutex<()>,
}

impl SyncHandle {
    pub fn new(data_dir: PathBuf) -> Arc<SyncHandle> {
        let secrets = vault::load(&data_dir);
        Arc::new(SyncHandle {
            data_dir,
            status: Mutex::new(SyncStatus::default()),
            secrets: Mutex::new(secrets),
            access: Mutex::new(None),
            wake: AtomicBool::new(true),
            running: Mutex::new(()),
        })
    }

    pub fn status(&self) -> SyncStatus {
        self.status.lock().map(|s| s.clone()).unwrap_or_default()
    }

    /// Ask the engine to sync now (after a sign-in, a settings change, a button press).
    pub fn wake(&self) {
        self.wake.store(true, Ordering::SeqCst);
    }

    fn set(&self, f: impl FnOnce(&mut SyncStatus)) {
        if let Ok(mut s) = self.status.lock() {
            f(&mut s);
        }
    }

    fn signed_in(&self) -> bool {
        self.secrets.lock().map(|s| s.is_some()).unwrap_or(false)
    }
}

// ----------------------------------------------------------------- settings

fn setting(conn: &Connection, key: &str) -> Option<String> {
    db::get_setting(conn, key)
        .ok()
        .flatten()
        .filter(|v| !v.is_empty())
}

pub fn device_id(conn: &Connection) -> rusqlite::Result<String> {
    Ok(db::get_setting(conn, "device_id")?.unwrap_or_default())
}

fn default_device_name() -> String {
    std::env::var("COMPUTERNAME")
        .map(|n| {
            let mut c = n.to_lowercase();
            if let Some(f) = c.get_mut(0..1) {
                f.make_ascii_uppercase();
            }
            c
        })
        .unwrap_or_else(|_| {
            if PLATFORM == "android" {
                "Android phone".into()
            } else {
                "This PC".into()
            }
        })
}

/// This device's name (as shown to the user's other devices).
pub fn device_name(conn: &Connection) -> String {
    let base = setting(conn, "device_name").unwrap_or_else(default_device_name);
    match crate::profile::name() {
        Some(p) if setting(conn, "device_name").is_none() => format!("{base} ({p})"),
        _ => base,
    }
}

fn now() -> NaiveDateTime {
    Local::now().naive_local()
}

fn today() -> String {
    now().format(db::DAY_FMT).to_string()
}

/// Refreshes the status shown in Settings from the database.
pub fn refresh_status(handle: &SyncHandle, conn: &Connection) {
    let enabled = setting(conn, "sync_enabled").as_deref() == Some("1");
    let signed_in = handle.signed_in();
    let pending = pending_count(conn).unwrap_or(0);
    let others = conn
        .query_row(
            &format!(
                "SELECT COUNT(*) FROM devices WHERE id != {}",
                db::OWN_DEVICE
            ),
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let dev = device_id(conn).unwrap_or_default();
    let name = device_name(conn);
    let email = setting(conn, "sync_email");
    let server = setting(conn, "sync_server");
    let last_sync = setting(conn, "sync_last");
    handle.set(|s| {
        s.signed_in = signed_in;
        s.enabled = enabled;
        s.email = email;
        s.server = server;
        s.device_id = dev;
        s.device_name = name;
        s.pending = pending;
        s.other_devices = others;
        s.last_sync = last_sync;
        if !(signed_in && enabled) {
            // A removed device keeps showing why it stopped.
            if s.state != "signed_out" {
                s.state = "off".into();
            }
        } else if s.state.is_empty() || s.state == "off" {
            s.state = "idle".into();
        }
    });
}

fn pending_count(conn: &Connection) -> rusqlite::Result<i64> {
    let mut n = 0i64;
    for t in db::OWNED_TABLES {
        n += conn.query_row(
            &format!(
                "SELECT COUNT(*) FROM {t} WHERE device_id = {} AND version > synced_version",
                db::OWN_DEVICE
            ),
            [],
            |r| r.get::<_, i64>(0),
        )?;
    }
    for t in db::SHARED_TABLES {
        n += conn.query_row(
            &format!("SELECT COUNT(*) FROM {t} WHERE version > synced_version"),
            [],
            |r| r.get::<_, i64>(0),
        )?;
    }
    n += conn.query_row(
        "SELECT COUNT(*) FROM sync_tombstones WHERE synced = 0",
        [],
        |r| r.get::<_, i64>(0),
    )?;
    Ok(n)
}

// --------------------------------------------------------------------- push

/// Where a pushed record came from, to mark it synced once acknowledged.
enum Source {
    Table(&'static str),
    Tombstone,
    Device,
}

struct Outgoing {
    env: Envelope,
    source: Source,
}

fn s(r: &rusqlite::Row, i: usize) -> rusqlite::Result<String> {
    r.get::<_, Option<String>>(i).map(|v| v.unwrap_or_default())
}

/// This device's records waiting to be uploaded (at most `limit`).
fn collect_pending(
    conn: &Connection,
    key: &AccountKey,
    limit: usize,
) -> rusqlite::Result<Vec<Outgoing>> {
    let me = device_id(conn)?;
    let mut out: Vec<Outgoing> = Vec::new();
    let own = db::OWN_DEVICE;
    let lim = limit as i64;
    let seal = |out: &mut Vec<Outgoing>,
                id: String,
                kind: &str,
                day: String,
                version: i64,
                hlc: Option<String>,
                deleted: bool,
                body: Value,
                source: Source| {
        out.push(Outgoing {
            env: Envelope::seal(
                key,
                &id,
                kind,
                &me,
                &day,
                version.max(1),
                hlc,
                deleted,
                &body,
            ),
            source,
        });
    };

    // The device record (name and status), whenever it changed.
    let name = device_name(conn);
    let dev_version: i64 = setting(conn, "device_record_version")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    if setting(conn, "device_record_synced").as_deref()
        != Some(&format!("{dev_version}:{}", device_signature(conn)))
    {
        seal(
            &mut out,
            me.clone(),
            kind::DEVICE,
            today(),
            dev_version + 1,
            None,
            false,
            json!({
                "name": name,
                "platform": PLATFORM,
                "appVersion": env!("CARGO_PKG_VERSION"),
                "paused": setting(conn, "paused").as_deref() == Some("1"),
                "syncError": setting(conn, "sync_error"),
            }),
            Source::Device,
        );
    }

    // Remote controls for devices (#66).
    let mut stmt = conn.prepare(
        "SELECT uuid, version, hlc, target, pause_tracking, sync_enabled FROM device_controls
         WHERE version > synced_version LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![lim], |r| {
        Ok((
            s(r, 0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, Option<String>>(2)?,
            json!({ "target": s(r, 3)?, "pauseTracking": r.get::<_, i64>(4)? != 0,
                    "syncEnabled": r.get::<_, i64>(5)? != 0 }),
        ))
    })?;
    for row in rows {
        let (id, v, h, body) = row?;
        seal(
            &mut out,
            id,
            kind::DEVICE_CONTROL,
            today(),
            v,
            h,
            false,
            body,
            Source::Table("device_controls"),
        );
    }

    let mut stmt = conn.prepare(&format!(
        "SELECT s.uuid, s.day, s.version, a.app_key, a.exe_name, COALESCE(a.exe_path, ''), a.display_name,
                s.start_time, s.end_time, s.duration_secs
         FROM usage_sessions s JOIN applications a ON a.id = s.app_id
         WHERE s.device_id = {own} AND s.version > s.synced_version ORDER BY s.id LIMIT ?1"
    ))?;
    let rows = stmt.query_map(params![lim], |r| {
        Ok((
            s(r, 0)?,
            s(r, 1)?,
            r.get::<_, i64>(2)?,
            json!({ "appKey": s(r, 3)?, "exe": s(r, 4)?, "path": s(r, 5)?, "name": s(r, 6)?,
                    "start": s(r, 7)?, "end": s(r, 8)?, "secs": r.get::<_, i64>(9)? }),
        ))
    })?;
    for row in rows {
        let (id, day, v, body) = row?;
        seal(
            &mut out,
            id,
            kind::USAGE,
            day,
            v,
            None,
            false,
            body,
            Source::Table("usage_sessions"),
        );
    }

    let mut stmt = conn.prepare(&format!(
        "SELECT uuid, day, version, start_time, end_time, duration_secs FROM idle_periods
         WHERE device_id = {own} AND version > synced_version ORDER BY id LIMIT ?1"
    ))?;
    let rows = stmt.query_map(params![lim], |r| {
        Ok((
            s(r, 0)?,
            s(r, 1)?,
            r.get::<_, i64>(2)?,
            json!({ "start": s(r, 3)?, "end": s(r, 4)?, "secs": r.get::<_, i64>(5)? }),
        ))
    })?;
    for row in rows {
        let (id, day, v, body) = row?;
        seal(
            &mut out,
            id,
            kind::IDLE,
            day,
            v,
            None,
            false,
            body,
            Source::Table("idle_periods"),
        );
    }

    let mut stmt = conn.prepare(&format!(
        "SELECT s.uuid, s.day, s.version, w.domain, s.browser, s.start_time, s.end_time, s.duration_secs
         FROM website_sessions s JOIN websites w ON w.id = s.website_id
         WHERE s.device_id = {own} AND s.version > s.synced_version ORDER BY s.id LIMIT ?1"
    ))?;
    let rows = stmt.query_map(params![lim], |r| {
        Ok((
            s(r, 0)?,
            s(r, 1)?,
            r.get::<_, i64>(2)?,
            json!({ "domain": s(r, 3)?, "browser": s(r, 4)?, "start": s(r, 5)?, "end": s(r, 6)?,
                    "secs": r.get::<_, i64>(7)? }),
        ))
    })?;
    for row in rows {
        let (id, day, v, body) = row?;
        seal(
            &mut out,
            id,
            kind::SITE,
            day,
            v,
            None,
            false,
            body,
            Source::Table("website_sessions"),
        );
    }

    let mut stmt = conn.prepare(&format!(
        "SELECT uuid, day, version, label, kind, planned_secs, start_time, end_time, actual_secs, completed,
                mode, subject, goal_text, block_distracting
         FROM focus_sessions WHERE device_id = {own} AND version > synced_version ORDER BY id LIMIT ?1"
    ))?;
    let rows = stmt.query_map(params![lim], |r| {
        Ok((
            s(r, 0)?,
            s(r, 1)?,
            r.get::<_, i64>(2)?,
            json!({ "label": s(r, 3)?, "kind": s(r, 4)?, "plannedSecs": r.get::<_, i64>(5)?,
                    "start": s(r, 6)?, "end": r.get::<_, Option<String>>(7)?, "actualSecs": r.get::<_, i64>(8)?,
                    "completed": r.get::<_, i64>(9)? != 0, "mode": s(r, 10)?, "subject": r.get::<_, Option<String>>(11)?,
                    "goalText": r.get::<_, Option<String>>(12)?, "blocking": r.get::<_, i64>(13)? != 0 }),
        ))
    })?;
    for row in rows {
        let (id, day, v, body) = row?;
        seal(
            &mut out,
            id,
            kind::FOCUS,
            day,
            v,
            None,
            false,
            body,
            Source::Table("focus_sessions"),
        );
    }

    let mut stmt = conn.prepare(
        "SELECT l.uuid, l.version, l.hlc, l.target_type, a.app_key, a.exe_name, COALESCE(a.exe_path, ''),
                a.display_name, w.domain, l.daily_limit_secs, l.warn_pct, l.action, l.enabled
         FROM usage_limits l
         LEFT JOIN applications a ON l.target_type = 'app' AND a.id = l.target_id
         LEFT JOIN websites w ON l.target_type = 'website' AND w.id = l.target_id
         WHERE l.version > l.synced_version LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![lim], |r| {
        Ok((
            s(r, 0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, Option<String>>(2)?,
            json!({ "targetType": s(r, 3)?, "appKey": s(r, 4)?, "exe": s(r, 5)?, "path": s(r, 6)?,
                    "name": s(r, 7)?, "domain": s(r, 8)?, "dailyLimitSecs": r.get::<_, i64>(9)?,
                    "warnPct": r.get::<_, i64>(10)?, "action": s(r, 11)?, "enabled": r.get::<_, i64>(12)? != 0 }),
        ))
    })?;
    for row in rows {
        let (id, v, h, body) = row?;
        seal(
            &mut out,
            id,
            kind::LIMIT,
            today(),
            v,
            h,
            false,
            body,
            Source::Table("usage_limits"),
        );
    }

    let mut stmt = conn.prepare(
        "SELECT g.uuid, g.version, g.hlc, g.kind, c.name, g.target, g.title, g.enabled
         FROM goals g LEFT JOIN categories c ON c.id = g.category_id
         WHERE g.version > g.synced_version LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![lim], |r| {
        Ok((
            s(r, 0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, Option<String>>(2)?,
            json!({ "kind": s(r, 3)?, "category": r.get::<_, Option<String>>(4)?, "target": r.get::<_, i64>(5)?,
                    "title": r.get::<_, Option<String>>(6)?, "enabled": r.get::<_, i64>(7)? != 0 }),
        ))
    })?;
    for row in rows {
        let (id, v, h, body) = row?;
        seal(
            &mut out,
            id,
            kind::GOAL,
            today(),
            v,
            h,
            false,
            body,
            Source::Table("goals"),
        );
    }

    let mut stmt = conn.prepare(
        "SELECT k.uuid, k.version, k.hlc, g.uuid, k.day FROM goal_checks k JOIN goals g ON g.id = k.goal_id
         WHERE k.version > k.synced_version LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![lim], |r| {
        Ok((
            s(r, 0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, Option<String>>(2)?,
            s(r, 4)?,
            json!({ "goal": s(r, 3)?, "day": s(r, 4)? }),
        ))
    })?;
    for row in rows {
        let (id, v, h, day, body) = row?;
        seal(
            &mut out,
            id,
            kind::GOAL_CHECK,
            day,
            v,
            h,
            false,
            body,
            Source::Table("goal_checks"),
        );
    }

    let mut stmt = conn.prepare(
        "SELECT uuid, kind, day, hlc, version FROM sync_tombstones WHERE synced = 0 LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![lim], |r| {
        Ok((s(r, 0)?, s(r, 1)?, s(r, 2)?, s(r, 3)?, r.get::<_, i64>(4)?))
    })?;
    for row in rows {
        let (id, k, day, h, v) = row?;
        let k: &'static str = match k.as_str() {
            "limit" => kind::LIMIT,
            "goal" => kind::GOAL,
            _ => kind::GOAL_CHECK,
        };
        seal(
            &mut out,
            id,
            k,
            day,
            v,
            Some(h),
            true,
            json!({}),
            Source::Tombstone,
        );
    }

    out.truncate(limit);
    Ok(out)
}

/// Marks acknowledged records as synced (only if they haven't changed since).
fn mark_synced(
    conn: &Connection,
    sent: &[Outgoing],
    acked: &HashMap<String, i64>,
) -> rusqlite::Result<()> {
    for o in sent {
        let Some(&v) = acked.get(&o.env.id) else {
            continue;
        };
        match o.source {
            Source::Table(t) => {
                conn.execute(
                    &format!("UPDATE {t} SET synced_version = ?2 WHERE uuid = ?1 AND version = ?2"),
                    params![o.env.id, v],
                )?;
            }
            Source::Tombstone => {
                conn.execute(
                    "UPDATE sync_tombstones SET synced = 1 WHERE uuid = ?1 AND version = ?2",
                    params![o.env.id, v],
                )?;
            }
            Source::Device => {
                db::set_setting(conn, "device_record_version", &v.to_string())?;
                db::set_setting(
                    conn,
                    "device_record_synced",
                    &format!("{v}:{}", device_signature(conn)),
                )?;
            }
        }
    }
    Ok(())
}

// --------------------------------------------------------------------- pull

/// Applies other devices' records. Runs inside the caller's transaction.
fn apply(
    conn: &Connection,
    key: &AccountKey,
    records: &[Envelope],
) -> rusqlite::Result<(usize, usize)> {
    let me = device_id(conn)?;
    let mut applied = 0;
    let mut unreadable = 0;
    let mut newest_seen: u64 = 0;
    db::set_setting(conn, "sync_applying", "1")?;
    for env in records {
        if env.device == me {
            continue;
        }
        if let Some(h) = env.hlc.as_deref().and_then(hlc::Hlc::parse) {
            newest_seen = newest_seen.max(h.wall_ms);
        }
        if env.deleted {
            apply_delete(conn, env)?;
            applied += 1;
            continue;
        }
        let body: Value = match env.open(key) {
            Ok(b) => b,
            Err(_) => {
                unreadable += 1;
                continue;
            }
        };
        if apply_one(conn, env, &body)? {
            applied += 1;
        }
    }
    if newest_seen > 0 {
        let seen: u64 = setting(conn, "hlc_seen")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        if newest_seen > seen {
            db::set_setting(conn, "hlc_seen", &newest_seen.to_string())?;
        }
    }
    db::set_setting(conn, "sync_applying", "0")?;
    Ok((applied, unreadable))
}

fn table_for(k: &str) -> Option<&'static str> {
    Some(match k {
        kind::USAGE => "usage_sessions",
        kind::IDLE => "idle_periods",
        kind::SITE => "website_sessions",
        kind::FOCUS => "focus_sessions",
        kind::LIMIT => "usage_limits",
        kind::GOAL => "goals",
        kind::GOAL_CHECK => "goal_checks",
        _ => return None,
    })
}

/// The local row's (version, hlc) for a record, if present.
fn local(
    conn: &Connection,
    table: &str,
    uuid: &str,
) -> rusqlite::Result<Option<(i64, Option<String>)>> {
    let hlc_col = if db::SHARED_TABLES.contains(&table) {
        "hlc"
    } else {
        "NULL"
    };
    conn.query_row(
        &format!("SELECT version, {hlc_col} FROM {table} WHERE uuid = ?1"),
        params![uuid],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .optional()
}

/// Whether an incoming record should replace the local copy.
fn incoming_wins(env: &Envelope, local: &Option<(i64, Option<String>)>) -> bool {
    match local {
        None => true,
        Some((v, h)) => {
            if kind::is_shared(&env.kind) {
                hlc::newer(env.hlc.as_deref().unwrap_or(""), h.as_deref().unwrap_or(""))
            } else {
                env.version > *v
            }
        }
    }
}

fn apply_delete(conn: &Connection, env: &Envelope) -> rusqlite::Result<()> {
    let Some(t) = table_for(&env.kind) else {
        return Ok(());
    };
    let l = local(conn, t, &env.id)?;
    if l.is_some() && incoming_wins(env, &l) {
        conn.execute(&format!("DELETE FROM {t} WHERE uuid = ?1"), params![env.id])?;
    }
    Ok(())
}

fn str_of<'a>(b: &'a Value, k: &str) -> &'a str {
    b.get(k).and_then(Value::as_str).unwrap_or("")
}

fn int_of(b: &Value, k: &str) -> i64 {
    b.get(k).and_then(Value::as_i64).unwrap_or(0)
}

fn bool_of(b: &Value, k: &str) -> bool {
    b.get(k).and_then(Value::as_bool).unwrap_or(false)
}

fn opt_str(b: &Value, k: &str) -> Option<String> {
    b.get(k).and_then(Value::as_str).map(str::to_string)
}

fn app_id(conn: &Connection, b: &Value) -> rusqlite::Result<i64> {
    let start = db::parse_time(str_of(b, "start")).unwrap_or_else(now);
    let name = str_of(b, "name").to_string();
    let exe = str_of(b, "exe");
    let (id, _) = db::get_or_create_app(conn, exe, str_of(b, "path"), || name, start)?;
    Ok(id)
}

/// Applies one record; returns whether anything changed.
fn apply_one(conn: &Connection, env: &Envelope, b: &Value) -> rusqlite::Result<bool> {
    if env.kind == kind::PURGE {
        // Another device deleted history; remove our copies too (#67).
        let device = opt_str(b, "device");
        purge_local(conn, str_of(b, "until"), device.as_deref())?;
        return Ok(true);
    }
    if env.kind == kind::DEVICE {
        conn.execute(
            "INSERT INTO devices (id, name, platform, app_version, last_seen, paused, sync_error)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET name = excluded.name, platform = excluded.platform,
                 app_version = excluded.app_version, last_seen = excluded.last_seen,
                 paused = excluded.paused, sync_error = excluded.sync_error",
            params![
                env.device,
                str_of(b, "name"),
                str_of(b, "platform"),
                str_of(b, "appVersion"),
                db::fmt_time(now()),
                bool_of(b, "paused"),
                opt_str(b, "syncError")
            ],
        )?;
        return Ok(true);
    }
    if env.kind == kind::DEVICE_CONTROL {
        return apply_control(conn, env, b);
    }
    let Some(t) = table_for(&env.kind) else {
        return Ok(false); // a newer app's record kind: ignore
    };
    let l = local(conn, t, &env.id)?;
    if !incoming_wins(env, &l) {
        return Ok(false);
    }
    let (dev, v) = (&env.device, env.version);
    match env.kind.as_str() {
        kind::USAGE => {
            let app = app_id(conn, b)?;
            conn.execute(
                "INSERT INTO usage_sessions (app_id, day, start_time, end_time, duration_secs, uuid, device_id, version, synced_version)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
                 ON CONFLICT(uuid) DO UPDATE SET app_id = excluded.app_id, day = excluded.day, start_time = excluded.start_time,
                     end_time = excluded.end_time, duration_secs = excluded.duration_secs, version = excluded.version,
                     synced_version = excluded.synced_version",
                params![app, env.day, str_of(b, "start"), str_of(b, "end"), int_of(b, "secs"), env.id, dev, v],
            )?;
        }
        kind::IDLE => {
            conn.execute(
                "INSERT INTO idle_periods (day, start_time, end_time, duration_secs, uuid, device_id, version, synced_version)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)
                 ON CONFLICT(uuid) DO UPDATE SET day = excluded.day, start_time = excluded.start_time, end_time = excluded.end_time,
                     duration_secs = excluded.duration_secs, version = excluded.version, synced_version = excluded.synced_version",
                params![env.day, str_of(b, "start"), str_of(b, "end"), int_of(b, "secs"), env.id, dev, v],
            )?;
        }
        kind::SITE => {
            let start = db::parse_time(str_of(b, "start")).unwrap_or_else(now);
            let site = db::get_or_create_website(conn, str_of(b, "domain"), start)?;
            conn.execute(
                "INSERT INTO website_sessions (website_id, browser, day, start_time, end_time, duration_secs, uuid, device_id, version, synced_version)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)
                 ON CONFLICT(uuid) DO UPDATE SET website_id = excluded.website_id, browser = excluded.browser, day = excluded.day,
                     start_time = excluded.start_time, end_time = excluded.end_time, duration_secs = excluded.duration_secs,
                     version = excluded.version, synced_version = excluded.synced_version",
                params![site, str_of(b, "browser"), env.day, str_of(b, "start"), str_of(b, "end"), int_of(b, "secs"), env.id, dev, v],
            )?;
        }
        kind::FOCUS => {
            conn.execute(
                "INSERT INTO focus_sessions (label, kind, planned_secs, day, start_time, end_time, actual_secs, completed,
                     block_distracting, mode, subject, goal_text, uuid, device_id, version, synced_version)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?15)
                 ON CONFLICT(uuid) DO UPDATE SET label = excluded.label, kind = excluded.kind, planned_secs = excluded.planned_secs,
                     day = excluded.day, start_time = excluded.start_time, end_time = excluded.end_time,
                     actual_secs = excluded.actual_secs, completed = excluded.completed,
                     block_distracting = excluded.block_distracting, mode = excluded.mode, subject = excluded.subject,
                     goal_text = excluded.goal_text, version = excluded.version, synced_version = excluded.synced_version",
                params![
                    str_of(b, "label"),
                    if str_of(b, "kind") == "break" { "break" } else { "focus" },
                    int_of(b, "plannedSecs"),
                    env.day,
                    str_of(b, "start"),
                    opt_str(b, "end"),
                    int_of(b, "actualSecs"),
                    bool_of(b, "completed"),
                    bool_of(b, "blocking"),
                    if str_of(b, "mode") == "study" { "study" } else { "focus" },
                    opt_str(b, "subject"),
                    opt_str(b, "goalText"),
                    env.id,
                    dev,
                    v
                ],
            )?;
        }
        kind::LIMIT => {
            let website = str_of(b, "targetType") == "website";
            let target = if website {
                let d = str_of(b, "domain");
                if d.is_empty() {
                    return Ok(false);
                }
                db::get_or_create_website(conn, d, now())?
            } else {
                if str_of(b, "exe").is_empty() {
                    return Ok(false);
                }
                app_id(conn, b)?
            };
            let tt = if website { "website" } else { "app" };
            // Two devices made a limit for the same app offline: keep the newest.
            let other: Option<(String, Option<String>)> = conn
                .query_row(
                    "SELECT uuid, hlc FROM usage_limits WHERE target_type = ?1 AND target_id = ?2 AND uuid IS NOT ?3",
                    params![tt, target, env.id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            if let Some((other_id, other_hlc)) = other {
                if !hlc::newer(
                    env.hlc.as_deref().unwrap_or(""),
                    other_hlc.as_deref().unwrap_or(""),
                ) {
                    return Ok(false);
                }
                conn.execute(
                    "DELETE FROM usage_limits WHERE uuid = ?1",
                    params![other_id],
                )?;
            }
            let action = match str_of(b, "action") {
                a @ ("notify" | "block" | "warn_then_block") => a,
                _ => "notify",
            };
            conn.execute(
                "INSERT INTO usage_limits (target_type, target_id, daily_limit_secs, warn_pct, block, action, enabled,
                     uuid, device_id, version, synced_version, hlc)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10, ?11)
                 ON CONFLICT(uuid) DO UPDATE SET target_type = excluded.target_type, target_id = excluded.target_id,
                     daily_limit_secs = excluded.daily_limit_secs, warn_pct = excluded.warn_pct, block = excluded.block,
                     action = excluded.action, enabled = excluded.enabled, device_id = excluded.device_id,
                     version = excluded.version, synced_version = excluded.synced_version, hlc = excluded.hlc",
                params![
                    tt,
                    target,
                    int_of(b, "dailyLimitSecs").max(60),
                    int_of(b, "warnPct").clamp(1, 100),
                    action != "notify",
                    action,
                    bool_of(b, "enabled"),
                    env.id,
                    dev,
                    v,
                    env.hlc
                ],
            )?;
        }
        kind::GOAL => {
            let goal_kind = str_of(b, "kind");
            if !crate::goals::KINDS.contains(&goal_kind) {
                return Ok(false);
            }
            let category: Option<i64> = match opt_str(b, "category") {
                Some(name) => conn
                    .query_row(
                        "SELECT id FROM categories WHERE name = ?1",
                        params![name],
                        |r| r.get(0),
                    )
                    .optional()?,
                None => None,
            };
            conn.execute(
                "INSERT INTO goals (kind, category_id, target, title, enabled, uuid, device_id, version, synced_version, hlc)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, ?9)
                 ON CONFLICT(uuid) DO UPDATE SET kind = excluded.kind, category_id = excluded.category_id,
                     target = excluded.target, title = excluded.title, enabled = excluded.enabled,
                     device_id = excluded.device_id, version = excluded.version, synced_version = excluded.synced_version,
                     hlc = excluded.hlc",
                params![goal_kind, category, int_of(b, "target"), opt_str(b, "title"), bool_of(b, "enabled"), env.id, dev, v, env.hlc],
            )?;
        }
        kind::GOAL_CHECK => {
            let goal: Option<i64> = conn
                .query_row(
                    "SELECT id FROM goals WHERE uuid = ?1",
                    params![str_of(b, "goal")],
                    |r| r.get(0),
                )
                .optional()?;
            let Some(goal) = goal else {
                return Ok(false);
            };
            conn.execute(
                "INSERT INTO goal_checks (goal_id, day, uuid, device_id, version, synced_version, hlc)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?6)
                 ON CONFLICT(goal_id, day) DO UPDATE SET uuid = excluded.uuid, device_id = excluded.device_id,
                     version = excluded.version, synced_version = excluded.synced_version, hlc = excluded.hlc",
                params![goal, str_of(b, "day"), env.id, dev, v, env.hlc],
            )?;
        }
        _ => return Ok(false),
    }
    Ok(true)
}

// ------------------------------------------------------------------- engine

#[derive(Debug)]
pub enum SyncError {
    NotSignedIn,
    /// The server no longer accepts this device (removed, or account deleted).
    Revoked,
    Client(ClientError),
    Db(rusqlite::Error),
}

impl std::fmt::Display for SyncError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyncError::NotSignedIn => write!(f, "Not signed in"),
            SyncError::Revoked => write!(
                f,
                "This device was removed from your account or signed out elsewhere. Sign in again to keep syncing."
            ),
            SyncError::Client(e) => write!(f, "{e}"),
            SyncError::Db(e) => write!(f, "database error: {e}"),
        }
    }
}

impl From<rusqlite::Error> for SyncError {
    fn from(e: rusqlite::Error) -> Self {
        SyncError::Db(e)
    }
}

impl From<ClientError> for SyncError {
    fn from(e: ClientError) -> Self {
        SyncError::Client(e)
    }
}

fn client_for(conn: &Connection) -> Result<Client, SyncError> {
    let server = setting(conn, "sync_server").ok_or(SyncError::NotSignedIn)?;
    Ok(Client::new(&server))
}

/// Calls the server with a valid access token, refreshing it once if needed.
fn call<T>(
    handle: &SyncHandle,
    conn: &Connection,
    f: impl Fn(&Client) -> client::Result<T>,
) -> Result<T, SyncError> {
    let mut c = client_for(conn)?;
    let cached = handle.access.lock().ok().and_then(|a| a.clone());
    if let Some(token) = cached {
        c.set_access_token(Some(token));
        match f(&c) {
            Err(e) if e.status() == Some(401) => {}
            other => return other.map_err(SyncError::from),
        }
    }
    let refresh = handle
        .secrets
        .lock()
        .ok()
        .and_then(|s| s.as_ref().map(|s| s.refresh_token.clone()))
        .ok_or(SyncError::NotSignedIn)?;
    let token = match c.refresh(&focustrack_sync_proto::api::RefreshRequest {
        device_id: device_id(conn)?,
        refresh_token: refresh,
    }) {
        Ok(t) => t.access_token,
        Err(e) if e.status() == Some(401) => return Err(SyncError::Revoked),
        Err(e) => return Err(e.into()),
    };
    if let Ok(mut a) = handle.access.lock() {
        *a = Some(token.clone());
    }
    c.set_access_token(Some(token));
    match f(&c) {
        Err(e) if e.status() == Some(401) => Err(SyncError::Revoked),
        other => other.map_err(SyncError::from),
    }
}

/// One full sync: push everything pending, then pull everything new.
pub fn sync_once(handle: &SyncHandle, conn: &mut Connection) -> Result<(), SyncError> {
    let _running = handle.running.lock().unwrap_or_else(|e| e.into_inner());
    let key = handle
        .secrets
        .lock()
        .ok()
        .and_then(|s| s.as_ref().map(|s| s.account_key.clone()))
        .ok_or(SyncError::NotSignedIn)?;

    // Push until nothing is pending (each batch is small; stop if no progress).
    for _ in 0..1000 {
        let batch = collect_pending(conn, &key, BATCH)?;
        if batch.is_empty() {
            break;
        }
        let envs: Vec<Envelope> = batch.iter().map(|o| o.env.clone()).collect();
        let res = call(handle, conn, |c| c.push(&envs))?;
        let mut acked: HashMap<String, i64> =
            res.acked.into_iter().map(|a| (a.id, a.version)).collect();
        if !res.rejected.is_empty() {
            eprintln!(
                "[sync] server refused {} record(s): {}",
                res.rejected.len(),
                res.rejected[0].reason
            );
            // They won't succeed unchanged; don't retry them forever.
            for r in res.rejected {
                if let Some(o) = batch.iter().find(|o| o.env.id == r.id) {
                    acked.insert(r.id, o.env.version);
                }
            }
        }
        let tx = conn.transaction()?;
        mark_synced(&tx, &batch, &acked)?;
        tx.commit()?;
        if acked.is_empty() {
            break;
        }
    }

    // Pull until the server says there's no more.
    let first = setting(conn, "sync_cursor").is_none();
    let mut cursor: i64 = setting(conn, "sync_cursor")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let mut unreadable = 0;
    for _ in 0..10_000 {
        let since = cursor;
        let res = call(handle, conn, |c| {
            c.pull(since, BATCH, first.then_some(FIRST_SYNC_DAYS))
        })?;
        let tx = conn.transaction()?;
        let (_, bad) = apply(&tx, &key, &res.records)?;
        unreadable += bad;
        db::set_setting(&tx, "sync_cursor", &res.cursor.to_string())?;
        tx.commit()?;
        cursor = res.cursor;
        if !res.more {
            break;
        }
    }
    db::set_setting(conn, "sync_last", &db::fmt_time(now()))?;
    if unreadable > 0 {
        handle.set(|s| {
            s.last_error = Some(format!(
                "{unreadable} record(s) couldn't be decrypted. Another device may be using a different account key."
            ))
        });
    }
    Ok(())
}

pub fn spawn(handle: Arc<SyncHandle>, db_path: PathBuf, shared: Arc<crate::tracker::Shared>) {
    thread::Builder::new()
        .name("focustrack-sync".into())
        .spawn(move || {
            let mut conn = match db::open(&db_path) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("[sync] cannot open database: {e}");
                    return;
                }
            };
            let mut last = Instant::now() - SYNC_EVERY;
            let mut backoff = Duration::ZERO;
            loop {
                if shared.shutdown.load(Ordering::SeqCst) {
                    break;
                }
                let woken = handle.wake.swap(false, Ordering::SeqCst);
                let due = last.elapsed() >= SYNC_EVERY.max(backoff);
                let enabled = setting(&conn, "sync_enabled").as_deref() == Some("1");
                if enabled && handle.signed_in() && (woken || due) {
                    handle.set(|s| s.state = "syncing".into());
                    let result = sync_once(&handle, &mut conn);
                    last = Instant::now();
                    // A remote control may have paused or resumed tracking here.
                    let paused = setting(&conn, "paused").as_deref() == Some("1");
                    if shared.paused.swap(paused, Ordering::SeqCst) != paused {
                        eprintln!(
                            "[sync] tracking {} by another device",
                            if paused { "paused" } else { "resumed" }
                        );
                    }
                    let _ = db::set_setting(
                        &conn,
                        "sync_error",
                        &match &result {
                            Ok(()) | Err(SyncError::Revoked) => String::new(),
                            Err(e) => e.to_string().chars().take(160).collect(),
                        },
                    );
                    match result {
                        Ok(()) => {
                            backoff = Duration::ZERO;
                            handle.set(|s| {
                                s.state = "idle".into();
                                if s.last_error
                                    .as_deref()
                                    .is_some_and(|e| !e.contains("decrypted"))
                                {
                                    s.last_error = None;
                                }
                            });
                        }
                        Err(SyncError::Revoked) => {
                            handle.set(|s| {
                                s.state = "signed_out".into();
                                s.last_error = Some(SyncError::Revoked.to_string());
                            });
                            let _ = db::set_setting(&conn, "sync_enabled", "0");
                        }
                        Err(e) => {
                            // Offline or server trouble: try again later, more slowly.
                            backoff = (backoff * 2).clamp(SYNC_EVERY, Duration::from_secs(15 * 60));
                            handle.set(|s| {
                                s.state = "error".into();
                                s.last_error = Some(e.to_string());
                            });
                        }
                    }
                }
                refresh_status(&handle, &conn);
                thread::sleep(Duration::from_secs(1));
            }
        })
        .expect("failed to spawn sync thread");
}

// ----------------------------------------------------------- sign-in / out

fn new_device(conn: &Connection) -> rusqlite::Result<NewDevice> {
    Ok(NewDevice {
        id: device_id(conn)?,
        platform: PLATFORM.into(),
        app_version: env!("CARGO_PKG_VERSION").into(),
    })
}

/// After signing in: everything this device recorded recently, and every
/// shared setting, uploads again (the server ignores what it already has).
fn reset_for_account(conn: &Connection) -> rusqlite::Result<()> {
    for t in db::OWNED_TABLES {
        conn.execute(
            &format!(
                "UPDATE {t} SET synced_version = 0 WHERE device_id = {} AND day >= date('now', 'localtime', '-35 days')",
                db::OWN_DEVICE
            ),
            [],
        )?;
    }
    for t in db::SHARED_TABLES {
        conn.execute(&format!("UPDATE {t} SET synced_version = 0"), [])?;
    }
    conn.execute("DELETE FROM sync_tombstones", [])?;
    for k in ["sync_cursor", "device_record_synced", "sync_last"] {
        db::set_setting(conn, k, "")?;
    }
    Ok(())
}

fn finish_sign_in(
    handle: &SyncHandle,
    conn: &Connection,
    server: &str,
    email: &str,
    name: &str,
    refresh_token: String,
    key: AccountKey,
) -> Result<(), String> {
    vault::save(
        &handle.data_dir,
        &Secrets {
            refresh_token: refresh_token.clone(),
            account_key: key.clone(),
        },
    )
    .map_err(|e| format!("Couldn't store the sign-in securely: {e}"))?;
    if let Ok(mut s) = handle.secrets.lock() {
        *s = Some(Secrets {
            refresh_token,
            account_key: key,
        });
    }
    let e = |e: rusqlite::Error| e.to_string();
    reset_for_account(conn).map_err(e)?;
    db::set_setting(conn, "sync_server", server).map_err(e)?;
    db::set_setting(conn, "sync_email", email).map_err(e)?;
    db::set_setting(conn, "device_name", name.trim()).map_err(e)?;
    db::set_setting(conn, "sync_enabled", "1").map_err(e)?;
    handle.set(|s| {
        s.state = "idle".into();
        s.last_error = None;
    });
    handle.wake();
    Ok(())
}

/// Whether an `http://` address stays on this computer or the home network
/// (127.x, localhost, 10.x, 172.16–31.x, 192.168.x), where a self-hosted test
/// server can run without a certificate. Everything else needs https.
fn local_http(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("http://") else {
        return false;
    };
    let host = rest.split(['/', ':']).next().unwrap_or("");
    if host == "localhost" {
        return true;
    }
    match host.parse::<std::net::Ipv4Addr>() {
        Ok(ip) => ip.is_loopback() || ip.is_private(),
        Err(_) => false,
    }
}

fn check_server(server: &str) -> Result<String, String> {
    let s = server.trim().trim_end_matches('/').to_string();
    if !(s.starts_with("https://") || local_http(&s)) {
        return Err(
            "The server address must start with https:// (http is only allowed for this computer              or your home network, e.g. http://192.168.1.20:8787)"
                .into(),
        );
    }
    Ok(s)
}

fn friendly(e: ClientError) -> String {
    match e.status() {
        Some(401) => "Wrong email or password".into(),
        _ => e.to_string(),
    }
}

/// Creates an account. Returns the recovery key to show the user once.
pub fn sign_up(
    handle: &SyncHandle,
    conn: &Connection,
    server: &str,
    email: &str,
    password: &str,
    name: &str,
    kdf: KdfParams,
) -> Result<String, String> {
    if password.chars().count() < 10 {
        return Err("Use a password of at least 10 characters".into());
    }
    let server = check_server(server)?;
    let device = new_device(conn).map_err(|e| e.to_string())?;
    let mut c = Client::new(&server);
    let (session, key) = client::signup(&mut c, email, password, kdf, device).map_err(friendly)?;
    if let Ok(mut a) = handle.access.lock() {
        *a = Some(session.access_token);
    }
    let rk = recovery::format(&key);
    finish_sign_in(
        handle,
        conn,
        &server,
        email,
        name,
        session.refresh_token,
        key,
    )?;
    Ok(rk)
}

/// Signs this device in to an existing account.
pub fn sign_in(
    handle: &SyncHandle,
    conn: &Connection,
    server: &str,
    email: &str,
    password: &str,
    name: &str,
) -> Result<(), String> {
    let server = check_server(server)?;
    let device = new_device(conn).map_err(|e| e.to_string())?;
    let mut c = Client::new(&server);
    let (session, key) = client::signin(&mut c, email, password, device).map_err(friendly)?;
    if let Ok(mut a) = handle.access.lock() {
        *a = Some(session.access_token);
    }
    finish_sign_in(
        handle,
        conn,
        &server,
        email,
        name,
        session.refresh_token,
        key,
    )
}

/// Signs out: forgets the account on this device and removes other devices'
/// data from it. This device's own tracking and history stay.
pub fn sign_out(handle: &SyncHandle, conn: &Connection) -> Result<(), String> {
    let _running = handle.running.lock().unwrap_or_else(|e| e.into_inner());
    // Best effort: tell the server this device is gone.
    let me = device_id(conn).unwrap_or_default();
    let _ = call(handle, conn, |c| c.remove_device(&me));
    vault::clear(&handle.data_dir);
    if let Ok(mut s) = handle.secrets.lock() {
        *s = None;
    }
    if let Ok(mut a) = handle.access.lock() {
        *a = None;
    }
    let e = |e: rusqlite::Error| e.to_string();
    db::set_setting(conn, "sync_applying", "1").map_err(e)?;
    for t in db::OWNED_TABLES {
        conn.execute(
            &format!("DELETE FROM {t} WHERE device_id IS NOT {}", db::OWN_DEVICE),
            [],
        )
        .map_err(e)?;
    }
    conn.execute(
        &format!("DELETE FROM devices WHERE id IS NOT {}", db::OWN_DEVICE),
        [],
    )
    .map_err(e)?;
    db::set_setting(conn, "sync_applying", "0").map_err(e)?;
    for k in [
        "sync_enabled",
        "sync_email",
        "sync_cursor",
        "sync_last",
        "device_record_synced",
    ] {
        db::set_setting(conn, k, "").map_err(e)?;
    }
    handle.set(|s| {
        s.state = "off".into();
        s.last_error = None;
    });
    refresh_status(handle, conn);
    Ok(())
}

/// What this device publishes about itself; the device record is re-sent when it changes.
fn device_signature(conn: &Connection) -> String {
    format!(
        "{}|{}|{}",
        device_name(conn),
        setting(conn, "paused").as_deref() == Some("1"),
        setting(conn, "sync_error").unwrap_or_default()
    )
}

/// A remote control from another device (or this one). Applies it here when
/// it targets this device: pause tracking, or turn sync off (#66).
fn apply_control(conn: &Connection, env: &Envelope, b: &Value) -> rusqlite::Result<bool> {
    let target = str_of(b, "target").to_string();
    if target.is_empty() {
        return Ok(false);
    }
    let local: Option<String> = conn
        .query_row(
            "SELECT hlc FROM device_controls WHERE target = ?1",
            params![target],
            |r| r.get(0),
        )
        .optional()?;
    if local
        .as_deref()
        .is_some_and(|h| !hlc::newer(env.hlc.as_deref().unwrap_or(""), h))
    {
        return Ok(false);
    }
    let (pause, sync) = (
        bool_of(b, "pauseTracking"),
        b.get("syncEnabled")
            .and_then(Value::as_bool)
            .unwrap_or(true),
    );
    conn.execute(
        "INSERT INTO device_controls (target, uuid, pause_tracking, sync_enabled, hlc, version, synced_version)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
         ON CONFLICT(target) DO UPDATE SET uuid = excluded.uuid, pause_tracking = excluded.pause_tracking,
             sync_enabled = excluded.sync_enabled, hlc = excluded.hlc, version = excluded.version,
             synced_version = excluded.synced_version",
        params![target, env.id, pause, sync, env.hlc, env.version],
    )?;
    if target == device_id(conn)? {
        db::set_setting(conn, "paused", if pause { "1" } else { "0" })?;
        if !sync {
            db::set_setting(conn, "sync_enabled", "0")?;
        }
    }
    Ok(true)
}

/// Pauses tracking or turns sync off on a device (this one or another). Another
/// device applies it at its next sync. Sync turned off remotely can only be
/// turned back on from that device.
pub fn set_device_control(
    handle: &SyncHandle,
    conn: &Connection,
    target: &str,
    pause_tracking: Option<bool>,
    sync_enabled: Option<bool>,
) -> Result<(), String> {
    let e = |e: rusqlite::Error| e.to_string();
    let me = device_id(conn).map_err(e)?;
    let current: Option<(i64, i64, i64)> = conn
        .query_row(
            "SELECT pause_tracking, sync_enabled, version FROM device_controls WHERE target = ?1",
            params![target],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .map_err(e)?;
    let (pause0, sync0, version) = current.unwrap_or((0, 1, 0));
    let pause = pause_tracking.unwrap_or(pause0 != 0);
    let sync = sync_enabled.unwrap_or(sync0 != 0);
    let hlc = format!("{}:0:{me}", chrono::Utc::now().timestamp_millis());
    conn.execute(
        "INSERT INTO device_controls (target, uuid, pause_tracking, sync_enabled, hlc, version, synced_version)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0)
         ON CONFLICT(target) DO UPDATE SET pause_tracking = excluded.pause_tracking,
             sync_enabled = excluded.sync_enabled, hlc = excluded.hlc, version = excluded.version",
        params![
            target,
            focustrack_sync_proto::name_id("device_control", target),
            pause,
            sync,
            hlc,
            version + 1
        ],
    )
    .map_err(e)?;
    if target == me {
        db::set_setting(conn, "paused", if pause { "1" } else { "0" }).map_err(e)?;
    }
    handle.wake();
    Ok(())
}

/// Devices on the account, with names and status from their device records.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceRow {
    pub id: String,
    pub name: String,
    pub platform: String,
    pub app_version: String,
    /// When the device last talked to the server (≈ last sync).
    pub last_seen: Option<String>,
    /// Synced in the last few minutes.
    pub online: bool,
    /// Its newest recorded usage that this device has.
    pub last_activity: Option<String>,
    /// Tracking is paused on it.
    pub paused: bool,
    /// The last sync problem it reported.
    pub sync_error: Option<String>,
    /// Sync was turned off for it from another device.
    pub sync_disabled: bool,
    pub current: bool,
}

pub fn devices(handle: &SyncHandle, conn: &Connection) -> Result<Vec<DeviceRow>, String> {
    let e = |e: rusqlite::Error| e.to_string();
    let list: Vec<DeviceInfo> = call(handle, conn, |c| c.devices()).map_err(|e| e.to_string())?;
    type Known = (String, bool, Option<String>);
    let known: HashMap<String, Known> = conn
        .prepare("SELECT id, name, paused, sync_error FROM devices")
        .and_then(|mut s| {
            s.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    (r.get(1)?, r.get::<_, i64>(2)? != 0, r.get(3)?),
                ))
            })?
            .collect::<Result<HashMap<_, _>, _>>()
        })
        .map_err(e)?;
    let disabled: HashMap<String, bool> = conn
        .prepare("SELECT target, sync_enabled FROM device_controls")
        .and_then(|mut s| {
            s.query_map([], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? == 0))
            })?
            .collect::<Result<HashMap<_, _>, _>>()
        })
        .map_err(e)?;
    let last_activity = |id: &str| -> Option<String> {
        conn.query_row(
            "SELECT MAX(end_time) FROM usage_sessions WHERE device_id = ?1",
            params![id],
            |r| r.get(0),
        )
        .ok()
        .flatten()
    };
    let now = chrono::Utc::now();
    Ok(list
        .into_iter()
        .map(|d| {
            let k = known.get(&d.id);
            let online = d.current
                || d.last_seen
                    .as_deref()
                    .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
                    .is_some_and(|t| (now - t.with_timezone(&chrono::Utc)).num_minutes() < 3);
            DeviceRow {
                name: if d.current {
                    device_name(conn)
                } else {
                    k.map(|k| k.0.clone())
                        .filter(|n| !n.is_empty())
                        .unwrap_or_else(|| "Unnamed device".into())
                },
                online,
                last_activity: last_activity(&d.id),
                paused: if d.current {
                    setting(conn, "paused").as_deref() == Some("1")
                } else {
                    k.is_some_and(|k| k.1)
                },
                sync_error: if d.current {
                    setting(conn, "sync_error")
                } else {
                    k.and_then(|k| k.2.clone())
                },
                sync_disabled: !d.current && disabled.get(&d.id).copied().unwrap_or(false),
                id: d.id,
                platform: d.platform,
                app_version: d.app_version,
                last_seen: d.last_seen,
                current: d.current,
            }
        })
        .collect())
}

pub fn remove_device(handle: &SyncHandle, conn: &Connection, id: &str) -> Result<(), String> {
    call(handle, conn, |c| c.remove_device(id)).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn set_enabled(handle: &SyncHandle, conn: &Connection, on: bool) -> Result<(), String> {
    db::set_setting(conn, "sync_enabled", if on { "1" } else { "0" }).map_err(|e| e.to_string())?;
    if on {
        handle.wake();
    }
    refresh_status(handle, conn);
    Ok(())
}

// ------------------------------------------------------- cloud data (#67)

/// Deletes usage history (all devices' rows, or one device's) recorded before
/// `until` (`YYYY-MM-DD HH:MM:SS`) from this device's database.
fn purge_local(conn: &Connection, until: &str, device: Option<&str>) -> rusqlite::Result<usize> {
    if db::parse_time(until).is_none() {
        return Ok(0);
    }
    let mut n = 0;
    for t in db::OWNED_TABLES {
        n += conn.execute(
            &format!("DELETE FROM {t} WHERE start_time < ?1 AND (?2 IS NULL OR device_id = ?2)"),
            params![until, device],
        )?;
    }
    Ok(n)
}

fn account_key(handle: &SyncHandle) -> Result<AccountKey, String> {
    handle
        .secrets
        .lock()
        .ok()
        .and_then(|s| s.as_ref().map(|s| s.account_key.clone()))
        .ok_or_else(|| "Sign in first".to_string())
}

/// Deletes synced history from the server and from every device: everything
/// (`before` = None) or what was recorded before a day, for all devices or
/// one. Returns how many records the server removed.
pub fn delete_history(
    handle: &SyncHandle,
    conn: &Connection,
    before: Option<&str>,
    device: Option<&str>,
) -> Result<u64, String> {
    let key = account_key(handle)?;
    let before_day = match before {
        Some(d) => Some(chrono::NaiveDate::parse_from_str(d, db::DAY_FMT).map_err(|_| "Bad date")?),
        None => None,
    };
    let now = now();
    let until = before_day
        .map(|d| db::fmt_time(d.and_time(chrono::NaiveTime::MIN)))
        .unwrap_or_else(|| db::fmt_time(now));
    // The server works in whole days: "everything" means up to and including today.
    let server_before = before_day
        .unwrap_or(now.date() + chrono::Duration::days(1))
        .format(db::DAY_FMT)
        .to_string();
    let me = device_id(conn).map_err(|e| e.to_string())?;
    let hlc = format!("{}:0:{me}", chrono::Utc::now().timestamp_millis());
    let purge = Envelope::seal(
        &key,
        &focustrack_sync_proto::new_id(),
        kind::PURGE,
        &me,
        &today(),
        1,
        Some(hlc),
        false,
        &json!({ "until": until, "device": device }),
    );
    // Tell the other devices first, then clear the server, then this device.
    call(handle, conn, |c| c.push(std::slice::from_ref(&purge))).map_err(|e| e.to_string())?;
    let deleted = call(handle, conn, |c| c.delete_data(&server_before, device))
        .map_err(|e| e.to_string())?
        .deleted;
    db::set_setting(conn, "sync_applying", "1").map_err(|e| e.to_string())?;
    let local = purge_local(conn, &until, device);
    db::set_setting(conn, "sync_applying", "0").map_err(|e| e.to_string())?;
    local.map_err(|e| e.to_string())?;
    Ok(deleted)
}

/// Deletes the account and all its cloud data, then signs this device out
/// (its own local history stays). Needs the password again.
pub fn delete_account(
    handle: &SyncHandle,
    conn: &Connection,
    password: &str,
) -> Result<(), String> {
    let email = setting(conn, "sync_email").ok_or("Sign in first")?;
    // Make sure we hold a fresh access token, so a 401 below means "wrong password".
    call(handle, conn, |c| c.account()).map_err(|e| e.to_string())?;
    let mut c = client_for(conn).map_err(|e| e.to_string())?;
    c.set_access_token(handle.access.lock().ok().and_then(|a| a.clone()));
    let params = c.auth_params(&email).map_err(|e| e.to_string())?;
    let master =
        focustrack_sync_proto::crypto::MasterKey::derive(password, &params.salt, &params.kdf)
            .map_err(|e| e.to_string())?;
    match c.delete_account(&master.auth_key()) {
        Ok(_) => {}
        Err(e) if e.status() == Some(401) => return Err("Wrong password".into()),
        Err(e) => return Err(e.to_string()),
    }
    sign_out(handle, conn)
}

/// Days of history the server keeps (None = forever).
pub fn retention(handle: &SyncHandle, conn: &Connection) -> Result<Option<u32>, String> {
    call(handle, conn, |c| c.account())
        .map(|a| a.retention_days)
        .map_err(|e| e.to_string())
}

pub fn set_retention(
    handle: &SyncHandle,
    conn: &Connection,
    days: Option<u32>,
) -> Result<Option<u32>, String> {
    call(handle, conn, |c| c.set_retention(days))
        .map(|a| a.retention_days)
        .map_err(|e| e.to_string())
}

/// Downloads everything stored in the account, decrypts it and writes it as
/// JSON to `path`. Returns how many records were written.
pub fn export(handle: &SyncHandle, conn: &Connection, path: &Path) -> Result<usize, String> {
    let key = account_key(handle)?;
    let mut since = 0;
    let mut records = Vec::new();
    loop {
        let page = call(handle, conn, |c| c.pull_all(since, BATCH)).map_err(|e| e.to_string())?;
        for env in page.records {
            let body: Value = env.open(&key).unwrap_or(Value::Null);
            records.push(json!({
                "id": env.id, "kind": env.kind, "device": env.device, "day": env.day,
                "version": env.version, "deleted": env.deleted, "data": body,
            }));
        }
        since = page.cursor;
        if !page.more {
            break;
        }
    }
    let doc = json!({
        "app": "FocusTrack",
        "exported": db::fmt_time(now()),
        "account": setting(conn, "sync_email"),
        "records": records,
    });
    let text = serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| format!("Couldn't write {}: {e}", path.display()))?;
    Ok(records.len())
}

pub fn rename_device(handle: &SyncHandle, conn: &Connection, name: &str) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 40 {
        return Err("Give this device a name of up to 40 characters".into());
    }
    db::set_setting(conn, "device_name", name).map_err(|e| e.to_string())?;
    handle.wake();
    refresh_status(handle, conn);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytics as an;

    /// The real sync server on a random port, with an in-memory database.
    fn server() -> String {
        let (tx, rx) = std::sync::mpsc::channel();
        thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async move {
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                tx.send(listener.local_addr().unwrap()).unwrap();
                let conn = focustrack_server::store::open(":memory:").unwrap();
                let app =
                    focustrack_server::app(conn, focustrack_server::Config { trust_proxy: false });
                focustrack_server::serve(listener, app, std::future::pending())
                    .await
                    .unwrap();
            });
        });
        format!("http://{}", rx.recv().unwrap())
    }

    struct TempDir(PathBuf);
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// One simulated device: its own database and secrets folder.
    struct Device {
        conn: Connection,
        handle: Arc<SyncHandle>,
        _dir: TempDir,
    }

    fn device() -> Device {
        let dir = std::env::temp_dir().join(format!("ft-sync-{}", focustrack_sync_proto::new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        Device {
            conn: db::open_in_memory().unwrap(),
            handle: SyncHandle::new(dir.clone()),
            _dir: TempDir(dir),
        }
    }

    fn at((h, m): (u32, u32)) -> NaiveDateTime {
        Local::now().date_naive().and_hms_opt(h, m, 0).unwrap()
    }

    impl Device {
        fn sync(&mut self) {
            sync_once(&self.handle, &mut self.conn).unwrap();
        }
        fn id(&self) -> String {
            device_id(&self.conn).unwrap()
        }
        fn use_app(&self, exe: &str, from: (u32, u32), to: (u32, u32)) -> i64 {
            let (app, _) =
                db::get_or_create_app(&self.conn, exe, "", || exe.into(), at(from)).unwrap();
            let id = db::insert_usage(&self.conn, app, at(from)).unwrap();
            db::update_span(&self.conn, db::SpanKind::Usage, id, at(from), at(to)).unwrap();
            id
        }
        fn count(&self, sql: &str) -> i64 {
            self.conn.query_row(sql, [], |r| r.get(0)).unwrap()
        }
    }

    fn pair() -> (Device, Device) {
        let url = server();
        let a = device();
        sign_up(
            &a.handle,
            &a.conn,
            &url,
            "sam@example.com",
            "a long password",
            "Laptop",
            KdfParams::insecure_for_tests(),
        )
        .unwrap();
        let b = device();
        sign_in(
            &b.handle,
            &b.conn,
            &url,
            "sam@example.com",
            "a long password",
            "Desktop",
        )
        .unwrap();
        (a, b)
    }

    #[test]
    fn usage_from_one_device_shows_up_combined_on_the_other() {
        let (mut a, mut b) = pair();
        a.use_app("code.exe", (9, 0), (10, 0)); // 1 h on the laptop
        b.use_app("steam.exe", (9, 0), (9, 30)); // 30 min on the desktop
        let site = db::get_or_create_website(&a.conn, "github.com", at((9, 0))).unwrap();
        let w = db::insert_website_session(&a.conn, site, "chrome", at((9, 0))).unwrap();
        db::update_span(&a.conn, db::SpanKind::Website, w, at((9, 0)), at((9, 20))).unwrap();

        a.sync();
        b.sync();
        a.sync();

        let today = an::fmt_day(Local::now().date_naive());
        for (name, d) in [("laptop", &a), ("desktop", &b)] {
            let (active, _) = an::totals(&d.conn, &today, &today).unwrap();
            assert_eq!(active, 90 * 60, "{name} shows combined screen time");
        }
        // Other devices' rows keep their device tag and aren't uploaded again.
        let from_a = b.count(&format!(
            "SELECT COUNT(*) FROM usage_sessions WHERE device_id = '{}'",
            a.id()
        ));
        assert_eq!(from_a, 1);
        assert_eq!(pending_count(&a.conn).unwrap(), 0);
        assert_eq!(pending_count(&b.conn).unwrap(), 0);
        assert_eq!(b.count("SELECT COUNT(*) FROM website_sessions"), 1);
        // Device names arrive with the device records.
        let name: String = b
            .conn
            .query_row(
                "SELECT name FROM devices WHERE id = ?1",
                params![a.id()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(name, "Laptop");
    }

    #[test]
    fn a_growing_session_updates_in_place_without_duplicates() {
        let (mut a, mut b) = pair();
        let id = a.use_app("code.exe", (9, 0), (9, 10));
        a.sync();
        b.sync();
        db::update_span(&a.conn, db::SpanKind::Usage, id, at((9, 0)), at((9, 45))).unwrap();
        a.sync();
        b.sync();
        assert_eq!(b.count("SELECT COUNT(*) FROM usage_sessions"), 1);
        assert_eq!(b.count("SELECT duration_secs FROM usage_sessions"), 45 * 60);
    }

    #[test]
    fn limits_stay_per_device_but_their_settings_sync() {
        let (mut a, mut b) = pair();
        // A sets a 1-hour Steam limit; A plays Steam for 20 min, B for 50.
        let (steam, _) =
            db::get_or_create_app(&a.conn, "steam.exe", "", || "Steam".into(), at((8, 0))).unwrap();
        a.conn
            .execute(
                "INSERT INTO usage_limits (target_type, target_id, daily_limit_secs, action) VALUES ('app', ?1, 3600, 'block')",
                params![steam],
            )
            .unwrap();
        a.use_app("steam.exe", (9, 0), (9, 20));
        b.use_app("steam.exe", (10, 0), (10, 50));
        a.sync();
        b.sync();
        a.sync();

        // The limit arrived on B; each device counts only its own usage (shared budgets are #70).
        let lb = crate::limits::current_status(&b.conn, at((11, 0))).unwrap();
        assert_eq!((lb.len(), lb[0].used_secs), (1, 50 * 60));
        let la = crate::limits::current_status(&a.conn, at((11, 0))).unwrap();
        assert_eq!(la[0].used_secs, 20 * 60);

        // B edits it; A gets the edit. Then A deletes it; B loses it.
        thread::sleep(Duration::from_millis(5));
        b.conn
            .execute("UPDATE usage_limits SET daily_limit_secs = 1800", [])
            .unwrap();
        b.sync();
        a.sync();
        assert_eq!(a.count("SELECT daily_limit_secs FROM usage_limits"), 1800);
        thread::sleep(Duration::from_millis(5));
        a.conn.execute("DELETE FROM usage_limits", []).unwrap();
        a.sync();
        b.sync();
        assert_eq!(b.count("SELECT COUNT(*) FROM usage_limits"), 0);
    }

    #[test]
    fn goals_and_their_ticks_sync_both_ways() {
        let (mut a, mut b) = pair();
        a.conn
            .execute(
                "INSERT INTO goals (kind, title) VALUES ('custom', 'Read 20 pages')",
                [],
            )
            .unwrap();
        a.sync();
        b.sync();
        let today = an::fmt_day(Local::now().date_naive());
        let goal_b = b.count("SELECT id FROM goals WHERE title = 'Read 20 pages'");
        assert!(
            crate::goals::toggle_check(&b.conn, goal_b, &today).unwrap(),
            "ticked on B"
        );
        b.sync();
        a.sync();
        assert_eq!(
            a.count("SELECT COUNT(*) FROM goal_checks"),
            1,
            "tick reached A"
        );
        let goal_a = a.count("SELECT id FROM goals");
        thread::sleep(Duration::from_millis(5));
        assert!(
            !crate::goals::toggle_check(&a.conn, goal_a, &today).unwrap(),
            "unticked on A"
        );
        a.sync();
        b.sync();
        assert_eq!(
            b.count("SELECT COUNT(*) FROM goal_checks"),
            0,
            "untick reached B"
        );
    }

    #[test]
    fn offline_changes_wait_and_upload_later() {
        let (mut a, mut b) = pair();
        let server = setting(&a.conn, "sync_server").unwrap();
        db::set_setting(&a.conn, "sync_server", "http://127.0.0.1:9").unwrap(); // nothing listens there
        a.use_app("code.exe", (9, 0), (9, 30));
        let err = sync_once(&a.handle, &mut a.conn).err().unwrap();
        assert!(
            matches!(&err, SyncError::Client(e) if e.is_transient()),
            "{err}"
        );
        assert!(
            pending_count(&a.conn).unwrap() > 0,
            "nothing marked synced while offline"
        );

        db::set_setting(&a.conn, "sync_server", &server).unwrap();
        a.sync();
        b.sync();
        assert_eq!(b.count("SELECT COUNT(*) FROM usage_sessions"), 1);
    }

    #[test]
    fn a_removed_device_is_told_and_signing_out_keeps_own_history() {
        let (mut a, mut b) = pair();
        a.use_app("code.exe", (9, 0), (9, 30));
        b.use_app("steam.exe", (9, 0), (9, 10));
        a.sync();
        b.sync();
        a.sync(); // A learns B's name
        assert_eq!(b.count("SELECT COUNT(*) FROM usage_sessions"), 2);

        let rows = devices(&a.handle, &a.conn).unwrap();
        assert_eq!(rows.len(), 2);
        assert!(
            rows.iter().any(|d| d.name == "Desktop" && !d.current),
            "{rows:?}"
        );
        remove_device(&a.handle, &a.conn, &b.id()).unwrap();
        assert!(matches!(
            sync_once(&b.handle, &mut b.conn),
            Err(SyncError::Revoked)
        ));

        sign_out(&b.handle, &b.conn).unwrap();
        assert_eq!(
            b.count("SELECT COUNT(*) FROM usage_sessions"),
            1,
            "only B's own usage is left"
        );
        assert!(
            vault::load(&b.handle.data_dir).is_none(),
            "secrets are gone"
        );
    }

    /// Signs a development profile in, for running two app copies as two
    /// devices on one PC (design §11). Uses the same code as Settings.
    /// FT_PROFILE_DIR, FT_SERVER, FT_EMAIL, FT_PASSWORD, FT_DEVICE_NAME, FT_SIGNUP=1 to create the account.
    #[test]
    #[ignore]
    fn provision_profile() {
        let env = |k: &str| std::env::var(k).unwrap();
        let dir = PathBuf::from(env("FT_PROFILE_DIR"));
        std::fs::create_dir_all(&dir).unwrap();
        let conn = db::open(&dir.join("focustrack.db")).unwrap();
        let handle = SyncHandle::new(dir);
        if std::env::var("FT_SIGNUP").as_deref() == Ok("1") {
            let rk = sign_up(
                &handle,
                &conn,
                &env("FT_SERVER"),
                &env("FT_EMAIL"),
                &env("FT_PASSWORD"),
                &env("FT_DEVICE_NAME"),
                KdfParams::standard(),
            )
            .unwrap();
            println!("signed up; recovery key {}…", &rk[..8]);
        } else {
            sign_in(
                &handle,
                &conn,
                &env("FT_SERVER"),
                &env("FT_EMAIL"),
                &env("FT_PASSWORD"),
                &env("FT_DEVICE_NAME"),
            )
            .unwrap();
            println!("signed in");
        }
        println!("device {}", device_id(&conn).unwrap());
    }

    #[test]
    fn plain_http_only_for_this_computer_and_the_home_network() {
        for ok in [
            "https://sync.example.com",
            "http://127.0.0.1:8787",
            "http://localhost:8787/",
            "http://192.168.1.20:8787",
            "http://10.0.0.5:8787",
            "http://172.20.1.2",
        ] {
            assert!(check_server(ok).is_ok(), "{ok}");
        }
        for bad in [
            "http://sync.example.com",
            "http://8.8.8.8:8787",
            "http://172.32.0.1",
            "http://192.168.1.20.evil.com",
            "ftp://192.168.1.2",
        ] {
            assert!(check_server(bad).is_err(), "{bad}");
        }
    }

    fn usage_count(d: &Device) -> i64 {
        d.count("SELECT COUNT(*) FROM usage_sessions")
    }

    #[test]
    fn deleting_all_history_reaches_every_device() {
        let (mut a, mut b) = pair();
        a.use_app("code.exe", (9, 0), (9, 30));
        b.use_app("steam.exe", (9, 0), (9, 10));
        a.sync();
        b.sync();
        a.sync();
        assert_eq!((usage_count(&a), usage_count(&b)), (2, 2));

        let removed = delete_history(&a.handle, &a.conn, None, None).unwrap();
        assert_eq!(removed, 2, "both devices' records left the server");
        assert_eq!(usage_count(&a), 0);
        b.sync();
        assert_eq!(usage_count(&b), 0, "B applied the purge");
        // A new device gets nothing back.
        let c = device();
        let url = setting(&a.conn, "sync_server").unwrap();
        sign_in(
            &c.handle,
            &c.conn,
            &url,
            "sam@example.com",
            "a long password",
            "New",
        )
        .unwrap();
        let mut c = c;
        c.sync();
        assert_eq!(usage_count(&c), 0);
        // Tracking carries on afterwards.
        b.use_app("steam.exe", (11, 0), (11, 5));
        b.sync();
        a.sync();
        assert_eq!(usage_count(&a), 1);
    }

    #[test]
    fn deleting_one_devices_history_or_older_history() {
        let (mut a, mut b) = pair();
        a.use_app("code.exe", (9, 0), (9, 30));
        b.use_app("steam.exe", (9, 0), (9, 10));
        a.sync();
        b.sync();
        a.sync();
        // Only B's history.
        delete_history(&a.handle, &a.conn, None, Some(&b.id())).unwrap();
        assert_eq!(
            a.count(&format!(
                "SELECT COUNT(*) FROM usage_sessions WHERE device_id = '{}'",
                b.id()
            )),
            0
        );
        assert_eq!(usage_count(&a), 1, "A's own history stays");
        b.sync();
        assert_eq!(usage_count(&b), 1, "B keeps only A's session");

        // History before today: nothing recorded today is touched.
        let today = Local::now().date_naive().format(db::DAY_FMT).to_string();
        assert_eq!(
            delete_history(&a.handle, &a.conn, Some(&today), None).unwrap(),
            0
        );
        assert_eq!(usage_count(&a), 1);
    }

    #[test]
    fn deleting_the_account_needs_the_password_and_signs_everyone_out() {
        let (mut a, mut b) = pair();
        a.use_app("code.exe", (9, 0), (9, 30));
        a.sync();
        assert_eq!(
            delete_account(&a.handle, &a.conn, "wrong").err().unwrap(),
            "Wrong password"
        );
        delete_account(&a.handle, &a.conn, "a long password").unwrap();
        assert!(!a.handle.signed_in());
        assert_eq!(usage_count(&a), 1, "local history stays");
        assert!(matches!(
            sync_once(&b.handle, &mut b.conn),
            Err(SyncError::Revoked)
        ));
    }

    #[test]
    fn export_writes_everything_decrypted() {
        let (mut a, mut b) = pair();
        a.use_app("code.exe", (9, 0), (9, 30));
        b.use_app("steam.exe", (9, 0), (9, 10));
        a.sync();
        b.sync();
        let path = a.handle.data_dir.join("export.json");
        let n = export(&a.handle, &a.conn, &path).unwrap();
        let doc: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let usage: Vec<&Value> = doc["records"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["kind"] == "usage")
            .collect();
        assert_eq!(usage.len(), 2, "this device's own records too");
        assert!(usage.iter().any(|r| r["data"]["exe"] == "code.exe"));
        assert!(n >= 4, "usage plus the two device records");
    }

    #[test]
    fn retention_round_trips() {
        let (a, _b) = pair();
        assert_eq!(retention(&a.handle, &a.conn).unwrap(), None);
        assert_eq!(
            set_retention(&a.handle, &a.conn, Some(365)).unwrap(),
            Some(365)
        );
        assert_eq!(retention(&a.handle, &a.conn).unwrap(), Some(365));
        assert!(set_retention(&a.handle, &a.conn, Some(3)).is_err());
    }

    #[test]
    fn devices_report_status_and_can_be_controlled_remotely() {
        let (mut a, mut b) = pair();
        a.use_app("code.exe", (9, 0), (9, 30));
        a.sync();
        b.sync();
        let rows = devices(&b.handle, &b.conn).unwrap();
        let laptop = rows.iter().find(|d| d.name == "Laptop").unwrap();
        assert!(laptop.online && !laptop.paused && !laptop.sync_disabled);
        assert!(
            laptop
                .last_activity
                .as_deref()
                .is_some_and(|t| t.ends_with("09:30:00")),
            "{laptop:?}"
        );

        // B pauses tracking on A; A applies it at its next sync and reports it back.
        set_device_control(&b.handle, &b.conn, &a.id(), Some(true), None).unwrap();
        b.sync();
        a.sync();
        assert_eq!(
            setting(&a.conn, "paused").as_deref(),
            Some("1"),
            "A is paused now"
        );
        a.sync(); // publishes its new status
        b.sync();
        let rows = devices(&b.handle, &b.conn).unwrap();
        assert!(
            rows.iter().find(|d| d.name == "Laptop").unwrap().paused,
            "B sees A as paused"
        );

        // ...and resumes it.
        set_device_control(&b.handle, &b.conn, &a.id(), Some(false), None).unwrap();
        b.sync();
        a.sync();
        assert_eq!(setting(&a.conn, "paused").as_deref(), Some("0"));

        // B turns sync off on A: A stops syncing (only A can turn it back on).
        set_device_control(&b.handle, &b.conn, &a.id(), None, Some(false)).unwrap();
        b.sync();
        a.sync();
        assert_eq!(setting(&a.conn, "sync_enabled").as_deref(), Some("0"));
        assert!(devices(&b.handle, &b.conn)
            .unwrap()
            .iter()
            .any(|d| d.name == "Laptop" && d.sync_disabled));
    }

    #[test]
    fn secrets_are_stored_encrypted() {
        let d = device();
        let key = AccountKey::generate();
        let secrets = Secrets {
            refresh_token: "token-123".into(),
            account_key: key.clone(),
        };
        vault::save(&d.handle.data_dir, &secrets).unwrap();
        let raw = std::fs::read(d.handle.data_dir.join("sync-secrets.bin")).unwrap();
        assert!(
            !String::from_utf8_lossy(&raw).contains("token-123"),
            "not readable as plain text"
        );
        let back = vault::load(&d.handle.data_dir).unwrap();
        assert_eq!(back.refresh_token, "token-123");
        assert_eq!(back.account_key.bytes(), key.bytes());
    }
}
