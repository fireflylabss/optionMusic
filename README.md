# ♪ optMusic

**optMusic** (*option music*) — minimal black & white CLI music player written in Rust.  
Powered by **MPV** (`libmpv`).

## Install

### System deps

Linux needs **libmpv** development files:

```bash
# Arch / CachyOS
sudo pacman -S mpv

# Debian / Ubuntu
sudo apt install libmpv-dev pkg-config

# Fedora
sudo dnf install mpv-libs-devel pkgconf-pkg-config
```

### Build & install

```bash
export CARGO_TARGET_DIR="$(pwd)/target"
cargo install --path . --force
```

Binaries on your PATH:

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
msc list ./music --recursive
msc info song.mp3
msc version
```

Global options:

| Flag | Meaning |
|------|---------|
| `-m` / `--music-dir DIR` | Library root (default `~/Music` when `play` has no paths) |

Play options:

| Flag | Meaning |
|------|---------|
| `-v` / `--volume` | 0–100 (default 80) |
| `-f` / `--speed` | playback speed factor |
| `-c` / `--crossfade` | audio-fade seconds between loads |
| `-s` / `--shuffle` | shuffle playlist |
| `-l` / `--loop` | loop playlist |

### Keyboard (during playback)

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
| `s` | stop |
| `h` / `?` | help toast |
| `q` | quit |

## Features

- MPV-backed playback (mp3, flac, ogg, wav, m4a, opus, aac, …)
- Mute, long seek, EQ presets, crossfade, speed & pitch
- Default music directory (`~/Music`)
- Live progress, centered B&W UI on an **alternate screen** (zero scrollback leak)
- Instant controls (no Enter)
- Shuffle & loop

## Requirements

- Rust 1.70+
- **libmpv** (see Install)
- System audio (PipeWire / PulseAudio / ALSA)

If the build fails with `failed to initialize libmpv` or missing `mpv` pkg-config, install the packages above and retry.

## License

MIT
