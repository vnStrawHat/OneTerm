#!/usr/bin/env bash
# scripts/bundle-macos.sh — Assemble the OneTerm.app bundle for macOS.
#
# On macOS a GUI binary that is NOT inside a .app bundle is treated by
# LaunchServices as a plain command-line tool. Double-clicking it in Finder
# therefore routes it through Terminal.app, which opens an extra Terminal
# window alongside the GUI — the macOS analog of the Windows "console window"
# problem (fixed there with `#![windows_subsystem = "windows"]`).
#
# Packaging the executable inside OneTerm.app with an Info.plist that declares
# it a GUI app (CFBundlePackageType=APPL, NSPrincipalClass=NSApplication) makes
# LaunchServices launch it directly, without Terminal.app.
#
# Usage:
#   scripts/bundle-macos.sh <repo_root> <release_dir> <stage_dir>
#     <repo_root>   - repo root (read Cargo.toml version + assets/macos/Info.plist, icons)
#     <release_dir> - directory containing the built `oneterm` binary
#     <stage_dir>   - dist staging dir; OneTerm.app is created under it
#
# The macOS-native sips + iconutil are used (best-effort) to build a .icns from
# the Windows .ico asset. If they are unavailable or the .ico can't be decoded,
# the bundle is still produced without a custom icon.
set -euo pipefail

REPO_ROOT="${1:?repo_root required}"
RELEASE_DIR="${2:?release_dir required}"
STAGE="${3:?stage_dir required}"

EXE="$RELEASE_DIR/oneterm"
if [[ ! -f "$EXE" ]]; then
  echo "ERROR: $EXE not found" >&2
  exit 1
fi

# ── Best-effort .icns generation ─────────────────────────────────────────────
# Convert a Windows .ico into a macOS .icns using sips + iconutil (macOS only).
generate_icns() {
  local src="$1" dst="$2"
  command -v sips >/dev/null 2>&1     || { echo "  sips not found — skipping icon";     return 0; }
  command -v iconutil >/dev/null 2>&1 || { echo "  iconutil not found — skipping icon"; return 0; }

  local tmpdir iconset master s d
  tmpdir="$(mktemp -d)"
  iconset="$tmpdir/oneterm.iconset"
  master="$tmpdir/master.png"
  mkdir -p "$iconset"

  # sips decodes the .ico (largest frame) to a PNG master.
  if ! sips -s format png "$src" --out "$master" >/dev/null 2>&1; then
    echo "  sips could not decode $src — skipping icon"
    rm -rf "$tmpdir"
    return 0
  fi
  # Normalize to a 1024px square master (upscale from the 96px source — best effort).
  sips -z 1024 1024 "$master" >/dev/null 2>&1 || true

  # Emit the standard .iconset sizes (1x + @2x) from the master.
  for s in 16 32 64 128 256 512 1024; do
    sips -z "$s" "$s" "$master" --out "$iconset/icon_${s}x${s}.png" >/dev/null 2>&1 || true
  done
  for s in 16 32 64 128 256 512; do
    d=$(( s * 2 ))
    sips -z "$d" "$d" "$master" --out "$iconset/icon_${s}x${s}@2x.png" >/dev/null 2>&1 || true
  done

  if iconutil -c icns "$iconset" -o "$dst" >/dev/null 2>&1; then
    echo "  icon: $dst"
  else
    echo "  iconutil failed — skipping icon"
  fi
  rm -rf "$tmpdir"
}

# ── Assemble OneTerm.app ──────────────────────────────────────────────────────
APP_BUNDLE="$STAGE/OneTerm.app"
CONTENTS="$APP_BUNDLE/Contents"
mkdir -p "$CONTENTS/MacOS" "$CONTENTS/Resources"

# Executable.
cp "$EXE" "$CONTENTS/MacOS/oneterm"
chmod +x "$CONTENTS/MacOS/oneterm"

# Info.plist — substitute {{VERSION}} from [workspace.package] in Cargo.toml.
VERSION="$(awk -F'"' '/^version = "/{print $2; exit}' "$REPO_ROOT/Cargo.toml")"
sed "s/{{VERSION}}/$VERSION/g" \
  "$REPO_ROOT/crates/app/assets/macos/Info.plist" > "$CONTENTS/Info.plist"

# App icon (best-effort).
generate_icns "$REPO_ROOT/crates/app/assets/icons/terminal-96x96.ico" \
  "$CONTENTS/Resources/oneterm.icns"

# Ad-hoc code signature (best-effort). An unsigned bundle built on one Mac may be
# refused by Gatekeeper on another ("damaged"); an ad-hoc signature (identity "-")
# is enough for a locally-run, non-notarised app. Only available on macOS.
if command -v codesign >/dev/null 2>&1; then
  if codesign --force --deep --sign - "$APP_BUNDLE" >/dev/null 2>&1; then
    echo "  codesign: ad-hoc signature applied"
  else
    echo "  codesign failed — bundle left unsigned"
  fi
else
  echo "  codesign not found — bundle left unsigned"
fi

echo "==> OneTerm.app assembled at: $APP_BUNDLE"
( cd "$STAGE" && find OneTerm.app -type f | sort )