# ♪ still

**still music** (*still*) — minimal black & white CLI music player written in Rust.
Powered by **MPV** (`libmpv`), with an optional discreet **cava** spectrum strip.

still is terminal-only. The former GPUI desktop app has been removed;
shared local config, library and cache files are left untouched.

<video src="website/public/demo.mp4" controls muted loop></video>

Help (`?` / `h`) is a **right sidebar**, while settings (`c`) and playlist (`l`) are **left sidebars**. The playlist makes layout space so it remains easy to use.

## Install

### Arch / CachyOS (AUR)

```bash
# Build from source
paru -S stillmusic
# Or install the prebuilt release
paru -S stillmusic-bin
```

Both packages install `still` and its short alias `msc`; choose one. The legacy
`optionmusic` package conflicts with them and can be replaced during installation.
The repository remains
[`fireflylabss/optionMusic`](https://github.com/fireflylabss/optionMusic).
See [AUR packaging](packaging/aur/README.md) for package sources and validation.

### Platform support

| Platform | Status |
|----------|--------|
| **Linux** | supported — developed against, built and tested in CI, packaged on the AUR |
| **macOS / Windows** | best-effort — built in CI, but not used day to day; `msc version` says so |
| **Other Unix** (BSD) | best-effort — should build with libmpv installed, untested |

Every platform needs a system **libmpv** to link against; the TUI assumes a
terminal that speaks ANSI, and the optional tools (`cava`, `yt-dlp`) are looked
up on `PATH`.
Linux and macOS builds also need SQLite development files. Windows builds bundle
SQLite, while still requiring the libmpv package described below.

### System deps

| Dep | Why |
|-----|-----|
| **libmpv** | playback engine (required) |
| **SQLite** | persistent library index (required) |
| **cava** | spectrum strip (optional) |
| **yt-dlp** | download command (optional) |
| **ffmpeg** | audio extract/convert for downloads (optional) |

```bash
# Arch / CachyOS (if building from source)
sudo pacman -S mpv sqlite cava yt-dlp ffmpeg

# Debian / Ubuntu
sudo apt install libmpv-dev libsqlite3-dev pkg-config cava yt-dlp ffmpeg

# Fedora
sudo dnf install mpv-libs-devel sqlite-devel pkgconf-pkg-config cava yt-dlp ffmpeg
```

PipeWire or PulseAudio should be running if you use cava.

### Windows

Built from source (no installer yet; CI also uploads `still.exe` / `msc.exe` artifacts).
Grab the **`mpv-dev-x86_64-*.7z`** package from the
[shinchiro/mpv-winbuild-cmake releases](https://github.com/shinchiro/mpv-winbuild-cmake/releases) — it
contains `libmpv-2.dll` and the `libmpv.dll.a` import library.

```powershell
# GNU toolchain (matches the import lib shipped with libmpv)
rustup toolchain install stable-x86_64-pc-windows-gnu
$env:RUSTFLAGS = "-L C:\path\to\mpv-dev"
cargo +stable-x86_64-pc-windows-gnu build --release

# put libmpv-2.dll next to the exe (or on PATH)
copy C:\path\to\mpv-dev\libmpv-2.dll target\release\
```

Optional tools: `winget install yt-dlp.yt-dlp Gyan.FFmpeg` for `msc dl`.
`cava` is not available on Windows, so the spectrum strip is disabled there.
Use a modern terminal (Windows Terminal) for proper colours and mouse support.

### Build from source

```bash
export CARGO_TARGET_DIR="$(pwd)/target"
cargo install --path . --force --locked
# remote source:
cargo install --git https://github.com/fireflylabss/optionMusic --locked
```

The Cargo package and crate are named `stillmusic`.

Use `--locked` so installation follows the checked-in `Cargo.lock`.

The current release line is `v0.3.0-beta.1`; `still --version` and `msc --version`
report the numeric Cargo version `0.3.0`. The beta revision identifies the
corrected release attempt; it does not change the readiness channel.

| Command | Description |
|---------|-------------|
| `still` | canonical name |
| `msc` | short alias |

## Usage

```bash
msc p song.mp3
msc song.mp3                          # bare path = play
msc pl ./album
still play ./music --shuffle --loop
msc play song.flac -v 60 -f 1.25 -c 2 --eq bass
msc play -m ~/Music --pitch 1.05
msc play album/ --loop-file --cava
msc ls ./music --recursive
msc i song.mp3
msc dl                                              # interactive wizard
msc dl https://youtu.be/… --audio                   # direct → cwd
msc download -p soundcloud "ambient" -a
msc browse                                          # library tree: artists → albums → tracks
msc browse -f                                       # favorites only
msc stats -n 10                                     # top tracks & artists
msc sleep 30                                        # fade out in 30 min (off to clear)
msc playlist ls                                     # saved playlists
msc radio                                           # endless radio from your library
msc radio "daft punk"                               # seed by search
msc rd --genre trip-hop --fresh                     # seed by genre, rediscovery mode
msc radio save morning "daft punk" --fresh          # save a station
msc radio load morning                              # play a station back
msc radio ls                                        # list stations
msc doctor                                          # check libmpv / audio / tools / config / data
msc sync export backup.tar.gz                     # backup config / playlists / stats / history / index
msc sync import backup.tar.gz --force             # restore a backup (needs --force to overwrite)
msc --help
```

### Global options

| Flag | Meaning |
|------|---------|
| `-m` / `--music-dir DIR` | Library root (default `~/Music`; env `STILLMUSIC_MUSIC_DIR`) |
| `--cava` | Enable cava spectrum strip (off by default) |
| `-q` / `--quiet` | Less stdout noise outside the TUI |
| `--json` | NDJSON playback events on stdout (forces non-interactive mode) |

### Play options

| Flag | Meaning |
|------|---------|
| `-v` / `--volume` | 0–100 (default: last used, else 80) |
| `-f` / `--speed` | playback speed factor (default: last used) |
| `--pitch` | pitch factor (default: last used, else 1.0) |
| `--eq` | starting EQ (default: last used, else `off`; `bass`, `treble`, `rock`, `vocal`, `lofi`, `acoustic`, `jazz`, `classical`, `electronic`, `pop`, `speech`, `warm`) |
| `-c` / `--crossfade` | audio-fade seconds between loads |
| `-s` / `--shuffle` | shuffle playlist |
| `-l` / `--loop` / `--repeat` | loop playlist |
| `--loop-file` / `--repeat-one` | repeat current track |

`msc play` with no paths and no playback flags **resumes** the last session —
saved queue order, last track paused at the saved position. Toggle with
settings `c` → Resume or `resume = false` in config.

### Scripts and pipes

Without a TTY (or with `--json`) playback runs headless: the queue plays
straight through, listening time still counts toward `msc stats`, and `Ctrl-C`
(or `SIGTERM`/`SIGHUP`) stops MPV, saves, and exits `128 + signo` (`130` for
Ctrl-C) — press it twice to bail out immediately.

`--json` prints one event per line, so it can be consumed while playing:

```bash
msc play --json ~/Music | jq -r 'select(.event == "track") | .title'
```

Events: `start`, `track` (index/total/path/title/artist/album/duration),
`track_end` (played seconds, `completed`), then `end` or `interrupted`.

### Download (`download` / `dl` / `d`)

Uses system **yt-dlp**. Interactive wizard (`msc dl`):

1. Pick **provider** (YouTube / YouTube Music / SoundCloud)
2. Enter a **search** or **URL(s)** — multiple URLs with `url1;url2`
3. Search shows **8 results/page** (n/p pages, multi-select); results cached 3 days in `~/.option/music/cache/dl/`
4. After picking **one** item, opt-in **preview** opens the normal still player on a temp audio file (`q` back)
5. Choose a **preset**, then quality / filetype / embeds (one screen each)
6. Batch options are the intersection of all selected items
7. Saves to the **current directory** by default (opt-in other dir)

| Flag | Meaning |
|------|---------|
| `QUERY` | URL or search (omit → interactive wizard) |
| `-p` / `--provider` | `youtube` · `youtube-music` · `soundcloud` |
| `-a` / `--audio` | extract audio only (direct mode) |
| `--video` | download video only (direct mode) |
| `--both` | video file + separate audio file (direct mode) |
| `-o` / `--output DIR` | output directory (default: cwd) |
| `--audio-format FMT` | audio container for direct `--audio` (default `mp3`) |
| `-i` / `--interactive` | force the wizard |
| `--ui arrows\|type` | wizard UI (default `arrows`; also settings `c` → Dl UI) |

### Library & extras

| Command | What |
|---------|------|
| `msc browse` / `br` | Tree: artists → albums → tracks, with queue playback; `-f` favorites only |
| `msc radio` / `rd` | Endless queue from your library — similarity walk (artist · album · genre · era) weighted by play stats, penalizing this week's listens plus skipped/disliked tracks and artists (7 days). Seed: `QUERY`, `--artist`, `--genre`; `--fresh` favors rarely-played tracks. Stations: `save NAME` / `load NAME` / `ls` / `rm`. In player: `d` dislike+skip, `n` skip. Non-interactive: `--dislike ID` / `--skip ID` (absolute path). Same playback flags as `play` |
| `msc stats` / `st` | Top tracks & artists, totals, skips/dislikes, scrobbles (`-n` limit) |
| `msc sleep MIN\|off` | Sleep timer — fades playback after N minutes |
| `msc playlist` / `pls` | `ls` · `create` · `import`/`export` M3U · `add` · `delete` |
| `msc library` / `lib` | `refresh` (incremental SQLite index) · `ls` · `paths` (script-friendly) · `roots` (configured folders). Multi-folder via `music_dirs` in config; `browse` watches folders live and refreshes the tree |
| `msc doctor` / `check` | libmpv + version, audio backend (PipeWire/Pulse/ALSA), `yt-dlp`/`ffmpeg`/`cava` on `PATH` with versions, `~/Music` + extra folder permissions, valid `config.toml`, intact SQLite library index, readable stats/history/sleep, scrobble status — one `OK`/`FALTA` line per check, exit 1 on any `FALTA` |
| `msc sync` / `backup` | `export FILE.tar.gz` / `import FILE.tar.gz` — backup/restore `config.toml`, `playlists/`, `stats.json`, `stations/`, `history.jsonl`, `sleep.json` and `cache/library.db` via system `tar` (rebuildable caches excluded); `--force` required to overwrite |

### Lyrics

During `msc play` or `msc radio`, press `y` to open lyrics. Timed lyrics follow
playback; plain text is also supported. With the lyrics panel open, use
`↑↓` / `j/k` or Page Up / Page Down to scroll, Enter to follow playback, and
`Y` to reload after changing a selection. The mouse wheel scrolls the panel.
In `msc browse`, `y` remains the year filter.

Lookup checks your saved selection, a local `.lrc` sidecar, the provider cache,
and embedded lyrics before searching online through LRCLIB, NetEase, and
lyrics.ovh. Online lookup sends track metadata to those services and runs
in the background when you open the panel.

Choose a different match or manage your own lyrics:

```bash
msc lyrics search --artist "Daft Punk" --title "Get Lucky"
msc lyrics select ./song.flac RESULT_ID
msc lyrics import ./song.flac ./song.lrc   # .txt also accepted
msc lyrics show ./song.flac               # offline, with source
msc lyrics remove ./song.flac             # remove only the saved selection
msc lyrics remove ./song.flac --cache     # also clear its automatic cache
```

Selections are saved separately in `~/.option/music/lyrics/` and included in
`msc sync` backups. Import and removal leave the audio file and sidecar untouched.

### Keyboard

| Key | Action |
|-----|--------|
| `space` | pause / resume |
| `n` / `p` | next / previous (previous restarts if >3s) |
| `d` | dislike + skip (radio: 7-day penalty for track/artist) |
| `←` / `→` | seek −5s / +5s |
| `↑` `↓` / `j` `k` | navigate the open playlist, help, or lyrics; otherwise arrows change tracks |
| `{` / `}` | seek −60s / +60s |
| `J` / `K` · `Shift+↑↓` | move track in playlist (open list first) |
| `a` / `A` | play next / append to end (open list first) |
| `D` | clear queue keeping current (`u` undo) |
| `u` · `Ctrl+Z` | undo queue edit |
| `g` | jump list cursor to current track |
| `+` / `-` | volume |
| `m` | mute |
| `e` | cycle EQ preset |
| `[` / `]` | speed down / up |
| `,` / `.` | pitch down / up |
| `0` | reset speed & pitch |
| `1`–`9` | jump to track N |
| `o` | cycle loop (`off` → `list` → `track`) |
| `l` | toggle playlist sidebar |
| `r` | shuffle |
| `f` | toggle filename / path line (off by default) |
| `v` | toggle cava strip |
| `y` / `Y` | toggle lyrics / reload lyrics while open |
| `c` | settings sidebar |
| `s` | stop |
| `h` / `?` | toggle help sidebar |
| `q` / Esc | quit (or close help / playlist / settings / lyrics) |

### Settings (`c`)

Left sidebar. Persisted in `~/.option/music/config.toml`. All sidebars follow
your rice automatically: when `~/.local/state/rice/scheme.json` (matugen)
exists, panels, focus rows, toasts, `browse` and the `dl` wizard use its
`surface` / `primary` / `onSurface` / `outline` roles (dark and light `mode`
both work; regen with `matugen --mode light` applies on the next frame).
Without it, the classic B&W palette + `Accent` below apply. `msc doctor`
reports which one is active (`rice` line).

Set `STILLMUSIC_CONFIG` to use a different config file. Existing data remains
under `~/.option/music/`; the rebrand does not move your library or settings.

Grouped into sections — **player** · **interface** · **library & dl**:

| Option | Meaning |
|--------|---------|
| Excess vol | Allow volume up to 200% |
| Resume | Persist prefs + session; `msc play` resumes (on by default) |
| Discord | Paused for now. The setting is hidden; `discord_rpc` in the file is ignored until it returns |
| Accent | Color accent (presets or `#RRGGBB` in the file) |
| LDM | Fewer animations, lighter redraw |
| Cava | Style (`bars` / `dense` / `mirror` / `dots`) and height |
| Toast pos | Toast anchor position |
| Toast stack | Stack up to 3 toasts (off = newest only) |
| Artists | Artists browser grouping: `metadata` (default) or `folder` |
| Dl UI | Download wizard UI: `arrows` (default) or `type` |
| Dl fallbk | yt-dlp 403 fallback: `ask` (default) / `auto` / `off` |
| Equalizer | Cycle all 13 EQ modes; `d` resets to off |

In the sidebar: `↑↓` move · `enter` / click toggle · `←→` cycle · `d` reset · `c` / Esc close.

### Playlist (`l`)

Left sidebar. Mouse wheel and navigation keys scroll · click a row to jump · click or drag the vertical scrollbar.

### Mouse

| Action | Effect |
|--------|--------|
| click / drag progress | seek / scrub |
| `◂` / `▸` | previous / next |
| `▶` / `⏸` / status | pause / resume |
| `−` / `+` | volume down / up |
| volume % | mute |
| `spd` / `ptch` / `eq` | nudge speed / pitch / cycle EQ |
| cava bars | toggle cava |
| playlist row | jump to track |
| playlist scrollbar | click / drag to scroll |
| scroll wheel on playlist | scroll list |
| scroll wheel elsewhere | seek ±5s |

## Cava bars

Off by default. With `--cava` or `v`, and `cava` installed, still draws a spectrum under the shortcut footer. Style and height are configurable in settings (`c` → Cava styles).

- PipeWire first, Pulse fallback
- Click the strip or press `v` to toggle
- Missing cava → strip unavailable; playback unaffected

## Discord presence

Paused. The player does not open a Discord IPC socket in this cut, and the
settings row is hidden. `discord_rpc` in `config.toml` is kept and ignored
until Rich Presence comes back.

## MPRIS & multimedia keys

On Linux the player exports `org.mpris.MediaPlayer2` on the session bus, so the
desktop drives it like any other player — multimedia keys, panel/lockscreen
widgets, GNOME/KDE media controls, `playerctl`:

```bash
playerctl -p stillmusic play-pause
playerctl -p stillmusic metadata
```

Play/pause, stop, next/prev, seek, position and volume are exposed, plus
title / artist / album / duration metadata. Media keys reach still
through the desktop's own routing (that is what MPRIS is for) — no global
grab. Terminals that forward media keys themselves (kitty keyboard protocol)
also work directly.

On by default; set `mpris = false` in `~/.option/music/config.toml` to opt out.
Without a session bus (TTY, containers) it silently stays off.

## Features

- MPV-backed playback (mp3, flac, ogg, wav, m4a, opus, aac, …)
- Session memory — prefs + queue/track/position persist and resume
- Library tree (`msc browse`), favorites, playlists (M3U), stats & history
- `msc radio` — endless similarity-walked queue from your own library (stations + dislike/skip feedback)
- Editable queue (`J/K` move · `a/A` queue · `D` clear · `u` undo · `g` jump to current)
- Multi-folder libraries (`music_dirs`) on a SQLite index with live folder watching in `browse`
- Opt-in ListenBrainz scrobbling (`listenbrainz_token`); autotag fills title/artist after `msc dl`
- `msc doctor` + `msc sync export/import` — environment checks and tarball backups
- Sleep timer, smart shuffle, and synced/plain lyrics in the terminal (`y`)
- Online lyrics search and per-track selection/import management (`msc lyrics`)
- Mute, long seek, EQ presets, crossfade, speed & pitch
- Discord Rich Presence is paused in this cut
- MPRIS on the session bus — multimedia keys, `playerctl`, desktop widgets
- Default music directory (`~/Music`)
- Optional cava spectrum bars (opt-in)
- yt-dlp downloader (YouTube / YouTube Music / SoundCloud)
- Mouse scrub + clickable controls
- Centered B&W UI on an **alternate screen** (zero scrollback leak)
- Compact layout for small terminals, with Unicode-aware clipping and toast borders
- Instant controls (no Enter)
- Shuffle & loop

## Requirements

- Rust **1.89+** (edition 2024)
- **libmpv** (see Install)
- System audio (PipeWire / PulseAudio / ALSA on Linux, WASAPI on Windows)
- Optional: **cava** for the spectrum strip (Linux only)
- Optional: **yt-dlp** (+ **ffmpeg** for audio) for `msc dl`

## License

Apache License 2.0 — see [`LICENSE`](LICENSE).
