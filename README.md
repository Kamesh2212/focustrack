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
| 4 · Productivity | Focus Mode (Pomodoro, presets, custom, distraction blocking, history, streaks), daily goals, productivity reports, activity heatmap, usage insights | **Implemented** |
| 4.5 · Improvements | Side-panel widgets (your own new tab is kept), real app blocking, limit actions & overrides, custom goals and to-dos on the dashboard, customizable dashboard, Study category & Study page, blocking profiles, notes, study resources | **Implemented** |
| 5 · Advanced | AI insights, achievements, sync | Planned |

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

**What the extension shows:** your browser's own new-tab page is left untouched (browsers don't let extensions
add to the built-in new tab). Instead, FocusTrack widgets live in the browser's **side panel**, which can stay open
beside any page, including your new tab. Choose and reorder widgets with **Customize**: screen time, goals & to-dos
(tick them off there), focus session, website usage, app usage, productivity summary, limits and a quick note.
The toolbar popup shows your first two widgets and opens the side panel. Data comes from `GET /v1/summary`;
`POST /v1/goals/toggle` and `POST /v1/notes` handle ticks and quick notes (extension-only, like the rest of the API).

## Focus Mode, goals and insights

- **Focus sessions** (`src-tauri/src/focus.rs`): presets for a 25-minute Pomodoro (4 rounds, 5-minute breaks,
  15-minute long break), 50-minute deep work and a 90-minute study session, or a custom length, breaks and
  rounds. The timer runs in the backend, so it keeps going with the window closed, and sends a notification
  at each round, break and finish. Optionally blocks distracting websites (entertainment, social media and
  gaming, adjusted by your own website categories) during focus rounds only.
- **Daily goals** (`goals.rs`): cap total screen time, a minimum or maximum for a category (apps + websites),
  or a number of focus sessions. Notifies once a day when a goal is reached or missed.
- **Insights** (`insights.rs`): plain-language observations computed locally, such as the biggest
  week-over-week changes, your busiest hours, distracting time, monthly productive trend and focus streak.
- **Reports** add a daily productive / neutral / distracting chart.

## Blocking

- **App blocking** (`blocker.rs`): every 2 s, running processes are checked against apps whose limit is used up and
  apps blocked by the current session's profile. A blocked app is first asked to close (so it can offer to save),
  then terminated if still open after 8 s, and closed again if reopened. Windows components, anything under
  `C:\Windows`, File Explorer, Task Manager, terminals and FocusTrack itself are never touched.
- **Limit actions**: *Notify only*, *Warn, then block* (1-minute heads-up), or *Block at the limit*, for apps and
  websites. A blocked limit can be extended by 15 minutes a configurable number of times per day (0 = strict).
- **Blocking profiles** (`profiles.rs`): Study Mode, Work Mode, Deep Focus (allow-only) and Personal are built in;
  create your own. A profile blocks (or, in allow-only mode, allows) categories plus specific apps and websites.
  The Focus and Study pages show exactly which apps and websites will be blocked before you start. Blocking lasts
  the whole session, breaks included unless the profile opens them, and lifts automatically at the end.
- **Ending early**: per profile — allowed, type a confirmation phrase, or strict (request, wait 1 minute, confirm).

## Study, notes and goals

- **Study** category (productive) and a **Study** page: study timer with subject and goal, Study Mode blocking,
  study time stats, saved study resources, and notes for the current subject.
- **Notes** page: notebooks, search, autosave; notes written during a session are linked to it. Quick notes from
  the dashboard and the browser side panel.
- **Goals** on the dashboard: measured goals (screen time, category time, focus sessions) and custom to-dos you tick
  off each day, with an evening reminder for anything still open.
- The **dashboard** is customizable: show, hide and reorder its sections.

## Limits and notifications

`src-tauri/src/limits.rs` checks every 15 s. Each enabled limit notifies once per day at its warning level
(default 80%) and once when reached. Website limits can also **block** the site: the extension redirects it
to a "blocked for today" page until midnight. App limits notify but don't close the app. Any installed app can be
limited, even before first use (`installed.rs` reads Start Menu shortcuts and the installed-programs registry). An optional break
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
  focus.rs               Focus sessions / Pomodoro engine
  goals.rs               Daily goals and progress
  insights.rs            Rule-based usage insights
  installed.rs           Installed-app discovery (Start Menu .lnk parser + registry)
  blocker.rs             App blocking (graceful close, then terminate; protected list)
  profiles.rs            Blocking profiles and what they resolve to
  notes.rs               Notes and study resources
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
