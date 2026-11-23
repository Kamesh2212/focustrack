//! Server storage (SQLite). Everything the server knows lives here.
//!
//! The server holds no keys: records are ciphertext; it stores hashes of auth
//! keys and tokens, never the secrets themselves.

use focustrack_sync_proto::api::{Ack, DeviceInfo, Rejected};
use focustrack_sync_proto::crypto::KdfParams;
use focustrack_sync_proto::{hlc, kind, Envelope};
use rand::RngCore;
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};

pub const ACCESS_TTL_SECS: i64 = 15 * 60;
const LOCK_AFTER_FAILS: i64 = 5;

pub fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

pub fn sha256_hex(s: &str) -> String {
    Sha256::digest(s.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub fn random_token() -> String {
    use base64::Engine;
    let mut b = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut b);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b)
}

/// Constant-time string comparison (hashes of secrets).
pub fn same(a: &str, b: &str) -> bool {
    use subtle::ConstantTimeEq;
    a.len() == b.len() && bool::from(a.as_bytes().ct_eq(b.as_bytes()))
}

pub fn open(path: &str) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA foreign_keys = ON;
         CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS accounts (
             id            TEXT PRIMARY KEY,
             email         TEXT NOT NULL UNIQUE COLLATE NOCASE,
             auth_hash     TEXT NOT NULL,
             salt          TEXT NOT NULL,
             kdf           TEXT NOT NULL,
             wrapped_key   TEXT NOT NULL,
             created       INTEGER NOT NULL,
             verified      INTEGER NOT NULL DEFAULT 0,
             next_seq      INTEGER NOT NULL DEFAULT 1,
             failed_logins INTEGER NOT NULL DEFAULT 0,
             locked_until  INTEGER NOT NULL DEFAULT 0
         );
         CREATE TABLE IF NOT EXISTS devices (
             id           TEXT PRIMARY KEY,
             account      TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
             platform     TEXT NOT NULL,
             app_version  TEXT NOT NULL,
             refresh_hash TEXT NOT NULL,
             created      INTEGER NOT NULL,
             last_seen    INTEGER,
             revoked      INTEGER NOT NULL DEFAULT 0
         );
         CREATE INDEX IF NOT EXISTS idx_devices_account ON devices(account);
         CREATE TABLE IF NOT EXISTS access_tokens (
             hash    TEXT PRIMARY KEY,
             device  TEXT NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
             expires INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS records (
             account TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
             id      TEXT NOT NULL,
             kind    TEXT NOT NULL,
             device  TEXT NOT NULL,
             day     TEXT NOT NULL,
             version INTEGER NOT NULL,
             hlc     TEXT,
             deleted INTEGER NOT NULL DEFAULT 0,
             payload TEXT NOT NULL,
             seq     INTEGER NOT NULL,
             PRIMARY KEY (account, id)
         );
         CREATE INDEX IF NOT EXISTS idx_records_seq ON records(account, seq);
         CREATE INDEX IF NOT EXISTS idx_records_day ON records(account, day);",
    )?;
    // Added after the first release: how long to keep history (#67).
    let has_retention: bool = conn
        .prepare("SELECT 1 FROM pragma_table_info('accounts') WHERE name = 'retention_days'")?
        .exists([])?;
    if !has_retention {
        conn.execute_batch("ALTER TABLE accounts ADD COLUMN retention_days INTEGER;")?;
    }
    Ok(conn)
}

/// A per-server secret (for fake parameters of unknown emails).
pub fn server_secret(conn: &Connection) -> rusqlite::Result<String> {
    if let Some(s) = conn
        .query_row("SELECT value FROM meta WHERE key = 'secret'", [], |r| {
            r.get(0)
        })
        .optional()?
    {
        return Ok(s);
    }
    let s = random_token();
    conn.execute(
        "INSERT INTO meta (key, value) VALUES ('secret', ?1)",
        params![s],
    )?;
    Ok(s)
}

pub struct AccountRow {
    pub id: String,
    pub auth_hash: String,
    pub salt: String,
    pub kdf: KdfParams,
    pub wrapped_key: String,
    pub failed_logins: i64,
    pub locked_until: i64,
}

pub fn account_by_email(conn: &Connection, email: &str) -> rusqlite::Result<Option<AccountRow>> {
    conn.query_row(
        "SELECT id, auth_hash, salt, kdf, wrapped_key, failed_logins, locked_until FROM accounts WHERE email = ?1",
        params![email],
        |r| {
            Ok(AccountRow {
                id: r.get(0)?,
                auth_hash: r.get(1)?,
                salt: r.get(2)?,
                kdf: serde_json::from_str(&r.get::<_, String>(3)?).unwrap_or_else(|_| KdfParams::standard()),
                wrapped_key: r.get(4)?,
                failed_logins: r.get(5)?,
                locked_until: r.get(6)?,
            })
        },
    )
    .optional()
}

