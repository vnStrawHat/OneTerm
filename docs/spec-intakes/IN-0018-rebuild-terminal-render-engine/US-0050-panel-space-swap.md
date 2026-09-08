# Work: Panel and Space rewrite, final cleanup, parity and performance sign-off

ID: US-0050
Intake: IN-0018
Created: 2026-09-08

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [ ] In progress
- [ ] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: existing-contract change (public API preserved; panel/space internals rewritten; intake closed)
- Risk lane: normal (public `PanelSpec`/`TerminalPanel` surface is preserved source-compatibly; `docks.json` untouched)
- Spec Intake: IN-0018

## Outcome

`src/panel/` and `src/space/` are rewritten in place against `TerminalView` (same public API,
same behaviors §2.1/§2.2, same tests), no old module remains, `#[allow(dead_code)]` is gone from
`lib.rs`, every parity checkbox in the HLD is ticked with evidence, the DOOM-fire performance
comparison against the old crate is recorded, and the old rendering/split docs are marked
historical with a pointer to IN-0018.

## Scope

- [ ] In scope: `src/panel/{mod, terminal_panel, ops, actions, title, tests}.rs`,
      `src/space/{mod, tree, node, ops, placeholder, render, drag, tests}.rs` (file set may be
      consolidated; keep `crate::space::{SpaceId, SplitContext}` and
      `crate::panel::{PanelSpec, TerminalPanel, DuplicateDestination}` paths), `lib.rs`,
      `status.rs`/`agent.rs`/`security.rs` touch-ups, `docs/terminal-rendering-optimization.md`,
      `docs/terminal-gap-analysis.md`, `docs/terminal-split/*.md`, `docs/README.md` index,
      `docs/agents/structure.md` directory tree for `crates/terminal-view`.
- [ ] Out of scope: layout persistence for terminal panels, pane swap / focus traversal,
      middle-click paste, any `crates/terminal` change.

## Acceptance

- [ ] HLD parity items US-0050 (§1, §1.6, §1.7, §2.1 1–29, §2.2 1–18, §2.18, §4) pass; the
      panel (11), space (15), duplicate (2), and title tests exist with unchanged intent.
- [ ] `crates/app` and `crates/session-ui` compile without source changes.
- [ ] `lib.rs` declares exactly `render, input, terminal_view, theme, highlight, url,
      completion, space, panel, status, agent, security`; no `#[allow(dead_code)]`; `cargo
      clippy --workspace --all-targets -- -D warnings` clean.
- [ ] Every checkbox in the HLD Parity Checklist (all five groups) is ticked, each with a test
      name or a manual-check note in Evidence.
- [ ] Performance evidence recorded: DOOM-fire (`crates/tools` workload) run for ≥ 30 s on the
      old crate (`main` @ 12b3c11, `--features terminal-diagnostics`) and on the new crate; the
      diagnostics log lines show for the new crate `rows_planned ≤ rows_candidate`, `layers ==
      1 or 2`, one `snapshot_calls` per frame, and p95 prepaint+paint not worse than the old
      crate's p95 on the same machine. An idle terminal logs `rows_planned == 0`,
      `shape_calls == 0`.
- [ ] Manual Windows checks: split R/L/U/D, drag tab into empty Space (same tab and cross tab),
      drop onto occupied Space is a no-op, close Space / close last tab resets in place, rename
      tab (empty name warning), duplicate to new tab / into Space / via split, "+" dropdown, zoom,
      Agent Panel navigation focuses the right Space, status bar breadcrumb/net stats, recording
      dot.
- [ ] Old docs carry a "Historical — superseded by IN-0018" banner and `docs/README.md` lists
      the HLD/LLDs as current.
