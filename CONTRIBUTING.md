# Contributing

still is a minimal black & white terminal player. Changes should keep the
UI compact and monochrome, and keep playback responsive — never block the TUI
frame loop on I/O or the network.

## Layout

| Path | What |
|------|------|
| `src/` | CLI + TUI (`still`, `msc`), playback engine wrapper over libmpv |
| `website/` | Vite + React + TypeScript site, run with Bun |
| `packaging/aur/` | Source and prebuilt AUR recipes; see its README for validation |

## Build

```bash
sudo pacman -S mpv          # Arch; Debian/Ubuntu: apt install libmpv-dev pkg-config
export CARGO_TARGET_DIR="$(pwd)/target"
cargo test
cargo build --release
cargo install --path . --force   # refresh `still` / `msc` on PATH
```

`msc doctor` reports the optional tools (`yt-dlp`, `ffmpeg`, `cava`) and whether
`msc dl` is ready.

Website:

```bash
cd website && bun install && bun run dev
```

## Before opening a PR

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
cd website && bun run test      # website changes only
```

The MSRV in `Cargo.toml` (`rust-version`) is enforced in CI — if a dependency
needs something newer, bump it in the same PR and say so.

## Conventions

- Conventional-commit subjects (`feat:`, `fix:`, `ci:`, `build:`, `docs:`).
- Keep PRs focused; unrelated formatting churn makes review harder.
- User-visible changes get a CHANGELOG entry; read [VERSIONING.md](VERSIONING.md)
  first. Follow Hory style: `## vX.Y.Z-channel · DD/MM/YYYY`, a short summary
  naming CLI, channel, date and tag, then feature bullets. CLI/TUI is the single
  installable surface, so new entries have no `m` or per-surface subsections.
  Channel labels (`alpha` / `beta` / `stable`) describe release readiness.
- New external tools must be optional and reported by `msc doctor`.
