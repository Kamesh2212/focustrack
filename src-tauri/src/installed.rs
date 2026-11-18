//! Discovers applications installed on this PC, so limits can be set for apps
//! that haven't been used yet (SRS 2.5).
//!
//! Sources, merged by executable name:
//!   1. Start Menu shortcuts (all users + current user) — best display names
//!   2. Installed-programs (`Uninstall`) registry entries with an .exe icon
//!
//! (`App Paths` is deliberately not used: it is mostly helper executables.)
//!
//! Store (UWP) apps are not listed; they still appear once they've been used.

use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InstalledApp {
    pub exe_name: String,
    pub name: String,
    pub path: String,
}

// ------------------------------------------------------------- .lnk parsing

fn u16le(d: &[u8], at: usize) -> Option<u16> {
    d.get(at..at + 2).map(|b| u16::from_le_bytes([b[0], b[1]]))
}

fn u32le(d: &[u8], at: usize) -> Option<u32> {
    d.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn cstr_ansi(d: &[u8], at: usize) -> Option<String> {
    let rest = d.get(at..)?;
    let end = rest.iter().position(|&b| b == 0)?;
    Some(String::from_utf8_lossy(&rest[..end]).into_owned())
}

fn cstr_utf16(d: &[u8], at: usize) -> Option<String> {
    let mut units = Vec::new();
    let mut i = at;
    loop {
        let u = u16le(d, i)?;
        if u == 0 {
            break;
        }
        units.push(u);
        i += 2;
    }
    Some(String::from_utf16_lossy(&units))
}

#[derive(Debug, Default, PartialEq)]
pub struct Shortcut {
    pub target: Option<String>,
    pub arguments: Option<String>,
}

/// Minimal [MS-SHLLINK] reader: the local target path and the arguments.
pub fn parse_lnk(d: &[u8]) -> Option<Shortcut> {
    if u32le(d, 0)? != 0x4C {
        return None;
    }
    let flags = u32le(d, 0x14)?;
    let unicode = flags & 0x80 != 0;
    let mut pos = 0x4C;
    if flags & 0x01 != 0 {
        // HasLinkTargetIDList
        pos += 2 + u16le(d, pos)? as usize;
    }
    let mut out = Shortcut::default();
    if flags & 0x02 != 0 {
        // HasLinkInfo
        let li = pos;
        let li_size = u32le(d, li)? as usize;
        let li_header = u32le(d, li + 4)? as usize;
        let li_flags = u32le(d, li + 8)?;
        if li_flags & 0x01 != 0 {
            let suffix_off = u32le(d, li + 0x18)? as usize;
            let base = if li_header >= 0x24 && u32le(d, li + 0x1C)? != 0 {
                cstr_utf16(d, li + u32le(d, li + 0x1C)? as usize)
            } else {
                cstr_ansi(d, li + u32le(d, li + 0x10)? as usize)
            };
            let suffix = if li_header >= 0x24 && u32le(d, li + 0x20).unwrap_or(0) != 0 {
                cstr_utf16(d, li + u32le(d, li + 0x20)? as usize)
            } else {
                cstr_ansi(d, li + suffix_off)
            };
            out.target = base.map(|b| b + suffix.as_deref().unwrap_or(""));
        }
        pos = li + li_size;
    }
    // StringData: NAME, RELATIVE_PATH, WORKING_DIR, ARGUMENTS, ICON_LOCATION
    for (bit, is_args) in [(0x04, false), (0x08, false), (0x10, false), (0x20, true)] {
        if flags & bit == 0 {
            continue;
        }
        let chars = u16le(d, pos)? as usize;
        pos += 2;
        let bytes = if unicode { chars * 2 } else { chars };
        let raw = d.get(pos..pos + bytes)?;
        if is_args {
            let s = if unicode {
                let units: Vec<u16> = raw.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
                String::from_utf16_lossy(&units)
            } else {
                String::from_utf8_lossy(raw).into_owned()
            };
            out.arguments = Some(s);
        }
        pos += bytes;
    }
    Some(out)
}

// ------------------------------------------------------------------ helpers

fn file_name(p: &str) -> &str {
    p.rsplit(['\\', '/']).next().unwrap_or(p)
}

/// Expands `%VAR%` references and strips quotes and `,0` icon indexes.
pub fn clean_path(raw: &str) -> String {
    let mut s = raw.trim().trim_matches('"').to_string();
    if let Some(i) = s.to_ascii_lowercase().find(".exe") {
        s.truncate(i + 4);
    }
    let mut out = String::new();
    let mut rest = s.as_str();
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) => {
                let var = &after[..end];
                out.push_str(&std::env::var(var).unwrap_or_else(|_| format!("%{var}%")));
                rest = &after[end + 1..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out.trim_matches('"').to_string()
}

/// Command hosts that shortcuts launch scripts with; the shortcut isn't the app.
const HOSTS: &[&str] = &[
    "cmd.exe", "powershell.exe", "pwsh.exe", "wscript.exe", "cscript.exe", "mmc.exe", "conhost.exe",
    "explorer.exe", "control.exe",
];

/// Uninstallers, installers, updaters, runtimes, helpers and crash reporters
/// aren't apps people use.
pub fn is_noise(exe_name: &str, name: &str) -> bool {
    let e = exe_name.to_ascii_lowercase();
    let n = name.to_ascii_lowercase();
    let exe_words = ["redist", "runtime", "clicktorun", "upgrader", "update", "plugin", "helper", "-amd64", "-x64", "-win"];
    let name_words = [
        "redistributable", "runtime", "driver", "plugin", " sdk", "update", "installation assistant",
        "component", "handler", "language preferences", "reload configuration", "shell ext",
    ];
    !e.ends_with(".exe")
        || HOSTS.contains(&e.as_str())
        || exe_words.iter().any(|w| e.contains(w))
        || name_words.iter().any(|w| n.contains(w))
        || e.starts_with("unins")
        || e.contains("uninstall")
        || e.contains("setup")
        || e.contains("installer")
        || e.contains("crashpad")
        || e.contains("crashreport")
        || e.contains("updater")
        || e == "update.exe"
        || e == "msiexec.exe"
        || e == "rundll32.exe"
        || n.contains("uninstall")
        || n.contains("readme")
        || n.contains("release notes")
        || n.contains("documentation")
        || n.ends_with(" help")
        || n.ends_with(" website")
}

/// Squirrel apps (Discord, Slack, Teams classic…) start via `Update.exe --processStart App.exe`.
pub fn squirrel_target(target: &str, args: Option<&str>) -> Option<String> {
    if !file_name(target).eq_ignore_ascii_case("update.exe") {
        return None;
    }
    let args = args?;
    let mut it = args.split_whitespace();
    while let Some(a) = it.next() {
        if a.eq_ignore_ascii_case("--processStart") {
            let exe = it.next()?.trim_matches('"');
            let dir = Path::new(target).parent()?;
            return Some(dir.join(exe).to_string_lossy().into_owned());
        }
    }
    None
}

fn add(map: &mut HashMap<String, InstalledApp>, path: &str, name: &str, check_exists: bool) {
    let exe = file_name(path).to_string();
    let name = name.trim();
    if exe.is_empty() || name.is_empty() || is_noise(&exe, name) {
        return;
    }
    let lower = path.to_ascii_lowercase();
    if lower.contains(r"\package cache\") || lower.contains(r"\installer\") || lower.contains(r"\windows\installer") {
        return;
    }
    if check_exists && !Path::new(path).exists() {
        return;
    }
    map.entry(exe.to_ascii_lowercase()).or_insert_with(|| InstalledApp {
        exe_name: exe,
        name: name.to_string(),
        path: path.to_string(),
    });
}

fn walk_lnk(dir: &Path, out: &mut Vec<PathBuf>, depth: u32) {
    if depth > 4 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk_lnk(&p, out, depth + 1);
        } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("lnk")) {
            out.push(p);
        }
    }
}