- [ ] `pwsh scripts/ci-local.ps1` green.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0018-rebuild-terminal-render-engine/high-level-design.md` — module map, interfaces, parity checklist, performance budget.
- `docs/spec-intakes/IN-0018-rebuild-terminal-render-engine/low-level-design/{shapes, render-pipeline, input}.md` — for sign-off cross-reference.
- `docs/spec-intakes/IN-0018-rebuild-terminal-render-engine/IN-0018.md` — requested outcome and validation shape.
- `docs/PROJECT.md` — public contracts (`AppServices`, panel names, `docks.json`).
- `docs/architecture.md`, `docs/agents/structure.md`, `docs/agents/crate-dependency-rules.md` — crate tree and R1–R12 (edge set unchanged).
- `docs/gui-layout.md` — dock composition the panel plugs into (`DockLayout`, `TabGroup`, `DockSkin`).
- `docs/terminal-split.md`, `docs/terminal-split/*.md` — behavior of Spaces (kept) and mechanism (superseded).
- `docs/agent-panel-display.md` — Agent Panel navigation into a Tab/Space.
- `docs/agents/persistence.md` — confirms no terminal-panel state is persisted (no change).
- `docs/README.md` — documentation index.

### Documentation Action

Update required: `docs/terminal-rendering-optimization.md`, `docs/terminal-gap-analysis.md`,
`docs/terminal-split/00-overview.md` … `07-roadmap-risks.md` (historical banner);
`docs/README.md` (index: IN-0018 HLD/LLDs current, old docs historical);
`docs/agents/structure.md` (new `crates/terminal-view` tree); `IN-0018.md` (tick packets, close
the intake); HLD parity checklist ticks.

Reason: these docs describe the old view's internals; the intake states they become historical
once the rewrite lands, and the structure doc must show the real module tree.

### Reconciliation

Before completion, list every doc changed and confirm no current (non-historical) doc names
`RowLayoutCache`, `TerminalElement` (old), `LocalTerminalView`, or `box_drawing`.

## Context

- Public items and `init` are unchanged; `register_panel` closure stays
  `PanelSpec::DefaultShell { workspace: Some(context.dock_area().entity_id()) }`.
- `TerminalPanel` implements both `gpui_base::dock::Panel` and `gpui_component::dock::Panel`
  (0.6 split): `panel_name = "terminal"`, `closable = false`, `zoom_control = Both`,
  `inner_padding = false`, no `dump` override.
- Space tree: `SpaceNode::{Leaf, Split}`, N-ary `Vec<SpaceNode>`, one `ResizableState` per
  split, owned-rebuild transforms, `CloseOutcome::LastSpaceClosed`; sizing delegated to
  `gpui_component::resizable`.
- Drag payload `DragTerminalTab { panel, title }`; drop only on empty placeholders.
- Agent grouping reads the live title directly from the view's session (no double lease).
- DOOM-fire workload: `crates/tools` (see `docs/agents/crate-dependency-rules.md`); run it in a
  local shell tab full screen.

## Plan

- [ ] Rewrite `space/` (ids, `SplitContext`, tree + ops, placeholder, render fast path, drag)
      with the 15 structure tests + `selected_space_uses_active_gutter_color`.
- [ ] Rewrite `panel/` (`PanelSpec`, `open`/`from_spec`, spawn helpers, ops incl. duplicate
      paths and `close_unplaced`, `set_active_space`/republish rules, drag-drop, tab title
      rendering + rename dialog + trimming, actions) with the 11 panel tests, duplicate tests,
      title tests.
- [ ] Remove `#[allow(dead_code)]` from `lib.rs`; clippy clean.
- [ ] Walk the HLD parity checklist; tick with evidence (test name or manual note).
- [ ] Performance runs (old vs new) with `--features terminal-diagnostics`; save log excerpts
      under `evidence/`.
- [ ] Mark old docs historical; update `docs/README.md`, `docs/agents/structure.md`,
      `IN-0018.md`.
- [ ] `pwsh scripts/ci-local.ps1`.

## Decisions

- `docs/decisions/DEC-0007-terminal-render-engine-cell-glyph-cache-and-quad-shapes.md`
  (Consequences: confirm "identical or better frame cost on DOOM-fire; seamless box drawing").

## Verification Plan

- Focused: `cargo test -p oneterm-terminal-view panel space` — `terminal_panel_disables_multi_tab_inner_padding`,
  `filling_space_one_does_not_renumber_space_two`,
  `closing_only_terminal_tab_keeps_empty_panel_for_new_tabs`,
  `closing_terminal_tab_with_sibling_removes_it`, `phase0_close_last_space_calls_session_close`,
  `phase0_close_non_last_space_closes_removed_session`,
  `phase1_shutdown_cancels_tasks_and_closes_session`, `phase1_shutdown_is_idempotent`,
  `duplicate_action_dispatches_to_the_active_space`,
  `tab_drop_onto_occupied_space_keeps_source_terminal`,
  `exited_behind_output_batch_marks_agent_ended`, `missing_live_cwd_uses_shell_default`,
  `local_duplicate_preserves_shell_config_and_replaces_cwd`, title tests, and the 15 space
  tests (`split_right_creates_two_leaves` … `alloc_id_is_monotonic`).
- Unit: `cargo test -p oneterm-terminal-view`; regression `cargo test --workspace`.
- E2E: manual Windows checklist (Acceptance) + DOOM-fire comparison.
- Gate: `pwsh scripts/ci-local.ps1`; `python scripts/verify-dependency-graph.py` (unchanged edges).

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

## Handoff

Depends on US-0049. Closes IN-0018.
