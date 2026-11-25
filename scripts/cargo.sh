#!/usr/bin/env bash
# scripts/cargo.sh <args>: cargo for this repository, run from any folder:
#     scripts/cargo.sh test -p oneterm-ssh
# Temp files of the build stay inside the project (.tmp), and a cargo that updates itself in
# the background (the fork that shares build output between projects) is told not to: the
# build tool changes only when someone updates it on purpose.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
mkdir -p "$root/.tmp"
export CARGO_FORK_AUTO_UPDATE=0
TMP="$(cygpath -w "$root/.tmp" 2>/dev/null || echo "$root/.tmp")"
export TMP TEMP="$TMP"
cd "$root"
exec cargo "$@"
