# FocusTrack Roadmap

FocusTrack exists to help people look after their wellbeing: understand their screen time, control
social-media, gaming and streaming habits, and get more focused work done. This roadmap lists what
comes after 1.0.0. Bugs and security items found during testing are in
[`AUDIT-1.0.0.md`](AUDIT-1.0.0.md) and tracked as GitHub issues. Want to work on something? Comment on
the issue first so we don't duplicate effort.

## 1.x — close the loopholes (highest priority)

These make limits and focus sessions trustworthy, which matters more than new features.

- **Count media and controller time** (W1, W2): treat the foreground app as active while it plays audio
  (Windows audio sessions) or receives gamepad input (XInput / Windows.Gaming.Input), or while it's
  full-screen.
- **Stable app identity** (W4): identify apps by path, publisher/signature and product name, not just the exe
  name; let users group several exes into one app.
- **App groups** ("Genshin Impact" = launcher + game exe + hoyolab.com): one limit across apps and websites.
- **Tamper resistance, with an escape hatch** (W3, W7, W8): confirmation + cool-down to pause tracking or quit
  during a session; record overrides; optional PIN; detect clock changes; optional watchdog service.
- **Persist focus sessions across restarts** (W9).
- **Safer closing** (W10): don't force-close while a save dialog is open; give a longer, visible countdown.
- **Subdomain roll-up** for website limits (W5); **incognito** blocking when the user allows the extension
  there (W6); warn when a browser is used without the extension.
- **Secure the local API** (S1, S2): pair the extension with a per-install token shown in Settings; per-user
  port.

## 1.x — wellbeing features

- **Schedules:** downtime/bedtime, school or work hours, per-weekday limits, weekly budgets.
- **Mindful friction:** an "Are you sure?" pause screen with a breathing countdown before opening a
  distracting app or site; optional grayscale during downtime.
- **Urge tracking:** count blocked attempts and show them ("You reached for Instagram 14 times today").
- **Mood & energy check-ins** linked to usage, to show how habits feel, not just how long they last.
- **Health breaks:** 20-20-20 eye-strain reminders, stand-up and hydration nudges.
- **Achievements & streaks** (SRS 3.3): days under limit, focus streaks, "reduced gaming 20%" badges.
- **Weekly review:** a summary every Sunday, with a goal for the next week.
- **Accountability partner / family mode:** share a weekly summary or let a trusted person hold the PIN.
- **Exclusions & retention** (SRS 3.5): exclude apps/sites from tracking; automatic data retention.
- **Notification controls** per type (SRS 2.9).
- **Notes upgrades:** Markdown formatting, attachments, linking notes to goals/tasks, export.
- **Insights v2:** optional on-device or opt-in cloud AI summaries (SRS 3.1), never sending raw data
  without consent.

## 2.0 — multiple devices

### The problem

A limit on one device is easy to sidestep: block Genshin Impact on the laptop after an hour and the user
picks up their phone and keeps playing. Wellbeing limits have to follow the **person**, not the device.

### Proposal: account-based, end-to-end-encrypted sync

```
 Windows (FocusTrack)      Android app             iOS app            macOS/Linux
        │                       │                     │                    │
        └──── encrypted usage events, limits, goals, profiles ─────────────┘
                                 │
                       Sync service (stateless relay +
                       encrypted blob store; self-hostable)
```

1. **Identity & devices.** A user creates an account and links devices with a QR code. Each device holds a
   key; data is encrypted on the device (E2EE) so the server only stores ciphertext. A self-hosted server and a
   no-account local-network sync mode are options for privacy-first users.
2. **Shared definitions.** Limits, goals, focus profiles and **app groups** sync to every device. An app group
   maps platform-specific identities to one thing: `Genshin Impact` = `GenshinImpact.exe` (Windows) +
   `com.miHoYo.GenshinImpact` (Android) + iOS bundle ID + `hoyolab.com`.
3. **Shared budget.** Each device records usage as append-only events per app group. Today's usage is the sum
   across devices, using a conflict-free counter (each device owns its own counter; totals merge cleanly
   offline). When the combined total reaches the limit, every device enforces it.
4. **Enforcement per platform.**
   - Windows: the existing blocker.
   - Android: `UsageStatsManager` for usage; an Accessibility service or a Device Owner/Family Link-style
     policy to block. Must follow Google Play policy for these APIs.
   - iOS: Apple's Screen Time APIs (FamilyControls, DeviceActivity, ManagedSettings). These need Apple's Family
     Controls entitlement and run as an app plus extensions.
   - macOS: Screen Time APIs / Endpoint Security; Linux: per-desktop-environment.
5. **Offline behaviour.** Each device enforces with what it knows and reconciles when it reconnects. A
   "minimum share" rule (e.g. each device may use its share of the remaining budget while offline) limits
   overspending.
6. **Cross-device focus sessions.** Starting a session on one device starts it everywhere (blocking the same
   profile), and ending it ends it everywhere.
7. **Family / accountability.** Optional guardian roles with consent, visible to the user, never covert.

### Milestones

1. Local export/import (JSON) and a documented event format.
2. Sync service + Windows client (multi-PC) with E2EE.
3. Android companion: usage + shared limits + blocking.
4. iOS companion (subject to Apple entitlement).
5. Web dashboard (read-only, E2EE in the browser).

## Platform & quality

- Code-signed installers and an auto-updater.
- macOS and Linux desktop builds (Tauri supports both; tracking/blocking need per-OS code).
- Extension store releases (Chrome Web Store, Edge Add-ons, Firefox AMO).
- Automated tests for the frontend (Vitest + Testing Library), extension, and end-to-end (WebDriver).
- Accessibility audit, keyboard navigation, screen-reader labels.
- Localisation.
- Optional at-rest encryption of the database (S4).
