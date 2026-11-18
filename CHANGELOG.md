# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org).

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

[1.1.0]: https://github.com/Kamesh2212/focustrack/releases/tag/v1.1.0
[1.0.0]: https://github.com/Kamesh2212/focustrack/releases/tag/v1.0.0
