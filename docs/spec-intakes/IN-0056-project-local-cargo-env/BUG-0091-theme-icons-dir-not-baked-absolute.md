# Work: oneterm-theme must not bake an absolute icons path into the shared target

ID: BUG-0091
Intake: IN-0056
Created: 2026-10-23

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [x] In progress
- [ ] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: bug
- Risk lane: normal
- Spec Intake: IN-0056

## Outcome

A build of `oneterm-theme` in one checkout never breaks a build in another checkout that shares
`CARGO_TARGET_DIR`. Today `crates/theme/build.rs` exports `ONETERM_UI_ICONS_DIR` as an absolute
path; cargo reuses the cached build-script output from a deleted worktree and `icon_named!`
panics: `failed to read '...\.claude\worktrees\agent-...\crates\theme\assets/icons'`.

## Scope

- [x] In scope: `crates/theme/build.rs`, `crates/theme/src/icon.rs`, `docs/agents/structure.md` (one line).
- [x] Out of scope: the `icon_named!` macro (reference/gpui-kit, never patched).
- [x] Audit of every `crates/*/build.rs`: `theme` bakes an absolute path (fixed here);
  `update` emits `rustc-env=TARGET` (a target triple, path-independent); `completion` reads
  `CARGO_MANIFEST_DIR` and writes under `OUT_DIR` (the generated file may embed `include_str!`
  paths, so it is judged by reading only, see Gaps); `app` uses `CARGO_MANIFEST_DIR` for the
  Windows resource and `OUT_DIR` for the `.rc` and copy steps (same).

## Acceptance

- [ ] `cargo build -p oneterm-theme` from the worktree, then from the main checkout, both pass with one shared target and no `cargo clean`.
- [ ] `pwsh scripts/ci-local.ps1` green in the worktree.

## Documentation

### Owning Docs Reviewed

- `AGENTS.md` § 3.1b (shared target) and § 3.4 (icon pipeline: drop an SVG, build.rs and the macro generate the variant): behaviour unchanged, no edit.
- `docs/agents/structure.md` line 132 describes build.rs as setting `ONETERM_UI_ICONS_DIR`: updated.

### Documentation Action

`docs/agents/structure.md` one-line correction.

## Context

Coordinator gate log on main after a worktree built against the shared root target and was deleted.

## Plan

Option (a) chosen. The macro supports a literal path "relative to the calling crate's
CARGO_MANIFEST_DIR", which rustc sets per crate at expansion time, so no path is ever stored in
cargo's cache. `icon.rs` uses `icon_named!(AppIcon, "assets/icons", ...)`; `build.rs` keeps only
its sanity check and `rerun-if-changed=assets/icons` (re-running the script is what makes cargo
recompile the crate when an SVG is added, since the proc macro does not track the directory).
Options (b) and (c) were not needed.

## Decisions

None.

## Verification Plan

Recipe from the brief (two `cargo build -p oneterm-theme` runs, worktree then main), then the full gate.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

### PROOF

Pending: cargo is held until the coordinator releases the shared target.

### Gaps

The `completion` and `app` build scripts are judged by reading only; the proof run covers theme.

## Handoff

None.
