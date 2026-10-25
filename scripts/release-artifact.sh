#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TAG="${1:?usage: release-artifact.sh <vX.Y.Z-channel> [binary-dir] [output-dir]}"
[[ "$TAG" =~ ^v([0-9]+\.[0-9]+\.[0-9]+)-(alpha|beta|stable)$ ]] || {
  echo "Expected a release tag such as v0.3.0-beta" >&2
  exit 1
}
VERSION="${BASH_REMATCH[1]}"
[[ "$(uname -s)" == Linux && "$(uname -m)" == x86_64 ]] || {
  echo "This artifact requires a native x86_64 Linux build" >&2
  exit 1
}
MANIFEST_VERSION="$(sed -n 's/^version = "\([^"]*\)"/\1/p' "$ROOT/Cargo.toml")"
[[ "$VERSION" == "$MANIFEST_VERSION" ]] || {
  echo "Tag $TAG does not match Cargo version $MANIFEST_VERSION" >&2
  exit 1
}

BINARY_DIR="$(cd "${2:-$ROOT/target/release}" && pwd)"
OUTPUT_DIR="${3:-$ROOT/target/dist}"
mkdir -p "$OUTPUT_DIR"
OUTPUT_DIR="$(cd "$OUTPUT_DIR" && pwd)"
STAGING_DIR="$(mktemp -d)"
trap 'rm -rf "$STAGING_DIR"' EXIT

for binary in still msc; do
  [[ -x "$BINARY_DIR/$binary" ]] || { echo "Missing executable: $binary" >&2; exit 1; }
  [[ "$("$BINARY_DIR/$binary" --version)" == "still $VERSION" ]] || {
    echo "$binary does not report still $VERSION" >&2
    exit 1
  }
  file "$BINARY_DIR/$binary" | grep -q 'ELF 64-bit.*x86-64' || {
    echo "$binary is not an x86_64 Linux ELF executable" >&2
    exit 1
  }
  install -m755 "$BINARY_DIR/$binary" "$STAGING_DIR/$binary"
done
install -m644 "$ROOT/LICENSE" "$ROOT/README.md" "$STAGING_DIR/"

ARCHIVE="stillmusic-${TAG}-x86_64-unknown-linux-gnu.tar.gz"
tar --sort=name --mtime="@${SOURCE_DATE_EPOCH:-0}" --owner=0 --group=0 --numeric-owner \
  -czf "$OUTPUT_DIR/$ARCHIVE" -C "$STAGING_DIR" LICENSE README.md msc still
(
  cd "$OUTPUT_DIR"
  sha256sum "$ARCHIVE" > SHA256SUMS
  sha256sum --check SHA256SUMS
)
echo "$OUTPUT_DIR/$ARCHIVE"
