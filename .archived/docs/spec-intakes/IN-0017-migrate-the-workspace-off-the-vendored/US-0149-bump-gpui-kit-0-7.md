# Work: Bump GPUI Kit to 0.7.0 (gpui-pre 0.3.7)

ID: US-0149
Intake: IN-0017
Created: 2026-09-28

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

- Change type: maintenance (dependency upgrade)
- Risk lane: normal — UI layer only; no auth, persistence schema, or public `oneterm-vt` API.
- Spec Intake: [`IN-0017`](IN-0017.md) (the intake that put OneTerm on published GPUI Kit)

## Outcome

The workspace requires `gpui-base` / `gpui-component` / `gpui-kit-assets` `0.7` and resolves
`gpui-pre` / `gpui-pre-platform` `0.3.7` (GPUI Kit 0.7.0 pins `gpui-pre =0.3.7`); the full
local CI gate passes; the UI builds with no local `[patch]` (DEC-0006 rules unchanged).

## Scope

- [x] In scope: root `Cargo.toml` requirements, `Cargo.lock`, any compile/behaviour fix-up the
  0.7.0 API forces, `THIRD-PARTY-NOTICES.md`, `deny.toml` if the graph needs it,
  `reference/gpui-kit` checked out at `v0.7.0`, and the doc reconciliation below.
- [x] Out of scope: adopting new 0.7 features (multi-cursor input, `InputGroup`,
  `DockArea::select_panel`, tab close controls, …).

## Acceptance

- [x] `cargo tree -i gpui-pre` / `-i gpui-component` each show one version (0.3.7 / 0.7.0).
- [x] `scripts/ci-local.sh` passes.
- [x] App launches (fast-dev) and the workspace, dock and a terminal tab render. Settings window
  and a live dialog/toast were not opened in the GUI (see Gaps).

## Documentation

### Owning Docs Reviewed

- `docs/agents/dependencies.md` — version table, upgrade procedure §4, reference tag §5.
- `docs/decisions/DEC-0006-depend-on-published-gpui-component-0-6.md` — rules inherited; a
  minor-version bump is the "ordinary version bump" it anticipates, no new decision.
- `docs/agents/structure.md`, `docs/gui-layout.md`, `docs/terminal-backend.md` — cite the
  reference tag / Kit version.

### Documentation Action

- Update required: `docs/agents/dependencies.md` (table, §2 snippet, §5 tag),
  `docs/agents/structure.md`, `docs/gui-layout.md`, `docs/terminal-backend.md` tag mentions.

Reason: they state the pinned version and reference tag, which change.

### Reconciliation

Changed: `docs/agents/dependencies.md` (table, §2 snippet, §5 tag), `docs/agents/structure.md`,
`docs/gui-layout.md`, `docs/terminal-backend.md`, and the `crates/settings-ui/src/panel.rs`
module note (Root now mounts the notification layer; the in-page rule stands). DEC-0006 rules
all still hold; no new decision.

## Plan

- [x] Bump requirements, `cargo update -p` the families, build, fix fallout.
- [x] Checkout `reference/gpui-kit` at `v0.7.0`.
- [x] Regenerate notices, update docs, run the gate, launch the app.

## Context

0.7.0 API breaks that reached OneTerm (from `reference/gpui-kit/release-notes.md` and the build):

- Dock tiles canvas removed (`PaneRef::Tiles`, `TilesRenderer`, persisted `"tiles"` info). OneTerm
  never created tiles, so `docks.json` written by OneTerm is unaffected; the skin wrapper and the
  match arm are deleted.
- `Root` owns the dialog / sheet / notification layers (a `RootPlugin`); `Root::render_*_layer`,
  `Root::open_dialog` and the `notification` field are gone. The workspace render drops its three
  layer children (they sat at the same full-window position Root now uses). The SFTP test counts
  toasts via `WindowExt::notifications`.
- `WindowExt` now reads the `Root` entity to find its plugin, so calling it inside
  `window.update(|root, ..|)` would double-lease and panic. The crash-report dialog and the
  elevated "settings could not be read" toast in `crates/app/src/window.rs` are now
  `window.defer`red. (The toast already had this shape on 0.6, via `Root::update`.)

## Verification Plan

`scripts/ci-local.sh`; `cargo tree -i` per family; fast-dev launch with a screenshot.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

- `CARGO_BUILD_JOBS=6 bash scripts/ci-local.sh --full` on Windows, 2026-09-28: all checks passed,
  141 test suites / 4819 passed / 0 failed; `cargo deny`: advisories, bans, licenses ok.
  At the default parallelism (20 jobs) the test build failed with `E0786 invalid metadata` even
  for `std`. That is exhaustion of the machine's commit limit (31.7 GB, ~12 GB free), not the
  change; `-j 6` built cleanly.
- `cargo tree -i` for `gpui-pre`, `gpui-pre-platform`, `gpui-base`, `gpui-kit-assets`: one version
  each (0.3.7 / 0.7.0). The lock also drops `gpui-pre-media` and `syntect`, and adds four
  macOS-only `objc2-*` crates. `THIRD-PARTY-NOTICES.md` was regenerated.
- `cargo build -p oneterm-app --profile fast-dev`, launched under a scratch HOME: the window
  opens with the title bar, center tabs + Command Prompt, the right dock (sessions + SFTP) and the
  status bar all rendering; no panic in the log. The only ERROR is the DXGI debug-layer probe
  (`0x887A002D`, SDK missing on this box).
- Gaps: the crash-report dialog, the elevated toast, the Settings window and live toasts/dialogs
  were not exercised in the GUI. Toast delivery is covered by the SFTP transfer test (TEST-25).
  `platform_proof` (Linux/macOS) waits for CI.