// --------------------------------------------------------------- discovery

static CACHE: std::sync::Mutex<Option<(std::time::Instant, Vec<InstalledApp>)>> = std::sync::Mutex::new(None);

/// `scan()` cached for 10 minutes (it reads the Start Menu and registry).
pub fn cached() -> Vec<InstalledApp> {
    if let Ok(c) = CACHE.lock() {
        if let Some((at, apps)) = c.as_ref() {
            if at.elapsed() < std::time::Duration::from_secs(600) {
                return apps.clone();
            }
        }
    }
    let apps = scan();
    if let Ok(mut c) = CACHE.lock() {
        *c = Some((std::time::Instant::now(), apps.clone()));
    }
    apps
}

pub fn scan() -> Vec<InstalledApp> {
    let mut map: HashMap<String, InstalledApp> = HashMap::new();

    // 1. Start Menu shortcuts
    let mut roots = Vec::new();
    for var in ["ProgramData", "APPDATA"] {
        if let Ok(base) = std::env::var(var) {
            roots.push(PathBuf::from(base).join(r"Microsoft\Windows\Start Menu\Programs"));
        }
    }
    let mut links = Vec::new();
    for r in &roots {
        walk_lnk(r, &mut links, 0);
    }
    for link in links {
        let Ok(data) = std::fs::read(&link) else { continue };
        let Some(sc) = parse_lnk(&data) else { continue };
        let Some(target) = sc.target else { continue };
        let target = squirrel_target(&target, sc.arguments.as_deref()).unwrap_or(target);
        let name = link.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        add(&mut map, &target, &name, true);
    }

    #[cfg(windows)]
    registry::scan(&mut map);

    let mut apps: Vec<InstalledApp> = map.into_values().collect();
    apps.sort_by_key(|a| a.name.to_lowercase());
    apps
}

