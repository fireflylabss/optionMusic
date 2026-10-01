# Desktop packaging

Assets and the release pipeline for the GPUI desktop shell (`optionmusic-gpui`).

## Layout

- `macos/` — `Info.plist` (`__VERSION__` is filled by the release workflow), `icon.icns`, first-launch `README.txt` shipped inside the DMG.
- `linux/` — `optionmusic.desktop`, hicolor `icons/`, `install.sh` (installs to `~/.local`), `README.txt` shipped in the tarball.
- `windows/` — `README.txt` shipped in the zip (the player needs `mpv-2.dll` next to the `.exe`).
- `icons/` — `optionmusic.svg` is the single icon source; `regenerate.sh` rebuilds the PNG hicolor set and `macos/icon.icns` (needs `rsvg-convert`, and `iconutil` on macOS for the icns).

## Cutting a release

`.github/workflows/release-desktop.yml` runs on tags `desktop-v*` (or manual dispatch with a tag input):

```bash
git tag -a desktop-v0.1.8-beta -m "optionMusic desktop 0.1.8-beta"
git push origin desktop-v0.1.8-beta
```

The workflow builds `cargo build --release --locked` in `src-gpui/` on each platform and uploads to a GitHub Release named after the tag:

| Asset | Contents |
| --- | --- |
| `optionMusic-macos-arm64.dmg` | `optionMusic.app` (arm64; needs `brew install mpv`) |
| `optionmusic-linux-x86_64.tar.gz` | binary + `.desktop` + icons + `install.sh` (needs distro `mpv`) |
| `optionmusic-windows-x86_64.zip` | `optionmusic-gpui.exe` + `mpv-2.dll` + README |

Tags containing a channel suffix (`-beta`, `-alpha`, …) are marked prerelease.
