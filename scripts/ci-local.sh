#!/usr/bin/env bash
# scripts/ci-local.sh — run the same quality gate as .github/workflows/ci.yml, locally.
#
# Usage:
#   scripts/ci-local.sh           # fmt, clippy, test + the Python policy checks
#   scripts/ci-local.sh --full    # also: cargo deny (needs cargo-deny installed)
#
# Stops at the first failing command and prints it. Keep this list in sync with
# ci.yml and AGENTS.md §4 (scripts/ci-local.ps1 is the PowerShell twin).

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

FULL=0
[[ "${1:-}" == "--full" ]] && FULL=1

step() {
  printf '\n==> %s\n' "$*"
  if ! "$@"; then
    printf '\nci-local: FAILED: %s\n' "$*" >&2
    exit 1
  fi
}

step cargo fmt --all -- --check
step cargo clippy --workspace --all-targets -- -D warnings
# `terminal-diagnostics` guards ~200 lines no other step compiles (`US-0090`).
step cargo clippy --workspace --all-targets --features oneterm-app/terminal-diagnostics -- -D warnings
step cargo test --workspace
# IN-0029 R-28: the VT engine's integrity walk is bounded to the rows an
# operation touched unless `vt-paranoid` is on. This is where the unbounded
# whole-history invariants are gated.
step cargo test -p oneterm-vt --features vt-paranoid

# `oneterm-vt` is published to crates.io, so its package, its feature matrix and
# its documentation are part of the gate. Its default feature set is empty, so
# the two builds below are the whole matrix.
step cargo build -p oneterm-vt --no-default-features --examples
step cargo build -p oneterm-vt --all-features --examples
step env RUSTDOCFLAGS='-D warnings' cargo doc -p oneterm-vt --no-deps --all-features
step python scripts/vt-public-api.py --check --no-doc
step cargo publish -p oneterm-vt --dry-run
# Published rustdoc must read for somebody who does not have this repository:
# no work-packet, decision or intake citations in `///` or `//!` text. A link to
# the public repository is the one allowed form.
printf '\n==> rustdoc self-containment (crates/vt/src)\n'
if grep -rn '^[[:space:]]*//[/!].*\(US-0[0-9]\{3\}\|BUG-0[0-9]\{3\}\|DEC-0[0-9]\{3\}\|IN-0[0-9]\{3\}\|docs/spec-intakes\)' \
    crates/vt/src --include='*.rs' | grep -v 'https://github.com/'; then
  printf '\nci-local: FAILED: published rustdoc cites a document only this repository has\n' >&2
  exit 1
fi

step python scripts/verify-dependency-graph.py
step python scripts/check-doc-paths.py
step python -m unittest scripts/test_check_english.py
step python scripts/check-english.py
step python scripts/completion-catalog.py validate
step python scripts/third-party-notices.py --check

if [[ $FULL -eq 1 ]]; then
  if command -v cargo-deny >/dev/null 2>&1; then
    step cargo deny check licenses bans advisories
  else
    echo "ci-local: cargo-deny not installed (cargo install cargo-deny); skipping" >&2
  fi
fi

printf '\nci-local: all checks passed.\n'
