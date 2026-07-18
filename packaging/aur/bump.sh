#!/usr/bin/env bash
# Bump packaging/aur for a tagged release (no makepkg required — CI-friendly).
# Usage: ./packaging/aur/bump.sh v0.2.5
#        ./packaging/aur/bump.sh 0.2.5
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
PKGBUILD="$ROOT/packaging/aur/PKGBUILD"
SRCINFO="$ROOT/packaging/aur/.SRCINFO"

TAG="${1:?usage: bump.sh <version|vVersion>}"
VER="${TAG#v}"
TARBALL_URL="https://github.com/fireflylabss/optionMusic/archive/refs/tags/v${VER}.tar.gz"

echo "==> waiting for $TARBALL_URL"
for _ in $(seq 1 12); do
  if curl -fsI "$TARBALL_URL" >/dev/null 2>&1; then
    break
  fi
  sleep 5
done

echo "==> hashing tarball"
SHA="$(curl -fsSL "$TARBALL_URL" | sha256sum | awk '{print $1}')"
echo "    sha256=$SHA"

echo "==> updating PKGBUILD → $VER"
sed -i "s/^pkgver=.*/pkgver=${VER}/" "$PKGBUILD"
sed -i "s/^pkgrel=.*/pkgrel=1/" "$PKGBUILD"
sed -i "s/^sha256sums=.*/sha256sums=('${SHA}')/" "$PKGBUILD"

echo "==> writing .SRCINFO"
cat > "$SRCINFO" <<EOF
pkgbase = optionmusic
	pkgdesc = Minimal black and white CLI music player powered by MPV
	pkgver = ${VER}
	pkgrel = 1
	url = https://github.com/fireflylabss/optionMusic
	arch = x86_64
	license = Apache-2.0
	makedepends = cargo
	depends = mpv
	depends = gcc-libs
	depends = glibc
	optdepends = cava: optional spectrum bars
	provides = optmusic
	conflicts = optmusic
	replaces = optmusic
	options = !lto
	source = optionmusic-${VER}.tar.gz::https://github.com/fireflylabss/optionMusic/archive/refs/tags/v${VER}.tar.gz
	sha256sums = ${SHA}

pkgname = optionmusic
EOF

echo "==> done (packaging/aur ready for AUR push)"
