#!/usr/bin/env bash
# SessionStart hook (US-0204): confine cargo and temp files to the project folder.
#
# Resolves the main checkout (even when the session runs inside a .claude/worktrees/*
# worktree) and appends exports to CLAUDE_ENV_FILE so every later Bash call inherits:
#   CARGO_HOME        -> <root>/.cargo-home   (registry + git caches, one copy per project)
#   CARGO_TARGET_DIR  -> <root>/target        (one build cache shared by all worktrees)
#   TMP/TEMP/TMPDIR   -> <root>/.tmp
#   CARGO_BUILD_JOBS  -> min(cpus, RAM_GB / 8)   (parallel rustc processes; OOM guard)
#
# Without CLAUDE_ENV_FILE the exports go to stdout, so a shell that missed the hook
# can run: eval "$(bash .claude/hooks/cargo-env.sh)"
set -eu

root="${CLAUDE_PROJECT_DIR:-$PWD}"
if common=$(git -C "$root" rev-parse --path-format=absolute --git-common-dir 2>/dev/null); then
  root=$(dirname "$common")
fi
# Git Bash gives /d/foo; cargo and rustc want D:/foo.
if command -v cygpath >/dev/null 2>&1; then
  root=$(cygpath -m "$root")
fi

mkdir -p "$root/.cargo-home" "$root/.tmp" "$root/target"

# OneTerm's full test build at 6 jobs on 32 GB died with os error 1455 ("paging file is
# too small", rustc mmap of a 90 MB rlib) while several UI lib-test crates compiled at once;
# 4 jobs is the measured safe figure. Windows without a pagefile cannot exceed physical RAM.
# ponytail: fixed RAM/8 ratio, measured on one box; tune the divisor if 0xc0000409 / 1455 recurs.
cpus=$(nproc 2>/dev/null || echo 4)
mem_kb=$(awk '/MemTotal/ {print $2}' /proc/meminfo 2>/dev/null || echo 0)
mem_gb=$(( (mem_kb + 524288) / 1048576 ))   # rounded: a "32 GB" box reports 31.7
jobs=$(( mem_gb / 8 ))
[ "$jobs" -lt 1 ] && jobs=1
[ "$jobs" -gt "$cpus" ] && jobs=$cpus

{
  echo "export CARGO_HOME=\"$root/.cargo-home\""
  echo "export CARGO_TARGET_DIR=\"$root/target\""
  echo "export TMP=\"$root/.tmp\""
  echo "export TEMP=\"$root/.tmp\""
  echo "export TMPDIR=\"$root/.tmp\""
  echo "export CARGO_BUILD_JOBS=$jobs"
} >> "${CLAUDE_ENV_FILE:-/dev/stdout}"

echo "cargo-env: root=$root (CARGO_HOME, CARGO_TARGET_DIR, TMP set; CARGO_BUILD_JOBS=$jobs)" >&2

# Worktrees and target/ eat disk fast; warn before a build fills the drive.
free_gb=$(( $(df -Pk "$root" | awk 'NR==2 {print $4}') / 1024 / 1024 ))
if [ "$free_gb" -lt 20 ]; then
  echo "cargo-env: WARNING only ${free_gb}G free on this drive; run 'cargo clean -p <crate>' or remove stale worktrees before building more" >&2
fi
