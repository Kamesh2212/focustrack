# FocusTrack 1.0.0 — Test & Audit Report

Date: 2026-11-18 · Scope: desktop app (Tauri/Rust/React), browser extension, local API, build & release.
This report records what was tested before tagging v1.0.0, what was found, and what was fixed.
Open findings are tracked as GitHub issues (labels `bug`, `security`, `enhancement`).

Each finding says how it was established: **verified** (reproduced or confirmed by running code / tests),
**code review** (follows directly from the source) or **needs verification** (likely, not yet reproduced).

## 1. What was tested

| Area | How | Result |
|---|---|---|
| Rust unit tests | `cargo test` — 54 tests: tracker (switching, idle, lock, pause, sleep, midnight, crash flush, websites), storage & migrations, analytics, limits & overrides, blocker rules, profiles, focus/Pomodoro/early-end rules, goals, insights, notes, installed-app discovery, URL/domain validation | ✅ 54/54 pass |
| Lint / format | `cargo clippy --all-targets -D warnings`, `cargo fmt --check`, `tsc` (strict) | ✅ clean (after tidy-up, see §4) |
| Dependency audit | `npm audit`, `cargo audit` | ✅ 0 vulnerabilities after fixing 1 (see §4); 6 "unmaintained" notices |
| Foreground tracking | Ran the app, switched between real apps, inspected the database | ✅ correct names, categories and second-level timings |
| Website tracking | Extension loaded in Brave; browsed; inspected `website_sessions` | ✅ domains recorded and attributed to Brave |
| Extension API security | `curl` with web-page `Origin`, missing `X-FocusTrack`, spoofed `Host`, for heartbeat, summary, notes and goal endpoints | ✅ all rejected with 403 |
| Website limits & notifications | Test limit already exceeded → API returned it as blocked; Windows notification fired once | ✅ |
| App blocking | Built a harmless test program, gave it an exceeded *block* limit, launched it twice | ✅ closed within 9 s both times |
| UI flows | Browser preview with sample data: dashboard customise, goals/to-dos, focus start with profile preview, early-end confirmation, Study, Notes autosave, Limits actions/overrides, Settings | ✅ no console errors |
| Release build | `tauri build` → NSIS + MSI installers | see §5 |

**Not covered (test gaps):** no automated frontend, extension or end-to-end tests; Firefox and Edge not
tried; the side panel not yet tried in Brave; no long-running soak test; installer not run on a clean machine.

## 2. Findings — wellbeing loopholes (the app's core purpose)

