#!/usr/bin/env bash
# scripts/build-release.sh — Build bản release cho OneTerm (Linux / macOS / WSL).
#
# Trên Windows native hãy dùng build-release.ps1 (stage dist/ + copy conpty.dll).
# Script này chủ yếu cho Linux/mac, nơi không cần conpty.dll/OpenConsole.exe.
#
# Chạy:  ./scripts/build-release.sh
#        TARGET=aarch64-unknown-linux-gnu ./scripts/build-release.sh
#
# Bin release = `oneterm` (gated bởi feature `release-bin` trong crates/app/Cargo.toml).
# Dev bin = `oneterm-debug` (feature `dev-bin`, default). Hai bin mutually-exclusive
# qua --no-default-features --features release-bin để release chỉ build `oneterm`.
#
# Kết quả:
#   - target/<triple>/release/oneterm       (release binary, đã strip + LTO)
#   - dist/oneterm-<triple>/oneterm          (Linux: bản đóng gói sạch để phát hành)
#   - dist/oneterm-<triple>/OneTerm.app      (macOS: .app bundle — double-click mà
#                                            không mở thêm cửa sổ Terminal)

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

TARGET="${TARGET:-}"
NO_DIST="${NO_DIST:-0}"

# Release build: chỉ build bin `oneterm` (bật release-bin, tắt dev-bin default).
RELEASE_ARGS=(build --release --no-default-features --features release-bin)
echo "==> cargo ${RELEASE_ARGS[*]}"
if [[ -n "$TARGET" ]]; then
  cargo "${RELEASE_ARGS[@]}" --target "$TARGET"
  TRIPLE="$TARGET"
  RELEASE_DIR="target/$TARGET/release"
else
  cargo "${RELEASE_ARGS[@]}"
  TRIPLE="$(rustc -vV | awk '/^host:/{print $2}')"
  # Khi không truyền --target, cargo ghi trực tiếp ra target/release (không có subdir triple).
  RELEASE_DIR="target/release"
fi

EXE="$RELEASE_DIR/oneterm"
if [[ ! -f "$EXE" ]]; then
  echo "ERROR: không tìm thấy $EXE" >&2
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
  # macOS: package the release binary into a proper OneTerm.app bundle.
  #
  # A raw GUI binary on macOS is treated by LaunchServices as a plain CLI
  # tool, so double-clicking it in Finder routes it through Terminal.app and
  # opens an extra Terminal window alongside the GUI (the macOS analog of
  # the Windows console-window problem fixed with `windows_subsystem =
  # "windows"`). Bundling it inside OneTerm.app with an Info.plist declaring
  # it a GUI app (NSPrincipalClass=NSApplication, CFBundlePackageType=APPL)
  # makes LaunchServices launch it directly. Shared with CI via this script.
  bash scripts/bundle-macos.sh "$REPO_ROOT" "$RELEASE_DIR" "$DIST_DIR"

  # Config files are NOT bundled: release builds read/write ~/.OneTerm/ (auto
  # created on first run), so no shipped config is needed inside the .app.
else
  # Linux: ship the raw binary + optional default config next to it.
  cp "$EXE" "$DIST_DIR/"
  for cfg in terminal.json docks.json; do
    [[ -f "$REPO_ROOT/$cfg" ]] && cp "$REPO_ROOT/$cfg" "$DIST_DIR/" || true
  done
fi

echo "==> dist staged tại: $DIST_DIR"
( cd "$DIST_DIR" && find . -type f | sort )