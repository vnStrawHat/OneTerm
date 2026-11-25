# Work: Migrate the dock shell onto the gpui-base layout tree and DockSkin seam

ID: US-0040
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

- Change type: existing-contract change
- Risk lane: high-risk (broad established behavior — the entire workspace shell)
- Spec Intake: IN-0017

## Outcome

OneTerm's workspace shell runs on the v0.6.0 dock: `DockLayout` instead of `DockItem`, `TabGroup`
instead of `TabPanel`, a `DockSkin` renderer installed on the `DockArea`, and every panel
implementing both halves of the split `Panel` trait — with the visible result indistinguishable
from the pre-migration build, and with no dependency on any vendored patch.

## Scope

- [x] In scope: `crates/state/src/dock_util.rs`; `crates/workspace/src/layout/workspace/**`
      (mod, layout, actions, persistence, layout_tests) and `layout/statusbar.rs`; the five `Panel`
      impls and their `register_panel` builders (agent-ui, app/ssh_client_panel, session-ui,
      sftp-ui, terminal-view); `crates/terminal-view/src/{panel/**, space/drag.rs, agent.rs}`;
      `crates/session-ui/src/common.rs` (`add_ssh_terminal_to_dock`); `crates/state/src/{app_state,
      active_terminal, commands}.rs` `DockArea` signatures; the affected test modules.
- [x] Out of scope: non-dock component drift (US-0041); persistence proof (US-0042); vendor
      deletion (US-0043). Any *behavior* change beyond restoring parity — if a v0.6.0 capability
      (tiles, `NavStack`, drag-target reporting) looks attractive, it is a separate intake.

## Acceptance

- [x] `cargo clippy --workspace --all-targets -- -D warnings` is clean.
- [x] No call to the removed vendor-patch API remains (`set_active_panel`, `panel_count`); the
      replacements are `TabGroup::select_tab` and `TabGroup::panels().len()`.
- [x] Every persisted panel name resolves with `panel_handle(..)` and reaches the tab bar with its
      real title — asserted by a test because installing a bare entity compiles and fails visually.
- [x] `OneTermDockSkin` suppresses the outer single-panel tab bar for `SshClientPanel`/Agent while
      `SshClientPanel` retains its internal split and headers. This is structurally and unit tested;
      final visual parity remains part of the manual gap.
- [x] The right dock still renders as OneTerm's chrome-free `SshClientPanel` with its internal
      split and its own title bars, not as a tab-barred group.
- [x] `PanelStyle::TabBar` behavior is preserved through the skin handle.
- [x] Zoom restore is covered by the 0.5.2 fixture integration test.
- [x] `workspace/layout_tests.rs`, `terminal-view/panel/tests.rs` and `space/tests.rs` pass with
      their intent unchanged.
- [ ] IN-0015/IN-0016 manual GUI scenarios were not run. Their automated regressions pass: a
      normalized empty center is recreated, and closing the final terminal retains an empty panel.
- [x] The old ghost-container workaround was replaced by GPUI Base's normalized-tree
      `DockArea::is_empty`/`DockLayout` path and pinned by `center_empty_check_follows_normalized_layout`.

## Documentation

### Owning Docs Reviewed

- `docs/gui-layout.md` — the owning contract for the workspace layout, dock composition, and the
  right-dock modes. Describes `DockItem` composition directly.
- `docs/terminal-split.md` + `docs/terminal-split/03-drag-drop.md` — Spaces, split R/L/U/D, and
  dragging a tab into a Space, built on `TabPanel` drag payloads.
- `docs/agent-panel-display.md` — Agent panel navigation, which activates an existing panel (the
  reason patch 0001 exists).
- `docs/architecture.md` — crate boundaries for the shell vs. feature crates.
- `docs/spec-intakes/IN-0015-*` and `IN-0016-*` — the ghost-`TabPanel` and last-tab-alive
  behaviors that this migration can silently undo.
- `docs/spec-intakes/IN-0007-*` — multi-tab content gap, fixed via `inner_padding=false`, which is
  now a `gpui_component::dock::Panel` method.
- `docs/agents/persistence.md` — dock state persistence mechanics.

### Documentation Action

