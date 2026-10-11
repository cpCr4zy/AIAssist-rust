#!/usr/bin/env bash
# Build the browser version and zip it:  $DIST/aiassist-web-<version>.zip
#
# Usage: packaging/web/package.sh [--skip-build]
# Needs: trunk (a prebuilt binary from https://github.com/trunk-rs/trunk/releases), zip and the
# wasm32-unknown-unknown target. The zip holds
# a self-contained static site that works from any URL path and inside an <iframe>.
set -euo pipefail
# shellcheck source=packaging/env.sh
. "$(dirname "${BASH_SOURCE[0]}")/../env.sh"
HERE="$ROOT/packaging/web"

command -v zip >/dev/null || { echo "error: zip not found (apt install zip / pacman -S zip)" >&2; exit 1; }
if [ "${1:-}" != "--skip-build" ]; then
  command -v trunk >/dev/null || { echo "error: trunk not found (https://github.com/trunk-rs/trunk/releases)" >&2; exit 1; }
  (cd "$ROOT/apps/aiassist-web" && trunk build --release)
fi

SITE="$ROOT/dist/web"
[ -f "$SITE/index.html" ] || { echo "error: $SITE/index.html missing; run without --skip-build" >&2; exit 1; }
# Paths must be relative so the site works under any prefix (public_url = "./" in Trunk.toml).
if grep -Eq '(src|href)="/[^/]' "$SITE/index.html"; then
  echo "error: $SITE/index.html has root-absolute URLs; it would break when served from a sub-path" >&2
  exit 1
fi

NAME="aiassist-web-$VERSION"
WORK="$CARGO_TARGET_DIR/web-package"
fresh_dir "$WORK"
mkdir -p "$WORK/$NAME"
cp -R "$SITE/." "$WORK/$NAME/"
cp "$HERE/_headers" "$WORK/$NAME/"
copy_docs "$WORK/$NAME"
(cd "$WORK" && zip -qr9 "$NAME.zip" "$NAME")
mv -f "$WORK/$NAME.zip" "$DIST/$NAME.zip"
echo "wrote $DIST/$NAME.zip"
ls -lh "$DIST/$NAME.zip"
