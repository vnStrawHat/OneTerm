#!/usr/bin/env bash
# scripts/build-release.sh — Build bản release cho myTerm2 (Linux / macOS / WSL).
#
# Trên Windows native hãy dùng build-release.ps1 (stage dist/ + copy conpty.dll).
# Script này chủ yếu cho Linux/mac, nơi không cần conpty.dll/OpenConsole.exe.
#
# Chạy:  ./scripts/build-release.sh
#        TARGET=aarch64-unknown-linux-gnu ./scripts/build-release.sh
#
# Kết quả:
#   - target/<triple>/release/myterm2 (binary đã strip + LTO)
#   - dist/myterm2-<triple>/myterm2    (bản đóng gói sạch để phát hành)

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

TARGET="${TARGET:-}"
NO_DIST="${NO_DIST:-0}"

echo "==> cargo build --release"
if [[ -n "$TARGET" ]]; then
  cargo build --release --target "$TARGET"
  TRIPLE="$TARGET"
  RELEASE_DIR="target/$TARGET/release"
else
  cargo build --release
  TRIPLE="$(rustc -vV | awk '/^host:/{print $2}')"
  # Khi không truyền --target, cargo ghi trực tiếp ra target/release (không có subdir triple).
  RELEASE_DIR="target/release"
fi

EXE="$RELEASE_DIR/myterm2"
if [[ ! -f "$EXE" ]]; then
  echo "ERROR: không tìm thấy $EXE" >&2
  exit 1
fi
echo "OK: $EXE"

if [[ "$NO_DIST" == "1" ]]; then
  exit 0
fi

DIST_DIR="dist/myterm2-$TRIPLE"
rm -rf "$DIST_DIR"
mkdir -p "$DIST_DIR"
cp "$EXE" "$DIST_DIR/"

# Copy config mặc định nếu có.
for cfg in terminal.json docks.json; do
  [[ -f "$REPO_ROOT/$cfg" ]] && cp "$REPO_ROOT/$cfg" "$DIST_DIR/" || true
done

echo "==> dist staged tại: $DIST_DIR"
( cd "$DIST_DIR" && find . -type f | sort )