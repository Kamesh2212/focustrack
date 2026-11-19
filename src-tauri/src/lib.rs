//! FocusTrack: tray-resident screen time tracker on Windows, and the
//! companion app on Android (same database, analytics, sync and screens).

// `let out = rows.collect(); out` is deliberate: in edition 2021 returning the
// iterator directly keeps the prepared statement borrowed past its lifetime.
#![allow(clippy::let_and_return)]
// Phones don't use the Windows tracker, blocker, tray or browser API.
#![cfg_attr(mobile, allow(dead_code, unused_mut, unused_imports, unused_variables))]

mod admin;
mod analytics;
mod blocker;
mod browser;
mod categories;
mod clock;
mod commands;
mod db;
mod focus;
mod goals;
mod identity;
mod insights;
mod installed;
mod limits;
mod notes;
mod phone;
mod platform;
mod popup;
mod profile;
mod profiles;
mod sync;
mod tracker;

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
#[cfg(desktop)]
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
#[cfg(desktop)]
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, WindowEvent};

pub struct AppState {
    pub db: Mutex<rusqlite::Connection>,
    pub shared: Arc<tracker::Shared>,
    pub data_dir: PathBuf,
    pub sync: Arc<sync::SyncHandle>,
}

/// Tray "Pause/Resume tracking" item, kept so its label follows the setting.
#[cfg(desktop)]
pub struct TrayPause(pub MenuItem<tauri::Wry>);

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        #[cfg(desktop)]
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Lets the tracker close its open session before the process exits.
pub(crate) fn quit(app: &AppHandle) {
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

#[cfg(desktop)]
fn build_tray(app: &tauri::App, paused: bool) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open FocusTrack", true, None::<&str>)?;
    let pause = MenuItem::with_id(
        app,
        "pause",
        if paused {
            "Resume tracking"
        } else {
            "Pause tracking (limits still apply)"
        },
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
    let mut builder = tauri::Builder::default();
    // A second launch just focuses the running instance (no double tracking).
    // Development profiles run side by side as separate devices instead.
    #[cfg(desktop)]
    {
        if profile::name().is_none() {
            builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
                show_main(app)
            }));
        }
        builder = builder.plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ));
    }
    builder
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_focustrack_usage::init())
        .setup(|app| {
            // Administrator mode is on but this copy isn't elevated (e.g. opened
            // from the Start menu): hand over to the administrator copy.
            #[cfg(desktop)]
            if profile::name().is_none() && admin::hand_over_if_needed() {
                std::process::exit(0);
            }
            let data_dir = profile::data_dir(app.path().app_data_dir()?);
            std::fs::create_dir_all(&data_dir)?;
            let db_path = data_dir.join("focustrack.db");

            let conn = db::open(&db_path)?;
            let settings = db::load_settings(&conn)?;
            let shared = Arc::new(tracker::Shared::new(
                settings.paused,
                settings.idle_threshold_secs,
            ));
            shared
                .website_tracking
                .store(settings.website_tracking, Ordering::SeqCst);
            shared
                .notifications_enabled
                .store(settings.notifications_enabled, Ordering::SeqCst);
            shared
                .break_reminder_mins
                .store(settings.break_reminder_mins, Ordering::SeqCst);
            // On Windows the tracker watches the foreground window and the
            // browser extension reports websites; phones import usage from the
            // OS instead (see the usage plugin).
            #[cfg(desktop)]
            {
                tracker::spawn(db_path.clone(), shared.clone());
                browser::spawn_server(shared.clone(), db_path.clone());
            }
            focus::spawn(app.handle().clone(), db_path.clone(), shared.clone());
            #[cfg(desktop)]
            blocker::spawn(app.handle().clone(), shared.clone());
            limits::spawn(app.handle().clone(), db_path.clone(), shared.clone());
            let sync = sync::SyncHandle::new(data_dir.clone());
            sync::refresh_status(&sync, &conn);
            #[cfg(mobile)]
            phone::spawn(app.handle().clone(), db_path.clone(), sync.clone());
            sync::spawn(sync.clone(), db_path, shared.clone());

            app.manage(AppState {
                db: Mutex::new(conn),
                shared,
                data_dir,
                sync,
            });
            #[cfg(desktop)]
            build_tray(app, settings.paused)?;
            if let (Some(name), Some(w)) = (profile::name(), app.get_webview_window("main")) {
                let _ = w.set_title(&format!("FocusTrack — {name}"));
            }

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
            commands::get_installed_apps,
            commands::ensure_app,
            commands::start_focus,
            commands::stop_focus,
            commands::get_focus,
            commands::get_goals,
            commands::set_goal,
            commands::delete_goal,
            commands::get_insights,
            commands::set_overrides_per_day,
            commands::set_goal_reminder,
            commands::set_dashboard_layout,
            commands::override_limit,
            commands::preview_profile,
            commands::request_stop_focus,
            commands::get_profiles,
            commands::save_profile,
            commands::delete_profile,
            commands::toggle_goal,
            commands::get_notebooks,
            commands::get_notes,
            commands::create_note,
            commands::update_note,
            commands::delete_note,
            commands::get_resources,
            commands::add_resource,
            commands::delete_resource,
            commands::open_url,
            commands::get_study_time,
            commands::get_admin_mode,
            commands::show_main_window,
            commands::enable_admin_mode,
            commands::disable_admin_mode,
            commands::sync_status,
            commands::sync_sign_up,
            commands::sync_sign_in,
            commands::sync_sign_out,
            commands::sync_now,
            commands::sync_set_enabled,
            commands::sync_devices,
            commands::sync_remove_device,
            commands::sync_rename_device,
            commands::get_platform,
            commands::get_insets,
            commands::phone_usage_access,
            commands::open_usage_access,
            commands::import_phone_usage,
        ])
        .run(tauri::generate_context!())
        .expect("error while running FocusTrack");
}
