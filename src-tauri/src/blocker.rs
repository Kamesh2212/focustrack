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
//! Apps are recognised by identity (path, launcher folder, original file
//! name — see `identity.rs`), not just exe name. Apps running as
//! administrator can only be closed when FocusTrack runs as administrator too.
//! Windows components, File Explorer, Task Manager and FocusTrack itself are
//! never touched, so the user can always get back to FocusTrack to end a
//! session or extend a limit.

use crate::categories::browser_family;
use crate::identity::Target;
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
const ADMIN_NOTICE_EVERY: Duration = Duration::from_secs(30 * 60);

/// Never blocked, whatever a profile or limit says.
pub const PROTECTED: &[&str] = &[
    "focustrack.exe",
    "explorer.exe",
    "taskmgr.exe",
    "dwm.exe",
    "csrss.exe",
    "winlogon.exe",
    "lsass.exe",
    "services.exe",
    "svchost.exe",
    "smss.exe",
    "wininit.exe",
    "fontdrvhost.exe",
    "sihost.exe",
    "ctfmon.exe",
    "runtimebroker.exe",
    "searchhost.exe",
    "searchapp.exe",
    "startmenuexperiencehost.exe",
    "shellexperiencehost.exe",
    "textinputhost.exe",
    "applicationframehost.exe",
    "lockapp.exe",
    "systemsettings.exe",
    "securityhealthsystray.exe",
    "msedgewebview2.exe",
    "conhost.exe",
    "dllhost.exe",
    "audiodg.exe",
    "spoolsv.exe",
    "logonui.exe",
    "userinit.exe",
    "consent.exe",
    "mmc.exe",
    "regedit.exe",
    "cmd.exe",
    "powershell.exe",
    "pwsh.exe",
    "windowsterminal.exe",
    "openconsole.exe",
    "msiexec.exe",
];

#[derive(Debug, Clone, Default)]
pub struct AppBlocks {
    /// Limit reached: what to block, and when enforcement starts.
    pub limit: Vec<(Target, NaiveDateTime)>,
    /// Blocked by the focus session's profile.
    pub focus: Vec<Target>,
    /// "Allow only" focus profiles: the apps that may be in front.
    pub allow_only: Option<Vec<Target>>,
    /// Shown in notifications, e.g. "Deep work until 17:30".
    pub focus_label: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Reason {
    Limit(String),
    Focus(String),
    NotAllowed(String),
}

/// What we know about a running process (cached per pid).
#[derive(Debug, Clone, Default)]
pub struct ProcInfo {
    pub exe: String,
    /// Lower-case full path, if Windows lets us read it.
    pub path: Option<String>,
    /// Lower-case original file name from the version resource.
    pub original: Option<String>,
}

fn own_exe() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy().to_ascii_lowercase())
        })
        .unwrap_or_default()
}

/// Windows components and core tools are never blocked.
pub fn is_protected_name(exe_lower: &str) -> bool {
    PROTECTED.contains(&exe_lower)
}

fn is_protected(p: &ProcInfo, pid: u32, own: &str) -> bool {
    if pid <= 4 || pid == std::process::id() || p.exe == own || is_protected_name(&p.exe) {
        return true;
    }
    let windir = std::env::var("WINDIR")
        .unwrap_or_else(|_| r"C:\Windows".into())
        .to_ascii_lowercase();
    match &p.path {
        Some(path) => path.starts_with(&windir),
        // Can't inspect it (system process): leave it alone.
        None => true,
    }
}

