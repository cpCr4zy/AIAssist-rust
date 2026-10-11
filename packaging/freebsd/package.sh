#!/usr/bin/env bash
# Build AI Assistant for FreeBSD and package it:
#   $DIST/aiassist-<version>-freebsd-<arch>.tar.gz   a /usr/local-style tree (bin/, share/)
#
# The desktop entry, AppStream metadata and icons are the freedesktop ones Linux uses.
# Usage: packaging/freebsd/package.sh [--skip-build]
# Needs: bash, a Rust toolchain and the GUI libraries listed in .github/workflows/freebsd.yml.
set -euo pipefail
# shellcheck source=packaging/env.sh
. "$(dirname "${BASH_SOURCE[0]}")/../env.sh"
LINUX="$ROOT/packaging/linux"
APP_ID=io.github.cpcr4zy.aiassist

SKIP_BUILD=0
if [ "${1:-}" = "--skip-build" ]; then SKIP_BUILD=1; fi

ARCH="$(uname -m)"
case "$ARCH" in
  amd64 | x86_64) ARCH=x86_64 ;;
  arm64 | aarch64) ARCH=aarch64 ;;
  *) echo "unsupported architecture $ARCH" >&2; exit 2 ;;
esac
BASENAME="aiassist-$VERSION-freebsd-$ARCH"

echo "==> AI Assistant $VERSION for FreeBSD $ARCH"

if [ "$SKIP_BUILD" = 0 ]; then
  (cd "$ROOT" && cargo build --release --locked -p aiassist)
fi
BIN="$CARGO_TARGET_DIR/release"
WORK="$CARGO_TARGET_DIR/freebsd-package"
STAGE="$WORK/$BASENAME"
fresh_dir "$WORK"

# FreeBSD's install(1) has no -D: create the directories first.
mkdir -p "$STAGE/bin" "$STAGE/share/applications" "$STAGE/share/metainfo" "$STAGE/share/icons" "$STAGE/share/doc/aiassist"
install -m 755 "$BIN/aiassist" "$STAGE/bin/"
strip "$STAGE/bin/aiassist" 2>/dev/null || true
install -m 644 "$LINUX/$APP_ID.desktop" "$STAGE/share/applications/$APP_ID.desktop"
sed -e "s/@VERSION@/$VERSION/g" -e "s/@DATE@/$AIASSIST_BUILD_DATE/g" \
  "$LINUX/$APP_ID.metainfo.xml.in" >"$STAGE/share/metainfo/$APP_ID.metainfo.xml"
cp -R "$ROOT/assets/app-icon/hicolor" "$STAGE/share/icons/"
copy_docs "$STAGE/share/doc/aiassist"

"$STAGE/bin/aiassist" --version
tar -C "$WORK" -czf "$DIST/$BASENAME.tar.gz" "$BASENAME"
echo "wrote $DIST/$BASENAME.tar.gz"
