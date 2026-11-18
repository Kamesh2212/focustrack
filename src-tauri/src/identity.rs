//! Which program is which (issue #5).
//!
//! Exe names alone aren't a reliable identity:
//! * many unrelated programs share generic names (`javaw.exe`, `python.exe`,
//!   `launcher.exe`) — those are identified by full path instead;
//! * game launchers start the real game from a subfolder (HoYoPlay's
//!   `launcher.exe` → `games\Genshin Impact game\GenshinImpact.exe`) — a limit
//!   or block on a launcher covers everything installed under its folder;
//! * a limited app can be renamed — it's still recognised by the original file
//!   name in its version resource.

use std::path::Path;

/// Exe names shared by unrelated programs.
const GENERIC: &[&str] = &[
    "java.exe",
    "javaw.exe",
    "python.exe",
    "pythonw.exe",
    "py.exe",
    "node.exe",
    "electron.exe",
    "launcher.exe",
    "game.exe",
    "app.exe",
    "client.exe",
    "start.exe",
    "run.exe",
    "main.exe",
    "bootstrapper.exe",
    "player.exe",
    "unitycrashhandler64.exe",
];

pub fn lower(s: &str) -> String {
    s.to_ascii_lowercase()
}

pub fn is_generic(exe_lower: &str) -> bool {
    GENERIC.contains(&exe_lower)
}

/// Launchers start other programs from their install folder.
pub fn is_launcher(exe_lower: &str) -> bool {
    exe_lower.contains("launcher") || is_generic(exe_lower) || exe_lower == "hyp.exe"
}

/// The key an app is stored under: its exe name, or its full path for generic names.
pub fn app_key(exe_name: &str, exe_path: &str) -> String {
    let exe = lower(exe_name);
    if is_generic(&exe) && !exe_path.trim().is_empty() {
        lower(exe_path.trim())
    } else {
        exe
    }
}

fn parent_dir(path_lower: &str) -> Option<String> {
    let idx = path_lower.rfind(['\\', '/'])?;
    Some(path_lower[..=idx].to_string())
}

/// Folders too broad to treat as one program's install folder.
fn is_broad(dir_lower: &str) -> bool {
    let d = dir_lower.trim_end_matches(['\\', '/']);
    let depth = d.split(['\\', '/']).filter(|p| !p.is_empty()).count();
    if depth < 3 {
        return true; // e.g. c:\program files
    }
    let env = |k: &str| {
        std::env::var(k)
            .ok()
            .map(|v| lower(v.trim_end_matches('\\')))
    };
    let roots = [
        env("WINDIR"),
        env("ProgramFiles"),
        env("ProgramFiles(x86)"),
        env("ProgramData"),
        env("LOCALAPPDATA"),
        env("APPDATA"),
        env("USERPROFILE"),
        env("LOCALAPPDATA").map(|l| format!("{l}\\programs")),
        env("USERPROFILE").map(|u| format!("{u}\\desktop")),
        env("USERPROFILE").map(|u| format!("{u}\\downloads")),
    ];
    roots.iter().flatten().any(|r| d == r) || env("WINDIR").is_some_and(|w| d.starts_with(&w))
}

/// For launchers, the folder whose programs a limit/block also covers.
pub fn scope_dir(exe_name: &str, exe_path: &str) -> Option<String> {
    let exe = lower(exe_name);
    if !is_launcher(&exe) || exe_path.trim().is_empty() {
        return None;
    }
    let dir = parent_dir(&lower(exe_path.trim()))?;
    (!is_broad(&dir)).then_some(dir)
}

/// Default category from where a program is installed (game stores and launchers).
pub fn category_from_path(path: &str) -> Option<&'static str> {
    let p = lower(path);
    const GAME_DIRS: &[&str] = &[
        "\\steamapps\\common\\",
        "\\steam\\",
        "\\epic games\\",
        "\\hoyoplay\\",
        "\\genshin impact",
        "\\riot games\\",
        "\\battle.net\\",
        "\\blizzard",
        "\\ea games\\",
        "\\electronic arts\\",
        "\\ubisoft\\",
        "\\gog galaxy\\",
        "\\gog games\\",
        "\\xboxgames\\",
        "\\rockstar games\\",
        "\\minecraft",
        "\\roblox\\",
        "\\faceit\\",
        "\\games\\",
    ];
    GAME_DIRS.iter().any(|d| p.contains(d)).then_some("Gaming")
}

/// Something to limit or block, and how to recognise its processes.
#[derive(Debug, Clone, PartialEq)]
pub struct Target {
    pub name: String,
    /// Lower-case exe name.
    pub exe: String,
    /// Lower-case full path, if known.
    pub path: Option<String>,
    /// Launchers: everything under this lower-case folder also matches.
    pub scope: Option<String>,
    pub generic: bool,
}

