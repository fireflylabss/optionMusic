#!/bin/sh
# Regenerate the app icon PNGs and macOS .icns from icons/optionmusic.svg.
#
# Needs: rsvg-convert (brew install librsvg / apt install librsvg2-bin)
#        iconutil    (macOS only — skipped elsewhere)
set -eu

cd "$(dirname "$0")/.."
SRC="icons/optionmusic.svg"

for size in 16 32 48 64 128 256 512 1024; do
  out="linux/icons/hicolor/${size}x${size}/apps"
  mkdir -p "$out"
  rsvg-convert -w "$size" -h "$size" "$SRC" -o "$out/optionmusic.png"
done

mkdir -p linux/icons/hicolor/scalable/apps
cp "$SRC" linux/icons/hicolor/scalable/apps/optionmusic.svg

if command -v iconutil >/dev/null 2>&1; then
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' EXIT
  set_dir="$tmp/optionmusic.iconset"
  mkdir -p "$set_dir"
  for pair in \
    "16 icon_16x16" "32 icon_16x16@2x" \
    "32 icon_32x32" "64 icon_32x32@2x" \
    "128 icon_128x128" "256 icon_128x128@2x" \
    "256 icon_256x256" "512 icon_256x256@2x" \
    "512 icon_512x512" "1024 icon_512x512@2x"; do
    set -- $pair
    rsvg-convert -w "$1" -h "$1" "$SRC" -o "$set_dir/$2.png"
  done
  iconutil -c icns "$set_dir" -o macos/icon.icns
fi

echo "Icons regenerated under linux/icons/hicolor/ and macos/icon.icns"
