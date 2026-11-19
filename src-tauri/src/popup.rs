//! The small always-on-top focus timer that pops up while a focus or study
//! session runs. It loads the same UI as the main window at `#mini`.

use tauri::{AppHandle, Manager};
#[cfg(desktop)]
use tauri::{PhysicalPosition, WebviewUrl, WebviewWindowBuilder};

pub const LABEL: &str = "mini";
const WIDTH: f64 = 320.0;
const HEIGHT: f64 = 200.0;

/// Shows the timer pop-up (bottom-right of the screen), creating it if needed.
/// Phones have no floating windows; the app's own focus screen is used instead.
#[cfg(mobile)]
pub fn open(_app: &AppHandle) {}

/// Shows the timer pop-up (bottom-right of the screen), creating it if needed.
#[cfg(desktop)]
pub fn open(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.show();
        return;
    }
    let built = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html#mini".into()))
        .title("FocusTrack timer")
        .inner_size(WIDTH, HEIGHT)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false)
        .build();
    match built {
        Ok(w) => {
            if let (Ok(Some(monitor)), Ok(size)) = (w.current_monitor(), w.outer_size()) {
                let area = monitor.work_area();
                let x = area.position.x + area.size.width as i32 - size.width as i32 - 24;
                let y = area.position.y + area.size.height as i32 - size.height as i32 - 24;
                let _ = w.set_position(PhysicalPosition::new(x, y));
            }
        }
        Err(e) => eprintln!("[popup] could not open the focus timer: {e}"),
    }
}

/// Removes the pop-up (when the session ends).
pub fn close(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.destroy();
    }
}

/// Short "what's blocked" text for the start notification.
pub fn blocked_summary(p: &crate::profiles::BlockPreview) -> String {
    if p.mode == "allow_only" {
        let n = p.allowed_apps.len();
        return format!(
            "Only {n} approved app{} and their websites are allowed",
            if n == 1 { "" } else { "s" }
        );
    }
    let mut names: Vec<&str> = p
        .blocked_apps
        .iter()
        .map(|a| a.name.as_str())
        .take(3)
        .collect();
    let more_apps = p.blocked_apps.len().saturating_sub(names.len());
    let sites = p.blocked_sites.len();
    if names.is_empty() && sites == 0 {
        return "Nothing is blocked".into();
    }
    let mut parts = Vec::new();
    if !names.is_empty() {
        if more_apps > 0 {
            names.push("…");
        }
        parts.push(names.join(", "));
    }
    if sites > 0 {
        parts.push(format!(
            "{sites} website{}",
            if sites == 1 { "" } else { "s" }
        ));
    }
    format!("{} blocked", parts.join(" and "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profiles::{AppRef, BlockPreview};

    fn app(name: &str) -> AppRef {
        AppRef {
            name: name.into(),
            exe_name: format!("{name}.exe"),
            category: "Gaming".into(),
            path: None,
        }
    }

    #[test]
    fn summary_text() {
        let p = BlockPreview {
            mode: "block".into(),
            blocked_apps: vec![app("Steam"), app("Genshin Impact")],
            blocked_sites: vec!["youtube.com".into(); 12],
            ..Default::default()
        };
        assert_eq!(
            blocked_summary(&p),
            "Steam, Genshin Impact and 12 websites blocked"
        );
        let many = BlockPreview {
            blocked_apps: vec![app("A"), app("B"), app("C"), app("D")],
            ..p.clone()
        };
        assert_eq!(blocked_summary(&many), "A, B, C, … and 12 websites blocked");
        assert_eq!(
            blocked_summary(&BlockPreview {
                mode: "block".into(),
                ..Default::default()
            }),
            "Nothing is blocked"
        );
    }
}
