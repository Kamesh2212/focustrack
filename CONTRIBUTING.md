# Contributing to FocusTrack

Thanks for helping people take back their time. This guide explains how to report problems, pick up work,
and get a change merged.

## Ways to help

- **Report a bug** or **suggest a feature** with the issue templates.
- **Fix an issue.** Issues labelled [`good first issue`](https://github.com/Kamesh2212/focustrack/labels/good%20first%20issue)
  are a good start; [`help wanted`](https://github.com/Kamesh2212/focustrack/labels/help%20wanted) needs more
  context.
- **Build a roadmap item** from [docs/ROADMAP.md](docs/ROADMAP.md). Open or comment on an issue first so we can
  agree on the approach.
- **Improve docs and tests.**

Security problems: please don't open a public issue — see [SECURITY.md](SECURITY.md).

## Workflow

1. **Pick or open an issue** and comment that you're working on it. A maintainer will assign it to you.
2. **Fork** the repository (maintainers can branch directly) and create a branch from `main`:

   | Type | Branch name |
   |---|---|
   | Bug fix | `fix/<issue-number>-short-description` |
   | Feature | `feat/<issue-number>-short-description` |
   | Docs | `docs/<short-description>` |
   | Refactor / tooling | `chore/<short-description>` |

   e.g. `fix/12-media-counts-as-idle`.
3. **Make your change** with tests where it makes sense (see below).
4. **Run the checks locally** (same as CI):

   ```bash
   npm ci
   npm run build                                   # TypeScript + UI build
   cd src-tauri
   cargo fmt --check
   cargo clippy --all-targets -- -D warnings
   cargo test
   ```
5. **Commit** with clear messages. We use [Conventional Commits](https://www.conventionalcommits.org):
   `fix: count fullscreen video as active time (#12)`, `feat(extension): …`, `docs: …`.
6. **Open a pull request** against `main` and fill in the template. Link the issue (`Fixes #12`).
7. **CI must pass** and a maintainer reviews. We squash-merge; the PR title becomes the commit message.

`main` is protected: changes land only through pull requests with passing checks.

## Development setup

Windows 10/11 with Node.js 20+, Rust stable (MSVC) and Visual Studio 2022 Build Tools (C++ workload).

```bash
npm install
npm run app:dev     # full desktop app with hot reload
npm run dev         # UI only in a browser with sample data — great for UI work
```

Load the extension from `extension/` as an unpacked extension to test website features.

If linking fails with `LNK1104`, your path is too long for the MSVC linker; set `CARGO_TARGET_DIR` to a short
path such as `C:\ft-target`.

## Code guidelines

- **Privacy first.** Never collect keystrokes, screenshots, window titles, full URLs or page content. New data
  must stay local unless the user explicitly opts in.
- **Never lock people out.** Blocking must always leave a way back (FocusTrack, Task Manager, Windows tools).
- **Rust:** `rustfmt` formatting, no clippy warnings, parameterised SQL only, errors returned as `Result`
  rather than panics in long-running threads. Put OS calls in `platform.rs`. Use the `Probe` trait or
  in-memory databases (`db::open_in_memory`) for tests.
- **Schema changes:** add a new `if version < N` migration in `db.rs`; never edit an existing one.
- **Frontend:** TypeScript strict, components in `src/components`, all backend calls through `lib/api.ts`, and
  matching sample data in `lib/mock.ts` so `npm run dev` keeps working.
- **Extension:** no remote code, no new permissions without discussion, only domains leave the browser.
- Keep UI text short and plain; check light and dark themes.

## Releases (maintainers)

1. Update `CHANGELOG.md` and bump the version in `package.json`, `src-tauri/Cargo.toml`,
   `src-tauri/tauri.conf.json` and `extension/manifest.json`.
2. Merge to `main`, then tag: `git tag vX.Y.Z && git push origin vX.Y.Z`.
3. The release workflow builds the installers and publishes the GitHub Release with the downloadable zip.

## Code of conduct

Be kind and constructive. See [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).
