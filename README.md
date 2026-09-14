# ♪ optionMusic

**optionMusic** (*option music*) — minimal black & white CLI music player written in Rust.  
Powered by **MPV** (`libmpv`), with an optional discreet **cava** spectrum strip.

<video src="website/public/demo.mp4" controls muted loop></video>

Help (`?` / `h`) is a **right sidebar**, while settings (`c`) and playlist (`l`) are **left sidebars**. The playlist makes layout space so it remains easy to use.

## Install

### Desktop app

The native GPUI desktop client lives in `src-gpui/` and shares the CLI engine
via the `optionmusic` crate:

```bash
cd src-gpui && cargo run    # toolchain pinned in rust-toolchain.toml
```

### Arch / CachyOS (AUR)

```bash
yay -S optionmusic
# or
paru -S optionmusic
```

(`msc` still works as a short alias after install.)
### System deps

| Dep | Why |
|-----|-----|
| **libmpv** | playback engine (required) |
| **cava** | spectrum strip (optional) |
| **yt-dlp** | download command (optional) |
| **ffmpeg** | audio extract/convert for downloads (optional) |

```bash
# Arch / CachyOS (if building from source)
sudo pacman -S mpv cava yt-dlp ffmpeg

# Debian / Ubuntu
sudo apt install libmpv-dev pkg-config cava yt-dlp ffmpeg

# Fedora
sudo dnf install mpv-libs-devel pkgconf-pkg-config cava yt-dlp ffmpeg
```

PipeWire or PulseAudio should be running if you use cava.

### Windows

Built from source (no installer yet; CI also uploads `optionmusic.exe` / `msc.exe` artifacts).
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
cargo install --path . --force
# or a tagged release:
cargo install --git https://github.com/fireflylabss/optionMusic --tag v0.2.14-beta
```

| Command | Description |
|---------|-------------|
| `optionmusic` | canonical name |
| `msc` | short alias |

## Usage

```bash
msc p song.mp3
msc song.mp3                          # bare path = play
msc pl ./album
optionmusic play ./music --shuffle --loop
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
msc doctor                                          # check yt-dlp / ffmpeg / cava on PATH
msc --help
```

### Global options

| Flag | Meaning |
|------|---------|
| `-m` / `--music-dir DIR` | Library root (default `~/Music`; env `OPTIONMUSIC_MUSIC_DIR`) |
| `--cava` | Enable cava spectrum strip (off by default) |
| `-q` / `--quiet` | Less stdout noise outside the TUI |

### Play options

| Flag | Meaning |
|------|---------|
| `-v` / `--volume` | 0–100 (default: last used, else 80) |
| `-f` / `--speed` | playback speed factor (default: last used) |
| `--pitch` | pitch factor (default: last used, else 1.0) |
| `--eq` | starting EQ (default: last used, else `off`; `bass` `treble` `rock` `vocal` `lofi`) |
| `-c` / `--crossfade` | audio-fade seconds between loads |
| `-s` / `--shuffle` | shuffle playlist |
| `-l` / `--loop` / `--repeat` | loop playlist |
| `--loop-file` / `--repeat-one` | repeat current track |

`msc play` with no paths and no playback flags **resumes** the last session —
saved queue order, last track paused at the saved position. Toggle with
settings `c` → Resume or `resume = false` in config.

### Download (`download` / `dl` / `d`)

Uses system **yt-dlp**. Interactive wizard (`msc dl`):

1. Pick **provider** (YouTube / YouTube Music / SoundCloud)
2. Enter a **search** or **URL(s)** — multiple URLs with `url1;url2`
3. Search shows **8 results/page** (n/p pages, multi-select); results cached 3 days in `~/.option/music/cache/dl/`
4. After picking **one** item, opt-in **preview** opens the normal optionMusic player on a temp audio file (`q` back)
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
| `msc radio` / `rd` | Endless queue from your library — similarity walk (artist · album · genre · era) weighted by play stats, penalizing what you heard this week. Seed: `QUERY`, `--artist`, `--genre`; `--fresh` favors rarely-played tracks. Same playback flags as `play` |
| `msc stats` / `st` | Top tracks & artists, totals (`-n` limit) |
| `msc sleep MIN\|off` | Sleep timer — fades playback after N minutes |
| `msc playlist` / `pls` | `ls` · `create` · `import`/`export` M3U · `add` · `delete` |
| `msc library` / `lib` | `refresh` (incremental index) · `ls` · `paths` (script-friendly) |
| `msc doctor` / `check` | Which external tools were found on `PATH`, what each one is for, and whether `msc dl` is ready |

### Keyboard

| Key | Action |
|-----|--------|
| `space` | pause / resume |
| `n` / `↓` | next |
| `p` / `↑` | previous (or restart if >3s) |
| `←` / `→` | seek −5s / +5s |
| `{` / `}` | seek −60s / +60s |
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
| `c` | settings sidebar |
| `s` | stop |
| `h` / `?` | toggle help sidebar |
| `←` `→` / `↑` `↓` / `j` `k` | scroll playlist (when open) |
| `q` / Esc | quit (or close settings / help / playlist) |

### Settings (`c`)

Left sidebar. Persisted in `~/.option/music/config.toml`:

Grouped into sections — **player** · **interface** · **library & dl**:

| Option | Meaning |
|--------|---------|
| Excess vol | Allow volume up to 200% |
| Resume | Persist prefs + session; `msc play` resumes (on by default) |
| Discord | Discord Rich Presence via local IPC (off by default) |
| Accent | Color accent (presets or `#RRGGBB` in the file) |
| LDM | Fewer animations, lighter redraw |
| Cava | Style (`bars` / `dense` / `mirror` / `dots`) and height |
| Lyrics | Docked lyrics strip: `above` / `below` / `hidden` |
| Toast pos | Toast anchor position |
| Toast stack | Stack up to 3 toasts (off = newest only) |
| Artists | Artists browser grouping: `metadata` (default) or `folder` |
| Dl UI | Download wizard UI: `arrows` (default) or `type` |
| Dl fallbk | yt-dlp 403 fallback: `ask` (default) / `auto` / `off` |

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

