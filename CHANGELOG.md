# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org).

## [Unreleased]

### Added
- **Your synced data** (Settings → Account & Sync, #67): export everything in your account as decrypted JSON;
  choose how long the server keeps history (30 days to 2 years, or forever); delete synced history (everything
  or older than a period) from the server and all your devices; delete one device's history; delete the
  account (asks for your password).
- Sync server: retention periods, applied hourly, and an export option to download your own records.
- **Device management** (#66): each device shows whether it's online, when it last synced, its last activity,
  whether tracking is paused and any sync problem. Pause or resume tracking on another device, or turn its sync
  off, from this one; it applies at that device's next sync. Database schema v7.
- **Limits across devices** (#70): a limit can count usage on all your synced devices instead of just this
  one; a website limit then also counts the site's phone app (youtube.com counts the YouTube app). The limits
  list shows how much came from other devices. Other devices' time arrives when they sync, so blocking can lag
  by a few minutes. Database schema v8.
- **Blocking profiles and categories sync** (#62): profiles you create, edit or delete, and the category you
  pick for an app or website, now sync to all your devices. If two devices edit the same profile while
  offline, the later edit wins; two new profiles with the same name are both kept. Database schema v9.
- **Focus sessions across devices** (#71): when starting a focus or study session, choose to run it on this
  device, all your devices, or ones you pick. The other devices join at their next sync (devices sync every
  15 seconds while such a session runs), show the same timer and apply the same blocking profile. Ending it on
  any device, following the profile's rules, ends it everywhere. The session is recorded once, on the device
  that started it. Database schema v10.
- **Goals across devices** (#72): goal progress counts every synced device, and with more than one device
  each goal shows how much came from each. "Goal achieved", "goal missed" and the evening reminder appear on
  one device only. Changing a goal lets it notify again. Database schema v11.
- **App blocking on Android** (#69): limits and focus sessions now block apps on the phone, with FocusTrack's
  block screen and "+15 min" like on a computer, also while FocusTrack is closed. It uses an accessibility
  service that only learns which app is open; FocusTrack explains this and asks before you turn it on
  (Settings → App blocking on this phone). The phone app, emergency calls and Settings are never blocked.

## [1.2.0] - 2026-11-19

Pre-release for testing. Multi-device sync and the Android app are **previews**: sync needs a FocusTrack sync
server that you run yourself (see [docs/SYNC-SETUP.md](docs/SYNC-SETUP.md)).

### Added
- **Multi-device sync (preview)**: sign in under Settings → Account & Sync to combine screen time from all your
  devices. End-to-end encrypted: devices encrypt everything before upload and the server can't read it. Create an
  account (you get a recovery key), sign in on your other devices, rename or remove devices, sign out.
- **All Devices dashboard and reports**: filter by all devices, computers, phones or one device; combined and
  *unique* screen time (time on two devices at once counted once); time per device and how recent its data is.
- **Android app (preview)**: screen time from Android's Usage access, imported in the app and in the background
  about every 15 minutes, synced with your computers; phone-friendly layout. No app blocking on Android yet.
- **FocusTrack sync server** (`focustrack-server`) to run on your own computer or server.


### Fixed
- Focus and study sessions survive a crash, restart or quitting FocusTrack: the session and its blocking pick up
  where they left off, and a round that ended while FocusTrack wasn't running keeps only the time it saw (#12).
- Changing the system clock no longer resets daily limits: until the real day ends, limits keep counting today's
  usage. Clock changes are recorded. Sleep, time sync, daylight saving and time-zone changes aren't affected (#11).
- Blocked apps showing a "Save changes?" dialog are never force-closed: FocusTrack waits for you to answer it,
  then asks the app to close again (#13).
- The productivity summary, reports and insights count browser time by website category, so YouTube is
  Entertainment and GitHub is Development instead of both being neutral (#14).
- Two different programs with the same file name (two `javaw.exe` or `launcher.exe` from different folders) are
  no longer merged when you switch between them.
- "Warn me at" now also warns for limits set to *Block at the limit*, before the app or site is blocked.
- Deleting data older than N days also removes that period's limit extensions and goal check-offs.
- Notes are no longer re-saved (and moved to the top) just by opening and leaving them, and a note deleted
  while an edit was pending isn't saved again.

### Changed
- Database schema v6 (records get IDs and device tags for sync); upgrades automatically.
- Reorder dashboard sections and side-panel widgets by drag and drop instead of one step at a time with
  arrows. The drag handle also moves an item with the arrow keys.

## [1.1.0] - 2026-11-18

### Added
- **Focus music**: built-in rain, ocean, brown, pink and white noise generated live, with volume, auto-play with
  sessions, pause during breaks, and a link to your own playlist.
- **Pop-up focus timer**: a small always-on-top window with time left, goal, what's blocked and music controls
  while a focus or study session runs; a notification when a session starts.
- **Administrator mode** (Settings): lets FocusTrack block games that run as administrator, such as Genshin
  Impact, after one Windows permission prompt.

### Fixed
- Watching videos and playing games no longer counts as idle: the app in front playing sound counts as
  activity (#1), and Xbox-compatible controller input counts as input (#3).
- Pausing tracking no longer bypasses limits: apps and websites with a limit keep counting and stay blocked (#4).
- Apps are recognised by identity, not just exe name (#5): programs sharing a name (`javaw.exe`,
  `launcher.exe`) are kept apart, a launcher's limit or block covers the games it installs (HoYoPlay →
  Genshin Impact), and renamed copies of limited apps are still caught.
- Games are categorised as Gaming by install folder (Steam, Epic, HoYoPlay, Riot, Battle.net…) and name, so
  Study Mode and Work Mode block them.
- Website limits include subdomains (`m.youtube.com` counts toward `youtube.com`) (#8).
- The browser extension connects reliably in Firefox and no longer reports "app not running" in Chrome or
  Brave because of blocked-tab redirects (#39).

### Changed
- Database schema v5 (applications keyed by identity); upgrades automatically.

## [1.0.0] - 2026-11-18

First public release.

### Added
- Automatic tracking of the foreground app, with idle, lock, sleep and midnight handling.
- Browser extension (Chrome, Edge, Brave, Opera, Vivaldi; Firefox) reporting the active tab's domain, with a
  customisable side panel and popup. Your browser's own new tab page stays untouched.
- Customisable dashboard: screen time, goals and to-dos, focus session, app and website usage, productivity,
  limits, quick note, insights, charts.
- Reports with daily/weekly/monthly ranges, categories, productivity, heatmap; history with CSV export.
- Usage limits for apps and websites: notify only, warn then block, or block, with daily "+15 min" extensions.
- App blocking that closes apps gracefully, then force-closes; never touches Windows components.
- Focus Mode and Study Mode with Pomodoro/custom sessions, blocking profiles (block or allow-only), a preview
  of what will be blocked, and early-end rules.
- Daily goals (measured and custom to-dos) with reminders.
- Notes with notebooks and autosave; study resources.
- Rule-based insights, focus streaks, Windows notifications, break reminders.
- Light/dark themes, start with Windows, system tray, single instance.

### Security
- Upgraded bundled SQLite (`rusqlite` 0.43 / `libsqlite3-sys` 0.40) for RUSTSEC-2026-0459.

### Known issues
- See [docs/AUDIT-1.0.0.md](docs/AUDIT-1.0.0.md) and the issue tracker.

[1.2.0]: https://github.com/Kamesh2212/focustrack/releases/tag/v1.2.0
[1.1.0]: https://github.com/Kamesh2212/focustrack/releases/tag/v1.1.0
[1.0.0]: https://github.com/Kamesh2212/focustrack/releases/tag/v1.0.0
