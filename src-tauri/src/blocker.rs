//! Application blocking (improvements doc §2, §7).
//!
//! Every 2 s this checks running processes against two lists kept in
//! `Shared::app_blocks`:
//!   * apps whose daily limit is used up (with blocking on), and
//!   * apps blocked by the running focus/study session's profile
//!     (or, for "allow only" profiles, any foreground app not on the list).
//!
//! A blocked app is first asked to close its windows (WM_CLOSE, so it can
//! offer to save work); if it is still running 8 s later it is terminated.
//! Windows components, File Explorer, Task Manager and FocusTrack itself are
//! never touched, so the user can always get back to FocusTrack to end a
//! session or extend a limit.

use crate::categories::browser_family;
use crate::limits::notify;
use crate::platform;
use crate::tracker::Shared;
use chrono::{Local, NaiveDateTime, Timelike};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

const CHECK_EVERY: Duration = Duration::from_secs(2);
const FORCE_AFTER: Duration = Duration::from_secs(8);
const NOTIFY_EVERY: Duration = Duration::from_secs(60);

/// Never blocked, whatever a profile or limit says.
pub const PROTECTED: &[&str] = &[
    "focustrack.exe", "explorer.exe", "taskmgr.exe", "dwm.exe", "csrss.exe", "winlogon.exe", "lsass.exe",
    "services.exe", "svchost.exe", "smss.exe", "wininit.exe", "fontdrvhost.exe", "sihost.exe", "ctfmon.exe",
    "runtimebroker.exe", "searchhost.exe", "searchapp.exe", "startmenuexperiencehost.exe", "shellexperiencehost.exe",
    "textinputhost.exe", "applicationframehost.exe", "lockapp.exe", "systemsettings.exe", "securityhealthsystray.exe",
    "msedgewebview2.exe", "conhost.exe", "dllhost.exe", "audiodg.exe", "spoolsv.exe", "logonui.exe", "userinit.exe",
    "consent.exe", "mmc.exe", "regedit.exe", "cmd.exe", "powershell.exe", "pwsh.exe", "windowsterminal.exe",
    "openconsole.exe", "msiexec.exe",
];

#[derive(Debug, Clone, Default)]
pub struct AppBlocks {
    /// Limit reached: exe (lower-case) → (display name, enforce from).
    pub limit: HashMap<String, (String, NaiveDateTime)>,
    /// Blocked by the focus session: exe (lower-case) → display name.
    pub focus: HashMap<String, String>,
    /// "Allow only" focus profiles: exes (lower-case) that may be in front.
    pub allow_only: Option<HashSet<String>>,
    /// Shown in notifications, e.g. "Deep work until 17:30".
    pub focus_label: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Reason {
    Limit(String),
    Focus(String),
    NotAllowed(String),
}

fn own_exe() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()))
        .unwrap_or_default()
}

/// Windows components and core tools are never blocked.
pub fn is_protected_name(exe_lower: &str) -> bool {
    PROTECTED.contains(&exe_lower)
}

fn is_protected(exe_lower: &str, pid: u32, own: &str) -> bool {
    if pid <= 4 || pid == std::process::id() || exe_lower == own || is_protected_name(exe_lower) {
        return true;
    }
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into()).to_ascii_lowercase();
    match platform::process_image(pid) {
        Some(path) => path.to_ascii_lowercase().starts_with(&windir),
        // Can't inspect it (elevated/system process): leave it alone.
        None => true,
    }
}

/// Why `exe` should be closed right now, if at all.
pub fn reason_for(blocks: &AppBlocks, exe_lower: &str, is_foreground: bool, now: NaiveDateTime) -> Option<Reason> {
    if let Some((name, from)) = blocks.limit.get(exe_lower) {
        if now >= *from {
            return Some(Reason::Limit(name.clone()));
        }
    }
    if let Some(name) = blocks.focus.get(exe_lower) {
        return Some(Reason::Focus(name.clone()));
    }
    if let Some(allowed) = &blocks.allow_only {
        // Only the app in front is checked; browsers are controlled per website.
        if is_foreground && !allowed.contains(exe_lower) && browser_family(exe_lower).is_none() {
            return Some(Reason::NotAllowed(platform::pretty_exe_name(exe_lower)));
        }
    }
    None
}

