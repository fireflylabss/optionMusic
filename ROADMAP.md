# optMusic Roadmap

Minimal black & white CLI music player. Stay compact; add power only when it earns its keep.

## Overview

**Status**: v0.2.0 — MPV engine, mute, long seek, EQ presets, crossfade, speed/pitch, music-dir, zero-leak UI.

---

## v0.3.0 — Polish

- [ ] Persist last volume / EQ / music-dir under `~/.config/optmusic/`
- [ ] Gapless playlist transitions (MPV playlist queue)
- [ ] Optional in-place zero-leak mode (no alternate screen) for nested terminals
- [ ] Cover / metadata display when tags are present
- [ ] ReplayGain / loudness normalization

---

## v0.4.0 — Library

- [ ] Indexed library scan with watch
- [ ] Simple playlists (m3u save/load)
- [ ] Fuzzy find / filter in list view
- [ ] Shuffle modes (album-aware)

---

## v0.5.0 — Beyond local

- [ ] URL / stream play via MPV + yt-dlp (opt-in)
- [ ] Search-and-play helper (optional, behind a flag)

---

## Non-goals

- Full TUI dashboard or colored themes by default
- Desktop GUI (see firemusic for that stack)
- Social / cloud sync

Keep the vibe: centered, black & white, one job — play music well from the terminal.
