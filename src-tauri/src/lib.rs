//! FocusTrack desktop app: tray-resident screen time tracker.

mod analytics;
mod browser;
mod categories;
mod commands;
mod db;
mod limits;
mod platform;
mod tracker;

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, WindowEvent};

pub struct AppState {
    pub db: Mutex<rusqlite::Connection>,
    pub shared: Arc<tracker::Shared>,
    pub data_dir: PathBuf,
}

/// Tray "Pause/Resume tracking" item, kept so its label follows the setting.
pub struct TrayPause(pub MenuItem<tauri::Wry>);

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Lets the tracker close its open session before the process exits.
fn quit(app: &AppHandle) {
    let shared = app.state::<AppState>().shared.clone();
    shared.shutdown.store(true, Ordering::SeqCst);
    for _ in 0..30 {
        if shared.stopped.load(Ordering::SeqCst) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    app.exit(0);
}

fn build_tray(app: &tauri::App, paused: bool) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open FocusTrack", true, None::<&str>)?;
    let pause = MenuItem::with_id(
        app,
        "pause",
        if paused { "Resume tracking" } else { "Pause tracking" },
        true,
        None::<&str>,
    )?;
    let sep = PredefinedMenuItem::separator(app)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit FocusTrack", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &pause, &sep, &quit_item])?;
    app.manage(TrayPause(pause));

    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("FocusTrack")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_main(app),
            "pause" => {
                let paused = app.state::<AppState>().shared.paused.load(Ordering::SeqCst);
                if let Err(e) = commands::apply_paused(app, !paused) {
                    eprintln!("[tray] pause toggle failed: {e}");
                }
            }
            "quit" => quit(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // A second launch just focuses the running instance (no double tracking).
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main(app)))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let db_path = data_dir.join("focustrack.db");

            let conn = db::open(&db_path)?;
            let settings = db::load_settings(&conn)?;
            let shared = Arc::new(tracker::Shared::new(settings.paused, settings.idle_threshold_secs));
            shared.website_tracking.store(settings.website_tracking, Ordering::SeqCst);
            shared.notifications_enabled.store(settings.notifications_enabled, Ordering::SeqCst);
            shared.break_reminder_mins.store(settings.break_reminder_mins, Ordering::SeqCst);
            tracker::spawn(db_path.clone(), shared.clone());
            browser::spawn_server(shared.clone());
            limits::spawn(app.handle().clone(), db_path, shared.clone());

            app.manage(AppState { db: Mutex::new(conn), shared, data_dir });
            build_tray(app, settings.paused)?;

            // Launched at Windows sign-in: stay in the tray.
            if !std::env::args().any(|a| a == "--minimized") {
                show_main(app.handle());
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window keeps tracking running in the tray.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_dashboard,
            commands::get_live_status,
            commands::get_apps,
            commands::get_categories,
            commands::set_app_category,
            commands::rename_app,
            commands::get_report,
            commands::get_history,
            commands::export_csv,
            commands::get_settings,
            commands::set_paused,
            commands::set_idle_threshold,
            commands::set_autostart,
            commands::set_theme,
            commands::delete_all_data,
            commands::delete_data_older_than,
            commands::set_notifications_enabled,
            commands::set_break_reminder,
            commands::set_website_tracking,
            commands::send_test_notification,
            commands::get_websites,
            commands::set_website_category,
            commands::get_browser_status,
            commands::get_limits,
            commands::set_limit,
            commands::delete_limit,
        ])
        .run(tauri::generate_context!())
        .expect("error while running FocusTrack");
}