fn message(reason: &Reason, label: Option<&str>) -> (String, String) {
    let session = label.unwrap_or("your focus session");
    match reason {
        Reason::Limit(name) => (
            format!("{name} is blocked for today"),
            format!("You've reached your daily limit for {name}. Extend it from FocusTrack → Usage Limits if you need to."),
        ),
        Reason::Focus(name) => (
            format!("{name} is blocked right now"),
            format!("{name} is blocked during {session}. It opens again when the session ends."),
        ),
        Reason::NotAllowed(name) => (
            format!("{name} isn't on your allowed list"),
            format!("Only approved apps can be used during {session}."),
        ),
    }
}

fn now() -> NaiveDateTime {
    let t = Local::now().naive_local();
    t.with_nanosecond(0).unwrap_or(t)
}

pub fn spawn(app: tauri::AppHandle, shared: Arc<Shared>) {
    thread::Builder::new()
        .name("focustrack-blocker".into())
        .spawn(move || {
            let own = own_exe();
            let mut asked: HashMap<u32, Instant> = HashMap::new();
            let mut notified: HashMap<String, Instant> = HashMap::new();
            loop {
                if shared.shutdown.load(Ordering::SeqCst) {
                    break;
                }
                thread::sleep(CHECK_EVERY);
                let blocks = match shared.app_blocks.lock() {
                    Ok(b) => b.clone(),
                    Err(_) => continue,
                };
                if blocks.limit.is_empty() && blocks.focus.is_empty() && blocks.allow_only.is_none() {
                    asked.clear();
                    continue;
                }
                let at = now();
                let fg = platform::foreground_pid();
                let procs = platform::processes();
                let alive: HashSet<u32> = procs.iter().map(|p| p.0).collect();
                asked.retain(|pid, _| alive.contains(pid));

                for (pid, exe) in procs {
                    let lower = exe.to_ascii_lowercase();
                    let Some(reason) = reason_for(&blocks, &lower, Some(pid) == fg, at) else { continue };
                    if is_protected(&lower, pid, &own) {
                        continue;
                    }
                    match asked.get(&pid) {
                        None => {
                            platform::close_windows(pid);
                            asked.insert(pid, Instant::now());
                            let due = notified.get(&lower).map_or(true, |t| t.elapsed() >= NOTIFY_EVERY);
                            if due {
                                let (title, body) = message(&reason, blocks.focus_label.as_deref());
                                notify(&app, &title, &body);
                                notified.insert(lower, Instant::now());
                            }
                        }
                        Some(t) if t.elapsed() >= FORCE_AFTER => {
                            if !platform::terminate(pid) {
                                eprintln!("[blocker] could not close {exe} ({pid})");
                            }
                            asked.remove(&pid);
                        }
                        Some(_) => {}
                    }
                }
            }
        })
        .expect("failed to spawn blocker thread");
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn at(h: u32, m: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, 18).unwrap().and_hms_opt(h, m, 0).unwrap()
    }

    #[test]
    fn limit_blocks_only_after_grace() {
        let mut b = AppBlocks::default();
        b.limit.insert("steam.exe".into(), ("Steam".into(), at(10, 1)));
        assert_eq!(reason_for(&b, "steam.exe", false, at(10, 0)), None);
        assert_eq!(reason_for(&b, "steam.exe", false, at(10, 1)), Some(Reason::Limit("Steam".into())));
        assert_eq!(reason_for(&b, "code.exe", true, at(10, 1)), None);
    }

    #[test]
    fn allow_only_checks_foreground_and_spares_browsers() {
        let mut b = AppBlocks::default();
        b.allow_only = Some(["code.exe".to_string()].into_iter().collect());
        assert_eq!(reason_for(&b, "code.exe", true, at(9, 0)), None);
        assert!(matches!(reason_for(&b, "discord.exe", true, at(9, 0)), Some(Reason::NotAllowed(_))));
        assert_eq!(reason_for(&b, "discord.exe", false, at(9, 0)), None, "background apps are left alone");
        assert_eq!(reason_for(&b, "chrome.exe", true, at(9, 0)), None, "websites are handled by the extension");
    }

    #[test]
    fn core_windows_tools_are_protected() {
        for exe in ["explorer.exe", "taskmgr.exe", "focustrack.exe", "svchost.exe"] {
            assert!(is_protected_name(exe));
        }
        assert!(!is_protected_name("steam.exe"));
    }
}
