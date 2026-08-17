#!/usr/bin/env bash
# scripts/ci-local.sh — run the same quality gate as .github/workflows/ci.yml, locally.
#
# Usage:
#   scripts/ci-local.sh           # fmt, clippy, build, test + the Python policy checks
#   scripts/ci-local.sh --full    # also: vendor/refresh.sh --check (network) + cargo deny
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
step cargo build --workspace
step cargo test --workspace
step python scripts/verify-dependency-graph.py
step python scripts/check-ui-fork.py
step python scripts/check-doc-paths.py
step python -m unittest scripts/test_check_english.py
step python scripts/check-english.py
step python scripts/completion-catalog.py validate
step python scripts/benchmark-scale.py --list

if [[ $FULL -eq 1 ]]; then
  step bash vendor/refresh.sh --check
  if command -v cargo-deny >/dev/null 2>&1; then
    step cargo deny check licenses bans advisories
  else
    echo "ci-local: cargo-deny not installed (cargo install cargo-deny); skipping" >&2
  fi
fi

printf '\nci-local: all checks passed.\n'