Update required: `docs/gui-layout.md`, `docs/terminal-split.md`,
`docs/terminal-split/03-drag-drop.md`, `docs/agent-panel-display.md` — each names `DockItem`,
`TabPanel` or `StackPanel` as the mechanism. Rewrite those mechanism descriptions against
`DockLayout` / `TabGroup` / `DockSkin` while leaving the *behavioral* contracts unchanged.

Reason: these docs describe how the layout is built, not only what it does, so the mechanism
rename makes them inaccurate rather than merely incomplete.

### Reconciliation

Before completion, confirm no current (non-archived) doc describes the dock in terms of
`DockItem`, `TabPanel` or `StackPanel`.

## Context

- 46 `gpui_component::dock` references across 28 files; 5 `Panel` impls; 5 `register_panel` sites;
  `DockItem::*` construction in 4 files.
- The trait split means `zoomable()` is now two answers: `gpui_base::dock::Panel::zoomable() ->
  bool` (may it zoom at all) and `gpui_component::dock::Panel::zoom_control() ->
  Option<PanelControl>` (where the affordance appears). Panels that returned `None` to refuse zoom
  must now return `false` **and** `None`.
- `set_active` timing changed upstream: exactly one notification per edge on the next tick, and a
  removed panel gets `on_removed` instead of `set_active(false)`. OneTerm's active-terminal
  tracking and the SFTP panel's active-state observation both depend on this.
- Base normalizes the tree after every edit and drops empty containers, which is why the IN-0015
  workaround may be obsolete.
- Detail design: `low-level-design/02-dock-migration.md`.

## Plan

- [x] Port shared dock traversal to the GPUI Base pane tree.
- [x] Split the five `Panel` impls; convert the five `register_panel` builders to
      `PanelBuildContext` returning `Arc<dyn PanelView>` wrapping a `PanelHandle`.
- [x] Install `DockSkin`, move `panel_style` to the skin handle, convert
      layout construction to `DockLayout`, port zoom restore to `TabGroup` + `PanelId`.
- [x] Reproduce the chrome-free right dock with a skin-suppressed outer tab group and composite
      `DockItem::Panel`; settle `PanelStyle` / `inner_padding` / `zoom_control` against actual
      rendering.
- [x] Port terminal-view from `TabPanel` to `TabGroup`, adopt `on_added_to`, and drop the
      patched-API calls.
- [x] Port the test modules and add all-five-name presentation assertions.
- [ ] Run the IN-0015/IN-0016 GUI scenarios; automated equivalents pass, but manual UAT is open.

## Decisions

- `docs/decisions/DEC-0006-depend-on-published-gpui-component-0-6.md`
- A new decision is required only if the right-dock composition has to change visibly, or if the
  IN-0015 workaround is removed on the strength of upstream normalization.

## Verification Plan

Unit: `workspace/layout_tests.rs`, `terminal-view/panel/tests.rs`, `space/tests.rs`, plus a new
focused test proving presentation survives the base seam (recovered `PanelHandle` returns a real
title, not `panel_name`).
Integration: `cargo test --workspace`.
E2E (manual): drag a tab between groups; drag a tab into an empty Space; split R/L/U/D; zoom in
and out; collapse and reopen the right dock; switch right-dock mode between SSH-client and Agent;
close the last tab; connect SSH after closing every tab.
Platform: Windows GUI run, since that is the development and release target.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Harness verification passes: `cargo clippy --workspace --all-targets -- -D warnings` plus 32
state, 193 terminal-view, and 9 workspace tests. The serial full local gate passed, and the new
fixture test proves real zoom restoration. Current `gui-layout.md` and the Terminal Split pages now
describe `DockLayout`, `TabGroup`, and `DockSkin`; persisted `"StackPanel"`/`"TabPanel"` strings
remain only where the on-disk schema is discussed.

The normalized empty-tree test records the IN-0015 decision: no ghost-container workaround is
retained. Manual Windows GUI checks (drag, visual chrome, zoom controls, dock mode switching,
last-tab plus button, and SSH-after-empty) and screenshot comparison were not run.

## Handoff

Depends on US-0039. Blocks US-0042 (persistence proof needs a compiling dock) and US-0043.
