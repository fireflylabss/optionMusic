# AGENTS.md — optMusic

Guidance for coding agents working on this repo.

## Product

**optMusic** (*option music*) — minimal black & white CLI music player powered by **MPV** (`libmpv2`).

Binaries: `optmusic` · `msc` (same entrypoint).

## After every change

When you finish a task that touches code (features, fixes, UI, deps):

1. **Build** — always verify compile (prefer release when shipping UX/audio changes):

   ```bash
   export CARGO_TARGET_DIR="$(pwd)/target"
   cargo test
   cargo build --release
   ```

2. **Install to PATH** — always refresh the local binaries so `msc` / `optmusic` match the working tree:

   ```bash
   export CARGO_TARGET_DIR="$(pwd)/target"
   cargo install --path . --force --offline
   ```

   Use `--offline` when deps are already fetched; drop it if the lockfile needs network.

Do **not** leave the user on a stale `~/.cargo/bin/msc` after finishing work.

## Sandbox / target dir

If builds appear to succeed but `./target/debug/msc` looks stale, check `CARGO_TARGET_DIR`. Prefer:

```bash
export CARGO_TARGET_DIR="$(pwd)/target"
```

## Stack notes

- Engine: `libmpv2` (system **libmpv** required)
- TUI: `crossterm` alternate screen (zero scrollback leak) + mouse capture
- Config: `~/option/music/config.toml` (settings popup: `c`)
- Default music dir: `~/Music` (`-m` / `--music-dir`)
- Optional **cava** spectrum bars (off by default; `--cava` or `v` to enable)

## Don’t

- Commit or push unless the user asks
- Force-push / skip hooks / amend pushed commits
- Regress the compact B&W minimalist UI without intent