| # | Severity | Finding | Basis |
|---|---|---|---|
| W1 | **High** | **Watching videos counts as idle.** After 5 min without keyboard/mouse input, time is idle, so a 2-hour YouTube/Netflix session or a cut-scene counts only ~5 minutes toward screen time and limits. | code review + unit test asserts this behaviour |
| W2 | **High** | **Controller gaming likely counts as idle.** `GetLastInputInfo` does not see XInput/gamepad input, so a game played with a controller can stop counting after 5 minutes. | needs verification |
| W3 | **High** | **Pausing tracking bypasses every limit.** While paused no usage is recorded, so limits can never be reached. Pausing needs no confirmation. | code review |
| W4 | **High** | **Apps are identified by executable name only.** Renaming `steam.exe` escapes its limit; blocking `javaw.exe`/`python.exe` closes every Java/Python program; some games are launched via a generic `launcher.exe` (e.g. Genshin Impact's installer entry) while the game runs as a different exe, so a limit on the launcher doesn't stop the game. | verified (installed-app scan lists Genshin as `launcher.exe`) + code review |
| W5 | Medium | **Subdomains are tracked separately.** `m.youtube.com` / `music.youtube.com` time does not count toward a `youtube.com` limit (blocking does match subdomains). | code review |
| W6 | Medium | **Private windows are neither tracked nor blocked**, and browsers without the extension (or with it disabled) bypass website limits and focus blocking. | code review |
| W7 | Medium | **Quitting FocusTrack or ending it in Task Manager lifts all blocking**, including "strict" sessions. Intentional escape hatch, but there is no friction, record or accountability. | code review |
| W8 | Medium | **Changing the system clock/date resets daily limits.** | code review |
| W9 | Medium | **Focus sessions don't survive a restart.** The timer and blocking live in memory; after a crash or reboot the session silently ends. | code review |
| W10 | Medium | **Blocked apps with unsaved work can lose it.** Apps get 8 s after the close request before being terminated; a "Save changes?" dialog left open is killed. (*Warn, then block* gives 60 s.) | code review |
| W11 | Low | Productivity split and insights ignore website categories — all browser time is "neutral". | code review |
| W12 | Low | Only the foreground window counts; a video on a second monitor isn't tracked. | code review |
| W13 | Low | "Start with Windows" is off by default, so tracking stops after a reboot until the user opens the app. | code review |

## 3. Findings — security & privacy

| # | Severity | Finding | Basis |
|---|---|---|---|
| S1 | Medium | **Any installed browser extension can use the local API**: read today's usage summary (sites, apps, goals, limits), send fake heartbeats, toggle goals and create notes. The API accepts any `chrome-extension://`/`moz-extension://` origin. Fix: pair the extension with a token, or pin extension IDs. | code review |
| S2 | Medium | **Any local process can use the local API** (no `Origin` from non-browser clients). On a shared PC another Windows user can read the summary, and only the first user's FocusTrack can bind port 17865. | code review |
| S3 | Low | **No Content-Security-Policy** in the app webview (`"csp": null`). React escapes output and notes render as text, but a CSP is defence-in-depth. | code review |
| S4 | Low | **Data is stored unencrypted** (`%APPDATA%\com.focustrack.app\focustrack.db`), including notes. The SRS asks to encrypt sensitive information where appropriate. The database is in the *roaming* profile, so it can sync on domain-joined PCs. | code review |
| S5 | Low | **Unsigned installers** trigger SmartScreen warnings and can't be verified beyond the published SHA-256 checksums. | release |
| S6 | Info | Webview can call every app command (no per-command capability scoping). Acceptable while the webview loads only bundled code. | code review |
| S7 | Info | Firefox MV3 may require the user to grant the `127.0.0.1` host permission manually before the extension can connect. | needs verification |

No SQL injection paths were found (all queries are parameterised; the only formatted SQL uses fixed table names).
`open_url` only opens `http(s)` links; the blocked page and extension UI insert text via `textContent`.

## 4. Fixed before release

- **RUSTSEC-2026-0459** (bundled SQLite CVE, medium 6.8): upgraded `rusqlite` 0.32 → 0.43 (`libsqlite3-sys` 0.40).
- Code formatted with `rustfmt`; 25 clippy findings resolved (style only, no behaviour change) so CI can enforce
  `clippy -D warnings` and `fmt --check`.
- Version unified at 1.0.0; the app now shows its version from `package.json`.

No functional changes were made after testing — open findings are left for follow-up PRs.

## 5. Release build & resource use

`tauri build` produced `FocusTrack_1.0.0_x64-setup.exe` (2.1 MB, NSIS) and an MSI (2.8 MB); the app binary
is 5.5 MB. The release build started minimised, migrated the existing database, tracked apps and served the
extension API (`/v1/status` → version 1.0.0).

Resource use of the release build, minimised to the tray, sampled over 60 s on the development machine
(Windows 11) while the user worked normally:

| Process | CPU | Memory |
|---|---|---|
| `focustrack.exe` (tracker, API, all loops) | 0.4 % of one core | 28 MB working set (7 MB private) |
| WebView2 processes for the (hidden) window | — | ~125 MB working set |

The backend meets the "minimal CPU/RAM" goal. The WebView stays loaded while the window is hidden; destroying
it on close and recreating it on open would save most of that memory (tracked in *"Measure and document
resource usage"*). The installer was not run on a clean machine.

## 6. Missing or partial versus the SRS / improvements document

| Item | Status |
|---|---|
| Exclude selected apps/websites from tracking (SRS 3.5) | Missing |
| Automatic data retention (SRS 3.5) | Manual only (delete older than N days) |
| Encrypt sensitive information (SRS 6.3) | Missing (S4) |
| Notification type toggles (SRS 2.9) | One master switch only |
| Weekday vs weekend / monthly charts (SRS 2.4) | Weekday/weekend averages only; no monthly chart |
| Achievements & streaks (SRS 3.3) | Focus streak only |
| AI insights (SRS 3.1) | Rule-based insights only |
| Drag-to-reposition widgets (improvements §1) | Show/hide + up/down ordering |
| Notes ↔ tasks association (improvements §6) | Notes link to sessions, not goals |
| Cloud sync / cross-device (SRS 9 Phase 5) | Missing — see `docs/ROADMAP.md` |

## 7. Resolved in 1.1.0

| Finding | Fix |
|---|---|
| W1 videos counted as idle | Foreground app playing sound counts as activity (#1, PR #40) |
| W2 controller gaming idle | XInput controller input counts as input (#3, PR #40) |
| W3 pause bypassed limits | Limited apps/sites keep counting and stay blocked while paused (#4, PR #41) |
| W4 exe-name identity | Path identity for generic names, launcher folder scope, renamed copies (#5, PR #42) |
| W5 subdomains | Website limits include subdomains (#8, PR #42) |
| Admin games couldn't be blocked (Genshin Impact) | Administrator mode (PR #42) |
| Extension "app not running" in Firefox / sometimes Chrome, Brave | CORS + `null` origin handling, heartbeat status fixes (PR #39) |
