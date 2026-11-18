//! Built-in categories (SRS 2.2) and default executable -> category mapping.
//! Users can re-categorise any application from the Applications screen.

/// (name, colour, kind). `kind` drives productivity statistics:
/// `productive`, `neutral` or `distracting`.
pub const DEFAULT_CATEGORIES: &[(&str, &str, &str)] = &[
    ("Productivity", "#6366f1", "productive"),
    ("Education", "#0ea5e9", "productive"),
    ("Study", "#14b8a6", "productive"),
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
        "code" | "code - insiders" | "cursor" | "devenv" | "idea64" | "pycharm64"
        | "webstorm64" | "rider64" | "clion64" | "goland64" | "android studio" | "studio64"
        | "sublime_text" | "notepad++" | "windowsterminal" | "wt" | "powershell" | "pwsh"
        | "cmd" | "conhost" | "git-bash" | "mintty" | "postman" | "docker desktop"
        | "gitkraken" | "githubdesktop" | "zed" | "nvim-qt" => "Development",

        "winword" | "excel" | "powerpnt" | "onenote" | "msaccess" | "mspub" | "notion"
        | "obsidian" | "evernote" | "acrobat" | "acrord32" | "sumatrapdf" | "figma"
        | "photoshop" | "illustrator" | "explorer" | "notepad" | "todoist" | "linear" => {
            "Productivity"
        }

        "anki" | "zotero" | "mendeley" | "kindle" | "duolingo" | "goodnotes" | "xodo"
        | "calibre" => "Study",

        "discord" | "slack" | "teams" | "ms-teams" | "outlook" | "olk" | "zoom" | "skype"
        | "whatsapp" | "telegram" | "signal" | "thunderbird" | "mailspring" => "Communication",

        "spotify" | "vlc" | "netflix" | "primevideo" | "wmplayer" | "music.ui" | "video.ui"
        | "mpc-hc64" | "potplayer64" | "itunes" | "applemusic" => "Entertainment",

        "instagram" | "facebook" | "twitter" | "x" | "tiktok" | "reddit" => "Social Media",

        "steam"
        | "steamwebhelper"
        | "epicgameslauncher"
        | "battle.net"
        | "riotclientservices"
        | "leagueclient"
        | "valorant"
        | "minecraft"
        | "minecraftlauncher"
        | "robloxplayerbeta"
        | "gog galaxy"
        | "xboxapp"
        | "gamebar"
        | "genshinimpact"
        | "yuanshen"
        | "starrail"
        | "zenlesszonezero"
        | "hyp"
        | "bh3"
        | "wutheringwaves"
        | "fortnitelauncher"
        | "fortniteclient-win64-shipping"
        | "eadesktop"
        | "ubisoftconnect"
        | "upc"
        | "galaxyclient" => "Gaming",

        _ => "Other",
    }
}

/// Known website domains and their default categories.
pub const DOMAIN_MAP: &[(&str, &str)] = &[
    ("youtube.com", "Entertainment"),
    ("youtu.be", "Entertainment"),
    ("netflix.com", "Entertainment"),
    ("primevideo.com", "Entertainment"),
    ("disneyplus.com", "Entertainment"),
    ("hotstar.com", "Entertainment"),
    ("twitch.tv", "Entertainment"),
    ("spotify.com", "Entertainment"),
    ("soundcloud.com", "Entertainment"),
    ("imdb.com", "Entertainment"),
    ("9gag.com", "Entertainment"),
    ("instagram.com", "Social Media"),
    ("facebook.com", "Social Media"),
    ("x.com", "Social Media"),
    ("twitter.com", "Social Media"),
    ("reddit.com", "Social Media"),
    ("tiktok.com", "Social Media"),
    ("linkedin.com", "Social Media"),
    ("pinterest.com", "Social Media"),
    ("snapchat.com", "Social Media"),
    ("threads.net", "Social Media"),
    ("tumblr.com", "Social Media"),
    ("quora.com", "Social Media"),
    ("github.com", "Development"),
    ("gitlab.com", "Development"),
    ("bitbucket.org", "Development"),
    ("stackoverflow.com", "Development"),
    ("stackexchange.com", "Development"),
    ("developer.mozilla.org", "Development"),
    ("npmjs.com", "Development"),
    ("crates.io", "Development"),
    ("docs.rs", "Development"),
    ("pypi.org", "Development"),
    ("vercel.com", "Development"),
    ("localhost", "Development"),
    ("claude.ai", "Productivity"),
    ("chatgpt.com", "Productivity"),
    ("docs.google.com", "Productivity"),
    ("drive.google.com", "Productivity"),
    ("sheets.google.com", "Productivity"),
    ("calendar.google.com", "Productivity"),
    ("notion.so", "Productivity"),
    ("figma.com", "Productivity"),
    ("trello.com", "Productivity"),
    ("asana.com", "Productivity"),
    ("linear.app", "Productivity"),
    ("office.com", "Productivity"),
    ("canva.com", "Productivity"),
    ("mail.google.com", "Communication"),
    ("gmail.com", "Communication"),
    ("outlook.live.com", "Communication"),
    ("outlook.office.com", "Communication"),
    ("web.whatsapp.com", "Communication"),
    ("web.telegram.org", "Communication"),
    ("discord.com", "Communication"),
    ("slack.com", "Communication"),
    ("teams.microsoft.com", "Communication"),
    ("meet.google.com", "Communication"),
    ("zoom.us", "Communication"),
    ("wikipedia.org", "Education"),
    ("coursera.org", "Study"),
    ("udemy.com", "Study"),
    ("edx.org", "Study"),
    ("khanacademy.org", "Study"),
    ("duolingo.com", "Study"),
    ("leetcode.com", "Study"),
    ("quizlet.com", "Study"),
    ("brilliant.org", "Study"),
    ("ankiweb.net", "Study"),
    ("notebooklm.google.com", "Study"),
    ("classroom.google.com", "Study"),
    ("arxiv.org", "Study"),
    ("jstor.org", "Study"),
    ("researchgate.net", "Study"),
    ("geeksforgeeks.org", "Education"),
    ("w3schools.com", "Education"),
    ("scholar.google.com", "Study"),
    ("store.steampowered.com", "Gaming"),
    ("steamcommunity.com", "Gaming"),
    ("epicgames.com", "Gaming"),
    ("roblox.com", "Gaming"),
    ("chess.com", "Gaming"),
    ("lichess.org", "Gaming"),
];

