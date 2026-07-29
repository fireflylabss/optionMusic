# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html) with an explicit **release channel** suffix.

### Release channels (`x.y.z-<channel>`)

| Channel | Tag example | Meaning |
|---------|-------------|---------|
| **alpha** | `0.1.0-alpha` | Extremely early. Features incomplete; bugs are expected and common. |
| **beta** | `0.1.2-beta` | Feature set nearly complete, but still rough — bugs and hard edges remain. |
| **stable** | `0.2.0-stable` | Production-ready: finished for that version, few or no known bugs. |

Desktop and CLI are versioned independently. Desktop **0.1.0** / **0.1.1** predate this scheme (shipped as plain versions). From **0.1.2-beta** onward, every desktop entry uses a channel suffix. Beta/alpha desktop builds are development snapshots — not GitHub Releases or AUR packages unless explicitly promoted to **stable**.

## [CLI 0.2.9] / [Desktop 0.1.3-beta] - 2026-07-29

### Changed

- Config, cache and playlists now live under **`~/.option/music/`** (same layout as optionTerm’s `~/.option/terminal/`). Existing data at `~/option/music/` is moved automatically on first run.

## [Desktop 0.1.2-beta] - 2026-07-21

> **Beta** — features nearly in place; still expect bugs and rough edges. Not a GitHub Release / AUR package.

### Added

**Listening tools (core → desktop)**

- Local playlists: create, rename, delete, add/remove tracks; import/export **M3U** (`~/.option/music/playlists/`). Desktop Playlists page + CLI `msc playlist …`.
- Smart shelves: **Played this week**, **No cover**, **Incomplete albums** (history log + tag/cover enrichment).
- **Listening focus** mode — hide chrome; cover + scrub only (Esc to exit).
- Desktop **yt-dlp download** dialog (search → select → download into the music folder).
- **Pitch & speed** controls in Settings (shared MPV engine; already in CLI).
- **ReplayGain** mode (`off` / `track` / `album`) via MPV, persisted in `config.toml`.
- Offline **lyrics** (embedded tags or `.lrc` / `.txt` sidecar) in the Stage panel.
- **Edit tags** dialog (title / artist / album) + Reveal in folder (context menu).

**Library performance**

- Two-phase library scan: path index first (fast UI), then background lofty tag enrichment in batches (`enrich_tags_batch`, ~32 tracks) after `scan_music_directories`.
- Partial library updates over `optmusic://library-enriched` (`LibraryEnrichUpdate { tracks, done }`) so artists/albums fill in without a full snapshot; UI merges via `useOptMusic`.
- Disk tag cache under `~/.option/music/cache/tags/` (`read_tags_cached` / `write_tag_cache`, keyed by path + mtime + size via `stable_cache_key`).
- Shared cache helpers: `cache_dir`, `tags_cache_dir`, `covers_cache_dir`, `stable_cache_key` in `config.rs`.
- Track scan metadata: `mtime`, `size`, `tags_enriched`; lazy `Track::from_path` + `enrich_tags` (no lofty on the hot path).
- O(1) path→track index on `CoreController` (`rebuild_path_index`) for play / next / queue / favorites lookups.

**Covers**

- Cover disk cache under `~/.option/music/cache/covers/` for embedded art (sidecar still preferred when present).
- New IPC `track_cover_url` → absolute file path for `convertFileSrc`; data-URL `track_cover` remains as client fallback.
- `CoverFile` / `resolve_cover_file` / `cover_file_path` on the core; UI `CoverThumb` prefers file URLs.
- Lazy cover loading: IntersectionObserver + concurrency-limited IPC queue (`coverQueue.ts`).

**UI shell**

- Virtualized track list (`VirtualTrackList`, viewport ± overscan) for large libraries.
- shadcn/ui preset `b37si9Aiv` (base-vega / stone / Lora / Phosphor, RTL-ready): `components.json`, theme tokens, `DirectionProvider`, `TooltipProvider`.
- Core shadcn primitives under `src-ui/components/ui/` (Button, Switch, Slider, Dialog, Tabs, ScrollArea, Separator, Badge, Input, DropdownMenu, Tooltip, Direction).
- Path aliases `@/*` → `src-ui/*` (tsconfig + Vite); helpers split into `lib/music.ts` + `lib/utils.ts` (`cn`).
- Marketing site scaffold in `website/` (Next.js + same shadcn preset) and product brief in `PRODUCT.md` (landing direction; not part of the desktop binary).

### Changed

**Listening tools**

- Playlists page is no longer a stub; Shelves and Download join the sidebar.
- Settings Audio tab: speed, pitch, ReplayGain.
- `TrackDto` gains `track_number` / `has_cover` for shelves.

**Architecture / IPC**

