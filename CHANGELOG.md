# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.6] - 2026-07-17

### Added

- Settings **left sidebar** with **`c`** — ↑↓ move, enter / click to toggle; cava submenu inside.
- Playlist as a **left sidebar** (`l`) — mouse-wheel / navigation keys scroll, click rows, and click / drag the scrollbar.
  - **Excess volume** — allow gain up to **200%**
  - **Cava styles** — style (`bars` / `dense` / `mirror` / `dots`) and height (`3` / `5` / `7`)
  - **LDM** — low-detail mode (fewer animations, ~30 fps redraw)
  - **Accent** — cycle presets (`default` · `cyan` · `green` · `amber` · `rose` · `blue` · `violet`) or set `#RRGGBB` in the config file
- Persistent config at **`~/option/music/config.toml`** (each option + “reset all” can revert to defaults; `d` resets the selected row).

### Changed

- Help / footer list the new `c` shortcut.
- Added a blank line between the speed / pitch / EQ status bar and the footer.

### Fixed

- Settings popup (`c`) redraws immediately when navigating or toggling by keyboard / mouse.

## [0.2.5] - 2026-07-17

### Added

- In-session **loop cycle** with **`o`**: `off` → `list` (playlist) → `track` (repeat one) → `off`.
- CLI: `--pitch`, `--eq`, `--quiet` / `-q`, `--loop-file` / `--repeat-one`, `--repeat` alias for `--loop`.
- CLI aliases: `p`/`pl`→`play`, `i`→`info`, `ls`→`list`, `ver`→`version`; bare `msc song.mp3` also plays.
- `OPTMUSIC_MUSIC_DIR` env for the default library.

### Changed

- **`--help`** restyled (clearer sections, keys + examples in `after_help`).
- Status row shows current `loop` mode.
- Filename / path line is **off by default** (`f` still toggles it on).

## [0.2.4] - 2026-07-17

### Changed

- Help (`?` / `h`) and playlist (`l`) are **overlays** — they no longer shift the centered player (same idea as cava).
- **`l` and `?` can be open at the same time**.
- Playlist sidebar: mouse-wheel scrolls when the pointer is over the panel; scrollbar always visible and easier to grab (click / drag).

### Packaging

- Available on the **AUR** as [`optmusic`](https://aur.archlinux.org/packages/optmusic).

## [0.2.3] - 2026-07-17

### Added

- **`f`** toggles the filename/path line under the track title (session-persistent; toast `filename off/on`).

### Changed

- **Cava** — classic vertical **bar** columns under the footer (default cava look); overlay only — toggling does not shift the player.
- **Toast** — floating boxed overlay in the **top-right** of the player area (fade in/out, no slide).
- Help (`?` / `h`): **right sidebar**; synchronized redraws avoid flicker.
- Playlist (`l`): **left sidebar** with mouse-wheel / ↑↓`jk` / PgUp·PgDn scroll and a **draggable scrollbar**; click a row to jump. Esc/`l` closes.

### Fixed

- **`n` / `p` and ◂ / ▸** — `loadfile replace` emits `EndFile(Stop)`, which was treated as natural EOF and auto-advanced (undoing prev, double-skipping next). Only `EndFile(Eof)` advances the playlist now.

## [0.2.2] - 2026-07-17

### Changed

- Status row: removed the decorative `eq_bars` viz after play/pause; live spectrum stays under the shortcut footer only.
- Cava strip: smoother 2-row spectrum (better glyphs, continuous sampling, softer greys).
- Help (`?` / `h`): opens as a **right sidebar** that shifts the player aside (play · seek · sound · more).
- Volume: visible `−` / `+` next to the level — clickable; click the percentage still mutes.
- Rust edition **2024** (MSRV **1.85**).

### Notes

- Cava still **off by default** (`--cava` or `v`).



## [0.2.1] - 2026-07-17

### Added

- **Cava spectrum strip** — optional discreet greyscale spectrum (requires `cava` on PATH).
  - Off by default; enable with `--cava` or toggle with `v` (click the strip to toggle too).
- **Richer mouse UI** — scrub progress; click `◂` / `⏸`/`▶` / `▸` for prev / pause / next; click volume (mute), spd, ptch, eq; click playlist rows to jump; scroll wheel seeks ±5s.
- **Pause glyph** — clearer `⏸` when paused.
- `AGENTS.md` — agent workflow (build + install to PATH after changes).
- `LICENSE` — Apache License 2.0.

### Changed

- README updated for cava (opt-in), mouse hits, Apache-2.0.
- Package license: MIT → **Apache-2.0**.

### Removed

- `ROADMAP.md` (tracked in issues / chat instead).



## [0.2.0] - 2026-07-17

### Added

- **MPV audio engine** via `libmpv2` — replaces rodio for broader format support and stronger control surface.
- **Mute** (`m`) during playback.
- **Long seek** `{` / `}` ±60s (short seek remains ← / → ±5s).
- **Equalizer presets** (`e`) — cycle: off → bass+ → treble+ → rock → vocal → lofi.
- **Crossfade** — CLI `-c` / `--crossfade SECONDS` maps to MPV `audio-fade`.
- **Speed & pitch** — `[` / `]` speed, `,` / `.` pitch, `0` resets both.
- **Default music dir** — global `-m` / `--music-dir` (default `~/Music` when `play` has no paths).
- **Zero-leak UI** — alternate-screen session with absolute redraw; scrollback restored cleanly on quit.
- **Unit tests** for playlist, EQ presets, MPV config clamps, music-dir resolution, CLI parsing, UI helpers.
- **CHANGELOG.md**.



### Changed

- Version bumped to **0.2.0**.
- `info` probes duration via a short-lived MPV instance (ao/video off).
- Keyboard help and README updated for the new controls.
- Dependency: `rodio` removed; `libmpv2` and `dirs` added.



### Notes

- Requires system **libmpv** (and pkg-config `mpv` on most distros). See README.



## [0.1.0] - 2026-07-17



### Added

- Initial release: dual bins `optmusic` / `msc`, rodio playback, alternate-screen B&W UI, play / list / info / version.