pub fn account_by_id(conn: &Connection, id: &str) -> rusqlite::Result<Option<AccountRow>> {
    let email: Option<String> = conn
        .query_row(
            "SELECT email FROM accounts WHERE id = ?1",
            params![id],
            |r| r.get(0),
        )
        .optional()?;
    match email {
        Some(e) => account_by_email(conn, &e),
        None => Ok(None),
    }
}

pub fn create_account(
    conn: &Connection,
    email: &str,
    auth_key: &str,
    salt: &str,
    kdf: &KdfParams,
    wrapped_key: &str,
) -> rusqlite::Result<Option<String>> {
    let id = focustrack_sync_proto::new_id();
    let n = conn.execute(
        "INSERT OR IGNORE INTO accounts (id, email, auth_hash, salt, kdf, wrapped_key, created)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            id,
            email,
            sha256_hex(auth_key),
            salt,
            serde_json::to_string(kdf).unwrap_or_default(),
            wrapped_key,
            now()
        ],
    )?;
    Ok((n == 1).then_some(id))
}

/// Records a failed sign-in; locks the account for a while after repeated failures.
pub fn login_failed(conn: &Connection, account: &str) -> rusqlite::Result<()> {
    let fails: i64 = conn.query_row(
        "UPDATE accounts SET failed_logins = failed_logins + 1 WHERE id = ?1 RETURNING failed_logins",
        params![account],
        |r| r.get(0),
    )?;
    if fails >= LOCK_AFTER_FAILS {
        // 1, 2, 4 … minutes, at most an hour.
        let mins = 1i64 << (fails - LOCK_AFTER_FAILS).min(6);
        conn.execute(
            "UPDATE accounts SET locked_until = ?2 WHERE id = ?1",
            params![account, now() + mins.min(60) * 60],
        )?;
    }
    Ok(())
}

pub fn login_ok(conn: &Connection, account: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE accounts SET failed_logins = 0, locked_until = 0 WHERE id = ?1",
        params![account],
    )?;
    Ok(())
}

pub enum DeviceRegistration {
    /// (refresh token, access token)
    Ok(String, String),
    /// The device ID belongs to another account.
    Taken,
}

/// Registers (or re-registers after sign-in) a device and issues tokens.
pub fn register_device(
    conn: &Connection,
    account: &str,
    id: &str,
    platform: &str,
    app_version: &str,
) -> rusqlite::Result<DeviceRegistration> {
    let owner: Option<String> = conn
        .query_row(
            "SELECT account FROM devices WHERE id = ?1",
            params![id],
            |r| r.get(0),
        )
        .optional()?;
    if owner.as_deref().is_some_and(|o| o != account) {
        return Ok(DeviceRegistration::Taken);
    }
    let refresh = random_token();
    conn.execute(
        "INSERT INTO devices (id, account, platform, app_version, refresh_hash, created, last_seen)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
         ON CONFLICT(id) DO UPDATE SET platform = excluded.platform, app_version = excluded.app_version,
             refresh_hash = excluded.refresh_hash, revoked = 0, last_seen = excluded.last_seen",
        params![id, account, platform, app_version, sha256_hex(&refresh), now()],
    )?;
    conn.execute("DELETE FROM access_tokens WHERE device = ?1", params![id])?;
    let access = issue_access(conn, id)?;
    Ok(DeviceRegistration::Ok(refresh, access))
}

pub fn issue_access(conn: &Connection, device: &str) -> rusqlite::Result<String> {
    let token = random_token();
    conn.execute(
        "DELETE FROM access_tokens WHERE expires < ?1",
        params![now()],
    )?;
    conn.execute(
        "INSERT INTO access_tokens (hash, device, expires) VALUES (?1, ?2, ?3)",
        params![sha256_hex(&token), device, now() + ACCESS_TTL_SECS],
    )?;
    Ok(token)
}

