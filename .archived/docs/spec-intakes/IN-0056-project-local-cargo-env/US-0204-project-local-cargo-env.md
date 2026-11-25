# Work: Project-local cargo environment

ID: US-0204
Intake: IN-0056
Created: 2026-10-23

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [x] In progress
- [x] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: maintenance (build environment; owner ruling 2026-10-23 on the template review)
- Risk lane: normal. No product behaviour, no persistence, no secrets. The shared target across worktrees is a known failure source (see Context); the owner accepts it.
- Spec Intake, when required: IN-0056

## Outcome

In a Claude Code session on this repository, including subagents in `.claude/worktrees/*`:

- cargo builds into the root `target/` and caches the registry in the root `.cargo-home/`;
- temp files (`TMP`, `TEMP`, `TMPDIR`) land in the root `.tmp/`;
- `CARGO_BUILD_JOBS` is capped from the machine's RAM unless a command sets it lower;
- the session prints a warning when the drive has under 20 GB free;
- a shell that missed the hook can restore the environment with `eval "$(bash .claude/hooks/cargo-env.sh)"`, and every agent prompt says so;
- new worktrees receive `.claude/settings.local.json`;
- third-party crates carry no debuginfo in `profile.dev` (and `fast-dev`, which inherits it).

## Scope

- [x] In scope: `.claude/hooks/cargo-env.sh`, `.claude/settings.json` (hook registration), `.gitignore` (track the hook and settings; ignore `.tmp/`, `.cargo-home/`), `.worktreeinclude`, root `Cargo.toml` (`[profile.dev.package."*"] debug = false`), the five agent prompts, `AGENTS.md` §3, `docs/agents/subagent-workflow.md` §3 machine rules.
- [x] Out of scope: the template's lints, LTO, PowerShell switch, permission allowlist, agents and workflow (not adopted); `.gitattributes` LF normalisation and splitting the long agent docs (owner: separate work later); CI (does not run the hook).

## Acceptance

- [x] `bash .claude/hooks/cargo-env.sh` without `CLAUDE_ENV_FILE` prints six `export` lines with `D:/...` paths under the repository root, and from inside a worktree the same root.
- [x] After `eval` of those exports, `cargo build -p oneterm-core` from a worktree writes into the root `target/`, not the worktree's.
- [x] The `.tmp/`, `.cargo-home/` directories exist and are git-ignored; `.claude/hooks/cargo-env.sh` and `.claude/settings.json` are tracked.
- [x] `[profile.dev.package."*"]` has `debug = false`; `cargo build --workspace` passes under it.
- [x] Every agent prompt carries the fallback line; `subagent-workflow.md` no longer says "own `target`".
- [x] Full `bash scripts/ci-local.sh` passes with the hook's environment applied (the bash twin: the PowerShell tool does not receive the hook's exports).

## Documentation

### Owning Docs Reviewed

- `AGENTS.md` §3.1-3.3 — development rules and common commands; says nothing about where cargo caches or temp files go.
- `docs/agents/subagent-workflow.md` §3 "Rules the workflow depends on" — "Never point two agents at one `target` from two different paths (build scripts bake the path)" and the `CARGO_BUILD_JOBS` table; both change with this packet.
- `docs/PROJECT.md` — no build-environment invariant.
- `.claude/agents/*.md` — "build only in your worktree's own `target`" (coder, coder-lite); changes.

### Documentation Action

- Update required: `AGENTS.md` §3 (one environment bullet), `docs/agents/subagent-workflow.md` §3 (shared target rule, job cap), the five agent prompts.

Reason: the standing rule is reversed by owner ruling; the docs must say the new rule and where the fallback is.

### Reconciliation

Changed: `AGENTS.md` §3.1b (new), `docs/agents/subagent-workflow.md` §3 (shared target rule, job cap), the five `.claude/agents/*.md` prompts (fallback line; "own `target`" removed).

## Context

