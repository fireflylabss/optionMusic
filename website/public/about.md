# About optionMusic

optionMusic is a local-first music player for people who still own their music. It wraps the MPV engine in a calm, black-and-white interface that stays out of the way.

## Why local-first

We believe music files should live on your drive, not in someone else's cloud. optionMusic does not require an account, does not upload metadata, and keeps playback offline. Your library is stored exactly where you put it, and your playlists are plain M3U files you can move anywhere.

## Open source

The CLI, desktop UI, and website source are in the same repository under the GNU General Public License. Source code, changelogs, and packaging scripts are available on GitHub. Contributions, bug reports, and packaging help are welcome via GitHub issues and pull requests.

## How it is built

The core is written in Rust and uses libmpv for audio decoding and playback. The desktop surface is built with Tauri and React, while the terminal interface is a crossterm-based TUI. Both share the same playback engine and configuration format.

---

- [Home](https://music.hory.one/)
- [About](https://music.hory.one/about)
- [Contact](https://music.hory.one/contact)
- [Privacy](https://music.hory.one/privacy)
- [Developer resources (llms.txt)](https://music.hory.one/llms.txt)
- [Sitemap](https://music.hory.one/sitemap.xml)
- [GitHub repository](https://github.com/fireflylabss/optionMusic)
