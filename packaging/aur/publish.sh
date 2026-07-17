#!/usr/bin/env bash
# Publish / update optmusic on the AUR (local helper).
# Usage:
#   ./packaging/aur/publish.sh           # push current packaging/aur
#   ./packaging/aur/publish.sh 0.2.5     # bump first, then push
#
# CI does this automatically on GitHub Release — see
# .github/workflows/publish-aur.yml
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
AUR_DIR="${AUR_DIR:-$HOME/aur/optmusic}"
PKGBUILD_SRC="$ROOT/packaging/aur/PKGBUILD"
SRCINFO_SRC="$ROOT/packaging/aur/.SRCINFO"
SSH_KEY="${AUR_SSH_KEY:-$HOME/.ssh/aur_synara}"
REMOTE="ssh://aur@aur.archlinux.org/optmusic.git"

NEW_VER="${1:-}"

if [[ -n "$NEW_VER" ]]; then
  "$ROOT/packaging/aur/bump.sh" "$NEW_VER"
fi

VER=$(grep '^pkgver=' "$PKGBUILD_SRC" | cut -d= -f2)
echo "==> publishing optmusic $VER to AUR"

mkdir -p "$AUR_DIR"
cp "$PKGBUILD_SRC" "$AUR_DIR/PKGBUILD"
cp "$SRCINFO_SRC" "$AUR_DIR/.SRCINFO"

if [[ ! -d "$AUR_DIR/.git" ]]; then
  echo "==> init AUR git repo in $AUR_DIR"
  (
    cd "$AUR_DIR"
    git init -b master
    git remote remove origin 2>/dev/null || true
    git remote add origin "$REMOTE"
  )
fi

export GIT_SSH_COMMAND="ssh -i ${SSH_KEY} -o IdentitiesOnly=yes"

cd "$AUR_DIR"
# refresh remote in case of divergence
git fetch origin master 2>/dev/null || true
git pull --rebase origin master 2>/dev/null || true

git add PKGBUILD .SRCINFO
if git diff --cached --quiet; then
  echo "==> nothing to commit"
else
  git commit -m "optmusic ${VER}"
fi

echo "==> pushing to AUR…"
git push -u origin HEAD:master
echo "==> done → https://aur.archlinux.org/packages/optmusic"
