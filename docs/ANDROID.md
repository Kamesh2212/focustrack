# FocusTrack for Android (preview)

The Android app is the same Tauri app as Windows: the same screens, database, analytics and end-to-end-encrypted
sync. What differs:

| | Windows | Android |
|---|---|---|
| Screen time | Foreground window, every second (`tracker.rs`) | Android **Usage access** events, imported on start, on return to the app and every minute (`phone.rs`, plugin `plugins/usage`) |
| Websites | Browser extension | Not tracked (shown from your computers) |
| Blocking | `blocker.rs` | Not yet (#69) |
| Tray, start with Windows, administrator mode, pop-up timer | Yes | No |

The usage plugin's Kotlin part (`plugins/usage/android`) reads `UsageStatsManager` events and app names (only for
apps with a launcher icon) and measures the status/navigation bars, which the WebView doesn't report to CSS.

## Build and run on the emulator

Needs the Android SDK and NDK, JDK 17+ (Gradle 9 runs on JDK 25), and the Rust Android targets
(`rustup target add x86_64-linux-android aarch64-linux-android`).

```powershell
$env:ANDROID_HOME = "$env:LOCALAPPDATA\Android\Sdk"
$env:NDK_HOME = "$env:ANDROID_HOME\ndk\27.0.12077973"
npx tauri android init                               # once; generates src-tauri/gen/android (not committed)
Copy-Item -Recurse -Force src-tauri\icons\android\* src-tauri\gen\android\app\src\main\res\
npx tauri android build --debug --apk --target x86_64
adb install -r src-tauri\gen\android\app\build\outputs\apk\universal\debug\app-universal-debug.apk
adb reverse tcp:8787 tcp:8787                        # the phone reaches the local sync server as 127.0.0.1:8787
```

Then open FocusTrack on the phone, tap **Allow** to grant Usage access, and sign in under Settings → Account &
Sync (the same account as your computers). For a quick test, usage access can also be granted with
`adb shell appops set com.focustrack.app GET_USAGE_STATS allow`.

Debug builds keep WebView debugging on: `chrome://inspect` on the PC shows the app's page.
