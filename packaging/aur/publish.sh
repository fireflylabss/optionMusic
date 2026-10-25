#!/usr/bin/env bash
set -euo pipefail

aur_root="$(cd "$(dirname "$0")" && pwd)"
aur_base="${AUR_DIR:-$HOME/aur}"
ssh_key="${AUR_SSH_KEY:-$HOME/.ssh/aur_devin}"
if [[ -n "${1:-}" ]]; then
  "$aur_root/bump.sh" "$1"
fi

[[ -r "$ssh_key" ]] || { echo "Missing AUR SSH key: $ssh_key" >&2; exit 1; }
export GIT_SSH_COMMAND="ssh -i '${ssh_key//\'/\'\\\'\'}' -o IdentitiesOnly=yes -o BatchMode=yes"

# Validate both recipes before making either remote change.
verify_dir="$(mktemp -d)"
trap 'rm -rf "$verify_dir"' EXIT
for package in stillmusic stillmusic-bin; do
  (
    cd "$aur_root/$package"
    bash -n PKGBUILD
    makepkg --printsrcinfo | diff -u .SRCINFO -
    if grep -Eq 'UNRELEASED|SKIP' PKGBUILD; then
      echo "$package needs real release checksums; run bump.sh first." >&2
      exit 1
    fi
    mkdir "$verify_dir/$package"
    cp PKGBUILD "$verify_dir/$package/PKGBUILD"
    cd "$verify_dir/$package"
    makepkg --verifysource
  )
done

for package in stillmusic stillmusic-bin; do
  (
    package_dir="$aur_base/$package"
    remote="ssh://aur@aur.archlinux.org/$package.git"
    mkdir -p "$package_dir"
    cd "$package_dir"
    if [[ ! -d .git ]]; then
      if [[ -n "$(ls -A)" ]]; then
        echo "Refusing to initialize a nonempty directory: $package_dir" >&2
        exit 1
      fi
      git init -b master
      git remote add origin "$remote"
    fi
    [[ "$(git remote get-url origin)" == "$remote" ]] || {
      echo "Unexpected origin in $package_dir" >&2
      exit 1
    }
    [[ -z "$(git status --porcelain)" ]] || {
      echo "Commit or stash changes in $package_dir before publishing." >&2
      exit 1
    }
    remote_head="$(git ls-remote --heads origin master)"
    if [[ -n "$remote_head" ]]; then
      git fetch origin master
      if git rev-parse --verify HEAD >/dev/null 2>&1; then
        [[ "$(git branch --show-current)" == master ]] || {
          echo "Expected branch master in $package_dir" >&2
          exit 1
        }
        git rebase origin/master
      else
        git checkout -B master origin/master
      fi
    fi
    cp "$aur_root/$package/PKGBUILD" PKGBUILD
    cp "$aur_root/$package/.SRCINFO" .SRCINFO
    version="$(sed -n 's/^pkgver=//p' PKGBUILD)"
    git add PKGBUILD .SRCINFO
    if ! git diff --cached --quiet; then
      git commit -m "$package $version"
    fi
    git push -u origin HEAD:master
    local_head="$(git rev-parse HEAD)"
    remote_head="$(git ls-remote origin refs/heads/master)"
    [[ "${remote_head%%[[:space:]]*}" == "$local_head" ]] || {
      echo "AUR remote verification failed for $package" >&2
      exit 1
    }
    echo "==> Verified: https://aur.archlinux.org/packages/$package ($local_head)"
  )
done
