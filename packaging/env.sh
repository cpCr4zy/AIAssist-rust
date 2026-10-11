# shellcheck shell=bash
# Shared setup for the packaging scripts. Source it: `. "$(dirname "$0")/../env.sh"`.
#
# Exports:
#   ROOT              workspace root
#   VERSION           [workspace.package] version from Cargo.toml (override: AIASSIST_VERSION)
#   DIST              output directory for release artifacts (default: $ROOT/dist/release)
#   AIASSIST_BUILD_SHA   git commit
#   AIASSIST_BUILD_DATE  UTC build date, YYYY-MM-DD
#   CARGO_TARGET_DIR  cargo's target dir (default: $ROOT/target)

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export ROOT

# The version lives in exactly one place: `[workspace.package] version` in the root Cargo.toml.
workspace_version() {
  awk '
    /^\[/ { in_pkg = ($0 == "[workspace.package]") ; next }
    in_pkg && $1 == "version" { gsub(/[" ]/, "", $3); print $3; exit }
  ' "$ROOT/Cargo.toml"
}

VERSION="${AIASSIST_VERSION:-$(workspace_version)}"
if [ -z "$VERSION" ]; then
  echo "error: could not read [workspace.package] version from $ROOT/Cargo.toml" >&2
  exit 1
fi
export VERSION

DIST="${DIST:-$ROOT/dist/release}"
mkdir -p "$DIST"
export DIST

if [ -z "${AIASSIST_BUILD_SHA:-}" ]; then
  AIASSIST_BUILD_SHA="$(git -C "$ROOT" rev-parse HEAD 2>/dev/null || true)"
fi
export AIASSIST_BUILD_SHA
export AIASSIST_BUILD_DATE="${AIASSIST_BUILD_DATE:-$(date -u +%Y-%m-%d)}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"

# Emit a GitHub Actions warning (plain stderr outside Actions).
warn() {
  if [ -n "${GITHUB_ACTIONS:-}" ]; then echo "::warning::$*"; else echo "warning: $*" >&2; fi
}

# Copy licence and readme files that exist into a package directory.
copy_docs() {
  local dest="$1" f
  for f in README.md LICENSE LICENSE-MIT LICENSE-APACHE COPYRIGHT NOTICE; do
    if [ -f "$ROOT/$f" ]; then cp "$ROOT/$f" "$dest/"; fi
  done
}

# A fresh, empty work directory under the target dir (refuses to touch anything else).
fresh_dir() {
  local dir="${1:?fresh_dir needs a path}"
  case "$dir" in
    "$CARGO_TARGET_DIR"/*) ;;
    *) echo "error: refusing to recreate $dir outside $CARGO_TARGET_DIR" >&2; exit 1 ;;
  esac
  rm -rf "${dir:?}"
  mkdir -p "$dir"
}
