#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TAG="${1:?usage: release-publish.sh <vX.Y.Z-channel> [artifact-dir]}"
[[ "$TAG" =~ ^v[0-9]+\.[0-9]+\.[0-9]+-(alpha|beta|stable)$ ]] || {
  echo "Expected a release tag such as v0.3.0-beta" >&2
  exit 1
}
ARTIFACT_DIR="$(cd "${2:-$ROOT/target/dist}" && pwd)"
ARCHIVE="stillmusic-${TAG}-x86_64-unknown-linux-gnu.tar.gz"
TEMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TEMP_DIR"' EXIT
(
  cd "$ARTIFACT_DIR"
  [[ "$(wc -l < SHA256SUMS)" -eq 1 ]]
  [[ "$(awk '{print $2}' SHA256SUMS)" == "$ARCHIVE" ]]
  sha256sum --check SHA256SUMS
)
awk -v tag="$TAG" '
  /^## / { if (found) exit; if (index($0, "## " tag " · ") == 1) { found = 1; next } }
  found { print }
' "$ROOT/CHANGELOG.md" > "$TEMP_DIR/notes.md"
[[ -s "$TEMP_DIR/notes.md" ]] || { echo "No changelog entry for $TAG" >&2; exit 1; }

if gh release view "$TAG" >/dev/null 2>&1; then
  gh release download "$TAG" --pattern "$ARCHIVE" --pattern SHA256SUMS --dir "$TEMP_DIR"
  cmp "$ARTIFACT_DIR/$ARCHIVE" "$TEMP_DIR/$ARCHIVE"
  cmp "$ARTIFACT_DIR/SHA256SUMS" "$TEMP_DIR/SHA256SUMS"
  [[ "$(gh release view "$TAG" --json isDraft --jq .isDraft)" == false ]] || {
    echo "Existing release $TAG is still a draft" >&2
    exit 1
  }
  echo "Release $TAG already contains these verified assets"
  exit 0
fi

FLAGS=(--prerelease --latest=false)
if [[ "$TAG" == *-stable ]]; then FLAGS=(--latest); fi
gh release create "$TAG" "$ARTIFACT_DIR/$ARCHIVE" "$ARTIFACT_DIR/SHA256SUMS" \
  --verify-tag --title "still $TAG" --notes-file "$TEMP_DIR/notes.md" "${FLAGS[@]}"
