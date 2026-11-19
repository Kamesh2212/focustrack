//! FocusTrack sync protocol: the record envelope, record kinds, end-to-end
//! encryption, the hybrid logical clock and the HTTP API types.
//!
//! Shared by the desktop app, the sync server and (later) the mobile apps so
//! they can never disagree about the format. See `docs/SYNC-DESIGN.md`.

pub mod api;
pub mod crypto;
pub mod hlc;
pub mod recovery;

#[cfg(feature = "client")]
pub mod client;

use serde::{Deserialize, Serialize};

/// Protocol version spoken by this crate.
pub const PROTOCOL: u32 = 1;

/// What the server stores and sees. The body is encrypted in `payload`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Envelope {
    /// UUIDv7 chosen by the creating device.
    pub id: String,
    pub kind: String,
    /// The device that created (and, for device-owned kinds, owns) the record.
    pub device: String,
    /// Local calendar day `YYYY-MM-DD` (plain text, see design §4.4).
    pub day: String,
    /// Increases on every change.
    pub version: i64,
    /// Hybrid logical clock; set for shared kinds, decides conflicts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hlc: Option<String>,
    #[serde(default)]
    pub deleted: bool,
    /// base64(nonce ‖ XChaCha20-Poly1305 ciphertext) of the JSON body.
    pub payload: String,
}

/// Record kinds (design §4.2).
pub mod kind {
    pub const DEVICE: &str = "device";
    pub const APP: &str = "app";
    pub const USAGE: &str = "usage";
    pub const IDLE: &str = "idle";
    pub const SITE: &str = "site";
    pub const FOCUS: &str = "focus";
    pub const DAY_SUMMARY: &str = "day_summary";
    pub const CATEGORY: &str = "category";
    pub const APP_CATEGORY: &str = "app_category";
    pub const SITE_CATEGORY: &str = "site_category";
    pub const LIMIT: &str = "limit";
    pub const GOAL: &str = "goal";
    pub const GOAL_CHECK: &str = "goal_check";
    pub const PROFILE: &str = "profile";
    pub const FOCUS_RUN: &str = "focus_run";
    pub const LIMIT_EXTENSION: &str = "limit_extension";
    pub const DEVICE_CONTROL: &str = "device_control";
    pub const PURGE: &str = "purge";

    pub const ALL: &[&str] = &[
        DEVICE,
        APP,
        USAGE,
        IDLE,
        SITE,
        FOCUS,
        DAY_SUMMARY,
        CATEGORY,
        APP_CATEGORY,
        SITE_CATEGORY,
        LIMIT,
        GOAL,
        GOAL_CHECK,
        PROFILE,
        FOCUS_RUN,
        LIMIT_EXTENSION,
        DEVICE_CONTROL,
        PURGE,
    ];

    /// Shared definitions: any device of the account may write them, and the
    /// newest hybrid logical clock wins. Everything else is owned by the
    /// device that created it.
    pub fn is_shared(kind: &str) -> bool {
        matches!(
            kind,
            CATEGORY
                | APP_CATEGORY
                | SITE_CATEGORY
                | LIMIT
                | GOAL
                | GOAL_CHECK
                | PROFILE
                | FOCUS_RUN
                | LIMIT_EXTENSION
                | DEVICE_CONTROL
                | PURGE
        )
    }

    pub fn is_known(kind: &str) -> bool {
        ALL.contains(&kind)
    }
}

/// A new time-ordered record or device ID.
pub fn new_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

/// A stable ID derived from a name, so every device computes the same ID for
/// the same thing (e.g. "goal X ticked on day Y").
pub fn name_id(namespace: &str, name: &str) -> String {
    use sha2::{Digest, Sha256};
    let h = Sha256::digest(format!("focustrack:{namespace}:{name}").as_bytes());
    let mut b = [0u8; 16];
    b.copy_from_slice(&h[..16]);
    b[6] = (b[6] & 0x0f) | 0x80; // version 8: custom
    b[8] = (b[8] & 0x3f) | 0x80; // RFC 4122 variant
    uuid::Uuid::from_bytes(b).to_string()
}

/// Checks the parts of an envelope a server can check without the key.
pub fn validate(env: &Envelope) -> Result<(), String> {
    uuid::Uuid::parse_str(&env.id).map_err(|_| "bad record id")?;
    uuid::Uuid::parse_str(&env.device).map_err(|_| "bad device id")?;
    if !kind::is_known(&env.kind) {
        return Err(format!("unknown kind {}", env.kind));
    }
    let d = env.day.as_bytes();
    let day_ok = d.len() == 10
        && d[4] == b'-'
        && d[7] == b'-'
        && d.iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit());
    if !day_ok {
        return Err("bad day".into());
    }
    if env.version < 1 {
        return Err("version must be positive".into());
    }
    if kind::is_shared(&env.kind) {
        let h = env.hlc.as_deref().ok_or("shared records need an hlc")?;
        hlc::Hlc::parse(h).ok_or("bad hlc")?;
    }
    if env.payload.len() > 64 * 1024 {
        return Err("payload too large".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(kind: &str) -> Envelope {
        Envelope {
            id: new_id(),
            kind: kind.into(),
            device: new_id(),
            day: "2026-11-19".into(),
            version: 1,
            hlc: None,
            deleted: false,
            payload: "AAAA".into(),
        }
    }

    #[test]
    fn validation() {
        assert!(validate(&env(kind::USAGE)).is_ok());
        assert!(validate(&env("nope")).is_err());
        assert!(validate(&Envelope {
            day: "2026-1-19".into(),
            ..env(kind::USAGE)
        })
        .is_err());
        assert!(validate(&Envelope {
            version: 0,
            ..env(kind::USAGE)
        })
        .is_err());
        assert!(
            validate(&env(kind::LIMIT)).is_err(),
            "shared kinds need an hlc"
        );
        assert!(validate(&Envelope {
            hlc: Some("1795041234567:0:dev".into()),
            ..env(kind::LIMIT)
        })
        .is_ok());
    }

    #[test]
    fn name_ids_are_stable_and_valid() {
        let a = name_id("goal_check", "g1|2026-11-19");
        assert_eq!(a, name_id("goal_check", "g1|2026-11-19"));
        assert_ne!(a, name_id("goal_check", "g1|2026-11-20"));
        assert!(uuid::Uuid::parse_str(&a).is_ok());
    }

    #[test]
    fn ids_are_time_ordered() {
        let a = new_id();
        std::thread::sleep(std::time::Duration::from_millis(2));
        assert!(new_id() > a);
    }
}
