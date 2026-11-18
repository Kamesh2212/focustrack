//! Built-in categories (SRS 2.2) and default executable -> category mapping.
//! Users can re-categorise any application from the Applications screen.

/// (name, colour, kind). `kind` drives productivity statistics:
/// `productive`, `neutral` or `distracting`.
pub const DEFAULT_CATEGORIES: &[(&str, &str, &str)] = &[
    ("Productivity", "#6366f1", "productive"),
    ("Education", "#0ea5e9", "productive"),
    ("Development", "#10b981", "productive"),
    ("Communication", "#f59e0b", "neutral"),
    ("Entertainment", "#ef4444", "distracting"),
    ("Social Media", "#ec4899", "distracting"),
    ("Gaming", "#8b5cf6", "distracting"),
    ("Other", "#94a3b8", "neutral"),
];

/// Best-guess category for a freshly seen executable (lower-case exe name).
pub fn default_category_for(exe_lower: &str) -> &'static str {
    let stem = exe_lower.strip_suffix(".exe").unwrap_or(exe_lower);
    match stem {
        "code" | "code - insiders" | "cursor" | "devenv" | "idea64" | "pycharm64" | "webstorm64"
        | "rider64" | "clion64" | "goland64" | "android studio" | "studio64" | "sublime_text"
        | "notepad++" | "windowsterminal" | "wt" | "powershell" | "pwsh" | "cmd" | "conhost"
        | "git-bash" | "mintty" | "postman" | "docker desktop" | "gitkraken" | "githubdesktop"
        | "zed" | "nvim-qt" => "Development",

        "winword" | "excel" | "powerpnt" | "onenote" | "msaccess" | "mspub" | "notion"
        | "obsidian" | "evernote" | "acrobat" | "acrord32" | "sumatrapdf" | "figma"
        | "photoshop" | "illustrator" | "explorer" | "notepad" | "todoist" | "linear" => {
            "Productivity"
        }

        "anki" | "zotero" | "mendeley" | "kindle" | "duolingo" => "Education",

        "discord" | "slack" | "teams" | "ms-teams" | "outlook" | "olk" | "zoom" | "skype"
        | "whatsapp" | "telegram" | "signal" | "thunderbird" | "mailspring" => "Communication",

        "spotify" | "vlc" | "netflix" | "primevideo" | "wmplayer" | "music.ui" | "video.ui"
        | "mpc-hc64" | "potplayer64" | "itunes" | "applemusic" => "Entertainment",

        "instagram" | "facebook" | "twitter" | "x" | "tiktok" | "reddit" => "Social Media",

        "steam" | "steamwebhelper" | "epicgameslauncher" | "battle.net" | "riotclientservices"
        | "leagueclient" | "valorant" | "minecraft" | "javaw" | "robloxplayerbeta" | "gog galaxy"
        | "xboxapp" | "gamebar" => "Gaming",

        _ => "Other",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_apps_map_to_categories() {
        assert_eq!(default_category_for("code.exe"), "Development");
        assert_eq!(default_category_for("winword.exe"), "Productivity");
        assert_eq!(default_category_for("discord.exe"), "Communication");
        assert_eq!(default_category_for("steam.exe"), "Gaming");
        assert_eq!(default_category_for("something-new.exe"), "Other");
    }

    #[test]
    fn every_default_target_exists() {
        let names: Vec<&str> = DEFAULT_CATEGORIES.iter().map(|c| c.0).collect();
        for exe in ["code.exe", "excel.exe", "anki.exe", "slack.exe", "vlc.exe", "tiktok.exe", "steam.exe", "x.exe"] {
            assert!(names.contains(&default_category_for(exe)));
        }
    }
}
