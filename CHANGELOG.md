# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

**Desktop** entries are full releases (not beta), but the desktop shell is still **unstable** — expect bugs while it matures. The CLI track is separate and versioned independently.

## [Desktop 0.1.1] - 2026-07-20

> **Unstable release** — Desktop builds are real releases (not beta), but the shell is still early; bugs and rough edges are expected.

### Added

- Audex-style left sidebar: brand + traffic lights, Library / Artists / Playlists / Favorites, Recent list, Search trigger, Open files, Settings.
- **⌘K / Ctrl+K** command palette to search and play tracks (↑↓ navigate, Enter play, Esc close) — replaces the old Search tab.
- Artists browser with two modes: **metadata** (default, tags via lofty) or **folder** names — shared `artist_source` in `~/option/music/config.toml` (CLI settings `c` → Artists, desktop Settings → Library).
- Track tags: `artist` / `album` / preferred title on the DTO; artist cards load cover art when available.
- Artist detail view: **Albums** grid + **Tracks** list (drill into an album).
- Session resume: last track, position, and queue persisted in config (`resume_track`, `resume_position`, `resume_queue`) and restored paused on reopen (desktop + shared config).
- Now-playing **listening rail** on the right: full-bleed cover, play/pause on art, title/artist/album, in-panel scrubber, Like / Queue actions, Up next list.
- Settings as a centered **popup** (tabs: Library / Playback / Audio) — folders, artists source, volume, excess volume, LDM, EQ grid; shared with CLI/`config.toml`.
- Album art in stage, mini-player, and artist/album cards — folder sidecars (`cover.jpg`, `folder.jpg`, …) or embedded tags via `lofty`.

### Changed

- Replaced the top masthead with a persistent navigation sidebar; Search is no longer a primary nav tab.
- Now-playing moved from left/stage-left to a compact right rail (~300–320px).
- Bottom player bar polish: larger artwork, clearer typography, thicker scrubber, refined control hierarchy and volume control.
- Window traffic lights (close / minimize / maximize) sized for easier clicking without dominating the chrome.
- Transport stays in the footer; stage focuses on cover and listening context.

### Fixed

- Header / stage / list alignment inconsistencies from the 0.1.0 layout pass.

## [CLI 0.2.7] - 2026-07-19

### Added

- **`download`** / **`dl`** / **`d`** — interactive yt-dlp downloader for **YouTube**, **YouTube Music**, and **SoundCloud**.
  - Wizard flow: **provider → search/URL(s) → select → preset → options → download**
  - URLs: paste one or many (`url1;url2`); asks audio vs video when the platform supports it (audio-only platforms skip straight to audio)
  - Search: 8 results per page with next/prev, multi-select, cached under `~/option/music/cache/dl/` (auto-purged every **3 days**)
  - Presets after selection: **best** · **economy** · **lower** · **custom**
  - Options: quality, filetype/container, embed thumbnail + music metadata, subtitles (scan → default **embed** when available; also separate / both / none)
  - Multi-select / multi-URL batch: only options common to **all** selected items (capability intersection)
  - Output defaults to the **current directory** (opt-in other dir); `-o` still works for direct mode
  - Direct mode: `msc dl URL --audio`, `msc download -p soundcloud "query" -a`
  - Wizard UI modes: **`arrows`** (default — ↑↓ / checkboxes) and **`type`** (typed prompts); set via `msc dl --ui arrows|type`, settings **`c` → Dl UI**, or `dl_ui` in `~/option/music/config.toml`
  - Download kinds: **audio only** · **video only** · **both** (separate files); each format choice is its own selection screen (no left/right cycling)
  - Embeds: full metadata pack, cover thumbnail inside the file, subtitles embedded into video (no loose `.srt` option)
  - Subtitles limited to **en/pt/es** (+ skip translated auto-subs) with `--ignore-errors` so a 429 can’t kill the video download
  - After selecting **exactly one** item: optional **audio preview** — quiet fetch (spinner only, no remux) into a slim player without list/settings (`q` returns to the download wizard)
  - Higher-contrast greyscale arrow UI (inverted focus row + clearer copy)
- Requires system **yt-dlp** (and **ffmpeg** for extract / embed).

## [Desktop 0.1.0] - 2026-07-19

> **Unstable release** — Desktop builds are real releases (not beta), but the shell is still early; bugs and rough edges are expected.

### Added

- First desktop release built with React, TypeScript, Vite, Bun and Tauri 2.
- A shared Rust `CoreController` drives the desktop and uses the existing `libmpv` player rather than WebView audio.
- Tauri commands and live `optmusic://state` snapshots for scanning, playback, seeking, queue operations, favorites, volume and EQ.
- Automatic scan of `~/Music`, plus picker-based additional library folders persisted in `~/option/music/config.toml`.
- Library search, favorites, queue add/remove/play-next actions, contextual file-manager reveal and desktop settings.
- Desktop playback controls for play/pause, previous/next, seek, volume and EQ presets.

