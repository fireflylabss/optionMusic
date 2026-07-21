# ♪ optMusic

**optMusic** (*option music*) — minimal black & white CLI music player written in Rust.  
Powered by **MPV** (`libmpv`), with an optional discreet **cava** spectrum strip.

```
♪  optMusic
   track title
   ───●────────
   ◂ ⏸ paused ▸  ·  1/12  ·  − 80% +
   space · n/p · ←→ · ?
      ▄ █ ▄
    ▄ █ █ █ ▄     ← cava bars under shortcuts (opt-in)
   ▁▅█████▅▁
```

Help (`?` / `h`) is a **right sidebar**, while settings (`c`) and playlist (`l`) are **left sidebars**. The playlist makes layout space so it remains easy to use.

## Install

### Desktop app

The React/Tauri desktop client lives alongside the Rust CLI. Install Bun, then
from the repository root:

```bash
bun install
bun run tauri:dev    # desktop app with libmpv playback
```

Browser-only `bun run dev` is a UI preview — it cannot play audio. Package with
`bun run tauri build` (Tauri / WebKit system deps may be required).
`bun run build` runs a locked `bun install` then Vite for the web assets.

### Arch / CachyOS (AUR)

```bash
yay -S optmusic
# or
paru -S optmusic
```

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

### Build from source

```bash
export CARGO_TARGET_DIR="$(pwd)/target"
cargo install --path . --force
# or a tagged release:
cargo install --git https://github.com/fireflylabss/optMusic --tag v0.2.7
```

| Command | Description |
|---------|-------------|
| `optmusic` | full name |
| `msc` | short alias |

## Usage

```bash
msc p song.mp3
msc song.mp3                          # bare path = play
msc pl ./album
optmusic play ./music --shuffle --loop
msc play song.flac -v 60 -f 1.25 -c 2 --eq bass
msc play -m ~/Music --pitch 1.05
msc play album/ --loop-file --cava
msc ls ./music --recursive
msc i song.mp3
msc dl                                              # interactive wizard
msc dl https://youtu.be/… --audio                   # direct → cwd
msc download -p soundcloud "ambient" -a
msc --help
```

### Global options

| Flag | Meaning |
|------|---------|
| `-m` / `--music-dir DIR` | Library root (default `~/Music`; env `OPTMUSIC_MUSIC_DIR`) |
| `--cava` | Enable cava spectrum strip (off by default) |
| `-q` / `--quiet` | Less stdout noise outside the TUI |

### Play options

| Flag | Meaning |
|------|---------|
| `-v` / `--volume` | 0–100 (default 80) |
| `-f` / `--speed` | playback speed factor |
| `--pitch` | pitch factor (default 1.0) |
| `--eq` | starting EQ (`off` `bass` `treble` `rock` `vocal` `lofi`) |
| `-c` / `--crossfade` | audio-fade seconds between loads |
| `-s` / `--shuffle` | shuffle playlist |
| `-l` / `--loop` / `--repeat` | loop playlist |
| `--loop-file` / `--repeat-one` | repeat current track |

### Download (`download` / `dl` / `d`)

Uses system **yt-dlp**. Interactive wizard (`msc dl`):

1. Pick **provider** (YouTube / YouTube Music / SoundCloud)
2. Enter a **search** or **URL(s)** — multiple URLs with `url1;url2`
3. Search shows **8 results/page** (n/p pages, multi-select); results cached 3 days in `~/option/music/cache/dl/`
4. After picking **one** item, opt-in **preview** opens the normal optMusic player on a temp audio file (`q` back)
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

Left sidebar. Persisted in `~/option/music/config.toml`:

| Option | Meaning |
|--------|---------|
| Excess volume | Allow volume up to 200% |
| Cava styles | Style (`bars` / `dense` / `mirror` / `dots`) and height |
| LDM | Fewer animations, lighter redraw |
| Accent | Color accent (presets or `#RRGGBB` in the file) |
| Dl UI | Download wizard UI: `arrows` (default) or `type` |

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

Off by default. With `--cava` or `v`, and `cava` installed, optMusic draws a spectrum under the shortcut footer. Style and height are configurable in settings (`c` → Cava styles).

- PipeWire first, Pulse fallback
- Click the strip or press `v` to toggle
- Missing cava → strip unavailable; playback unaffected

## Features

- MPV-backed playback (mp3, flac, ogg, wav, m4a, opus, aac, …)
- Mute, long seek, EQ presets, crossfade, speed & pitch
- Default music directory (`~/Music`)
- Optional cava spectrum bars (opt-in)
- yt-dlp downloader (YouTube / YouTube Music / SoundCloud)
- Mouse scrub + clickable controls
- Centered B&W UI on an **alternate screen** (zero scrollback leak)
- Instant controls (no Enter)
- Shuffle & loop

## Requirements

- Rust **1.85+** (edition 2024)
- **libmpv** (see Install)
- System audio (PipeWire / PulseAudio / ALSA)
- Optional: **cava** for the spectrum strip
- Optional: **yt-dlp** (+ **ffmpeg** for audio) for `msc dl`

## License

Apache License 2.0 — see [`LICENSE`](LICENSE).