/// Why a process should be closed right now, if at all.
pub fn reason_for(
    blocks: &AppBlocks,
    p: &ProcInfo,
    is_foreground: bool,
    now: NaiveDateTime,
) -> Option<Reason> {
    let hit = |t: &Target| t.matches(&p.exe, p.path.as_deref(), p.original.as_deref());
    if let Some((t, _)) = blocks.limit.iter().find(|(t, from)| now >= *from && hit(t)) {
        return Some(Reason::Limit(t.name.clone()));
    }
    if let Some(t) = blocks.focus.iter().find(|t| hit(t)) {
        return Some(Reason::Focus(t.name.clone()));
    }
    if let Some(allowed) = &blocks.allow_only {
        // Only the app in front is checked; browsers are controlled per website.
        if is_foreground && !allowed.iter().any(hit) && browser_family(&p.exe).is_none() {
            return Some(Reason::NotAllowed(platform::pretty_exe_name(&p.exe)));
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

fn reason_name(reason: &Reason) -> &str {
    match reason {
        Reason::Limit(n) | Reason::Focus(n) | Reason::NotAllowed(n) => n,
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
            let elevated = platform::is_elevated();
            let mut asked: HashMap<u32, Instant> = HashMap::new();
            let mut notified: HashMap<String, Instant> = HashMap::new();
            let mut info: HashMap<u32, ProcInfo> = HashMap::new();
            let mut originals: HashMap<String, Option<String>> = HashMap::new();
            loop {
                if shared.shutdown.load(Ordering::SeqCst) {
                    break;
                }
                thread::sleep(CHECK_EVERY);
                let blocks = match shared.app_blocks.lock() {
                    Ok(b) => b.clone(),
                    Err(_) => continue,
                };
                if blocks.limit.is_empty() && blocks.focus.is_empty() && blocks.allow_only.is_none()
                {
                    asked.clear();
                    continue;
                }
                let at = now();
                let fg = platform::foreground_pid();
                let procs = platform::processes();
                let alive: HashSet<u32> = procs.iter().map(|p| p.0).collect();
                asked.retain(|pid, _| alive.contains(pid));
                info.retain(|pid, _| alive.contains(pid));

                for (pid, exe) in procs {
                    let lower = exe.to_ascii_lowercase();
                    let p = info
                        .entry(pid)
                        .or_insert_with(|| {
                            let path = platform::process_image(pid).map(|p| p.to_ascii_lowercase());
                            let original = path.as_ref().and_then(|path| {
                                originals
                                    .entry(path.clone())
                                    .or_insert_with(|| platform::original_file_name(path))
                                    .clone()
                            });
                            ProcInfo { exe: lower.clone(), path, original }
                        })
                        .clone();
                    let Some(reason) = reason_for(&blocks, &p, Some(pid) == fg, at) else {
                        continue;
                    };
                    if is_protected(&p, pid, &own) {
                        continue;
                    }
                    match asked.get(&pid) {
                        None => {
                            platform::close_windows(pid);
                            asked.insert(pid, Instant::now());
                            let due = notified
                                .get(&lower)
                                .is_none_or(|t| t.elapsed() >= NOTIFY_EVERY);
                            if due {
                                let (title, body) = message(&reason, blocks.focus_label.as_deref());
                                notify(&app, &title, &body);
                                notified.insert(lower, Instant::now());
                            }
                        }
                        Some(t) if t.elapsed() >= FORCE_AFTER => {
                            if !platform::terminate(pid) && !elevated {
                                // Games like Genshin Impact run as administrator; Windows
                                // doesn't let a normal app close them.
                                let key = format!("admin:{lower}");
                                if notified.get(&key).is_none_or(|t| t.elapsed() >= ADMIN_NOTICE_EVERY) {
                                    let name = reason_name(&reason);
                                    notify(
                                        &app,
                                        &format!("FocusTrack can't close {name}"),
                                        &format!("{name} runs as administrator. Turn on Administrator mode in FocusTrack → Settings so it can be blocked."),
                                    );
                                    notified.insert(key, Instant::now());
                                }
                                if let Ok(mut s) = shared.needs_admin.lock() {
                                    s.insert(name_for(&reason));
                                }
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

fn name_for(reason: &Reason) -> String {
    reason_name(reason).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn at(h: u32, m: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, 18)
            .unwrap()
            .and_hms_opt(h, m, 0)
            .unwrap()
    }

    fn proc(exe: &str, path: &str) -> ProcInfo {
        ProcInfo {
            exe: exe.into(),
            path: Some(path.to_ascii_lowercase()),
            original: None,
        }
    }

    #[test]
    fn limit_blocks_only_after_grace() {
        let b = AppBlocks {
            limit: vec![(
                Target::new("Steam", "steam.exe", Some(r"C:\Steam\steam.exe")),
                at(10, 1),
            )],
            ..Default::default()
        };
        let steam = proc("steam.exe", r"c:\steam\steam.exe");
        assert_eq!(reason_for(&b, &steam, false, at(10, 0)), None);
        assert_eq!(
            reason_for(&b, &steam, false, at(10, 1)),
            Some(Reason::Limit("Steam".into()))
        );
        assert_eq!(
            reason_for(&b, &proc("code.exe", r"c:\vs\code.exe"), true, at(10, 1)),
            None
        );
    }

    #[test]
    fn launcher_block_covers_the_game_it_starts() {
        // Genshin Impact: HoYoPlay's launcher.exe starts games\…\GenshinImpact.exe.
        let b = AppBlocks {
            focus: vec![Target::new(
                "Genshin Impact",
                "launcher.exe",
                Some(r"C:\Program Files\HoYoPlay\launcher.exe"),
            )],
            ..Default::default()
        };
        let game = proc(
            "genshinimpact.exe",
            r"C:\Program Files\HoYoPlay\games\Genshin Impact game\GenshinImpact.exe",
        );
        assert_eq!(
            reason_for(&b, &game, false, at(9, 0)),
            Some(Reason::Focus("Genshin Impact".into()))
        );
        let other_launcher = proc("launcher.exe", r"C:\Program Files\Other\launcher.exe");
        assert_eq!(reason_for(&b, &other_launcher, false, at(9, 0)), None);
    }

    #[test]
    fn renamed_copy_of_a_limited_app_is_blocked() {
        let b = AppBlocks {
            limit: vec![(Target::new("Steam", "steam.exe", None), at(9, 0))],
            ..Default::default()
        };
        let renamed = ProcInfo {
            exe: "notsteam.exe".into(),
            path: Some(r"c:\tmp\notsteam.exe".into()),
            original: Some("steam.exe".into()),
        };
        assert!(reason_for(&b, &renamed, false, at(9, 0)).is_some());
    }

    #[test]
    fn allow_only_checks_foreground_and_spares_browsers() {
        let b = AppBlocks {
            allow_only: Some(vec![Target::new("VS Code", "code.exe", None)]),
            ..Default::default()
        };
        assert_eq!(
            reason_for(&b, &proc("code.exe", r"c:\vs\code.exe"), true, at(9, 0)),
            None
        );
        assert!(matches!(
            reason_for(
                &b,
                &proc("discord.exe", r"c:\d\discord.exe"),
                true,
                at(9, 0)
            ),
            Some(Reason::NotAllowed(_))
        ));
        assert_eq!(
            reason_for(
                &b,
                &proc("discord.exe", r"c:\d\discord.exe"),
                false,
                at(9, 0)
            ),
            None,
            "background apps are left alone"
        );
        assert_eq!(
            reason_for(&b, &proc("chrome.exe", r"c:\c\chrome.exe"), true, at(9, 0)),
            None,
            "websites are handled by the extension"
        );
    }

    #[test]
    fn core_windows_tools_are_protected() {
        for exe in [
            "explorer.exe",
            "taskmgr.exe",
            "focustrack.exe",
            "svchost.exe",
        ] {
            assert!(is_protected_name(exe));
        }
        assert!(!is_protected_name("steam.exe"));
    }
}
