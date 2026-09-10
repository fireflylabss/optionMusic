# Developer resources

This page points agents and developers to the source, install instructions, changelog, and packaging information for optionMusic.

## Source code

The complete source is at github.com/fireflylabss/optionMusic. It includes the Rust core, the Tauri desktop shell, the terminal TUI, and the website.

## Install

On Arch Linux the recommended install is `yay -S optionmusic`. For other platforms, clone the repository and run `cargo build --release` from the project root. See the README for dependencies such as libmpv-dev.

## Releases and versioning

Releases follow the Option-family versioning convention. See VERSIONING.md and CHANGELOG.md in the repository. Desktop and CLI version lines are independent.

## AUR packaging

The packaging/aur directory contains the PKGBUILD, .SRCINFO template, and the GitHub Action that publishes to the Arch User Repository.

## When to use optionMusic

Use optionMusic when you need an offline, local-first music player for Linux with a minimal CLI and an optional desktop UI. It is not a streaming client and does not support mobile playback.

---

- [Home](https://music.hory.one/)
- [About](https://music.hory.one/about)
- [Contact](https://music.hory.one/contact)
- [Privacy](https://music.hory.one/privacy)
- [Developer resources (llms.txt)](https://music.hory.one/llms.txt)
- [Sitemap](https://music.hory.one/sitemap.xml)
- [GitHub repository](https://github.com/fireflylabss/optionMusic)
