# Security Policy

## Reporting a vulnerability

Please **don't open a public issue**. Use GitHub's private reporting:
**Security → Report a vulnerability** on this repository. Include steps to reproduce and the version
affected. We aim to reply within a week and will credit you in the release notes unless you'd rather not.

## Supported versions

| Version | Supported |
|---|---|
| 1.0.x | ✅ |

## Security model

FocusTrack is a local, offline-first app.

- **Data at rest:** a SQLite database in `%APPDATA%\com.focustrack.app\`, readable by your Windows user
  account. It is not encrypted.
- **Network:** no internet access and no telemetry. The only listener is the extension API on
  `127.0.0.1:17865`.
- **Extension API:** accepts requests only from browser-extension origins with an `X-FocusTrack` header and a
  local `Host`, so ordinary websites can't use it (including through DNS rebinding).
- **What's collected:** foreground app, active website domain (with the extension), timings and idle state.
  Never keystrokes, screenshots, window titles, full URLs or page content. Private windows aren't reported.
- **Blocking** closes apps with `WM_CLOSE` and then `TerminateProcess`; it never touches Windows components,
  Explorer, Task Manager, terminals or FocusTrack.

## Known limitations (1.0.0)

See [docs/AUDIT-1.0.0.md](docs/AUDIT-1.0.0.md) §3. In short:

- Any installed browser extension, or any local process, can call the local API (read today's usage
  summary, send heartbeats, add notes, tick goals). Pairing the extension with a token is planned.
- On a shared PC, only the first signed-in user's FocusTrack can use the extension port.
- No Content-Security-Policy in the app webview yet.
- Installers are not code-signed; verify downloads against `SHA256SUMS.txt` in the release.
