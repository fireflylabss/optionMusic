# Changelog

We follow [Semantic Versioning](https://semver.org/) and [Keep a Changelog](https://keepachangelog.com/). CLI and Desktop version separately.

<details>
<summary>To see more about versioning, expand this.</summary>

Every version string starts with `v` (required), e.g. `v0.2.14`, `v0.1.6-beta`.

Here the installable surfaces are **CLI** and **Desktop**. Other Option apps swap in their own names the same way — e.g. **GTK**, **Web**, **GNOME** — whatever you actually ship.

| Part | What you install | Example |
| --- | --- | --- |
| **CLI** | `optionmusic` / alias `msc` in the terminal | `v0.2.14` |
| **Desktop** | the visual app | `v0.1.6-beta` |

A new CLI does not always mean a new Desktop app, and the other way around.

Sometimes one cut ships **both** surfaces. That is a **mixed release**: one tag with an `m` before the channel (ex: `v0.2.12m-beta`), and the notes break out each surface so a small touch on one side does not look equal to a large cut on the other.

Each release heading is the version and date (`## v0.2.12m-beta · 03/08/2026`); under it, a short summary ends with a plain sentence like: “This version was made for both desktop and CLI with a beta release channel on 03/08/2026 (v0.2.12m-beta).”

### What the Desktop suffix means

| Suffix | In plain words |
| --- | --- |
| **-alpha** | Very early. Expect missing pieces and lots of bugs. |
| **-beta** | Mostly there, but still rough. Fine to try; not the “official” install. |
| **-stable** | Ready for daily use. This is what we put on GitHub Releases and the AUR. |

We only call something **stable** when we mean it. While the desktop app is still settling, builds stay **beta**.

Older desktop builds (`v0.1.0`, `v0.1.1`) had no channel suffix. From `v0.1.2-beta` on, every desktop (and mixed) version includes one.

</details>

## v0.2.14-beta · 29/08/2026

Library core on the CLI: incremental refresh, an artists → albums → tracks browser, and genre/year search across the tree. This version was made for CLI with a beta release channel on 29/08/2026 (v0.2.14-beta).

### CLI (`0.2.14`)

- New `src/library.rs` shared library core: one-time scan plus incremental `refresh()` driven by mtime/size — removed/renamed files are pruned, changed files are re-tagged, unchanged files keep their cached tags (zero re-parse).
- `msc library` / `msc lib` subcommand: `refresh` prints `+added · -removed · changed · total`, `ls` lists tracks with indices, `paths` prints absolute paths for scripts / `xargs`.
- `msc browse` / `msc br`: tree navigation (artists → albums → tracks) with album/track counts, genre (`g`) and year (`y`) filters, unified search (`/`), favorites toggle (`f`) plus a favorites-only mode (`1` / `--favorites`), and an in-memory queue (`a` append, `n` play-next, `tab` queue view with `j/k` reorder, `enter` play).
- `AudioTags`/`Track` now read genre and year (cache key bumped to `v3`); incomplete-metadata and missing-cover badges shown in the browser.
- Permission-denied subtrees during a scan are surfaced as distinct warnings instead of a generic empty list.
- Various other small tweaks

## v0.2.13-stable · 22/08/2026

The legacy binary alias era ends: `optmusic` is gone, `optionmusic` and `msc` remain. This version was made for CLI with a stable release channel on 22/08/2026 (v0.2.13-stable).

### CLI (`0.2.13`)

- Removed the legacy `optmusic` binary alias. The canonical binary is `optionmusic`, with the `msc` short alias.
- Bumped the package to `0.2.13`.

## v0.2.12m-beta · 03/08/2026

Shared path ownership, safe playlist persistence, and desktop download boundary hardening. This version was made for both desktop and CLI with a beta release channel on 03/08/2026 (v0.2.12m-beta).

### Desktop (`0.1.6-beta`)

- Resolve the Tauri download destination through the same validated output-directory boundary used by the CLI.
- Use the shared SDK identity and migration path for the music configuration tree.

### CLI (`0.2.12`)

- Playlist ids are strict single-segment ids and no longer collide for same-second creations.
- Playlist/config/M3U writes use the SDK atomic-write primitive.
- Adopt `optionSDK` 0.1.3 and the canonical `optionMusic` display identity.

## v0.2.11m-beta · 01/08/2026

Product branding is optionMusic everywhere, with matching desktop package and event names. This version was made for both desktop and CLI with a beta release channel on 01/08/2026 (v0.2.11m-beta).

### Desktop

- Package / lib renamed to `optionmusic-desktop` / `optionmusic_desktop_lib`.
- Tauri events are now `optionmusic://state` and `optionmusic://library-enriched`.
- UI hook renamed to `useOptionMusic`.
- Various other small tweaks

### CLI

- Product and crate branding is **optionMusic** / `optionmusic` everywhere. **`optmusic`** remains only as a binary alias (with `msc`).
- Env var is `OPTIONMUSIC_MUSIC_DIR` (was `OPTMUSIC_MUSIC_DIR`).
- Various other small tweaks

## v0.2.10m-beta · 29/07/2026

Desktop bundle id rename, with a matching CLI identity touch. This version was made for both desktop and CLI with a beta release channel on 29/07/2026 (v0.2.10m-beta).

### Desktop

- Bundle identifier is now `io.option.music` (was `com.aefireflylabs.optmusic`). Reinstall the `.desktop` file if an old install still shows up under the previous id.
- Various other small tweaks

### CLI

- Help / about text now mention the same `io.option.music` family id where relevant.
- Various other small tweaks

## v0.2.9m-beta · 29/07/2026

Shared config path move under `~/.option/music/`. This version was made for both desktop and CLI with a beta release channel on 29/07/2026 (v0.2.9m-beta).

### Desktop

- Settings UI and on-disk state read/write the new `~/.option/music/` layout (config, cache, playlists).
- First launch migrates leftover data from `~/option/music/` when present.
- Various other small tweaks

### CLI

- `msc` / `optmusic` resolve config and cache from `~/.option/music/` the same way as Desktop.
- Various other small tweaks

## v0.2.8m-beta · 21/07/2026

Big Desktop beta plus shared playlist / shelves / listening tools on the CLI engine. This version was made for both desktop and CLI with a beta release channel on 21/07/2026 (v0.2.8m-beta).

### Desktop

- Local playlists: create, rename, delete, add/remove tracks; import and export M3U. Same engine as `msc playlist …` on the CLI.
- Smart shelves for recent listening gaps: Played this week, No cover, Incomplete albums.
- Listening focus mode hides the chrome so you only get cover + scrub (Esc to leave).
- Desktop yt-dlp download dialog (search → select → download into the music folder).
- Pitch & speed, ReplayGain (`off` / `track` / `album`), offline lyrics, and light tag edit from the UI.
- Two-phase library scan with background tag enrichment, disk tag/cover caches, and virtualized track lists for large libraries.
- Modular UI shell (Sidebar, Catalog, Stage, PlayerBar, Settings, Command palette) on Tailwind v4 + shadcn; marketing site scaffold in `website/`.
- Various other UI polish

### CLI

- `playlist` / `plists` / `pls` — list, create, import M3U, export, add track, delete.
- Play history (`~/.option/music/cache/history.jsonl`) for “played this week”.
- Tag write + richer reads (track/disc number, ReplayGain fields, lyrics); config `replaygain` (`off` | `track` | `album`).
- Core APIs for smart shelves, lyrics, and tag edit — shared with Desktop.
- Various other small tweaks

## v0.1.1 · 20/07/2026

Sidebar shell, artists/albums, session resume, and listening rail. This version was made for desktop on 20/07/2026 (v0.1.1).

- Audex-style left sidebar: brand + traffic lights, Library / Artists / Playlists / Favorites, Recent list, Search trigger, Open files, Settings.
- ⌘K / Ctrl+K command palette to search and play tracks (↑↓ navigate, Enter play, Esc close).
- Artists browser with metadata or folder name modes; shared `artist_source` in `~/.option/music/config.toml`.
- Artist detail view with Albums grid + Tracks list; album art in stage, mini-player, and cards.
- Session resume: last track, position, and queue restored paused on reopen.
- Now-playing listening rail on the right; Settings as a centered popup (Library / Playback / Audio).
- Various other UI polish

## v0.2.7 · 19/07/2026

Interactive yt-dlp downloader for YouTube, YouTube Music, and SoundCloud. This version was made for CLI on 19/07/2026 (v0.2.7).

- `download` / `dl` / `d` wizard: provider → search/URL(s) → select → preset → options → download.
- Search with paging and multi-select; results cached under `~/.option/music/cache/dl/` (auto-purged every 3 days).
- Presets (best · economy · lower · custom), quality/filetype options, embeds, and limited subtitles (en/pt/es).
- Wizard UI modes: `arrows` (default) or `type`, via `--ui`, settings `c`, or `dl_ui` in config.
- Optional audio preview after picking a single item; requires system yt-dlp (and ffmpeg for extract / embed).
- Various other small tweaks

## v0.1.0 · 19/07/2026

First desktop shell on Tauri 2 with the shared Rust core. This version was made for desktop on 19/07/2026 (v0.1.0).

- React / TypeScript / Vite / Bun + Tauri 2; `CoreController` drives playback via `libmpv` (not WebView audio).
- Live `optionmusic://state` snapshots for scanning, playback, queue, favorites, volume, and EQ.
- Automatic `~/Music` scan plus picker-based library folders in `~/.option/music/config.toml`.
- Library search, favorites, queue actions, reveal in file manager, and desktop settings.
- Various other bug fixes

## v0.2.6 · 17/07/2026

Persistent settings sidebar and richer playlist panel. This version was made for CLI on 17/07/2026 (v0.2.6).

- Settings left sidebar with `c` — excess volume, cava styles, LDM, accent presets; config at `~/.option/music/config.toml`.
- Playlist as a left sidebar (`l`) with mouse-wheel scroll, clickable rows, and a draggable scrollbar.
- Various other bug fixes

## v0.2.5 · 17/07/2026

Loop cycle, richer CLI flags, and cleaner help. This version was made for CLI on 17/07/2026 (v0.2.5).

- In-session loop cycle with `o`: off → list → track → off.
- CLI flags for pitch, EQ, quiet, loop-file / repeat-one; aliases `p`/`pl`, `i`, `ls`, `ver`; bare path plays.
- Filename / path line off by default (`f` still toggles it).
- Various other small tweaks

## v0.2.4 · 17/07/2026

Help and playlist as overlays, plus AUR packaging. This version was made for CLI on 17/07/2026 (v0.2.4).

- Help (`?` / `h`) and playlist (`l`) are overlays that no longer shift the centered player; both can be open at once.
- Playlist sidebar: mouse-wheel scroll and an easier scrollbar.
- Available on the AUR as [`optmusic`](https://aur.archlinux.org/packages/optmusic) (later renamed to `optionmusic`).
- Various other small tweaks

## v0.2.3 · 17/07/2026

Cava bar look, toast overlay, and solid next/prev. This version was made for CLI on 17/07/2026 (v0.2.3).

- `f` toggles the filename/path line under the track title.
- Cava classic vertical bars under the footer; toast is a floating top-right overlay.
- Help as a right sidebar; playlist as a left sidebar with scroll and click-to-jump.
- Fixed `n` / `p` double-skip from treating `EndFile(Stop)` as natural EOF.
- Various other bug fixes

## v0.2.2 · 17/07/2026

Cleaner status row, richer mouse hits around volume, and Rust 2024. This version was made for CLI on 17/07/2026 (v0.2.2).

- Status row no longer shows decorative EQ bars after play/pause; live spectrum stays under the footer.
- Smoother cava strip; help sidebar; clickable volume − / +.
- Rust edition 2024 (MSRV 1.85).
- Various other small tweaks

## v0.2.1 · 17/07/2026

Optional cava spectrum, richer mouse UI, and Apache-2.0. This version was made for CLI on 17/07/2026 (v0.2.1).

- Optional greyscale cava strip (`--cava` or `v`); richer mouse scrub / transport / clickable status chips.
- Clearer pause glyph; `AGENTS.md` and Apache License 2.0.
- Package license MIT → Apache-2.0.
- Various other small tweaks

## v0.2.0 · 17/07/2026

MPV engine rewrite with mute, EQ, speed/pitch, and a zero-leak TUI. This version was made for CLI on 17/07/2026 (v0.2.0).

- Audio engine via `libmpv2` (replaces rodio); mute, long seek, EQ presets, crossfade, speed & pitch.
- Default music dir (`-m` / `--music-dir`); alternate-screen UI with clean scrollback restore.
- Unit tests for playlist, EQ, MPV clamps, music-dir resolution, CLI parsing, and UI helpers.
- Requires system libmpv. Various other small tweaks

## v0.1.0 · 17/07/2026

Initial CLI release. This version was made for CLI on 17/07/2026 (v0.1.0).

- Dual bins `optmusic` / `msc`, rodio playback, alternate-screen B&W UI, play / list / info / version.
