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
# `regex` gates a whole matcher, so the suite runs under it too: the literal
# half must answer identically with the feature on.
step cargo test -p oneterm-vt --features regex

# Other projects consume `oneterm-vt` as a git dependency, so its package, its
# feature matrix and its documentation are part of the gate. The default set is
# `pty` (`US-0104`), so the two builds below really are different
# configurations and only the first one is transport-free.
step cargo build -p oneterm-vt --no-default-features --examples
# Building it is not running it: `tests/engine_without_pty.rs` is gated
# `#[cfg(not(feature = "pty"))]`, so this is the only step that executes it.
step cargo test -p oneterm-vt --no-default-features
step cargo build -p oneterm-vt --all-features --examples
# The six-dependency claim the README makes is a claim about
# `--no-default-features` specifically now that a *default* feature adds
# dependencies. Without this assertion it would be prose.
printf '\n==> cargo tree -p oneterm-vt -e normal --no-default-features\n'
vt_leaves="$(cargo tree -p oneterm-vt -e normal --no-default-features --prefix none |
  awk 'NR > 1 {print $1}' | sort -u | paste -sd' ' -)"
if [ "$vt_leaves" != 'bitflags log memchr rustc-hash unicode-segmentation unicode-width' ]; then
  printf '\nci-local: FAILED: oneterm-vt --no-default-features must be exactly six leaf dependencies, got: %s\n' \
    "$vt_leaves" >&2
  exit 1
fi
step cargo run -p oneterm-vt --example headless
step env RUSTDOCFLAGS='-D warnings' cargo doc -p oneterm-vt --no-deps --all-features
# ...and again with no features: an intra-doc link to a cfg-gated item resolves
# under `--all-features` and is broken in a default build, which is the build
# most embedders get (`US-0101` verification note 4).
step env RUSTDOCFLAGS='-D warnings' cargo doc -p oneterm-vt --no-deps
# Two snapshots, one per platform family (`US-0104`): `--check` compares the
# host's, `--diff-platforms` asserts the other one differs only inside
# `oneterm_vt::pty` and needs no rustdoc.
step python scripts/vt-public-api.py --check --no-doc
step python scripts/vt-public-api.py --diff-platforms
# What the package carries, and that it reaches nothing outside `crates/vt`.
# `--allow-dirty` because an agent runs this gate with uncommitted work; the
# workflow packages a clean checkout without it.
printf '\n==> cargo package -p oneterm-vt --list | verify-dependency-graph.py --package-list -\n'
# `set -o pipefail` is on at the top of this script, so a `cargo package`
# failure fails the pipeline rather than being masked by python's status.
if ! cargo package -p oneterm-vt --allow-dirty --list |
    python scripts/verify-dependency-graph.py --package-list -; then
  printf '\nci-local: FAILED: the oneterm-vt package is missing a required file\n' >&2
  exit 1
fi
# Published rustdoc must read for somebody who does not have this repository:
# no work-packet, decision or intake citations in `///` or `//!` text, and no
# bare `crates/...` or `docs/...` path either -- a consumer's vendored copy has
# neither. A link to the public repository is the one allowed form.
printf '\n==> rustdoc self-containment (crates/vt/src)\n'
if grep -rn '^[[:space:]]*//[/!].*\(US-0[0-9]\{3\}\|BUG-0[0-9]\{3\}\|DEC-0[0-9]\{3\}\|IN-0[0-9]\{3\}\|crates/\|docs/\)' \
    crates/vt/src --include='*.rs' | grep -v 'https://github.com/'; then
  printf '\nci-local: FAILED: the crate rustdoc cites a document only this repository has\n' >&2
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
