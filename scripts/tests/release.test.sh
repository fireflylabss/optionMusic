#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
FIXTURE="$(mktemp -d)"
trap 'rm -rf "$FIXTURE"' EXIT
mkdir -p "$FIXTURE/scripts" "$FIXTURE/bin" "$FIXTURE/mock-bin"
cp "$ROOT/scripts/release-artifact.sh" "$ROOT/scripts/release-publish.sh" "$FIXTURE/scripts/"
printf 'version = "0.3.0"\n' > "$FIXTURE/Cargo.toml"
printf 'Fixture license\n' > "$FIXTURE/LICENSE"
printf 'Fixture README\n' > "$FIXTURE/README.md"
cat > "$FIXTURE/CHANGELOG.md" <<'NOTES'
## v0.3.0-beta.1 · 25/10/2026

Corrected beta attempt.

## v0.3.0-beta · 24/10/2026

Initial beta attempt.

## v0.3.0-stable · 26/10/2026

Stable fixture.
NOTES
cat > "$FIXTURE/cli.c" <<'C'
#include <stdio.h>
int main(void) { puts("still 0.3.0"); return 0; }
C
cc "$FIXTURE/cli.c" -o "$FIXTURE/bin/still"
cp "$FIXTURE/bin/still" "$FIXTURE/bin/msc"
cat > "$FIXTURE/mock-bin/gh" <<'GH'
#!/usr/bin/env bash
set -euo pipefail
case "$1 $2" in
  'release view') exit 1 ;;
  'release create')
    printf '%s\n' "$@" > "$GH_TEST_ARGS"
    while (( $# )); do
      if [[ "$1" == --notes-file ]]; then cp "$2" "$GH_TEST_NOTES"; break; fi
      shift
    done
    ;;
  *) echo "Unexpected gh invocation" >&2; exit 1 ;;
esac
GH
chmod +x "$FIXTURE/mock-bin/gh"
export PATH="$FIXTURE/mock-bin:$PATH"
export GH_TEST_ARGS="$FIXTURE/gh-args"
export GH_TEST_NOTES="$FIXTURE/release-notes"

for tag in v0.3.0-beta v0.3.0-beta.1 v0.3.0-stable; do
  output_dir="$FIXTURE/$tag"
  bash "$FIXTURE/scripts/release-artifact.sh" "$tag" "$FIXTURE/bin" "$output_dir"
  archive="stillmusic-${tag}-x86_64-unknown-linux-gnu.tar.gz"
  [[ -f "$output_dir/$archive" ]]
  [[ "$(tar -tzf "$output_dir/$archive")" == $'LICENSE\nREADME.md\nmsc\nstill' ]]
  bash "$FIXTURE/scripts/release-publish.sh" "$tag" "$output_dir"
  grep -Fxq -- "$tag" "$GH_TEST_ARGS"
  grep -Fxq -- "$output_dir/$archive" "$GH_TEST_ARGS"
  if [[ "$tag" == *-stable ]]; then
    grep -Fxq -- '--latest' "$GH_TEST_ARGS"
    ! grep -Fxq -- '--prerelease' "$GH_TEST_ARGS"
    grep -Fxq 'Stable fixture.' "$GH_TEST_NOTES"
  else
    grep -Fxq -- '--prerelease' "$GH_TEST_ARGS"
    grep -Fxq -- '--latest=false' "$GH_TEST_ARGS"
    if [[ "$tag" == *-beta.1 ]]; then
      grep -Fxq 'Corrected beta attempt.' "$GH_TEST_NOTES"
      ! grep -Fq 'Initial beta attempt.' "$GH_TEST_NOTES"
    else
      grep -Fxq 'Initial beta attempt.' "$GH_TEST_NOTES"
    fi
  fi
done

for tag in v0.3.0-beta.0 v0.3.0-beta.01 v0.3.0-stable.1 v0.3.0-rc.1; do
  if bash "$FIXTURE/scripts/release-artifact.sh" "$tag" "$FIXTURE/bin" "$FIXTURE/rejected" 2>/dev/null; then
    echo "Accepted invalid artifact tag: $tag" >&2; exit 1
  fi
  if bash "$FIXTURE/scripts/release-publish.sh" "$tag" "$FIXTURE/rejected" 2>/dev/null; then
    echo "Accepted invalid publication tag: $tag" >&2; exit 1
  fi
done

printf 'PASS: release artifacts, checksums, notes and channel flags for beta, beta.1 and stable\n'
