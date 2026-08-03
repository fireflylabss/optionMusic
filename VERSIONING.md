# Versioning

We follow [Semantic Versioning](https://semver.org/) and [Keep a Changelog](https://keepachangelog.com/). CLI and Desktop version separately.

The public changelog is the source of truth for headings, mixed releases, and channel language — see the expandable blurb at the top of [CHANGELOG.md](CHANGELOG.md).

## Surfaces

| Surface | What you install | Artifact version today |
| --- | --- | --- |
| **CLI** | `optionmusic` / aliases `optmusic` · `msc` | `0.2.12` (`Cargo.toml`) |
| **Desktop** | Tauri visual app | `0.1.6` (`package.json` / `tauri.conf.json`) |

Changelog tags always include the leading `v`. Mixed cuts that change both surfaces use an `m` before the channel (e.g. `v0.2.12m-beta`).

## Channels

| Suffix | Meaning |
| --- | --- |
| **-alpha** | Very early. Missing pieces and lots of bugs. |
| **-beta** | Mostly there, but still rough. Not the official install. |
| **-stable** | Ready for daily use — GitHub Releases / AUR. |

Do **not** label something `stable` unless it is release-ready. Prefer **beta** for Desktop while the Tauri shell is maturing. Older Desktop builds (`v0.1.0`, `v0.1.1`) had no channel suffix; from `v0.1.2-beta` on, every Desktop (and mixed) version includes one.
