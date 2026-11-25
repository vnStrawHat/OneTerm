# Work: Bump GPUI Kit to 0.7.3 (gpui-pre 0.3.9)

ID: US-0190
Intake: IN-0052
Created: 2026-10-15

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [ ] Planned
- [ ] In progress
- [x] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: new capability | existing-contract change | bug | maintenance | acceptance rework
- Change type: maintenance (dependency upgrade)
- Risk lane: normal — UI layer only; no auth, persistence schema, or public `oneterm-vt` API.
- Spec Intake, when required: IN-0052

## Outcome

The workspace resolves `gpui-base` / `gpui-component` / `gpui-kit-assets` `0.7.3` and `gpui-pre` / `gpui-pre-platform` `0.3.9` (GPUI Kit 0.7.3 pins `gpui-pre =0.3.9`); the full local CI gate passes; the UI builds with no local `[patch]` (DEC-0006 rules unchanged).

## Scope

- [x] In scope: `Cargo.lock` (requirements `0.3` / `0.7` already admit the new versions), any compile or behaviour fix-up the new versions force, `THIRD-PARTY-NOTICES.md`, `deny.toml` if the graph needs it, `reference/gpui-kit` at `v0.7.3`, the doc reconciliation below.
- [x] Out of scope: adopting new Kit features. Note: 0.7.3 does not contain `longbridge/gpui-kit` #3518 (vertical wheel on tab strips, merged on `main` 2026-10-15).

## Acceptance

- [x] `cargo tree -i` shows one version each: `gpui-pre` / `gpui-pre-platform` 0.3.9, `gpui-base` / `gpui-component` / `gpui-kit-assets` 0.7.3.
- [x] `force_width_copy_matches_gpui` passes against `gpui-pre` 0.3.9 (dependencies.md §4 step 4).
- [x] `scripts/ci-local.sh` passes (the `--full` run's `cargo deny` advisories step fails on rustls, unrelated; see Evidence).
- [ ] The app launches; the workspace, dock and a terminal tab render.

## Documentation

### Owning Docs Reviewed

- `docs/agents/dependencies.md` — version table, upgrade procedure §4, reference tag §5.
- `docs/decisions/DEC-0006-depend-on-published-gpui-component-0-6.md` — an ordinary version bump; rules unchanged.
- `docs/agents/structure.md`, `docs/terminal-backend.md`, `docs/gui-layout.md` — cite the reference tag or the `gpui-pre` version.

### Documentation Action

Choose one and explain it before implementation:

- Update required: `docs/agents/dependencies.md` (table, §5 tag), `docs/agents/structure.md` (reference tag), `docs/terminal-backend.md` (reference tag); `docs/gui-layout.md` lines that cite gpui-pre 0.3.7 behaviour are re-checked against 0.3.9 and updated only if still true.
- No contract change: explain why the reviewed docs already describe the correct behavior.

Reason: they state the pinned versions and the reference tag, which change.

### Reconciliation

Changed: `docs/agents/dependencies.md` (table, §5 tag), `docs/agents/structure.md`, `docs/terminal-backend.md`, `docs/gui-layout.md` (two gpui-pre version citations, re-checked true for 0.3.9: `GpuSpecs` still has no backend field and macOS `gpu_specs` still returns `None`; `Window::reuse_prepaint` still does not replay AccessKit nodes), and the matching version citations in `crates/terminal-view/src/render/glyphs.rs`, `crates/workspace/src/layout/mod.rs`, `crates/workspace/src/widgets/fps_hud.rs`. DEC-0006 rules all still hold; no new decision.

## Context

Add only relevant code, decisions, dependencies, and invariants not already clear from the owning docs.

## Plan

- [x] `cargo update -p gpui-component -p gpui-base -p gpui-kit-assets -p gpui-pre -p gpui-pre-platform`; `cargo tree -i` each.
- [x] Build: no fallout, no OneTerm source change needed.
- [x] `reference/gpui-kit` checkout `v0.7.3`.
- [x] Notices, docs, gate. App launch left to the owner.

## Decisions

Link consequential decisions; do not repeat their rationale here.

## Verification Plan

`cargo tree -i` per family; `scripts/ci-local.sh`; fast-dev or debug launch with a screenshot.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

- `cargo tree -q -i <pkg> --depth 0`: gpui-pre 0.3.9, gpui-pre-platform 0.3.9, gpui-base 0.7.3, gpui-component 0.7.3, gpui-kit-assets 0.7.3, one version each. `Cargo.lock` -590/+136 lines (the 0.3.9 family drops log/tracing-subscriber/sval and several macOS media crates, adds `notify`, `similar`, `objc2-security`). `THIRD-PARTY-NOTICES.md` regenerated.
- `cargo clippy --workspace --all-targets -D warnings`: clean with no source change.
- `apply_force_width` copy: gpui-pre 0.3.7 and 0.3.9 `text_system/line_layout.rs` are identical (`diff` exit 0); the gate's `force_width_copy_matches_gpui` passes.
- `CARGO_BUILD_JOBS=4 bash scripts/ci-local.sh --full` (2026-10-15): every step up to and including `third-party-notices.py --check` passed; `cargo deny` bans ok, licenses ok, advisories FAILED on RUSTSEC-2026-0346/0347/0348/0349 in `rustls` 0.23.45 (via `reqwest` in `oneterm-update`). `rustls` is unchanged by this bump (0.23.45 before and after); the advisories are new and fail on the base commit too. Fixed in a separate intake, as IN-0034 did.
- `deny.toml`: the `bzip2-1.0.6` allowance is dropped because the bump removed its only user (`libbz2-rs-sys`); `docs/license-analysis.md` status note says so. The `zlog` / `ztracing` / `ztracing_macro` exceptions were already unmatched before this bump (none of the three is in the old or the new lock) and are left for whoever owns that analysis.
- Gaps: the app was not launched (owner asked for shorter runs); `platform_proof` waits for CI.

## Handoff

Use only across actors or sessions: current state, next owner/action, and blockers.