- Memory `worktree-shared-target-clobber` (2026-08-17 to 2026-09-28) records three failure forms of a shared target: workspace-crate rmeta/rlib overwritten across worktrees (E0463 / E0786), build scripts (`crates/theme/build.rs`) caching a worktree's absolute path, and `env!("CARGO_MANIFEST_DIR")` in tests baking a worktree path. Recovery was `cargo clean -p <workspace crates>`. The owner rules: apply the shared target anyway and report if it recurs.
- Measured on this machine (memory `gpui-kit-0-7-bump`): `CARGO_BUILD_JOBS=6` is the safe figure for full test builds on 32 GB; the template's `RAM/3` would give 10. The first gate run at `RAM_GB / 5` (6 jobs) died in `cargo test --workspace` with os error 1455 ("paging file is too small", rustc mmap of `libgpui_component` rlib, reported as E0786) while the UI lib-test crates compiled together, so the hook uses `RAM_GB / 8` (4 here); the agent briefs still pass explicit lower values.
- `profile.dev.package."*"` matches non-workspace packages only, so first-party crates keep `debug = "limited"`.

## Plan

- [x] Records (this packet, IN-0056, HLD).
- [x] Hook, settings, gitignore, worktreeinclude, Cargo.toml.
- [x] Agent prompts, AGENTS.md, subagent-workflow.md.
- [x] Verify (hook output, worktree build target, full gate); fill Evidence.

## Decisions

None; the owner ruled per item in the intake Source.

## Verification Plan

- `bash .claude/hooks/cargo-env.sh` (stdout mode) from the root and from a worktree.
- `eval "$(bash .claude/hooks/cargo-env.sh)" && cargo build -p oneterm-core` from a worktree; confirm the artifact path.
- `pwsh scripts/ci-local.ps1` with the exports applied.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

- Hook, stdout mode, from the root and from `.claude/worktrees/<dir>/` with `CLAUDE_PROJECT_DIR` set to the worktree: both print `root=D:/TrungKFC-Research/Rust/myTerm2`, six exports, `CARGO_BUILD_JOBS=4` (20 cpus, 31.7 GB rounded to 32, / 8).
- Gate run 1 at `RAM_GB / 5` = 6 jobs: fmt, both clippy passes green; `cargo test --workspace` failed compiling `oneterm-app` (lib test) with `error[E0786]: found invalid metadata files for crate gpui_component ... failed to mmap ... libgpui_component-97c63ba5f6a83909.rlib: The paging file is too small for this operation to complete. (os error 1455)`. Out of memory, not the worktree clobber (no other build was running). Cap lowered to `RAM_GB / 8`.
- Gate run 2 at 4 jobs (`bash scripts/ci-local.sh`, log `.tmp/ci-local-us0204-run2.log`): all 31 steps passed, `test result` lines sum to 5155 passed, 0 failed; `ci-local: all checks passed.`
- Worktree proof: `git worktree add --detach .claude/worktrees/us0204-probe HEAD`, `eval` of the hook there, `cargo build -p oneterm-core` compiled the worktree's `crates/core` and wrote `target/debug/deps/liboneterm_core-34e0cee3869d4235.rlib` in the ROOT target (timestamp matched); the worktree had no `target/` of its own. Worktree removed afterwards.
- `debug = false` for third-party crates: `libgpui_component` rlib 115 MB -> 87 MB; drive free space 57 GB -> 134 GB after the rebuild replaced the old artifacts.
- `.claude/settings.json`: the agent's first write was refused by the auto-mode classifier (self-modification); the owner asked for it explicitly and the second write went through. The hook is live from the next session; this session applied it with the `eval` fallback.
- Gaps: `cargo deny` (`--full`) not run; `platform_proof` is n/a (CI does not run the hook). The first-session verification the template recommends (a subagent printing `$CARGO_TARGET_DIR`) is still to be observed in a real dispatch.

## Handoff

None.
