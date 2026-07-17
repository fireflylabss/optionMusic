#!/usr/bin/env bash
# Publish / update optmusic on the AUR.
# Usage:
#   ./packaging/aur/publish.sh           # sync files + push current pkgver
#   ./packaging/aur/publish.sh 0.2.5     # bump to a new tag/version first
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
AUR_DIR="${AUR_DIR:-$HOME/aur/optmusic}"
PKGBUILD_SRC="$ROOT/packaging/aur/PKGBUILD"
SRCINFO_SRC="$ROOT/packaging/aur/.SRCINFO"
SSH_KEY="${AUR_SSH_KEY:-$HOME/.ssh/aur_synara}"
REMOTE="ssh://aur@aur.archlinux.org/optmusic.git"

NEW_VER="${1:-}"

if [[ -n "$NEW_VER" ]]; then
  VER="$NEW_VER"
  echo "==> bumping to $VER"
  SHA=$(curl -fsSL "https://github.com/fireflylabss/optMusic/archive/refs/tags/v${VER}.tar.gz" | sha256sum | awk '{print $1}')
  sed -i "s/^pkgver=.*/pkgver=${VER}/" "$PKGBUILD_SRC"
  sed -i "s/^sha256sums=.*/sha256sums=('${SHA}')/" "$PKGBUILD_SRC"
else
  VER=$(grep '^pkgver=' "$PKGBUILD_SRC" | cut -d= -f2)
  echo "==> using pkgver=$VER from PKGBUILD"
fi

mkdir -p "$AUR_DIR"
cp "$PKGBUILD_SRC" "$AUR_DIR/PKGBUILD"
(
  cd "$AUR_DIR"
  makepkg --printsrcinfo > .SRCINFO
)
cp "$AUR_DIR/.SRCINFO" "$SRCINFO_SRC"

if [[ ! -d "$AUR_DIR/.git" ]]; then
  echo "==> init AUR git repo in $AUR_DIR"
  (
    cd "$AUR_DIR"
    git init -b master
    git remote remove origin 2>/dev/null || true
    git remote add origin "$REMOTE"
  )
fi

GIT_SSH_COMMAND="ssh -i ${SSH_KEY} -o IdentitiesOnly=yes"
export GIT_SSH_COMMAND

cd "$AUR_DIR"
git add PKGBUILD .SRCINFO
if git diff --cached --quiet; then
  echo "==> nothing to commit"
else
  git commit -m "optmusic ${VER}"
fi

echo "==> pushing to AUR…"
git push -u origin HEAD:master
echo "==> done → https://aur.archlinux.org/packages/optmusic"