/// Refresh token → new access token, if the device is still active.
pub fn refresh(
    conn: &Connection,
    device: &str,
    refresh_token: &str,
) -> rusqlite::Result<Option<String>> {
    let row: Option<(String, i64)> = conn
        .query_row(
            "SELECT refresh_hash, revoked FROM devices WHERE id = ?1",
            params![device],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    match row {
        Some((hash, 0)) if same(&hash, &sha256_hex(refresh_token)) => {
            Ok(Some(issue_access(conn, device)?))
        }
        _ => Ok(None),
    }
}

/// The (account, device) an access token belongs to, if valid.
pub fn authenticate(conn: &Connection, token: &str) -> rusqlite::Result<Option<(String, String)>> {
    let row: Option<(String, String)> = conn
        .query_row(
            "SELECT d.account, d.id FROM access_tokens t JOIN devices d ON d.id = t.device
             WHERE t.hash = ?1 AND t.expires > ?2 AND d.revoked = 0",
            params![sha256_hex(token), now()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    if let Some((_, device)) = &row {
        conn.execute(
            "UPDATE devices SET last_seen = ?2 WHERE id = ?1 AND COALESCE(last_seen, 0) < ?2 - 60",
            params![device, now()],
        )?;
    }
    Ok(row)
}

fn fmt_ts(ts: i64) -> String {
    chrono::DateTime::from_timestamp(ts, 0)
        .map(|t| t.to_rfc3339())
        .unwrap_or_default()
}

pub fn devices(
    conn: &Connection,
    account: &str,
    current: &str,
) -> rusqlite::Result<Vec<DeviceInfo>> {
    let mut stmt = conn.prepare(
        "SELECT id, platform, app_version, created, last_seen FROM devices
         WHERE account = ?1 AND revoked = 0 ORDER BY created",
    )?;
    let rows = stmt.query_map(params![account], |r| {
        let id: String = r.get(0)?;
        Ok(DeviceInfo {
            current: id == current,
            id,
            platform: r.get(1)?,
            app_version: r.get(2)?,
            created: fmt_ts(r.get(3)?),
            last_seen: r.get::<_, Option<i64>>(4)?.map(fmt_ts),
        })
    })?;
    rows.collect()
}

/// Revokes a device: its tokens stop working immediately.
pub fn revoke_device(conn: &Connection, account: &str, device: &str) -> rusqlite::Result<bool> {
    let n = conn.execute(
        "UPDATE devices SET revoked = 1, refresh_hash = '' WHERE id = ?1 AND account = ?2 AND revoked = 0",
        params![device, account],
    )?;
    conn.execute(
        "DELETE FROM access_tokens WHERE device = ?1",
        params![device],
    )?;
    Ok(n == 1)
}

pub fn change_password(
    conn: &Connection,
    account: &str,
    new_auth_key: &str,
    salt: &str,
    kdf: &KdfParams,
    wrapped_key: &str,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE accounts SET auth_hash = ?2, salt = ?3, kdf = ?4, wrapped_key = ?5 WHERE id = ?1",
        params![
            account,
            sha256_hex(new_auth_key),
            salt,
            serde_json::to_string(kdf).unwrap_or_default(),
            wrapped_key
        ],
    )?;
    Ok(())
}

pub fn delete_account(conn: &Connection, account: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM accounts WHERE id = ?1", params![account])?;
    Ok(())
}

/// Stores a batch of records for `device` (design §6.2). Idempotent.
pub fn push(
    conn: &mut Connection,
    account: &str,
    device: &str,
    records: &[Envelope],
) -> rusqlite::Result<(Vec<Ack>, Vec<Rejected>)> {
    let tx = conn.transaction()?;
    let mut acked = Vec::new();
    let mut rejected = Vec::new();
    for env in records {
        let reject = |reason: &str| Rejected {
            id: env.id.clone(),
            reason: reason.into(),
        };
        if let Err(e) = focustrack_sync_proto::validate(env) {
            rejected.push(reject(&e));
            continue;
        }
        // The writer always stamps itself as the record's device.
        if env.device != device {
            rejected.push(reject("records must be written by their own device"));
            continue;
        }
        let shared = kind::is_shared(&env.kind);
        let existing: Option<(String, String, i64, Option<String>)> = tx
            .query_row(
                "SELECT kind, device, version, hlc FROM records WHERE account = ?1 AND id = ?2",
                params![account, env.id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?;
        if let Some((k, owner, version, hlc_old)) = &existing {
            if *k != env.kind {
                rejected.push(reject("record kind can't change"));
                continue;
            }
            let keep_existing = if shared {
                // Last writer (by hybrid logical clock) wins; an older or equal edit is a no-op.
                !hlc::newer(
                    env.hlc.as_deref().unwrap_or(""),
                    hlc_old.as_deref().unwrap_or(""),
                )
            } else {
                if owner != device {
                    rejected.push(reject("record belongs to another device"));
                    continue;
                }
                *version >= env.version
            };
            if keep_existing {
                acked.push(Ack {
                    id: env.id.clone(),
                    version: env.version,
                });
                continue;
            }
        }
        let seq: i64 = tx.query_row(
            "UPDATE accounts SET next_seq = next_seq + 1 WHERE id = ?1 RETURNING next_seq - 1",
            params![account],
            |r| r.get(0),
        )?;
        tx.execute(
            "INSERT INTO records (account, id, kind, device, day, version, hlc, deleted, payload, seq)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(account, id) DO UPDATE SET device = excluded.device, day = excluded.day,
                 version = excluded.version, hlc = excluded.hlc, deleted = excluded.deleted,
                 payload = excluded.payload, seq = excluded.seq",
            params![
                account,
                env.id,
                env.kind,
                env.device,
                env.day,
                env.version,
                env.hlc,
                env.deleted,
                env.payload,
                seq
            ],
        )?;
        acked.push(Ack {
            id: env.id.clone(),
            version: env.version,
        });
    }
    tx.commit()?;
    Ok((acked, rejected))
}

/// Records from other devices newer than `since` (design §6.3).
pub fn pull(
    conn: &Connection,
    account: &str,
    device: &str,
    since: i64,
    limit: usize,
    min_detail_day: Option<&str>,
) -> rusqlite::Result<(Vec<Envelope>, i64, bool)> {
    let mut stmt = conn.prepare(
        "SELECT id, kind, device, day, version, hlc, deleted, payload, seq FROM records
         WHERE account = ?1 AND seq > ?2 AND device != ?3
           AND (?5 IS NULL OR kind NOT IN ('usage', 'idle', 'site') OR day >= ?5)
         ORDER BY seq LIMIT ?4",
    )?;
    let mut rows = stmt.query(params![
        account,
        since,
        device,
        (limit + 1) as i64,
        min_detail_day
    ])?;
    let mut out = Vec::new();
    let mut last_seq = since;
    let mut more = false;
    while let Some(r) = rows.next()? {
        if out.len() == limit {
            more = true;
            break;
        }
        last_seq = r.get(8)?;
        out.push(Envelope {
            id: r.get(0)?,
            kind: r.get(1)?,
            device: r.get(2)?,
            day: r.get(3)?,
            version: r.get(4)?,
            hlc: r.get(5)?,
            deleted: r.get::<_, i64>(6)? != 0,
            payload: r.get(7)?,
        });
    }
    // Nothing more to read: move the cursor past this device's own records too.
    let cursor = if more {
        last_seq
    } else {
        conn.query_row(
            "SELECT next_seq - 1 FROM accounts WHERE id = ?1",
            params![account],
            |r| r.get(0),
        )?
    };
    Ok((out, cursor.max(since), more))
}

/// How many days of history the account keeps (None = forever).
pub fn retention(conn: &Connection, account: &str) -> rusqlite::Result<Option<u32>> {
    conn.query_row(
        "SELECT retention_days FROM accounts WHERE id = ?1",
        params![account],
        |r| r.get(0),
    )
}

pub fn set_retention(conn: &Connection, account: &str, days: Option<u32>) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE accounts SET retention_days = ?2 WHERE id = ?1",
        params![account, days],
    )?;
    Ok(())
}

/// Deletes history older than each account's retention period. Returns the
/// number of records removed. `today` is `YYYY-MM-DD`.
pub fn apply_retention(conn: &Connection, today: chrono::NaiveDate) -> rusqlite::Result<u64> {
    let accounts: Vec<(String, u32)> = conn
        .prepare("SELECT id, retention_days FROM accounts WHERE retention_days IS NOT NULL")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let mut n = 0;
    for (account, days) in accounts {
        let cutoff = (today - chrono::Duration::days(days as i64))
            .format("%Y-%m-%d")
            .to_string();
        n += delete_before(conn, &account, &cutoff, None)?;
    }
    Ok(n)
}

/// Deletes device-owned history before `before` (shared definitions stay).
pub fn delete_before(
    conn: &Connection,
    account: &str,
    before: &str,
    device: Option<&str>,
) -> rusqlite::Result<u64> {
    let shared: Vec<String> = kind::ALL
        .iter()
        .filter(|k| kind::is_shared(k))
        .map(|k| format!("'{k}'"))
        .collect();
    let sql = format!(
        "DELETE FROM records WHERE account = ?1 AND day < ?2 AND (?3 IS NULL OR device = ?3)
         AND kind NOT IN ({}) AND kind != 'device'",
        shared.join(",")
    );
    Ok(conn.execute(&sql, params![account, before, device])? as u64)
}
