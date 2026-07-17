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

Help (`?` / `h`) opens a **left sidebar** and shifts the player right.

## Install

### System deps

| Dep | Why |
|-----|-----|
| **libmpv** | playback engine (required) |
| **cava** | spectrum strip (optional) |

```bash
# Arch / CachyOS
sudo pacman -S mpv cava

# Debian / Ubuntu
sudo apt install libmpv-dev pkg-config cava

# Fedora
sudo dnf install mpv-libs-devel pkgconf-pkg-config cava
```

PipeWire or PulseAudio should be running if you use cava.

### Build & install

```bash
export CARGO_TARGET_DIR="$(pwd)/target"
cargo install --path . --force
```

| Command | Description |
|---------|-------------|
| `optmusic` | full name |
| `msc` | short alias |

## Usage

```bash
msc play song.mp3
optmusic play ./music --shuffle --loop
msc play song.flac -v 60 -f 1.25 -c 2
msc play -m ~/Music
msc play song.mp3 --cava             # enable spectrum strip
msc list ./music --recursive
msc info song.mp3
msc version
```

### Global options

| Flag | Meaning |
|------|---------|
| `-m` / `--music-dir DIR` | Library root (default `~/Music` when `play` has no paths) |
| `--cava` | Enable cava spectrum strip (off by default) |

### Play options

| Flag | Meaning |
|------|---------|
| `-v` / `--volume` | 0–100 (default 80) |
| `-f` / `--speed` | playback speed factor |
| `-c` / `--crossfade` | audio-fade seconds between loads |
| `-s` / `--shuffle` | shuffle playlist |
| `-l` / `--loop` | loop playlist |

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
| `l` | toggle playlist |
| `r` | shuffle |
| `f` | toggle filename / path line |
| `v` | toggle cava strip |
| `s` | stop |
| `h` / `?` | toggle help sidebar |
| `q` / Esc | quit (or close help) |

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
| scroll wheel | seek ±5s |

## Cava bars

Off by default. With `--cava` or `v`, and `cava` installed, optMusic draws **classic vertical bars under the shortcut footer** (content width, soft greys). No decorative viz in the status row.

- PipeWire first, Pulse fallback
- Click the strip or press `v` to toggle
- Missing cava → strip unavailable; playback unaffected

## Features

- MPV-backed playback (mp3, flac, ogg, wav, m4a, opus, aac, …)
- Mute, long seek, EQ presets, crossfade, speed & pitch
- Default music directory (`~/Music`)
- Optional cava spectrum bars (opt-in)
- Mouse scrub + clickable controls
- Centered B&W UI on an **alternate screen** (zero scrollback leak)
- Instant controls (no Enter)
- Shuffle & loop

## Requirements

- Rust **1.85+** (edition 2024)
- **libmpv** (see Install)
- System audio (PipeWire / PulseAudio / ALSA)
- Optional: **cava** for the spectrum strip

## License

Apache License 2.0 — see [`LICENSE`](LICENSE).