impl Target {
    pub fn new(name: &str, exe_name: &str, exe_path: Option<&str>) -> Self {
        let exe = lower(exe_name);
        let path = exe_path.map(str::trim).filter(|p| !p.is_empty()).map(lower);
        Target {
            name: name.to_string(),
            scope: path.as_deref().and_then(|p| scope_dir(&exe, p)),
            generic: is_generic(&exe),
            exe,
            path,
        }
    }

    /// Whether a running program is this target. `path` and `original` (the
    /// version resource's original file name) are lower-case when known.
    pub fn matches(&self, exe: &str, path: Option<&str>, original: Option<&str>) -> bool {
        if let (Some(scope), Some(p)) = (&self.scope, path) {
            if p.starts_with(scope.as_str()) {
                return true;
            }
        }
        if self.generic {
            return match (&self.path, path) {
                (Some(mine), Some(p)) => mine == p,
                // Unknown path for a generic name: don't guess.
                _ => false,
            };
        }
        exe == self.exe || original == Some(self.exe.as_str())
    }
}

/// `C:\HoYoPlay\launcher.exe` → "HoYoPlay"; used to name generic exes.
pub fn folder_name(exe_path: &str) -> Option<String> {
    Path::new(exe_path.trim())
        .parent()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .filter(|n| !n.is_empty() && !n.eq_ignore_ascii_case("bin"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOYO: &str = r"C:\Program Files\HoYoPlay\launcher.exe";
    const GENSHIN: &str = r"c:\program files\hoyoplay\games\genshin impact game\genshinimpact.exe";

    #[test]
    fn launcher_limit_covers_games_in_its_folder() {
        let t = Target::new("Genshin Impact", "launcher.exe", Some(HOYO));
        assert_eq!(t.scope.as_deref(), Some(r"c:\program files\hoyoplay\"));
        assert!(t.matches("genshinimpact.exe", Some(GENSHIN), None));
        assert!(t.matches("launcher.exe", Some(&lower(HOYO)), None));
        // Another program that happens to be called launcher.exe isn't caught.
        assert!(!t.matches(
            "launcher.exe",
            Some(r"c:\program files\other\launcher.exe"),
            None
        ));
    }

    #[test]
    fn generic_names_need_the_same_path() {
        let mc = Target::new(
            "Minecraft",
            "javaw.exe",
            Some(r"C:\Games\Minecraft\runtime\bin\javaw.exe"),
        );
        assert!(mc.matches(
            "javaw.exe",
            Some(r"c:\games\minecraft\runtime\bin\javaw.exe"),
            None
        ));
        assert!(!mc.matches(
            "javaw.exe",
            Some(r"c:\program files\java\jdk-21\bin\javaw.exe"),
            None
        ));
        assert!(
            !mc.matches("javaw.exe", None, None),
            "unknown path: don't guess"
        );
        assert_eq!(app_key("javaw.exe", r"C:\A\javaw.exe"), r"c:\a\javaw.exe");
        assert_eq!(app_key("Steam.exe", r"C:\Steam\steam.exe"), "steam.exe");
    }

    #[test]
    fn renamed_copies_are_still_recognised() {
        let steam = Target::new(
            "Steam",
            "steam.exe",
            Some(r"C:\Program Files (x86)\Steam\steam.exe"),
        );
        assert!(steam.matches(
            "notsteam.exe",
            Some(r"c:\temp\notsteam.exe"),
            Some("steam.exe")
        ));
        assert!(!steam.matches("code.exe", Some(r"c:\vs\code.exe"), Some("code.exe")));
        assert_eq!(steam.scope, None, "steam.exe isn't a generic launcher name");
    }

    #[test]
    fn broad_folders_are_never_a_scope() {
        assert_eq!(scope_dir("launcher.exe", r"C:\launcher.exe"), None);
        assert_eq!(
            scope_dir("launcher.exe", r"C:\Program Files\launcher.exe"),
            None
        );
        assert_eq!(
            scope_dir("code.exe", r"C:\Apps\VS Code\code.exe"),
            None,
            "not a launcher"
        );
    }

    #[test]
    fn games_are_recognised_by_install_folder() {
        assert_eq!(category_from_path(GENSHIN), Some("Gaming"));
        assert_eq!(
            category_from_path(r"D:\SteamLibrary\steamapps\common\Hades\Hades.exe"),
            Some("Gaming")
        );
        assert_eq!(
            category_from_path(r"C:\Program Files\Microsoft VS Code\Code.exe"),
            None
        );
        assert_eq!(folder_name(HOYO).as_deref(), Some("HoYoPlay"));
    }
}
