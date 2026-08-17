#!/usr/bin/env bash
# scripts/bump-version.sh — Bump the project version (semver).
#
# Usage:
#   scripts/bump-version.sh patch        # 0.1.0 -> 0.1.1
#   scripts/bump-version.sh minor        # 0.1.0 -> 0.2.0
#   scripts/bump-version.sh major        # 0.1.0 -> 1.0.0
#   scripts/bump-version.sh 0.3.0        # explicit version
#
# Reads VERSION, computes the new version, writes it to BOTH version sources —
# the repo-root VERSION file (release workflow, macOS bundle) and the
# `[workspace.package] version` in the root Cargo.toml (CARGO_PKG_VERSION for
# every crate) — refreshes the workspace-member entries in Cargo.lock, and prints
# the new version (without leading 'v') to stdout. The release workflow uses this
# to bump before tagging. `scripts/verify-dependency-graph.py` fails CI when the
# two sources disagree.
#
# The script is pure bash + awk — no other deps — so it runs in any CI runner.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION_FILE="$REPO_ROOT/VERSION"
MANIFEST="$REPO_ROOT/Cargo.toml"
LOCKFILE="$REPO_ROOT/Cargo.lock"

if [[ ! -f "$VERSION_FILE" ]]; then
  echo "ERROR: VERSION file not found at $VERSION_FILE" >&2
  exit 1
fi
if [[ ! -f "$MANIFEST" ]]; then
  echo "ERROR: root Cargo.toml not found at $MANIFEST" >&2
  exit 1
fi

CURRENT="$(tr -d '[:space:]' < "$VERSION_FILE")"

# Validate current version: 1-4 dot-separated non-negative integers.
validate() {
  local v="$1"
  if ! [[ "$v" =~ ^[0-9]+(\.[0-9]+){0,3}$ ]]; then
    echo "ERROR: invalid version '$v' (expected 1-4 dot-separated ints)" >&2
    exit 1
  fi
}
validate "$CURRENT"

bump() {
  local kind="$1" base="$2"
  IFS='.' read -ra parts <<< "$base"
  local major="${parts[0]:-0}"
  local minor="${parts[1]:-0}"
  local patch="${parts[2]:-0}"
  case "$kind" in
    patch) patch=$((patch + 1)) ;;
    minor) minor=$((minor + 1)); patch=0 ;;
    major) major=$((major + 1)); minor=0; patch=0 ;;
    *) echo "ERROR: unknown bump kind '$kind'" >&2; exit 1 ;;
  esac
  echo "${major}.${minor}.${patch}"
}

ARG="${1:-}"
if [[ -z "$ARG" ]]; then
  echo "Usage: $0 <patch|minor|major|<version>>" >&2
  exit 1
fi

# If the arg is itself a version string, use it directly; otherwise treat as bump kind.
if [[ "$ARG" =~ ^[0-9]+(\.[0-9]+){0,3}$ ]]; then
  NEW="$ARG"
else
  case "$ARG" in
    patch|minor|major) NEW="$(bump "$ARG" "$CURRENT")" ;;
    *) echo "ERROR: argument must be patch|minor|major or an explicit version" >&2; exit 1 ;;
  esac
fi

validate "$NEW"

if [[ "$CURRENT" == "$NEW" ]]; then
  echo "ERROR: new version ($NEW) is same as current ($CURRENT)" >&2
  exit 1
fi

# Normalize to MAJOR.MINOR.PATCH (drop any 4th component for the VERSION file).
IFS='.' read -ra np <<< "$NEW"
NORMALIZED="${np[0]}.${np[1]:-0}.${np[2]:-0}"

# Rewrite the `version = "..."` line inside [workspace.package] only (the first
# `version` key after that table header, before the next table header).
rewrite_manifest() {
  local file="$1" new="$2"
  awk -v new="$new" '
    /^\[workspace\.package\]/ { in_pkg = 1; print; next }
    /^\[/                     { in_pkg = 0 }
    in_pkg && !done && /^version[[:space:]]*=/ {
      print "version = \"" new "\""; done = 1; next
    }
    { print }
    END { if (!done) exit 3 }
  ' "$file" > "$file.tmp" || { rm -f "$file.tmp"; return 1; }
  mv "$file.tmp" "$file"
}

# Cargo rewrites Cargo.lock on the next build anyway; refresh the workspace
# member entries now so a `--locked` build sees a consistent tree.
rewrite_lockfile() {
  local file="$1" old="$2" new="$3"
  [[ -f "$file" ]] || return 0
  awk -v old="$old" -v new="$new" '
    /^name = "oneterm-/ { member = 1; print; next }
    member && /^version = / { sub("\"" old "\"", "\"" new "\""); member = 0 }
    /^$/ { member = 0 }
    { print }
  ' "$file" > "$file.tmp"
  mv "$file.tmp" "$file"
}

rewrite_manifest "$MANIFEST" "$NORMALIZED" || {
  echo "ERROR: could not find 'version = ' under [workspace.package] in $MANIFEST" >&2
  exit 1
}
rewrite_lockfile "$LOCKFILE" "$CURRENT" "$NORMALIZED"
printf '%s\n' "$NORMALIZED" > "$VERSION_FILE"
echo "$NORMALIZED"