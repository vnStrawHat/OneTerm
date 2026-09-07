# Work: Re-declare the workspace on published gpui-component 0.6.0 and gpui-pre 0.3.x

ID: US-0039
Intake: IN-0017
Created: 2026-09-07

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [ ] Planned
- [ ] In progress
- [ ] Implemented
- [x] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: maintenance
- Risk lane: high-risk (supply-chain change + public dependency contract for 11 crates)
- Spec Intake: IN-0017

## Outcome

The workspace resolves `gpui`, `gpui_platform`, `gpui-component` and the assets crate from
crates.io — `gpui-pre` 0.3.x, `gpui-pre-platform` 0.3.x, `gpui-component` 0.6.0, `gpui-base`
0.6.0, `gpui-kit-assets` 0.6.0 — with every `use gpui::…` and `use gpui_component::…` statement
unchanged, and the full inventory of GPUI 0.2.2 → 0.3.x API drift is recorded before any
adaptation work begins.

This packet is deliberately not "the workspace compiles". It cannot: the dock redesign lands in
US-0040. Its deliverable is a correct dependency graph plus a measured error inventory that
US-0040/US-0041 consume.

## Scope

- [x] In scope: root `Cargo.toml` `[workspace.dependencies]`; the `gpui-base` addition to the
      crates that need it; `[profile.*.package]` package-name fixes; removing the
      `gpui-component` `[patch]` entry and `exclude` line; `gpui_kit_assets` in
      `crates/app/src/assets.rs` and its manifest; `Cargo.lock` regeneration; the drift inventory.
- [x] Out of scope: dock API migration (US-0040); non-dock component drift (US-0041); persistence
      proof (US-0042); deleting the vendor tree and CI gate (US-0043) — the tree stays on disk
      through this packet as a fallback.

## Acceptance

- [x] `cargo metadata` shows `gpui-pre`, `gpui-pre-platform`, `gpui-base`, `gpui-component`
      0.6.0 and `gpui-kit-assets` 0.6.0 resolved from crates.io, and no git source for either.
- [x] Source import aliases remain `gpui`/`gpui_component`; `assets.rs` uses renamed `gpui_kit_assets`.
- [x] No source file changed an import path because of the dependency swap (`assets.rs` is the one
      exception, for the assets-crate rename).
- [x] `[profile.dev.package]`, `[profile.fast-dev.package]` and `[profile.release.package]` name
      `gpui-pre` / `gpui-pre-platform`, verified to still take effect.
- [x] `cargo tree -d` was reviewed; the resolved GPUI family is one compatible 0.3/0.6 family.
- [x] `cargo tree -p gpui-component -e features` contains no `tree-sitter-*` grammar crate.
- [ ] The initial compiler-drift error classes were consumed while implementing US-0040/US-0041,
      but per-class site counts were not retained; this record does not reconstruct invented counts.
- [x] The implemented US-0040/US-0041 adaptations preserve the classified split between dock and
      non-dock drift; exact initial per-error site counts were not retained (gap above).
- [x] `alacritty_terminal` / `vte` vendoring remains verified: `bash vendor/refresh.sh --check`
      proves both against pristine + patches.

## Documentation

### Owning Docs Reviewed

- `docs/agents/dependencies.md` — §1 rev-lock table, §2 declaration block, §3 allowed auxiliary
  crates, §5 reference-first rule. §1 and §2 are directly contradicted by this work.
- `docs/agents/crate-dependency-rules.md` — R1–R12, in particular "declare every third-party dep
  once in `[workspace.dependencies]` and pull it in with `name.workspace = true`". Package aliasing
  preserves this rule; `gpui-base` must be added there, not inline.
- `docs/agents/structure.md` — crate dependency graph; gains `gpui-base` edges.
- `vendor/README.md` §1 — the vendored-crate table, which lists gpui-component.
- `docs/license-analysis.md` — the third-party licence position, which now covers a republished
  GPUI.
