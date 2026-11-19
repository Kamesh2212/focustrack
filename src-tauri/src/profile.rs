//! Development profiles: run more than one FocusTrack on one PC, each acting
//! as a separate device (design §11).
//!
//! `FOCUSTRACK_PROFILE=laptop2` gives this copy its own data folder
//! (`…\com.focustrack.app\profiles\laptop2`), its own local API port
//! (`FOCUSTRACK_API_PORT`, default 17866) and lets it run next to the normal
//! copy. Without the variable nothing changes.

use std::sync::OnceLock;

pub const DEFAULT_PORT: u16 = 17865;

/// The profile name, if one is set and valid (letters, digits, `-`, `_`; up to 20).
pub fn name() -> Option<&'static str> {
    static NAME: OnceLock<Option<String>> = OnceLock::new();
    NAME.get_or_init(|| {
        std::env::var("FOCUSTRACK_PROFILE").ok().filter(|n| {
            !n.is_empty()
                && n.len() <= 20
                && n.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        })
    })
    .as_deref()
}

/// Port of the local API the browser extension talks to.
pub fn api_port() -> u16 {
    static PORT: OnceLock<u16> = OnceLock::new();
    *PORT.get_or_init(|| {
        std::env::var("FOCUSTRACK_API_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(if name().is_some() {
                DEFAULT_PORT + 1
            } else {
                DEFAULT_PORT
            })
    })
}

/// The data folder for this copy.
pub fn data_dir(base: std::path::PathBuf) -> std::path::PathBuf {
    match name() {
        Some(n) => base.join("profiles").join(n),
        None => base,
    }
}
