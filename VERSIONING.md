# Hory versioning

We follow [Semantic Versioning](https://semver.org/) and [Keep a Changelog](https://keepachangelog.com/). still has one installable surface: CLI, including its interactive TUI.

The public changelog follows Hory style and is the source of truth for release headings and channel language. See the expandable blurb at the top of [CHANGELOG.md](CHANGELOG.md).

## Surface

| Surface | What you install | Cargo version | Release tag |
| --- | --- | --- | --- |
| **CLI** | `still` / alias `msc` | `0.3.0` | `v0.3.0-beta.1` |

`Cargo.toml` owns the numeric artifact version; `Cargo.lock` must match it. Both binaries report that same version. The channel belongs to the release tag and changelog heading, so Cargo stays plain SemVer.

The private marketing site in `website/` is not another installable player surface. Its package version is independent of the CLI. The AUR packages `stillmusic` (source) and `stillmusic-bin` (prebuilt) use the same numeric version, channel and revision as the release, encoded as `0.3.0beta1` for Arch version ordering. Both install the same CLI surface and conflict with each other and the legacy `optionmusic` package. The GitHub repository keeps its existing name.

## Changelog entries

New release tags use `vX.Y.Z-channel`, with `alpha`, `beta` or `stable` as the channel. Alpha and beta can append a positive revision number (`vX.Y.Z-beta.1`); the channel stays beta and Cargo stays numeric. Stable tags have no revision suffix. A release heading combines that tag and a date in `DD/MM/YYYY` format:

```md
## v0.3.0-beta.1 · 25/10/2026

still music rebranding, a persistent library index, queue editing and local lyrics management. This version was made for CLI with a beta release channel on 25/10/2026 (v0.3.0-beta.1).

- Describe the concrete changes a listener will notice.
```

Use one short summary paragraph, ending with the surface, channel, date and tag in human prose, followed by feature bullets. CLI/TUI is one surface: no `m` in new tags and no `### CLI`, `### Added` or `### Removed` subsections. Use at most one catch-all bullet such as `Various other small tweaks` per entry.

Historical Desktop and mixed entries remain in the changelog with their original versions and dates. They describe the removed desktop app and do not define a current release line.

## Channels

| Suffix | Meaning |
| --- | --- |
| **-alpha** | Very early. Missing pieces and lots of bugs. |
| **-beta** | Mostly there, but still rough. Not the official install. |
| **-stable** | Ready for daily use. |

Do **not** label something `stable` unless it is release-ready. Use **alpha** for early work with missing pieces and **beta** for builds that still need refinement. GitHub Releases and AUR may carry beta builds; publication alone does not make a version stable. Legacy entries without channel suffixes retain their original tags.

## Work in progress

Expand the matching top entry while a version is being prepared. Choose a new version once the previous one already appears in the historical release notes; do not reuse a release tag for different work. The current rebrand and CLI improvements form `v0.3.0-beta.1`. The `v0.3.0-beta` tag remains immutable after its cancelled release attempt; it did not produce a published release. The earlier `v0.2.19-beta` draft was never released.

A corrected alpha or beta attempt increments the revision (`-beta` → `-beta.1` → `-beta.2`) instead of deleting, moving or force-pushing a tag. Revision tags retain the same readiness channel and get their own changelog heading and artifacts. The release workflow must pass both its build and dependency audit before publication.

Changing these local files does not publish a release. Git tags, GitHub Releases and AUR updates happen separately when publication is requested. Preserve historical tags and dates, including the original CLI and Desktop entries that share legacy tag names.
