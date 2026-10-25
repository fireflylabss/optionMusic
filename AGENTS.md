# AGENTS.md — still

Guidance for coding agents working on this repo.

## Product

**still music** (*still*) — minimal black & white CLI music player powered by **MPV** (`libmpv2`).

Binaries: `still` (canonical) · `msc` (short alias).

The Cargo package and crate are `stillmusic`. Use **still** in the interface.
The GitHub repository remains `fireflylabss/optionMusic`.
AUR packages: `stillmusic` (source) and `stillmusic-bin` (prebuilt).
The legacy `optionmusic` package is kept separately for migration.

## After every change

When you finish a task that touches code (features, fixes, UI, deps):

1. **Build** — always verify compile (prefer release when shipping UX/audio changes):

   ```bash
   export CARGO_TARGET_DIR="$(pwd)/target"
   cargo test
   cargo build --release
   ```

2. **Install to PATH** — always refresh the local binaries so `still` / `msc` match the working tree:

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
- Config: `~/.option/music/config.toml` (settings popup: `c`; override with `STILLMUSIC_CONFIG`)
- Default music dir: `~/Music` (`-m` / `--music-dir`; env `STILLMUSIC_MUSIC_DIR`)
- Optional **cava** spectrum bars (off by default; `--cava` or `v` to enable)
- Optional **yt-dlp** downloader: `msc dl` wizard (provider → search/URLs → presets/options → cwd); UI `arrows` (default) or `type` via `--ui` / settings `c` / `dl_ui` in config; cache in `~/.option/music/cache/dl/`

## Release channels

See [VERSIONING.md](VERSIONING.md) and the blurb in [CHANGELOG.md](CHANGELOG.md). Short rules for agents:

- still has one installable surface: CLI/TUI. Changelog headings use Hory style (`## v0.3.0-beta.1 · DD/MM/YYYY`): one summary paragraph and feature bullets, without per-surface or change-category subsections.
- Alpha and beta tags can add a positive revision suffix (`-beta.1`, `-alpha.2`) for a corrected release attempt. Tags are immutable; increment the revision instead of moving an existing tag. Cargo keeps the numeric version (`0.3.0`).
- Do **not** label something `stable` unless it is actually release-ready; use **beta** for builds that still need refinement and **alpha** for brand-new surfaces.
- Historical Desktop and mixed entries keep their original tags and dates. Do not add new desktop entries or revive the `m` mixed-release form.

When documenting uncommitted work, prepend or expand the matching `vX.Y.Z-<channel>` entry rather than inventing a parallel scheme.

## Don’t

- Commit or push unless the user asks
- Force-push / skip hooks / amend pushed commits
- Regress the compact B&W minimalist UI without intent
- Reintroduce the GPUI desktop shell, its packaging or its release workflow
