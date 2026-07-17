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

Help (`?` / `h`) and playlist (`l`) open as **overlays** (player stays centered). Both can be open at once.

## Install

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

```bash
# Arch / CachyOS (if building from source)
sudo pacman -S mpv cava

# Debian / Ubuntu
sudo apt install libmpv-dev pkg-config cava

# Fedora
sudo dnf install mpv-libs-devel pkgconf-pkg-config cava
```

PipeWire or PulseAudio should be running if you use cava.

### Build from source

```bash
export CARGO_TARGET_DIR="$(pwd)/target"
cargo install --path . --force
# or a tagged release:
cargo install --git https://github.com/fireflylabss/optMusic --tag v0.2.5
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
| `s` | stop |
| `h` / `?` | toggle help sidebar |
| `↑` `↓` / `j` `k` | scroll playlist (when open) |
| `q` / Esc | quit (or close help / playlist) |

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
