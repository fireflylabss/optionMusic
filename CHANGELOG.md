# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
- **CHANGELOG.md** and **ROADMAP.md**.

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
