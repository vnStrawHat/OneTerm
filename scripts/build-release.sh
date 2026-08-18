#!/usr/bin/env bash
# scripts/build-release.sh — Build and stage a OneTerm release for Linux, macOS, or WSL.
#
# On native Windows, use build-release.ps1 (it also stages conpty.dll/OpenConsole.exe).
# The release workflow (.github/workflows/release.yml) calls THIS script for the
# Linux and macOS targets, so local and CI packaging produce the same layout.
#
# Usage: ./scripts/build-release.sh
#        TARGET=aarch64-unknown-linux-gnu ./scripts/build-release.sh
#        NO_DIST=1 ./scripts/build-release.sh          # build only, do not stage dist/
#
# The release binary is `oneterm` (gated by the `release-bin` feature in
# crates/app/Cargo.toml). The development binary is `oneterm-debug` (the default
# `dev-bin` feature). Passing --no-default-features --features release-bin makes
# the release build produce only `oneterm`. Only `oneterm-app` is built (-p): the
# other workspace members (diagnostics in crates/tools, …) are not part of a release.
#
# Outputs (VERSION = repo-root VERSION file, TRIPLE = target triple):
#   - target/<triple>/release/oneterm                       (release binary with strip + LTO)
#   - dist/oneterm-<VERSION>-<triple>/oneterm               (Linux)
#   - dist/oneterm-<VERSION>-<triple>/OneTerm.app           (macOS application bundle)
#   - dist/oneterm-<VERSION>-<triple>.tar.gz + .sha256      (archive + checksum)
#
# The staged directory contains only build outputs — never developer state such as a
# repo-root terminal.json / docks.json (release builds create ~/.OneTerm/ on first run).

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

TARGET="${TARGET:-}"
NO_DIST="${NO_DIST:-0}"
VERSION="$(tr -d '[:space:]' < VERSION)"

# Build only `oneterm`: enable release-bin and disable the default dev-bin.
RELEASE_ARGS=(build -p oneterm-app --release --no-default-features --features release-bin)
echo "==> cargo ${RELEASE_ARGS[*]}${TARGET:+ --target $TARGET}"
if [[ -n "$TARGET" ]]; then
  cargo "${RELEASE_ARGS[@]}" --target "$TARGET"
  TRIPLE="$TARGET"
  RELEASE_DIR="target/$TARGET/release"
else
  cargo "${RELEASE_ARGS[@]}"
  TRIPLE="$(rustc -vV | awk '/^host:/{print $2}')"
  # Without --target, Cargo writes directly to target/release.
  RELEASE_DIR="target/release"
fi
# Honour CARGO_TARGET_DIR the same way Cargo does.
if [[ -n "${CARGO_TARGET_DIR:-}" ]]; then
  RELEASE_DIR="${CARGO_TARGET_DIR}/${RELEASE_DIR#target/}"
fi

EXE="$RELEASE_DIR/oneterm"
if [[ ! -f "$EXE" ]]; then
  echo "ERROR: release binary not found: $EXE" >&2
  exit 1
fi
echo "OK: $EXE"

if [[ "$NO_DIST" == "1" ]]; then
  exit 0
fi

DIST_NAME="oneterm-${VERSION}-${TRIPLE}"
DIST_DIR="dist/${DIST_NAME}"
rm -rf "$DIST_DIR" "dist/${DIST_NAME}.tar.gz" "dist/${DIST_NAME}.tar.gz.sha256"
mkdir -p "$DIST_DIR"

if [[ "$TRIPLE" == *darwin* ]]; then
  # Package the release binary into a proper OneTerm.app bundle on macOS.
  #
  # LaunchServices treats a raw GUI binary as a command-line tool, so opening it
  # in Finder routes it through Terminal.app and opens an extra Terminal window.
  # An application bundle with an Info.plist that declares a GUI app launches
  # directly instead. bundle-macos.sh also applies an ad-hoc code signature.
  bash scripts/bundle-macos.sh "$REPO_ROOT" "$RELEASE_DIR" "$DIST_DIR"
else
  cp "$EXE" "$DIST_DIR/"
fi

echo "==> Distribution staged at: $DIST_DIR"
( cd "$DIST_DIR" && find . -type f | sort )

# Archive + checksum (same names the release workflow publishes).
( cd dist && tar -czf "${DIST_NAME}.tar.gz" "${DIST_NAME}" )
if command -v sha256sum >/dev/null 2>&1; then
  ( cd dist && sha256sum "${DIST_NAME}.tar.gz" > "${DIST_NAME}.tar.gz.sha256" )
else
  ( cd dist && shasum -a 256 "${DIST_NAME}.tar.gz" > "${DIST_NAME}.tar.gz.sha256" )
fi
echo "==> Archive: dist/${DIST_NAME}.tar.gz"
cat "dist/${DIST_NAME}.tar.gz.sha256"
