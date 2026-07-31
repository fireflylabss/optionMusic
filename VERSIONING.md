# Versioning

This project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html) with an explicit **release channel** suffix, and [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## Surfaces

| Surface | What it is |
|---------|------------|
| **CLI** | `optmusic` / `msc` (Rust) |
| **Desktop** | Tauri / visual shell |

Desktop and CLI are versioned **independently**. Changelog headings name the surface, e.g. `## [CLI 0.2.10]` / `## [Desktop 0.1.4-beta]`.

Desktop **0.1.0** / **0.1.1** predate channel suffixes (shipped as plain versions). From **0.1.2-beta** onward, every desktop entry uses a channel suffix. Beta/alpha desktop builds are development snapshots — not GitHub Releases or AUR packages unless explicitly promoted to **stable**.

## Release channels (`x.y.z-<channel>`)

| Channel | Tag example | Meaning |
|---------|-------------|---------|
| **alpha** | `0.1.0-alpha` | Extremely early. Features incomplete; bugs are expected and common. |
| **beta** | `0.1.2-beta` | Feature set nearly complete, but still rough — bugs and hard edges remain. |
| **stable** | `0.2.0-stable` | Production-ready: finished for that version, few or no known bugs. |

Do **not** label something `stable` unless it is actually release-ready. Prefer **beta** for desktop while the Tauri shell is still maturing; use **alpha** only for brand-new / half-built surfaces.
