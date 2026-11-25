#!/usr/bin/env bash
# The gate: scripts/check.sh [prod]. Everything must pass before work is called done; the CI
# workflow runs this same script.
#   prod: also builds and tests in release.
# CARGO names another cargo than scripts/cargo.sh.
set -uo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
cd "$here/.."
# Text is read as UTF-8 whatever shell runs the gate.
export LC_ALL=C.UTF-8
cargo="${CARGO:-$here/cargo.sh}"
fail=0
# .archived is OneTerm 0.6's source, kept as it was: no check reads it.
say() { echo "$*"; fail=1; }
files() { find . \( -name target -o -name dist -o -name .git -o -name .tmp -o -name .cache -o -name .private -o -name .archived \) -prune -o -type f "$@" -print; }

# 1. No PowerShell.
ps1=$(files -name '*.ps1')
[ -z "$ps1" ] || say "check: PowerShell scripts are not allowed: $ps1"

# 2. Checks of this machine, kept out of git (where present).
[ ! -f .private/check.sh ] || . .private/check.sh

# 3. No source file over 300 lines.
long=$(files \( -name '*.rs' -o -name '*.sh' -o -name '*.py' \) | xargs -r wc -l |
	awk '$2 != "total" && $1 > 300 { print $2 " (" $1 " lines)" }')
[ -z "$long" ] || say "check: files over 300 lines: $long"

# 4. The design language's ladders in the views, and the release notes script's own cases.
PYTHONUTF8=1 python scripts/check_design.py || fail=1
PYTHONUTF8=1 python scripts/release-notes.py --check || say 'check: scripts/release-notes.py fails its cases'

# 5. Format, lints, tests, with the lock file as committed.
"$cargo" fmt --all --check || say 'check: run scripts/cargo.sh fmt --all'
"$cargo" clippy --workspace --all-targets --locked --quiet -- -D warnings || say 'check: clippy failed'
"$cargo" test --workspace --locked --quiet || say 'check: tests failed'

if [ "${1:-}" = prod ]; then
	"$cargo" build --workspace --release --locked --quiet || say 'check: release build failed'
	"$cargo" test --workspace --release --locked --quiet || say 'check: release tests failed'
fi

[ $fail -eq 0 ] && echo 'check: ok'
exit $fail
