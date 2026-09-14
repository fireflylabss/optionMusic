## What

<!-- The behaviour change, in one or two lines. -->

## Why

<!-- The problem it solves. Link an issue if there is one. -->

## Checks

- [ ] `cargo fmt --all --check`
- [ ] `cargo clippy --all-targets -- -D warnings`
- [ ] `cargo test`
- [ ] `cargo build --release` (and `cargo run -p optionmusic-gpui` for desktop changes)
- [ ] `cd website && bun run test` (website changes)
- [ ] CHANGELOG entry for a user-visible change (see VERSIONING.md)
