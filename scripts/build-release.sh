#!/usr/bin/env bash
# scripts/build-release.sh — Build a OneTerm release for Linux, macOS, or WSL.
#
# On native Windows, use build-release.ps1 to stage dist/ and copy conpty.dll.
# This script primarily targets Linux and macOS, where conpty.dll and
# OpenConsole.exe are not required.
#
# Usage: ./scripts/build-release.sh
#        TARGET=aarch64-unknown-linux-gnu ./scripts/build-release.sh
#
# The release binary is `oneterm` (gated by the `release-bin` feature in
# crates/app/Cargo.toml). The development binary is `oneterm-debug` (the default
# `dev-bin` feature). Passing --no-default-features --features release-bin makes
# the release build produce only `oneterm`.
#
# Outputs:
#   - target/<triple>/release/oneterm       (release binary with strip + LTO)
#   - dist/oneterm-<triple>/oneterm         (clean Linux distribution)
#   - dist/oneterm-<triple>/OneTerm.app     (macOS application bundle)

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

TARGET="${TARGET:-}"
NO_DIST="${NO_DIST:-0}"

# Build only `oneterm`: enable release-bin and disable the default dev-bin.
RELEASE_ARGS=(build --release --no-default-features --features release-bin)
echo "==> cargo ${RELEASE_ARGS[*]}"
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

EXE="$RELEASE_DIR/oneterm"
if [[ ! -f "$EXE" ]]; then
  echo "ERROR: release binary not found: $EXE" >&2
  exit 1
fi
echo "OK: $EXE"

if [[ "$NO_DIST" == "1" ]]; then
  exit 0
fi

DIST_DIR="dist/oneterm-$TRIPLE"
rm -rf "$DIST_DIR"
mkdir -p "$DIST_DIR"

OS_FAMILY="$(uname -s)"
if [[ "$OS_FAMILY" == "Darwin" ]]; then
  # Package the release binary into a proper OneTerm.app bundle on macOS.
  #
  # LaunchServices treats a raw GUI binary as a command-line tool, so opening it
  # in Finder routes it through Terminal.app and opens an extra Terminal window.
  # An application bundle with an Info.plist that declares a GUI app launches
  # directly instead. CI uses this script for the same packaging behavior.
  bash scripts/bundle-macos.sh "$REPO_ROOT" "$RELEASE_DIR" "$DIST_DIR"

  # Release builds create and use ~/.OneTerm/ on first run, so no configuration
  # files need to be shipped inside the application bundle.
else
  # Ship the Linux binary and any optional default configuration beside it.
  cp "$EXE" "$DIST_DIR/"
  for cfg in terminal.json docks.json; do
    [[ -f "$REPO_ROOT/$cfg" ]] && cp "$REPO_ROOT/$cfg" "$DIST_DIR/" || true
  done
fi

echo "==> Distribution staged at: $DIST_DIR"
( cd "$DIST_DIR" && find . -type f | sort )
