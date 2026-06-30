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
#   - target/<triple>/release/oneterm (release binary, đã strip + LTO)
#   - dist/oneterm-<triple>/oneterm  (bản đóng gói sạch để phát hành)

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
cp "$EXE" "$DIST_DIR/"

# Copy config mặc định nếu có.
for cfg in terminal.json docks.json; do
  [[ -f "$REPO_ROOT/$cfg" ]] && cp "$REPO_ROOT/$cfg" "$DIST_DIR/" || true
done

echo "==> dist staged tại: $DIST_DIR"
( cd "$DIST_DIR" && find . -type f | sort )