- Split the monolithic desktop UI into focused modules: `Sidebar`, `Catalog`, `Stage`, `PlayerBar`, `SettingsPanel`, `CommandPalette`, `ContextMenu`, plus shared `types` / `lib` and a `useOptMusic` hook — `main.tsx` is now a thin composition shell.
- Playback ticker and playback-only commands (`pause`, `seek`, `volume`, `next`, …) refresh via `playback_state` / `optmusic://state` instead of re-serializing the full library snapshot.
- Playback ticker uses `try_lock` and skips a tick when the controller mutex is busy (avoids blocking the UI during enrichment).
- `TrackDto` uses mtime captured at scan time (no per-track `fs::metadata` on every snapshot).
- Library walk no longer follows symlinks; default `~/Music` is not scanned twice when already listed in `music_dirs` (`paths_equivalent`).

**Visual / frontend stack**

- Desktop UI migrated to Tailwind v4 (`@tailwindcss/vite`) + stone dark theme; layout CSS bridged to semantic tokens (`--background`, `--sidebar`, …).
- Icons: Lucide → Phosphor; display font: Lora Variable; dark `theme` class on `index.html`.
- Settings toggles use shadcn `Switch`; primary actions (Open files / Add folder / Done) use shadcn `Button`.
- Command palette (⌘K) and Settings use the native `<dialog>` API (`showModal` / `::backdrop`) instead of custom veil overlays.
- Artist and album grids render as semantic lists (`<ul>` / `<li>`) for clearer structure and accessibility.

**Docs / agent guidance**

- Explicit release-channel scheme (`alpha` / `beta` / `stable`) documented in `CHANGELOG.md` and `AGENTS.md`; desktop 0.1.2 labeled **beta** (local/dev only — not GitHub Release / AUR).
- `.gitignore` entries for `website/` build outputs.

## [CLI 0.2.8] - 2026-07-21

### Added

- **`playlist`** / **`plists`** / **`pls`** — list, create, import M3U, export, add track, delete.
- Play history (`~/.option/music/cache/history.jsonl`) for “played this week”.
- Tag write + richer reads (track/disc number, ReplayGain fields, lyrics).
- Config `replaygain` (`off` | `track` | `album`).
- Core APIs for smart shelves, lyrics, tag edit — shared with desktop.

### Changed

- Tag cache key bumped (`v2`) to include new fields.

## [Desktop 0.1.1] - 2026-07-20

> Predates channel suffixes — treated historically as an early unstable desktop release.

### Added

- Audex-style left sidebar: brand + traffic lights, Library / Artists / Playlists / Favorites, Recent list, Search trigger, Open files, Settings.
- **⌘K / Ctrl+K** command palette to search and play tracks (↑↓ navigate, Enter play, Esc close) — replaces the old Search tab.
- Artists browser with two modes: **metadata** (default, tags via lofty) or **folder** names — shared `artist_source` in `~/.option/music/config.toml` (CLI settings `c` → Artists, desktop Settings → Library).
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
  - Search: 8 results per page with next/prev, multi-select, cached under `~/.option/music/cache/dl/` (auto-purged every **3 days**)
  - Presets after selection: **best** · **economy** · **lower** · **custom**
  - Options: quality, filetype/container, embed thumbnail + music metadata, subtitles (scan → default **embed** when available; also separate / both / none)
  - Multi-select / multi-URL batch: only options common to **all** selected items (capability intersection)
  - Output defaults to the **current directory** (opt-in other dir); `-o` still works for direct mode
  - Direct mode: `msc dl URL --audio`, `msc download -p soundcloud "query" -a`
  - Wizard UI modes: **`arrows`** (default — ↑↓ / checkboxes) and **`type`** (typed prompts); set via `msc dl --ui arrows|type`, settings **`c` → Dl UI**, or `dl_ui` in `~/.option/music/config.toml`
  - Download kinds: **audio only** · **video only** · **both** (separate files); each format choice is its own selection screen (no left/right cycling)
  - Embeds: full metadata pack, cover thumbnail inside the file, subtitles embedded into video (no loose `.srt` option)
  - Subtitles limited to **en/pt/es** (+ skip translated auto-subs) with `--ignore-errors` so a 429 can’t kill the video download
  - After selecting **exactly one** item: optional **audio preview** — quiet fetch (spinner only, no remux) into a slim player without list/settings (`q` returns to the download wizard)
  - Higher-contrast greyscale arrow UI (inverted focus row + clearer copy)
- Requires system **yt-dlp** (and **ffmpeg** for extract / embed).

## [Desktop 0.1.0] - 2026-07-19

> Predates channel suffixes — first desktop shell (historically an early unstable release).

### Added

- First desktop release built with React, TypeScript, Vite, Bun and Tauri 2.
- A shared Rust `CoreController` drives the desktop and uses the existing `libmpv` player rather than WebView audio.
- Tauri commands and live `optmusic://state` snapshots for scanning, playback, seeking, queue operations, favorites, volume and EQ.
- Automatic scan of `~/Music`, plus picker-based additional library folders persisted in `~/.option/music/config.toml`.
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
- Persistent config at **`~/.option/music/config.toml`** (each option + “reset all” can revert to defaults; `d` resets the selected row).

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
