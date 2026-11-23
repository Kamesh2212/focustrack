# FocusTrack for Android (preview)

The Android app is the same Tauri app as Windows: the same screens, database, analytics and end-to-end-encrypted
sync. What differs:

| | Windows | Android |
|---|---|---|
| Screen time | Foreground window, every second (`tracker.rs`) | Android **Usage access** events, imported on start, on return to the app and every minute, and **in the background about every 15 minutes** while the app is closed, followed by a sync (`phone.rs`, plugin `plugins/usage`) |
| Websites | Browser extension | Not tracked (shown from your computers) |
| Blocking | `blocker.rs` | Limits and focus sessions, through an accessibility service the user turns on (#69, see below) |
| Tray, start with Windows, administrator mode, pop-up timer | Yes | No |

The background job is a WorkManager periodic task (`BackgroundImport.kt`). Android reads the events in Kotlin
and passes them to the same Rust code through JNI (`Java_com_focustrack_usage_BackgroundImport_run` in
`phone.rs`), which records them and syncs. Recording takes the database's write lock and re-reads how far the
last import got, so the app and the job can run at the same time without recording anything twice.

To run the job now for testing: `adb shell cmd jobscheduler run -f com.focustrack.app <job id>` (find the id
with `adb shell dumpsys jobscheduler | grep "JOB #.*focustrack"`). WorkManager skips a periodic run that isn't
due yet, so a forced run right after an earlier one may do nothing.

## App blocking (#69)

FocusTrack's Rust code writes a **block plan** (`blocks.json` next to the database): the focus session that
applies on this phone (its own, or one started on another device for it) with the apps its profile blocks, and
each limit on a phone app (or a website limit across devices: its app) with the usage counted so far. It is
rewritten every few seconds while the app runs, after every background import, and after "+15 min".

`BlockerService.kt` is an **accessibility service**. Android tells it which app comes to the front (package
name only; `canRetrieveWindowContent` is off), and `Blocker.kt` decides: it adds the time Android recorded since
the plan was written, so limits apply on time with FocusTrack closed. A blocked app gets FocusTrack's block
screen (`BlockActivity.kt`), with "+15 min" (the same daily allowance as on a computer, through JNI:
`Java_com_focustrack_usage_Blocker_extend`). Warn-then-block limits show a one-minute warning. While the phone
is in use the service also imports, syncs and rewrites the plan every couple of minutes. Never blocked:
FocusTrack, the phone app and emergency calls, Settings, the home screen, the keyboard, system UI and
permission screens.

Google Play policy for accessibility services: FocusTrack shows what the service sees, what it does and that the
data stays on the phone, and only opens Android's accessibility settings after the user agrees (Settings → App
blocking on this phone). The Play listing needs the matching declaration in the Play Console. On Android 13+ a
sideloaded APK must first be allowed "restricted settings" (App info → ⋮) before the service can be turned on.

To try it on the emulator: set a 1-minute limit on an app, open the app, and after a minute the block screen
appears. `adb shell settings put secure enabled_accessibility_services
com.focustrack.app/com.focustrack.usage.BlockerService` turns the service on without the settings screens.

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
