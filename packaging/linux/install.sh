#!/bin/sh
# optionMusic desktop installer — installs into $PREFIX (default ~/.local).
# Run from the unpacked release tarball: ./install.sh [--uninstall]
set -eu

PREFIX="${PREFIX:-$HOME/.local}"
HERE="$(cd "$(dirname "$0")" && pwd)"

uninstall() {
  rm -f "$PREFIX/bin/optionmusic-gpui"
  rm -f "$PREFIX/share/applications/optionmusic.desktop"
  find "$HERE/icons/hicolor" -type f -name 'optionmusic.*' | while read -r icon; do
    rm -f "$PREFIX/share/icons/hicolor/${icon#"$HERE/icons/hicolor/"}"
  done
  echo "Removed optionmusic-gpui from $PREFIX"
}

if [ "${1:-}" = "--uninstall" ]; then
  uninstall
  exit 0
fi

install -Dm755 "$HERE/optionmusic-gpui" "$PREFIX/bin/optionmusic-gpui"
install -Dm644 "$HERE/optionmusic.desktop" "$PREFIX/share/applications/optionmusic.desktop"
(cd "$HERE/icons" && find hicolor -type f -name 'optionmusic.*') | while read -r icon; do
  install -Dm644 "$HERE/icons/$icon" "$PREFIX/share/icons/$icon"
done

if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database "$PREFIX/share/applications" >/dev/null 2>&1 || true
fi

echo "Installed to $PREFIX"
echo "  - ensure '$PREFIX/bin' is on your PATH"
echo "  - ensure 'mpv' is installed (see README.txt)"