#[cfg(windows)]
mod registry {
    use super::{add, clean_path, InstalledApp};
    use std::collections::HashMap;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ};
    use winreg::RegKey;

    pub fn scan(map: &mut HashMap<String, InstalledApp>) {
        let hives = [RegKey::predef(HKEY_LOCAL_MACHINE), RegKey::predef(HKEY_CURRENT_USER)];

        // 2. Installed programs whose icon is their exe.
        let uninstall = [
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
            r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
        ];
        for hive in &hives {
            for path in uninstall {
                let Ok(root) = hive.open_subkey_with_flags(path, KEY_READ) else { continue };
                for key in root.enum_keys().flatten() {
                    let Ok(k) = root.open_subkey_with_flags(&key, KEY_READ) else { continue };
                    if k.get_value::<u32, _>("SystemComponent").unwrap_or(0) == 1 {
                        continue;
                    }
                    let (Ok(name), Ok(icon)) = (k.get_value::<String, _>("DisplayName"), k.get_value::<String, _>("DisplayIcon")) else {
                        continue;
                    };
                    let exe = clean_path(&icon);
                    if exe.to_ascii_lowercase().ends_with(".exe") {
                        add(map, &exe, &name, true);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a minimal unicode .lnk with LinkInfo and arguments.
    fn make_lnk(target: &str, args: &str) -> Vec<u8> {
        let mut d = vec![0u8; 0x4C];
        d[0..4].copy_from_slice(&0x4Cu32.to_le_bytes());
        let flags: u32 = 0x02 | 0x20 | 0x80; // LinkInfo, Arguments, Unicode
        d[0x14..0x18].copy_from_slice(&flags.to_le_bytes());
        // LinkInfo with 0x1C header, ANSI base path, empty suffix.
        let header = 0x1Cu32;
        let base_off = header;
        let suffix_off = base_off + target.len() as u32 + 1;
        let size = suffix_off + 1;
        let mut li = Vec::new();
        for v in [size, header, 1, 0, base_off, 0, suffix_off] {
            li.extend_from_slice(&v.to_le_bytes());
        }
        li.extend_from_slice(target.as_bytes());
        li.push(0);
        li.push(0);
        d.extend_from_slice(&li);
        let units: Vec<u16> = args.encode_utf16().collect();
        d.extend_from_slice(&(units.len() as u16).to_le_bytes());
        for u in units {
            d.extend_from_slice(&u.to_le_bytes());
        }
        d
    }

    #[test]
    fn parses_shortcut_target_and_args() {
        let d = make_lnk(r"C:\Users\me\AppData\Local\Discord\Update.exe", "--processStart Discord.exe");
        let sc = parse_lnk(&d).unwrap();
        assert_eq!(sc.target.as_deref(), Some(r"C:\Users\me\AppData\Local\Discord\Update.exe"));
        assert_eq!(sc.arguments.as_deref(), Some("--processStart Discord.exe"));
        assert_eq!(
            squirrel_target(sc.target.as_deref().unwrap(), sc.arguments.as_deref()).as_deref(),
            Some(r"C:\Users\me\AppData\Local\Discord\Discord.exe")
        );
        assert!(parse_lnk(b"not a shortcut").is_none());
    }

    #[test]
    fn filters_noise() {
        assert!(is_noise("unins000.exe", "Uninstall Foo"));
        assert!(is_noise("Update.exe", "Discord"));
        assert!(is_noise("foo.exe", "Foo Website"));
        assert!(is_noise("VC_redist.x64.exe", "Microsoft Visual C++ Redistributable"));
        assert!(is_noise("cmd.exe", "Install Additional Tools for Node.js"));
        assert!(is_noise("SDXHelper.exe", "Microsoft Office SDX Helper"));
        assert!(!is_noise("WINWORD.EXE", "Word"));
        assert!(!is_noise("Discord.exe", "Discord"));
        assert!(!is_noise("Code.exe", "Visual Studio Code"));
        assert!(!is_noise("steam.exe", "Steam"));
    }

    #[test]
    fn cleans_registry_paths() {
        assert_eq!(clean_path(r#""C:\Apps\x.exe",0"#), r"C:\Apps\x.exe");
        assert_eq!(clean_path(r"C:\Apps\y.exe -flag"), r"C:\Apps\y.exe");
        std::env::set_var("FT_TEST_DIR", r"D:\T");
        assert_eq!(clean_path(r"%FT_TEST_DIR%\z.exe"), r"D:\T\z.exe");
    }

    #[test]
    #[cfg(windows)]
    fn finds_some_apps_on_this_machine() {
        let apps = scan();
        assert!(!apps.is_empty(), "expected at least one installed app");
        assert!(apps.iter().all(|a| a.exe_name.to_ascii_lowercase().ends_with(".exe")));
    }
}