### Changed

- The desktop app is now started with `bun run tauri:dev`; browser-only preview clearly states that playback and library access require Tauri.
- Playback, library state, queue and preferences now come from the Rust core; the frontend contains presentation state only.
- Local paths are validated by the core before revealing them in the file manager.
- Live ticker emits a lightweight `PlaybackState` (position / pause / current) instead of re-sending the full library on every tick.
- Frontend keeps the library list stable across ticks and only re-renders the track list when transport state actually changes.
- Single-click anywhere on a track row starts playback (not only the title button / double-click).
- Settings EQ options use the core preset labels (`off` · `bass+` · `treble+` · `rock` · `vocal` · `lofi`).

### Fixed

- Playback appeared dead with large libraries: the 250 ms ticker was pushing ~0.5 MB JSON (full library) through the WebView IPC, starving UI clicks.
- `libmpv` init failed with `Null` under desktop locales such as `pt_BR` — GTK/WebKit resets `LC_NUMERIC`; the core now forces `C` before creating / driving MPV.
- `play` could mark a track as current even when `loadfile` failed; current is only committed after a successful open.
- Footer Play called `toggle_pause` as a no-op when the player was idle / stopped; it now resumes or restarts the current track.
- Play / command errors were silent whenever the library was non-empty; failures now show an dismissible error banner.

## [CLI 0.2.6] - 2026-07-17

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

## [CLI 0.2.5] - 2026-07-17

### Added

- In-session **loop cycle** with **`o`**: `off` → `list` (playlist) → `track` (repeat one) → `off`.
- CLI: `--pitch`, `--eq`, `--quiet` / `-q`, `--loop-file` / `--repeat-one`, `--repeat` alias for `--loop`.
- CLI aliases: `p`/`pl`→`play`, `i`→`info`, `ls`→`list`, `ver`→`version`; bare `msc song.mp3` also plays.
- `OPTMUSIC_MUSIC_DIR` env for the default library.

### Changed

- **`--help`** restyled (clearer sections, keys + examples in `after_help`).
- Status row shows current `loop` mode.
- Filename / path line is **off by default** (`f` still toggles it on).

## [CLI 0.2.4] - 2026-07-17

### Changed

- Help (`?` / `h`) and playlist (`l`) are **overlays** — they no longer shift the centered player (same idea as cava).
- **`l` and `?` can be open at the same time**.
- Playlist sidebar: mouse-wheel scrolls when the pointer is over the panel; scrollbar always visible and easier to grab (click / drag).

### Packaging

- Available on the **AUR** as [`optmusic`](https://aur.archlinux.org/packages/optmusic).

## [CLI 0.2.3] - 2026-07-17

### Added

- **`f`** toggles the filename/path line under the track title (session-persistent; toast `filename off/on`).

### Changed

- **Cava** — classic vertical **bar** columns under the footer (default cava look); overlay only — toggling does not shift the player.
- **Toast** — floating boxed overlay in the **top-right** of the player area (fade in/out, no slide).
- Help (`?` / `h`): **right sidebar**; synchronized redraws avoid flicker.
- Playlist (`l`): **left sidebar** with mouse-wheel / ↑↓`jk` / PgUp·PgDn scroll and a **draggable scrollbar**; click a row to jump. Esc/`l` closes.

### Fixed

- **`n` / `p` and ◂ / ▸** — `loadfile replace` emits `EndFile(Stop)`, which was treated as natural EOF and auto-advanced (undoing prev, double-skipping next). Only `EndFile(Eof)` advances the playlist now.

## [CLI 0.2.2] - 2026-07-17

### Changed

- Status row: removed the decorative `eq_bars` viz after play/pause; live spectrum stays under the shortcut footer only.
- Cava strip: smoother 2-row spectrum (better glyphs, continuous sampling, softer greys).
- Help (`?` / `h`): opens as a **right sidebar** that shifts the player aside (play · seek · sound · more).
- Volume: visible `−` / `+` next to the level — clickable; click the percentage still mutes.
- Rust edition **2024** (MSRV **1.85**).

### Notes

- Cava still **off by default** (`--cava` or `v`).



## [CLI 0.2.1] - 2026-07-17

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



## [CLI 0.2.0] - 2026-07-17

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



## [CLI 0.1.0] - 2026-07-17



### Added

- Initial release: dual bins `optmusic` / `msc`, rodio playback, alternate-screen B&W UI, play / list / info / version.
