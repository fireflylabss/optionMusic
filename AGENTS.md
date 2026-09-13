# AGENTS.md — optionMusic

Guidance for coding agents working on this repo.

## Product

**optionMusic** (*option music*) — minimal black & white CLI music player powered by **MPV** (`libmpv2`).

Binaries: `optionmusic` (canonical) · `msc` (short alias).

## After every change

When you finish a task that touches code (features, fixes, UI, deps):

1. **Build** — always verify compile (prefer release when shipping UX/audio changes):

   ```bash
   export CARGO_TARGET_DIR="$(pwd)/target"
   cargo test
   cargo build --release
   ```

2. **Install to PATH** — always refresh the local binaries so `optionmusic` / `msc` match the working tree:

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
- Config: `~/.option/music/config.toml` (settings popup: `c`)
- Default music dir: `~/Music` (`-m` / `--music-dir`)
- Optional **cava** spectrum bars (off by default; `--cava` or `v` to enable)
- Optional **yt-dlp** downloader: `msc dl` wizard (provider → search/URLs → presets/options → cwd); UI `arrows` (default) or `type` via `--ui` / settings `c` / `dl_ui` in config; cache in `~/.option/music/cache/dl/`

## Release channels

See [VERSIONING.md](VERSIONING.md) and the blurb in [CHANGELOG.md](CHANGELOG.md). Short rules for agents:

- Desktop and CLI are **independent** version lines; changelog headings use Option style (`## v0.1.2-beta · DD/MM/YYYY`, or `## v0.2.12m-beta · …` when both surfaces change).
- Mixed releases: insert `m` before the channel, heavier surface section first.
- Do **not** label something `stable` unless it is actually release-ready.
- Prefer **beta** for desktop while the GPUI shell is still maturing; use **alpha** only for brand-new / half-built surfaces.
- Alpha/beta desktop builds are normally changelog + local/dev artifacts — not GitHub Release / AUR — unless the user explicitly promotes a **stable** cut.

When documenting uncommitted desktop work, prepend or expand the matching `vX.Y.Z[-m]-<channel>` entry rather than inventing a parallel scheme.

## Don’t

- Commit or push unless the user asks
- Force-push / skip hooks / amend pushed commits
- Regress the compact B&W minimalist UI without intent
- Ship or changelog a desktop cut as `stable` while it still belongs in alpha/beta
