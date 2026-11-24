//! Reads which app was in the foreground, and when, from the phone's
//! operating system (Android `UsageStatsManager`, which needs the user to grant
//! "Usage access" in system settings). FocusTrack turns these events into
//! usage sessions in its own database.
//!
//! On other platforms the plugin does nothing and reports no access.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tauri::plugin::{Builder, TauriPlugin};
use tauri::{Manager, Runtime};

/// Android `UsageEvents.Event` types FocusTrack uses.
pub mod kind {
    pub const ACTIVITY_RESUMED: i32 = 1;
    pub const ACTIVITY_PAUSED: i32 = 2;
    pub const SCREEN_INTERACTIVE: i32 = 15;
    pub const SCREEN_NON_INTERACTIVE: i32 = 16;
    pub const KEYGUARD_SHOWN: i32 = 17;
    pub const KEYGUARD_HIDDEN: i32 = 18;
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UsageEvent {
    /// App package, e.g. `com.google.android.youtube`.
    pub package: String,
    /// See [`kind`].
    pub kind: i32,
    /// Milliseconds since the Unix epoch.
    pub time: i64,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageEvents {
    pub events: Vec<UsageEvent>,
    /// Human-readable app names by package.
    pub labels: HashMap<String, String>,
    /// Home-screen launchers (time on the home screen isn't app use).
    #[serde(default)]
    pub home: Vec<String>,
}

#[cfg(target_os = "android")]
#[derive(Deserialize)]
struct Granted {
    granted: bool,
}

#[cfg(target_os = "android")]
#[derive(Deserialize)]
struct Enabled {
    enabled: bool,
}

/// Heights of the system bars (status bar, navigation bar) in CSS pixels.
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize)]
pub struct Insets {
    pub top: f64,
    pub bottom: f64,
}

pub struct Usage<R: Runtime> {
    #[cfg(target_os = "android")]
    handle: tauri::plugin::PluginHandle<R>,
    #[cfg(not(target_os = "android"))]
    _r: std::marker::PhantomData<fn() -> R>,
}

impl<R: Runtime> Usage<R> {
    /// Whether the user has granted usage access.
    pub fn has_access(&self) -> Result<bool, String> {
        #[cfg(target_os = "android")]
        return self
            .handle
            .run_mobile_plugin::<Granted>("hasAccess", serde_json::json!({}))
            .map(|g| g.granted)
            .map_err(|e| e.to_string());
        #[cfg(not(target_os = "android"))]
        Ok(false)
    }

    /// Opens the system screen where the user grants usage access.
    pub fn open_access_settings(&self) -> Result<(), String> {
        #[cfg(target_os = "android")]
        return self
            .handle
            .run_mobile_plugin::<serde_json::Value>("openAccessSettings", serde_json::json!({}))
            .map(|_| ())
            .map_err(|e| e.to_string());
        #[cfg(not(target_os = "android"))]
        Err("Usage access is only needed on phones".into())
    }

    /// Whether FocusTrack's app blocker (an accessibility service) is on.
    pub fn blocking_enabled(&self) -> Result<bool, String> {
        #[cfg(target_os = "android")]
        return self
            .handle
            .run_mobile_plugin::<Enabled>("blockingStatus", serde_json::json!({}))
            .map(|e| e.enabled)
            .map_err(|e| e.to_string());
        #[cfg(not(target_os = "android"))]
        Ok(false)
    }

    /// Opens the system screen where the user turns the blocker on or off.
    pub fn open_blocking_settings(&self) -> Result<(), String> {
        #[cfg(target_os = "android")]
        return self
            .handle
            .run_mobile_plugin::<serde_json::Value>("openBlockingSettings", serde_json::json!({}))
            .map(|_| ())
            .map_err(|e| e.to_string());
        #[cfg(not(target_os = "android"))]
        Err("App blocking settings are only on phones".into())
    }

    /// Tells the background job where the app's data lives and schedules it
    /// (about every 15 minutes, while the app is closed too).
    pub fn enable_background(&self, data_dir: &str) -> Result<(), String> {
        #[cfg(target_os = "android")]
        return self
            .handle
            .run_mobile_plugin::<serde_json::Value>(
                "enableBackground",
                serde_json::json!({ "dataDir": data_dir }),
            )
            .map(|_| ())
            .map_err(|e| e.to_string());
        #[cfg(not(target_os = "android"))]
        {
            let _ = data_dir;
            Ok(())
        }
    }

    /// System bar sizes, so the UI can keep clear of them (the WebView doesn't
    /// report them to CSS when the app is drawn edge to edge).
    pub fn insets(&self) -> Result<Insets, String> {
        #[cfg(target_os = "android")]
        return self
            .handle
            .run_mobile_plugin::<Insets>("insets", serde_json::json!({}))
            .map_err(|e| e.to_string());
        #[cfg(not(target_os = "android"))]
        Ok(Insets::default())
    }

    /// Foreground and screen events between two times (Unix milliseconds).
    pub fn events(&self, since_ms: i64, until_ms: i64) -> Result<UsageEvents, String> {
        #[cfg(target_os = "android")]
        return self
            .handle
            .run_mobile_plugin::<UsageEvents>(
                "events",
                serde_json::json!({ "since": since_ms, "until": until_ms }),
            )
            .map_err(|e| e.to_string());
        #[cfg(not(target_os = "android"))]
        {
            let _ = (since_ms, until_ms);
            Ok(UsageEvents::default())
        }
    }
}

pub trait UsageExt<R: Runtime> {
    fn usage(&self) -> &Usage<R>;
}

impl<R: Runtime, T: Manager<R>> UsageExt<R> for T {
    fn usage(&self) -> &Usage<R> {
        self.state::<Usage<R>>().inner()
    }
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("focustrack-usage")
        .setup(|app, _api| {
            #[cfg(target_os = "android")]
            let usage = Usage {
                handle: _api.register_android_plugin("com.focustrack.usage", "UsagePlugin")?,
            };
            #[cfg(not(target_os = "android"))]
            let usage = Usage::<R> {
                _r: std::marker::PhantomData,
            };
            app.manage(usage);
            Ok(())
        })
        .build()
}
