#!/usr/bin/env bash
#
# refresh.sh — regenerate the vendored OneTerm forks of `vte` + `alacritty_terminal`
# from their PRISTINE upstream revisions plus the patches under `vendor/patches/`.
#
# Model (per crate):
#
#     pristine upstream @ pinned rev
#         └── apply vendor/patches/<crate>/*.patch  (in order)
#                 └── vendor/<crate>/               (what Cargo builds via [patch])
#
# This is the "create-from-rev → apply-patch" half of the workflow. To ADD or CHANGE a
# patch, use the git-based half documented in vendor/README.md (§ "Editing the patches").
#
# Usage:
#   bash vendor/refresh.sh            # rebuild vendor/<crate> from pristine + patches
#   bash vendor/refresh.sh --check    # verify vendor/<crate> == pristine + patches (no writes)
#
# Pristine sources are taken from the local Cargo cache when present (byte-exact), else
# fetched from the network (crates.io / GitHub). Requires: bash, patch, diff, tar; and
# git + curl only for the network fallback.

set -euo pipefail

# ── Pinned upstream revisions (keep in sync with the root Cargo.toml [patch] section
#    and docs/agents/dependencies.md §1/§3) ────────────────────────────────────────────
VTE_VERSION="0.15.0"                                          # crates.io
VTE_VCS_SHA1="3b3da71c34cc1256c7e20981cf03f8eb95e08ffc"       # .cargo_vcs_info.json (provenance)
ALA_URL="https://github.com/zed-industries/alacritty"
ALA_REV="fcf32feacb367b75ec84dd40f041e4fd411d3cc1"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VENDOR="$SCRIPT_DIR"
PATCHES="$VENDOR/patches"
CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"

CHECK=0
[[ "${1:-}" == "--check" ]] && CHECK=1

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

log()  { printf '>> %s\n' "$*"; }
die()  { printf 'refresh.sh: error: %s\n' "$*" >&2; exit 1; }

# ── fetch_vte <dest> : populate <dest> with pristine vte $VTE_VERSION ────────────────────
fetch_vte() {
  local dest="$1" src crate
  src="$(ls -d "$CARGO_HOME"/registry/src/*/vte-"$VTE_VERSION" 2>/dev/null | head -1 || true)"
  if [[ -n "$src" ]]; then
    log "vte $VTE_VERSION: cargo registry src cache (byte-exact)"
    cp -a "$src/." "$dest/"; return
  fi
  crate="$(ls "$CARGO_HOME"/registry/cache/*/vte-"$VTE_VERSION".crate 2>/dev/null | head -1 || true)"
  if [[ -n "$crate" ]]; then
    log "vte $VTE_VERSION: cargo registry .crate tarball"
    tar -xzf "$crate" -C "$TMP"; cp -a "$TMP/vte-$VTE_VERSION/." "$dest/"; return
  fi
  command -v curl >/dev/null || die "vte pristine not in Cargo cache and curl is unavailable"
  log "vte $VTE_VERSION: downloading from crates.io"
  curl -fsSL "https://crates.io/api/v1/crates/vte/$VTE_VERSION/download" -o "$TMP/vte.crate"
  tar -xzf "$TMP/vte.crate" -C "$TMP"; cp -a "$TMP/vte-$VTE_VERSION/." "$dest/"
}

# ── fetch_ala <dest> : populate <dest> with pristine alacritty_terminal @ $ALA_REV ───────
fetch_ala() {
  local dest="$1" co
  co="$(ls -d "$CARGO_HOME"/git/checkouts/alacritty-*/"${ALA_REV:0:7}"*/alacritty_terminal 2>/dev/null | head -1 || true)"
  if [[ -n "$co" ]]; then
    log "alacritty_terminal @ ${ALA_REV:0:7}: cargo git checkout cache (byte-exact)"
    cp -a "$co/." "$dest/"; return
  fi
  command -v git >/dev/null || die "alacritty pristine not in Cargo cache and git is unavailable"
  log "alacritty_terminal @ ${ALA_REV:0:7}: cloning $ALA_URL"
  git clone --quiet --no-checkout "$ALA_URL" "$TMP/ala"
  git -C "$TMP/ala" checkout --quiet "$ALA_REV"
  cp -a "$TMP/ala/alacritty_terminal/." "$dest/"
}

# ── apply_patches <dir> <crate> ──────────────────────────────────────────────────────────
apply_patches() {
  local dir="$1" crate="$2" p had=0
  shopt -s nullglob
  for p in "$PATCHES/$crate"/*.patch; do
    had=1
    log "  apply $(basename "$p")"
    ( cd "$dir" && patch -p1 --no-backup-if-mismatch <"$p" >/dev/null )
  done
  shopt -u nullglob
  [[ $had -eq 1 ]] || die "no patches found in $PATCHES/$crate"
}

# ── regen <crate> <fetch_fn> ─────────────────────────────────────────────────────────────
regen() {
  local crate="$1" fetch="$2"
  local build="$TMP/build_$crate"
  rm -rf "$build"; mkdir -p "$build"
  "$fetch" "$build"
  chmod -R u+w "$build"
  apply_patches "$build" "$crate"

  if [[ $CHECK -eq 1 ]]; then
    if diff -ru "$build" "$VENDOR/$crate" >/dev/null; then
      log "$crate: OK — vendor/$crate == pristine + patches"
    else
      log "$crate: MISMATCH — vendor/$crate differs from pristine + patches:"
      diff -ru "$build" "$VENDOR/$crate" | sed 's/^/    /' | head -80
      return 1
    fi
  else
    rm -rf "$VENDOR/$crate"
    mv "$build" "$VENDOR/$crate"
    log "$crate: regenerated -> vendor/$crate"
  fi
}

rc=0
regen vte              fetch_vte || rc=1
regen alacritty_terminal fetch_ala || rc=1

if [[ $rc -eq 0 ]]; then
  [[ $CHECK -eq 1 ]] && log "all crates verified." || log "all crates regenerated."
else
  die "one or more crates failed (see above)."
fi
exit $rc
