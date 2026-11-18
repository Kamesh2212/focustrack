# FocusTrack

Windows screen time & digital wellbeing app — like Screen Time / Digital Wellbeing, for your laptop.
Built with **Tauri 2 + React + TypeScript + Rust + SQLite** per the [requirements spec](docs/SRS.md).

All data stays on your PC. FocusTrack records which app is in the foreground and when, idle time, and
(with the browser extension) the domain of the active tab. It never records keystrokes, window contents,
screenshots, passwords, full URLs or page content.

## Status

| Phase | Scope | State |
|---|---|---|
| 1 · Core tracking | Foreground app detection, idle/lock/sleep detection, session recording, SQLite, start with Windows, tray | **Implemented** |
| 2 · Dashboard & analytics | Dashboard, app breakdown & categories, daily/weekly reports, charts, heatmap, history, CSV export, settings | **Implemented** |
| 3 · Advanced tracking | Browser extension, website tracking & categories, app/website daily limits, website blocking, Windows notifications, break reminders | **Implemented** |
| 4 · Productivity | Focus mode / Pomodoro, goals, insights | Planned (placeholder screen) |
| 5 · Advanced | AI insights, streaks, sync | Planned |

## How tracking works

`src-tauri/src/tracker.rs` runs on its own thread and polls once a second:

- **Foreground app**: `GetForegroundWindow` → process id → exe path. Store apps hosted in `ApplicationFrameHost.exe` resolve to the real app. Display names come from the exe's `FileDescription`.
- **Idle**: `GetLastInputInfo` (time of last input only, not the input itself). After the idle threshold (default 5 min), the app session is trimmed back to the last input and an idle period starts.
- **Locked**: the lock screen (`LockApp.exe`) or an unswitchable input desktop. Nothing is recorded.
- **Sleep / hibernate**: a jump of more than 20 s in wall-clock time between polls. The open session is closed at the last poll before the jump.
- **Reliability**: rows are inserted when a session starts and updated in place every 5 s, so a crash loses at most 5 s and never duplicates rows. Sessions are split at midnight. A single-instance guard prevents double tracking.

Screen time = active + idle. Locked, asleep and paused time is excluded.

## Website tracking

The extension in [`extension/`](extension) (Manifest V3; Chrome, Edge, Brave, Opera, Vivaldi, Firefox 121+)
sends the active tab's **domain only** to `http://127.0.0.1:17865/v1/heartbeat` when the tab or window focus
changes and every 30 s. Private/incognito tabs are never reported. The desktop app counts website time only
while that browser is the foreground app and you're not idle, so it's always a subset of the browser's app time.

The local API (`src-tauri/src/browser.rs`) listens on 127.0.0.1 only and rejects requests from web pages
(non-extension `Origin`, missing `X-FocusTrack` header, or a foreign `Host`).

**Install the extension (development):**

- Chrome / Edge / Brave: open `chrome://extensions` (`edge://`, `brave://`), enable **Developer mode**,
  click **Load unpacked**, select the `extension` folder.
- Firefox: `about:debugging` → This Firefox → **Load Temporary Add-on** → `extension/manifest.json`
  (temporary until Firefox restarts; a signed build is needed for permanent install).

## Limits and notifications

`src-tauri/src/limits.rs` checks every 15 s. Each enabled limit notifies once per day at its warning level
(default 80%) and once when reached. Website limits can also **block** the site: the extension redirects it
to a "blocked for today" page until midnight. App limits notify but don't close the app. An optional break
reminder fires after N minutes of continuous use (a 5-minute gap resets it).

## Project layout

```
src/                     React UI
  pages/                 Dashboard, Applications, Websites, Reports, Usage Limits, History, Settings
  components/            UI kit + Recharts charts
  lib/api.ts             Typed wrappers over Tauri commands (falls back to sample data in a browser)
src-tauri/src/
  tracker.rs             Background tracking engine: apps, websites, idle (+ tests with a fake OS probe)
  browser.rs             Local HTTP API for the browser extension
  limits.rs              Daily limits, blocklist, notifications, break reminders
  platform.rs            Win32 calls (windows-sys)
  db.rs                  SQLite schema, migrations, writes
  analytics.rs           Aggregations: totals, per-day, per-app, hourly, heatmap, sessions
  commands.rs            Tauri commands used by the UI
  categories.rs          Built-in categories and exe → category defaults
  lib.rs                 App setup, tray menu, autostart, single instance
extension/               Browser extension (MV3)
docs/SRS.md              Requirements
```

Database location: `%APPDATA%\com.focustrack.app\focustrack.db`.

## Development

Prerequisites (Windows 10/11):

1. Node.js 20+
2. Rust via [rustup](https://rustup.rs) (stable, MSVC toolchain)
3. Visual Studio 2022 Build Tools with **Desktop development with C++**
4. WebView2 (preinstalled on Windows 11)

```bash
npm install
npm run app:dev      # run the desktop app with hot reload
npm run app:build    # build installers (NSIS + MSI) into src-tauri/target/release/bundle
cd src-tauri && cargo test   # tracker / storage / analytics tests
```

If the build fails with `LNK1104: cannot open file ...build_script_build.exe`, the project path is too long
for the MSVC linker (260-character limit). Point Cargo at a short build directory:

```bash
export CARGO_TARGET_DIR="C:/Users/$USERNAME/.focustrack-target"
```

UI only, without Rust (uses sample data):

```bash
npm run dev          # http://localhost:1420
```