/// Best-guess category for a website domain (lower-case, no `www.`).
/// Matches the domain itself or any parent domain (`m.youtube.com` -> youtube.com).
pub fn default_category_for_domain(domain: &str) -> &'static str {
    // Most specific match wins: try the full domain, then each parent.
    let mut d = domain;
    loop {
        if let Some((_, cat)) = DOMAIN_MAP.iter().find(|(k, _)| *k == d) {
            return cat;
        }
        match d.find('.') {
            Some(i) => d = &d[i + 1..],
            None => return "Other",
        }
    }
}

/// Exe stems and domains that moved from Education to Study in schema v4.
pub const STUDY_EXES: &[&str] = &[
    "anki.exe",
    "zotero.exe",
    "mendeley.exe",
    "kindle.exe",
    "duolingo.exe",
];
pub const STUDY_DOMAINS: &[&str] = &[
    "coursera.org",
    "udemy.com",
    "edx.org",
    "khanacademy.org",
    "duolingo.com",
    "leetcode.com",
    "scholar.google.com",
];

/// Browser family for a foreground executable, matching what the extension
/// reports (`chrome`, `edge`, `brave`, `firefox`, `opera`, `vivaldi`).
pub fn browser_family(exe_name: &str) -> Option<&'static str> {
    let lower = exe_name.to_ascii_lowercase();
    let stem = lower.strip_suffix(".exe").unwrap_or(&lower);
    Some(match stem {
        "chrome" | "chromium" => "chrome",
        "msedge" => "edge",
        "brave" => "brave",
        "firefox" | "librewolf" | "waterfox" | "zen" | "floorp" => "firefox",
        "opera" | "opera_gx" => "opera",
        "vivaldi" => "vivaldi",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domains_map_to_categories() {
        assert_eq!(default_category_for_domain("youtube.com"), "Entertainment");
        assert_eq!(
            default_category_for_domain("m.youtube.com"),
            "Entertainment"
        );
        assert_eq!(
            default_category_for_domain("mail.google.com"),
            "Communication"
        );
        assert_eq!(
            default_category_for_domain("docs.google.com"),
            "Productivity"
        );
        assert_eq!(default_category_for_domain("google.com"), "Other");
        assert_eq!(default_category_for_domain("en.wikipedia.org"), "Education");
        assert_eq!(default_category_for_domain("example.org"), "Other");
    }

    #[test]
    fn browsers_detected() {
        assert_eq!(browser_family("chrome.exe"), Some("chrome"));
        assert_eq!(browser_family("MSEDGE.EXE"), Some("edge"));
        assert_eq!(browser_family("brave.exe"), Some("brave"));
        assert_eq!(browser_family("firefox.exe"), Some("firefox"));
        assert_eq!(browser_family("code.exe"), None);
    }

    #[test]
    fn known_apps_map_to_categories() {
        assert_eq!(default_category_for("code.exe"), "Development");
        assert_eq!(default_category_for("winword.exe"), "Productivity");
        assert_eq!(default_category_for("discord.exe"), "Communication");
        assert_eq!(default_category_for("steam.exe"), "Gaming");
        assert_eq!(default_category_for("genshinimpact.exe"), "Gaming");
        assert_eq!(
            default_category_for("javaw.exe"),
            "Other",
            "Java apps aren't all games"
        );
        assert_eq!(default_category_for("something-new.exe"), "Other");
    }

    #[test]
    fn every_default_target_exists() {
        let names: Vec<&str> = DEFAULT_CATEGORIES.iter().map(|c| c.0).collect();
        for exe in [
            "code.exe",
            "excel.exe",
            "anki.exe",
            "slack.exe",
            "vlc.exe",
            "tiktok.exe",
            "steam.exe",
            "x.exe",
        ] {
            assert!(names.contains(&default_category_for(exe)));
        }
    }
}
