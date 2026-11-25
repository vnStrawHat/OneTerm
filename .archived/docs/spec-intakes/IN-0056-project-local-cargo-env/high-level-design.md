# High-Level Design: Project-local cargo environment

Intake: IN-0056
Lane: normal
Date: 2026-10-23

## Idea

A SessionStart hook resolves the main checkout (also from inside a `.claude/worktrees/*` worktree, via `git rev-parse --git-common-dir`) and exports `CARGO_HOME`, `CARGO_TARGET_DIR`, `TMP`, `TEMP`, `TMPDIR` and `CARGO_BUILD_JOBS` into `CLAUDE_ENV_FILE`, so every later Bash call of the session and its subagents inherits them. Dependencies compile once for every worktree; temp files stay in the project; the job cap guards against rustc running out of memory; a warning fires under 20 GB free.

## Diagram

```text
SessionStart ─► .claude/hooks/cargo-env.sh ─► CLAUDE_ENV_FILE
                    │ root = dirname(git-common-dir)       │
                    │ CARGO_HOME=<root>/.cargo-home        ▼
                    │ CARGO_TARGET_DIR=<root>/target   every Bash call
                    │ TMP/TEMP/TMPDIR=<root>/.tmp       (main + worktrees)
                    │ CARGO_BUILD_JOBS=min(cpus, RAM/8)
                    └ warn when < 20 GB free
shell that missed the hook:  eval "$(bash .claude/hooks/cargo-env.sh)"
```

## UI Wireframe

N/A — no UI surface.

## Data Flow

1. Claude Code starts a session, runs the hook, the hook appends exports to `CLAUDE_ENV_FILE` and prints one `cargo-env:` line to stderr.
2. Each Bash call sources that file; cargo reads `CARGO_HOME` / `CARGO_TARGET_DIR` / `CARGO_BUILD_JOBS`; programs read `TMP`.
3. `EnterWorktree` copies `.claude/settings.local.json` (per `.worktreeinclude`) so the permission allowlist follows the agent.

## Detail Design

- [x] Detail design: not needed
- Reason: one shell script and configuration lines; the owner has ruled on each item.