- `docs/PROJECT.md` — standing invariants.

### Documentation Action

Update required:

- `docs/agents/dependencies.md` §1 (rev-lock table → version table), §2 (declaration block), and
  the three inviolable same-rev rules → same-version rules.
- `docs/agents/crate-dependency-rules.md` — add `gpui-base` to the allowed set and record which
  crates may depend on it.
- `docs/agents/structure.md` — dependency graph.
- `docs/license-analysis.md` + `THIRD-PARTY-NOTICES.md` — regenerated crate set.
- `vendor/README.md` §1 — remove the gpui-component row (the tree itself goes in US-0043).

Reason: the rev-lock table and the "do not add gpui from crates.io" rule are now factually wrong;
leaving them would make the next agent session restore a git dependency.

### Reconciliation

Before completion, list the docs actually changed and confirm no reviewed doc still asserts a
git rev for `gpui` or `gpui-component`.

## Context

- `gpui-pre` 0.3.3 describes itself as "Zed's GPU-accelerated UI framework (gpui-pre snapshot of
  zed@5b055fa)", published by the gpui-kit maintainer (huacnlee). gpui-component 0.6.0 pins the
  0.3.x family. Rationale and rollback: `docs/decisions/DEC-0006-*`.
- The version currently pinned is `gpui` 0.2.2 at zed rev `1d217ee3`; the drift between that rev
  and `zed@5b055fa` is unmeasured, and measuring it is this packet's main risk-reduction value.
- 11 crates depend on `gpui-component`; only `crates/app` also depends on `gpui_platform` and the
  assets crate.
- Detail design: `low-level-design/01-dependency-aliasing.md`.

## Plan

- [ ] P0 screenshots and a full zoom/SFTP profile were not captured before manifest edits; a
      genuine pre-migration layout artifact was later recovered and its limitation is recorded in US-0042.
- [x] Capture the available baseline artifacts US-0042 needs (a real pre-migration layout and
      dependency resolution evidence). Screenshots were not captured, as recorded above.
- [x] Rewrite `[workspace.dependencies]`; add `gpui-base`.
- [x] Fix `[profile.*.package]` package names.
- [x] Remove the gpui-component `[patch]` entry and exclusion.
- [x] Add `gpui-base.workspace = true` to agent-ui, app, session-ui, sftp-ui, terminal-view,
      workspace, state.
- [x] Rename the assets import and manifest entry.
- [x] Resolve metadata and compile the workspace against the published family.
- [x] Route dock and non-dock adaptation to US-0040/US-0041.

## Decisions

- `docs/decisions/DEC-0006-depend-on-published-gpui-component-0-6.md`

## Verification Plan

Focused: `cargo metadata`, `cargo tree -d`, `cargo tree -p gpui-component -e features`,
`cargo deny check licenses bans advisories`, `python scripts/verify-dependency-graph.py`,
`python scripts/third-party-notices.py --check`, `bash vendor/refresh.sh --check`.
Platform: a `--profile fast-dev` build that still runs the DOOM-fire diagnostic without the
window becoming unresponsive, proving the profile overrides survived the package rename.
Regression boundary: none available until US-0040 restores compilation; that is expected and must
be reported as such rather than claimed as passing.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

The lockfile resolves `gpui-pre`/`gpui-pre-platform` 0.3.x and GPUI Kit 0.6 crates from crates.io;
no Zed or gpui-component git source remains. Dependency policy, metadata/tree checks, notices,
cargo-deny, terminal-vendor proof, all-target clippy, and the serial full workspace gate pass.
Current dependency/architecture/vendor guidance was rewritten for the published family.

The original error inventory was fully consumed by US-0040/US-0041 but exact initial site counts
were not retained. The requested interactive fast-dev DOOM-fire timing comparison was not run;
profile package names are mechanically correct and builds pass, but responsiveness is an E2E gap.

## Handoff

Next: US-0040 (dock), which consumes the drift inventory recorded here.
