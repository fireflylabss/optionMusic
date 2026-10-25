#!/usr/bin/env bash
set -euo pipefail

aur_root="$(cd "$(dirname "$0")" && pwd)"
tag="${1:?usage: bump.sh <vX.Y.Z-channel>}"
tag="v${tag#v}"
if [[ ! "$tag" =~ ^v([0-9]+\.[0-9]+\.[0-9]+)(-(alpha|beta)(\.([1-9][0-9]*))?|-stable)?$ ]]; then
  echo "Invalid release tag: $tag" >&2
  exit 1
fi
pkgver="${BASH_REMATCH[1]}"
channel="${BASH_REMATCH[3]:-}"
revision="${BASH_REMATCH[5]:-}"
if [[ "$channel" == alpha || "$channel" == beta ]]; then
  pkgver+="$channel$revision"
fi
version="${tag#v}"
command -v makepkg >/dev/null || {
  echo 'makepkg is required to regenerate .SRCINFO (run on Arch Linux).' >&2
  exit 1
}

release_url='https://github.com/fireflylabss/optionMusic'
work_dir="$(mktemp -d)"
trap 'rm -rf "$work_dir"' EXIT

for package in stillmusic stillmusic-bin; do
  mkdir "$work_dir/$package"
  case "$package" in
    stillmusic)
      source_url="$release_url/archive/refs/tags/$tag.tar.gz"
      ;;
    stillmusic-bin)
      source_url="$release_url/releases/download/$tag/stillmusic-$tag-x86_64-unknown-linux-gnu.tar.gz"
      ;;
  esac
  echo "==> Downloading $source_url"
  curl --fail --location --retry 5 --retry-delay 3 \
    --output "$work_dir/$package.tar.gz" "$source_url"
  tar -tzf "$work_dir/$package.tar.gz" > "$work_dir/$package.files"
  if [[ "$package" == stillmusic ]]; then
    grep -Fxq "optionMusic-$version/Cargo.toml" "$work_dir/$package.files"
  else
    for file in still msc LICENSE README.md; do
      grep -Eq "^(\\./)?$file$" "$work_dir/$package.files"
    done
  fi
  checksum="$(sha256sum "$work_dir/$package.tar.gz")"
  checksum="${checksum%% *}"
  sed -e "s/^pkgver=.*/pkgver=$pkgver/" \
      -e "s/^_tag=.*/_tag=$version/" \
      -e 's/^pkgrel=.*/pkgrel=1/' \
      -e "s/^sha256sums=.*/sha256sums=('$checksum')/" \
      "$aur_root/$package/PKGBUILD" > "$work_dir/$package/PKGBUILD"
  (cd "$work_dir/$package" && makepkg --printsrcinfo > .SRCINFO)
done

for package in stillmusic stillmusic-bin; do
  cp "$work_dir/$package/PKGBUILD" "$aur_root/$package/PKGBUILD"
  cp "$work_dir/$package/.SRCINFO" "$aur_root/$package/.SRCINFO"
done
echo "==> Both AUR recipes now use verified downloads for $tag"
