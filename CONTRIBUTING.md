# Contributing

optionMusic is a minimal black & white terminal player. Changes should keep the
UI compact and monochrome, and keep playback responsive — never block the TUI
frame loop on I/O or the network.

## Layout

| Path | What |
|------|------|
| `src/` | CLI + TUI (`optionmusic`, `msc`), playback engine wrapper over libmpv |
| `src-gpui/` | native GPUI desktop shell, a workspace member reusing the `optionmusic` crate |
| `website/` | Vite + React + TypeScript site, run with Bun |

## Build

```bash
sudo pacman -S libmpv        # Arch; Debian/Ubuntu: apt install libmpv-dev pkg-config
export CARGO_TARGET_DIR="$(pwd)/target"
cargo test
cargo build --release
cargo install --path . --force   # refresh `optionmusic` / `msc` on PATH
```

`msc doctor` reports the optional tools (`yt-dlp`, `ffmpeg`, `cava`) and whether
`msc dl` is ready.

The desktop shell pins its own toolchain in `src-gpui/rust-toolchain.toml`:

```bash
cargo run -p optionmusic-gpui   # from the repo root
```

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
  first — CLI and desktop are independent version lines and channel labels
  (`alpha` / `beta` / `stable`) are meaningful.
- New external tools must be optional and reported by `msc doctor`.