Off by default. With `--cava` or `v`, and `cava` installed, optionMusic draws a spectrum under the shortcut footer. Style and height are configurable in settings (`c` → Cava styles).

- PipeWire first, Pulse fallback
- Click the strip or press `v` to toggle
- Missing cava → strip unavailable; playback unaffected

## Discord presence

Opt-in Rich Presence — toggle in settings `c` → Discord or `discord_rpc = true`
in `~/.option/music/config.toml`. It talks to the Discord client over the local
IPC socket only (no network): title / artist — album with a live progress bar,
pause-aware, and works in `msc browse` too.

Works out of the box via the built-in `optionMusic` application id. To use
your own app instead, set `discord_rpc_id` in `config.toml`.

Cover art: Discord can only render image *URLs*, so tracks coming from
YouTube — streamed via `msc play <url>` or downloaded files with the
`[videoId]` tag in the name — show the video thumbnail through Discord's
media proxy. Everything else falls back to the `logo` art asset registered
on the app (local embedded covers can't be sent over IPC).

## Features

- MPV-backed playback (mp3, flac, ogg, wav, m4a, opus, aac, …)
- Session memory — prefs + queue/track/position persist and resume
- Library tree (`msc browse`), favorites, playlists (M3U), stats & history
- `msc radio` — endless similarity-walked queue from your own library
- Sleep timer, smart shuffle, lyrics
- Mute, long seek, EQ presets, crossfade, speed & pitch
- Discord Rich Presence (opt-in, local IPC, thumbnails for YT tracks)
- Default music directory (`~/Music`)
- Optional cava spectrum bars (opt-in)
- yt-dlp downloader (YouTube / YouTube Music / SoundCloud)
- Mouse scrub + clickable controls
- Centered B&W UI on an **alternate screen** (zero scrollback leak)
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
