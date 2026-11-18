## What and why

<!-- What does this change, and why? Link the issue: "Fixes #123". -->

Fixes #

## How it was tested

<!-- Tests added/updated, and what you checked by hand (app, extension, browser preview). -->

## Checklist

- [ ] `npm run build` passes
- [ ] `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test` pass (in `src-tauri`)
- [ ] Sample data in `src/lib/mock.ts` updated if API types changed
- [ ] Schema changes use a new migration in `db.rs`
- [ ] No new data leaves the device; nothing can lock users out
- [ ] Docs / CHANGELOG updated if user-facing
- [ ] Screenshots for UI changes (light and dark)
