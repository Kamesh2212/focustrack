# Multi-device sync — design

Status: **draft for review** · Issue: #54 · Epic: #35

This document decides how FocusTrack syncs usage, limits, goals and focus sessions between a user's devices
(Windows PCs first, then Android, iOS and macOS). It is the basis for issues #56–#74. Anything marked
**Open question** needs a decision before the issue that depends on it starts.

## 1. Goals and non-goals

**Goals**

- One account, many devices; combined and per-device screen time on every device.
- Offline-first: every device keeps tracking locally and syncs when it can. Nothing is lost offline.
- Opt-in: no account needed for local use; sync is off until the user turns it on.
- Private by design: the server can't read anyone's usage history.
- Self-hostable: the sync server is open source and can run on the user's own machine.
- Cheap to run: the hosted service must cost little per user (it may fund itself through Premium, #74).
- Incremental: only changes travel; a device offline for weeks catches up without re-uploading everything.

**Non-goals (for now)**

- A web dashboard (it would need the browser to decrypt data; possible later, see §12).
- Real-time (sub-second) sync. Usage syncs within about a minute; focus sessions and limits faster (§6.5).
- Family/guardian accounts (roadmap, separate design).
- Syncing notes (sensitive free text; opt-in later, see §4.3).

## 2. Key decisions

| # | Decision | Choice | Why |
|---|---|---|---|
| D1 | Privacy model | **End-to-end encryption (E2EE).** Devices encrypt every record; the server stores ciphertext. Totals are computed on devices. | Matches FocusTrack's promise and the roadmap. Data is small (§3), so devices can add it up themselves. Server-side aggregation would let the operator read everyone's usage. **This replaces the requirements' "server-side aggregation service" (§10.1, §10.3)**: the same combined numbers are produced, but on each device (§8). |
| D2 | Server | **Rust (axum + tokio), one binary.** Storage behind a trait: **SQLite** first (local testing, self-hosting), **PostgreSQL** for the hosted service. | Same language as the app; shared protocol crate; a single binary is easy to self-host. Under E2EE the server is mostly an authenticated, ordered record store, so storage needs are simple. |
| D3 | Sync protocol | **Record log.** Every change is an encrypted record with a UUID and a per-device version. Devices *push* changed records and *pull* everything newer than a server cursor. | Idempotent (re-sending is harmless), incremental, resumable, and easy to reason about offline. |
| D4 | Conflicts | Usage records are **owned by one device** (no conflicts). Shared definitions (limits, goals, profiles, categories) use **last-writer-wins per record** with a **hybrid logical clock**; deletes are tombstones. | Simple and predictable; HLC tolerates devices whose clocks disagree. |
| D5 | Identity | Records reference apps by a **platform-qualified app key** (`win:steam.exe`, `win:c:\…\javaw.exe`, `android:com.android.chrome`), never local integer IDs. | Local IDs differ per device; the app key from #5 is already stable. |
| D6 | Keys | A random **account key** encrypts data. It's stored on the server **wrapped** with a key derived from the password (never sent), plus a **recovery key** the user saves. | The standard password-manager scheme: new devices can sign in with just the password, and the server never sees the password or the key. |
| D7 | History on new devices | Detailed records for the **last 35 days**; older days as **daily summary records**. | Keeps first sync small (§3) while keeping long-term trends. |
| D8 | Local storage of other devices' data | **Same tables**, with a `device_id` column. | Every existing query and report keeps working; add `WHERE device_id = ?` to filter. |

## 3. How much data

Measured on a development PC: about **400 records per device per day** (≈310 app spans, ≈100 website spans,
plus idle and focus). At ~200 bytes encrypted each that is **≈80 KB per device per day**, ~2.4 MB a month
(less with gzip).

- Steady state: a few KB per sync. Negligible.
- New device joining an account with 3 devices: 35 days × 3 × 80 KB ≈ **8 MB** detailed, plus small daily
  summaries for older history. Acceptable on Wi-Fi; the phone app defers it to Wi-Fi/charging.
- Server storage: ~30 MB per device per year before summaries. Older detail can be pruned to summaries by the
  owning device (§6.6).

## 4. Data model

### 4.1 Envelope (what the server sees)

```jsonc
{
  "id": "0193b1a2-…",        // UUIDv7, chosen by the creating device
  "kind": "usage",            // record type (plain text, see below)
  "device": "0193a0ff-…",     // creating/owning device
  "day": "2026-11-18",        // local calendar day (plain text, see 4.4)
  "version": 42,              // per-record, increases on every change by the owner
  "hlc": "1795041234567:3:dev", // hybrid logical clock, for shared definitions
  "deleted": false,           // tombstone
  "payload": "base64(…)"      // XChaCha20-Poly1305 ciphertext of the record body
}
```

The server stores the envelope plus a server sequence number (§6.2). It can see *that* a device recorded N
items on a day, but not what they were.

### 4.2 Record kinds (encrypted bodies)

| kind | Owner | Body (encrypted) | Mutable? |
|---|---|---|---|
| `device` | that device | name, platform, app version | yes (LWW) |
| `app` | creating device | app key, display name, platform | yes (LWW) |
| `usage` | device | app key, start, end, seconds | while open, then final |
| `idle` | device | start, end, seconds | while open |
| `site` | device | domain, browser, start, end, seconds | while open |
| `focus` | device | label, mode, kind, planned/actual secs, completed, profile id, goal | yes |
| `day_summary` | device | per-app and per-site seconds, hourly buckets, active/idle totals for one day | replaced when recomputed |
| `category` | shared | name, colour, kind | LWW |
| `app_category` / `site_category` | shared | app key or domain → category | LWW |
| `limit` | shared | target (app key / domain / app group), seconds, action, warn %, scope (`device` / `all`) | LWW |
| `goal` | shared | kind, target, category, title, enabled | LWW |
| `goal_check` | shared | goal id, day, done | LWW |
| `profile` | shared | blocking profile | LWW |
| `focus_run` | shared | an active cross-device focus session: plan, start, end, participating devices (#71) | LWW |
| `limit_extension` | shared | limit id, day, extra seconds (#70) | LWW |
| `device_control` | shared | target device, pause tracking, sync on/off (#66) — the target device applies it | LWW |
| `purge` | shared | "delete history before day X" (optionally for one device) — every device deletes its local copies (#67) | append-only |

Usage spans stay open while an app is in front. The device pushes the open span with its current end every
sync (same `id`, higher `version`), and the final version when it closes. Receivers keep the highest version.

### 4.3 Not synced

Device-only settings (start with Windows, idle threshold, administrator mode, website tracking switch,
notification switches), sent-notification bookkeeping, the clock-change log (#11), the local saved focus
session (#12; cross-device sessions use `focus_run`). **Notes** are not synced in the first version.
**Open question Q4:** sync notes later as an opt-in?

### 4.4 Plain-text metadata (privacy trade-off)

Kept unencrypted so the server can do its job:

- `kind`, `device`, `id`, `version`, `deleted`: needed to store, order and authorise records.
- `day` (date only): needed for server-side retention and "delete history before a date" (#67), and lets a new
  device fetch only recent days. It reveals *on which days* a device was used, not for how long or on what.
- Record **sizes and timing** of syncs: inherent to any sync service.

Account email and device count are also visible. Device names are encrypted (`device` record).

## 5. Encryption and accounts

### 5.1 Keys

```
password ──Argon2id(salt, high cost)──► master key (never leaves the device)
                                           ├─ HKDF "auth" ──► auth key ──► sent at sign-in; server stores Argon2id(auth key)
                                           └─ HKDF "wrap" ──► wrap key ──► encrypts the account key

account key (random 256-bit, made once) ──HKDF per kind──► record keys ──XChaCha20-Poly1305──► payloads
```

- **Sign-up** (first device): pick a password; device creates the account key, uploads `wrap(account key)`,
  the Argon2 salt and parameters, and the auth-key hash. It shows a **recovery key** (the account key in a
  printable form, 24 words) to save.
- **Sign-in** (new device): fetch salt/params for the email → derive master key → send auth key → receive tokens
  and the wrapped account key → unwrap locally.
- **Forgot password:** the server can reset the *login*, but without the recovery key old data can't be
  decrypted. The UI must say this clearly at sign-up. With the recovery key, the user sets a new password and the
  account key is re-wrapped.
- **Change password:** re-wrap the account key; data isn't re-encrypted.
- **Optional (later):** link a new device by scanning a QR code from a signed-in one (transfers the account key
  directly, no password typing). Roadmap item.
- **Open question Q2:** also offer "Sign in with Google/Microsoft/Apple"? It's convenient, but the password
  above is still needed for the encryption key (or a recovery key / QR link), which can confuse users.

AEAD associated data for every payload = `id | kind | device | day | version`, so the server can't move a
payload to another record or day without decryption failing.

### 5.2 Tokens and devices

- Sign-in registers the device and returns a **device refresh token** (random 256-bit, stored hashed on the
  server) and a short-lived **access token** (15 min).
- Every sync call carries the access token; the server checks it belongs to an active device of the account.
- **Removing a device** revokes its refresh token immediately; its next call fails and it stops syncing (#66).
  It keeps its local data. Removing a device does **not** rotate the account key (it could still read data it
  already downloaded); **Open question Q5:** offer "remove and re-key" for lost devices (re-encrypt everything
  with a new account key)? Costly, but the right answer for a stolen laptop.
- On Windows, tokens and the account key are stored with **DPAPI** (Windows Credential Manager), not in the
  SQLite file. Android: Keystore. iOS: Keychain.

## 6. Sync protocol

### 6.1 Endpoints (v1)

| Method | Path | Purpose |
|---|---|---|
| GET | `/v1/auth/params?email=` | Argon2 salt and parameters (a fake but stable answer for unknown emails) |
| POST | `/v1/auth/signup` | email, auth-key hash input, wrapped account key, salt, params, first device |
| POST | `/v1/auth/signin` | email, auth key, device info → tokens, wrapped account key |
| POST | `/v1/auth/refresh` | refresh token → new access token |
| POST | `/v1/auth/verify-email`, `/v1/auth/reset` | email verification and login reset |
| GET/PATCH/DELETE | `/v1/devices[/:id]` | list, rename (encrypted name record), remove |
| POST | `/v1/sync/push` | upload a batch of envelopes |
| GET | `/v1/sync/pull?since=&limit=&days=` | records with server sequence > `since` |
| DELETE | `/v1/data?before=&device=` | delete server history before a day (#67); the client also pushes a `purge` record so other devices delete their copies |
| DELETE | `/v1/account` | delete the account and all data |
| GET | `/v1/status` | server version, limits |

All over HTTPS (TLS 1.2+). JSON bodies, gzip. Batches up to 1,000 records or 1 MB.

### 6.2 Push

1. The device selects local records whose `version > synced_version` (indexed), oldest first, in batches.
2. Server, per envelope, in one transaction:
   - Reject if `device` isn't the caller (devices can only write their own records), **except** shared
     definitions, which any device of the account may write.
   - If a record with the same `id` exists with `version >= incoming` → ignore (idempotent).
   - Otherwise upsert and assign the next **server sequence** (per account, strictly increasing).
3. Server replies with `{id, version}` for every accepted or already-present record.
4. Device sets `synced_version = version` for exactly those. Nothing local is deleted or marked synced before
   this acknowledgement. If the reply is lost, the next push re-sends and gets the same answer.

For shared definitions the server keeps the record with the higher **HLC** (not version), so two devices editing
the same limit converge on the same result regardless of upload order.

### 6.3 Pull

1. Device calls `pull?since=<cursor>`; the server returns records with `seq > cursor` in order, with the new
   cursor and `more: true/false`. Records the device itself wrote are skipped.
2. Device decrypts, verifies, and upserts them locally (higher version / HLC wins), then stores the cursor
   **after** the batch is committed. A crash mid-batch just repeats that batch.
3. First sync on a new device: `days=35` limits detailed kinds to the last 35 days; `day_summary` records cover
   older history.

### 6.4 Schedule and triggers

- Push when there are pending changes, at most every **60 s**; pull every **60 s** while the app runs.
- Immediately (debounced 2 s) after a change to shared definitions, a focus session start/stop, or a limit
  extension.
- Exponential backoff on errors (1 s → 5 min, with jitter); a clear error state after repeated failures.
- Phones: WorkManager / BGTaskScheduler, roughly every 15 min plus when the app is opened.

### 6.5 Faster signals (later)

Cross-device focus (#71) and limits (#70) benefit from quicker delivery. v1 relies on the 60 s pull. v2 can add
a **long-poll** or server-sent-events endpoint that just says "something changed, pull now" (no data), and
push notifications (FCM/APNs) for phones. Payloads still only travel through `pull`.

### 6.6 Summaries and pruning

- At the end of each day (and on catch-up), the owning device writes a `day_summary` record for that day.
- Detailed records older than the retention window (default 35 days on the server, configurable in #67) may
  be **deleted by their owning device** (tombstones), leaving the summary. Devices keep their own detail
  locally as long as the user's local retention allows.

### 6.7 Clock handling

- Records carry local wall-clock times (as today) plus `day`. Devices don't need synchronised clocks for usage.
- Shared definitions use an HLC `(max(wall ms, last seen) , counter, device)`, so a device with a wrong clock
  can't silently win every conflict forever.
- The manual-clock-change detection from #11 stays local; a device with a shifted clock still records the real
  `day` for limits.

## 7. Local database changes (schema v6, issue #56)

- New `devices` table: `id` (UUID), `name`, `platform`, `is_self`, `last_seen`, `last_sync`.
- On syncable tables (`usage_sessions`, `idle_periods`, `website_sessions`, `focus_sessions`, `goals`,
  `goal_checks`, `usage_limits`, `limit_overrides`, `block_profiles`, `categories`) add:
  `uuid TEXT UNIQUE`, `device_id TEXT`, `version INTEGER`, `synced_version INTEGER`, `hlc TEXT` (shared kinds),
  `deleted INTEGER` (tombstone, shared kinds).
- `applications` / `websites` keep local integer IDs for joins; remote records are mapped through `app_key` /
  `domain` on arrival.
- `sync_state` (key/value): cursor, last push/pull, last error, pending count.
- Writes bump `version` (a trigger or the existing update helpers). Index `(version, synced_version)`.
- Migration backfills UUIDs and `device_id = <this device>` for existing rows; `synced_version = 0` so the
  first sync uploads history (subject to the 35-day rule; older days upload as summaries).

Every existing query keeps working because it reads all devices' rows. Device filters (#64) add
`AND device_id IN (…)`. Today-only enforcement (blocker, tracker) keeps using **this device's** rows unless a
limit is `scope = all` (#70).

## 8. Aggregation on devices

- **Combined screen time** = sum of every device's active (and idle) seconds.
- **Unique active time** = length of the union of all devices' active intervals (sort by start, merge
  overlaps). Shown next to the combined figure with a short explanation (#64).
- **Cross-device apps** (#63): group by app key; apps from different platforms join through a mapping table
  (`win:chrome.exe` ↔ `android:com.android.chrome`) and user app groups (#6).
- **Cross-device limits** (#70): a limit with `scope = all` counts this device's live usage plus the latest
  synced usage from others. The UI shows "includes phone usage up to 4 min ago".
- **Goals** (#72): measured goals add all devices; `goal_check` records make ticks sync.

## 9. Server design

- Crate `server/` (axum). Shared crate `crates/sync-proto` with envelope types, record bodies and the crypto
  helpers (used by the desktop app, the server's tests, and later the Android app through Tauri or FFI).
- Tables: `accounts` (email, verified, auth hash, salt/params, wrapped key, plan), `devices` (id, account,
  refresh-token hash, revoked, created, last_seen), `records` (account, id, kind, device, day, version, hlc,
  deleted, payload, seq), `account_seq`.
  Indexes: `(account, seq)`, `(account, id)` unique, `(account, day)`.
- Rate limiting per IP and per account; request size limits; audit log of sign-ins and device changes.
- Backups: nightly database dump; restores are safe because clients re-push anything the server lacks
  (`synced_version` is reset when the server reports an unknown account epoch).
- Observability: request metrics, sync error counts, no payload logging.
- **Open question Q1:** where to host the official service (e.g. a small VPS or Fly.io with managed Postgres),
  and its cost ceiling. Not needed for local testing.

## 10. Threats and mitigations

| Threat | Mitigation |
|---|---|
| Server operator or a database leak reads usage | E2EE; server holds only ciphertext and §4.4 metadata. |
| Password guessing | Argon2id client-side and server-side; rate limits; lockout with backoff. |
| Stolen access token | 15-min lifetime; refresh tokens are per device and revocable. |
| Lost or stolen device | Remove it in device management (revokes tokens); optional re-key (Q5); account key stored with DPAPI/Keystore. |
| Malicious server swaps or replays payloads | AEAD associated data binds payload to id/kind/device/day/version; versions only increase. |
| A device writes as another device | Server only accepts device-owned records from that device. |
| Forgotten password | Recovery key; clear warning that without it, synced history can't be decrypted (local data stays). |
| Email enumeration | `auth/params` returns deterministic fake parameters for unknown emails. |
| Supply chain | Pinned dependencies, `cargo audit` in CI (already in place for the app). |

## 11. Local testing plan

Everything can be built and tested on one PC without pushing anything:

1. **Server locally:** `cargo run -p focustrack-server` on `http://127.0.0.1:8787` with a SQLite file. TLS is
   required for non-localhost URLs only.
2. **Two "devices" on one PC:** a `FOCUSTRACK_PROFILE=<name>` environment variable gives the app a separate data
   folder, a separate local API port and skips the single-instance check, so two copies can run side by side
   as two devices on one account.
3. **Automated:** an integration test starts the server in-process and drives two or three sync clients with
   in-memory databases: offline for days, interrupted pushes, duplicate acknowledgements, conflicting edits,
   removed device, wrong password, recovery key.
4. **Android (later):** an emulator talking to the server on the host (`10.0.2.2:8787`).

## 12. Implementation order

1. `crates/sync-proto`: envelope, record bodies, key derivation, encrypt/decrypt + tests.
2. Schema v6 (#56).
3. Server: accounts, devices, push/pull on SQLite (#57, #58, #60).
4. Desktop sync engine + Account & Sync settings (#59, #61), tested with two profiles.
5. Shared definitions with HLC (#62).
6. Device filter, combined and unique time (#63, #64), device management (#66), deletion (#67).
7. Android (#68, #69), then cross-device limits/focus/goals (#70–#72), iOS (#73), Premium (#74).

Later: web dashboard (decrypting in the browser), QR device linking, long-poll/push signals, PostgreSQL backend
for hosting.

## 13. Requirements coverage

| Requirements section | Where it's handled |
|---|---|
| 2 Accounts & device registration | §5, §6.1; #57, #58, #59 |
| 3.1 Combined screen time (+ unique active time) | §8; #64 |
| 3.2 Device-specific statistics | §7 (`device_id` on every row); #65 |
| 3.3–3.4 Combined app and website usage | D5, §8; #63 |
| 4 Automatic sync, offline, reliability, no duplicates | §6.2–6.4, D3; #61 |
| 5 Unified dashboard and filters | §7, §8; #64 |
| 6 Cross-device limits (and offline honesty) | §4.2 `limit` scope, §8; #70 |
| 7 Cross-device focus | §4.2 `focus_run`, §6.5; #71 |
| 8 Cross-device goals | §4.2 `goal`/`goal_check`, §8; #72 |
| 9 Device management (incl. remote pause, sign-out) | §5.2, §4.2 `device_control`; #66 |
| 10 Architecture: auth, TLS, device credentials, incremental sync | §5, §6, §9 — **aggregation on devices instead of the server** (D1) |
| 11 Privacy: opt-in, deletion, account deletion, retention | §1, §4.4, §6.6, `purge`; #59, #67 |
| 12 Premium | Q6; #74 |
| 13 Performance, reliability, scalability | §3, §6.4, §6.6, §9 |

## 14. Open questions

| # | Question | Proposed answer |
|---|---|---|
| Q1 | Hosting and cost ceiling for the official server | **Decided:** the existing Oracle Cloud Always Free ARM VM (4 cores, 24 GB, ~83 GB free disk, UK South) behind its Caddy HTTPS proxy, as a Docker container; £0 until there are customers. Needs: a domain (or free dynamic-DNS name), protection from idle reclaim (Pay As You Go upgrade + £1 budget alert), nightly backups to free Object Storage, an email sender. Estimated capacity: ~5,000 users with 3 devices each. Local-only until the feature works. |
| Q2 | Social sign-in (Google/Microsoft/Apple)? | Not in v1; email + password + recovery key. |
| Q3 | Detail window for new devices | 35 days detailed + daily summaries. |
| Q4 | Sync notes? | Later, opt-in. |
| Q5 | Re-key the account when removing a lost device? | Offer it as "Remove and secure account" in v1.1. |
| Q6 | Premium model vs MIT licence (#74) | Hosted sync as the paid part; self-hosting free. |
