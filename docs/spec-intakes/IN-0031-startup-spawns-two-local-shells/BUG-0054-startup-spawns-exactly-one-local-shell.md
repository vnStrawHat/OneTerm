# Work: Startup spawns exactly one local shell

ID: BUG-0054
Intake: IN-0031
Created: 2026-09-14

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

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
- Spec Intake, when required: `IN-0031`

## Outcome

One application launch spawns exactly one local shell. `persistence::load_layout` no longer builds
the persisted center's panels, because `OneTermWorkspace::new` resets the center immediately
afterwards; nothing is constructed only to be pruned, so no `LocalSession` is dropped during
startup and no `cmd.exe` loses its console host mid-initialisation. The right dock panel, its width
and open state, `sftp_table_state` and `zoomed_panel` restore exactly as they do today.

## Scope

- [x] In scope: `persistence::load_layout` in
  `crates/workspace/src/layout/workspace/persistence.rs` — drop the center subtree from the
  deserialised `DockAreaState` before it reaches `DockArea::load`; the doc comment on
  `OneTermWorkspace::new` and `docs/gui-layout.md` §Persistence; the focused assertion in
  `crates/workspace/src/layout/workspace/layout_tests.rs`.
- [x] Out of scope:
  - `DockArea::load` itself (`gpui-base`, an external dependency) and any change to how it walks
    a state it is given.
  - `TerminalPanel::open` / `LocalSession::spawn` — building a terminal panel must keep spawning
    its shell; this packet only stops a build that should never happen.
  - The orphan `cmd.exe` that outlives a pruned panel — `BUG-0055`.
  - `reset_center_only` also rebuilding `ssh_client_panel` and discarding the loaded one. Real,
    observed while reading this path, but it spawns no process and costs one entity; it is not
    this outcome and gets its own packet if it is ever worth fixing.
  - The persisted format. `MAIN_DOCK_VERSION` stays `3`, `schema_version` stays `1`, and the
    document written after startup is byte-identical to today's for the same window state.

## Acceptance

- [ ] Loading a document whose center holds `terminal` panels builds **zero** of them; the
  subsequent `reset_center_only` builds exactly one. Asserted by build count, not by tree shape.
- [ ] The right dock keeps its saved panel name, width and open state across
  load → reset → save, and `zoomed_panel` still restores by name.
- [ ] `pre_migration_fixture_loads_and_saves_without_semantic_drift` still passes untouched. It
  drives `DockArea::load` directly and asserts the full center round-trips, which is the check
  that this fix did not reach into the dock crate's behaviour.
- [ ] Ten consecutive launches of the built binary with a saved `docks.json` holding a center
  terminal: exactly one `cmd.exe` child per launch, no `#32770` "Application Error" dialog, and
  no `cmd.exe` surviving application exit.
- [ ] `pwsh scripts/ci-local.ps1` exits 0.

## Documentation

### Owning Docs Reviewed

- `docs/gui-layout.md` §Persistence — "Startup intentionally resets the center to one terminal tab
  while retaining right-dock state and then restores zoom by panel name." Describes the intended
  behaviour correctly but omits that the loaded document's center is currently built first, which
  is the whole defect. Needs one sentence of sharpening.
- `docs/gui-layout.md` §Panel registration and presentation — establishes that the shell builds
  feature panels by registered name through `PanelRegistry`; the invariant this packet depends on
  is that building the `terminal` panel is what spawns a session.
- `docs/terminal-backend.md` §6.2 — `LocalSession::spawn`, the PTY owner thread, and the
  `Conpty`-first field ordering whose `ClosePseudoConsole` is the far end of the race. Unchanged
  by this packet; reviewed to confirm the fix belongs in the workspace, not the transport.
- `crates/workspace/src/layout/workspace/mod.rs` — the `OneTermWorkspace::new` doc comment ("load
  the old layout (keep right dock + settings), but reset the center") and the `PERF-27` comment on
  reading `docks.json` once.
- `docs/agents/persistence.md` — confirms `dock_persistence` owns the document; this packet does
  not read or write it differently.

### Documentation Action

Update required:

- `docs/gui-layout.md` §Persistence — state that a loaded document contributes its side docks,
  sizes and zoom, and that its center subtree is deliberately **not** built because startup
  replaces the center. Naming the reason is what stops the next refactor from reinstating the
  double spawn.
- `crates/workspace/src/layout/workspace/persistence.rs` — the `load_layout` doc comment must say
  that the center is dropped before `DockArea::load` and why (building a `terminal` panel spawns a
  shell).

Reason: the accepted contract already claims the correct user-visible behaviour, so this is not a
contract change; but both docs are silent on the mechanism that makes it true, and that silence is
what allowed the defect. No other owning doc changes: no schema, no public interface, no session
lifecycle moves.

### Reconciliation

Before completion, list docs changed or confirm the recorded no-change reason remains valid.

## Context

`OneTermWorkspace::new` (`crates/workspace/src/layout/workspace/mod.rs:119-134`):

```rust
let document = persistence::read_dock_document().unwrap_or_else(|error| { … });
let saved_zoom = document.as_ref().and_then(|d| d.zoomed_panel.clone());
let loaded = document
    .and_then(|document| persistence::load_layout(&dock_area, &document, window, cx).ok())
    .is_some();
if loaded {
    layout::reset_center_only(weak_dock_area, window, cx);
} else {
    layout::reset_default_layout(weak_dock_area, window, cx);
}
```

`DockArea::load` (`gpui-base-0.6.0/src/dock/dock_area.rs:820`) rebuilds every panel in the state
through `RegistryPanelBuilder`, which calls `PanelRegistry::build_panel`. `crates/terminal-view`
registers `panel_names::TERMINAL` as `TerminalPanel::open(PanelSpec::DefaultShell { … })`, and
`TerminalPanel::from_spec` → `spawn_local_view` → `spawn_local_session` →
`LocalSession::spawn`. So a saved center terminal spawns a shell during `load_layout`.

`reset_center_only` → `apply_center_reset` then calls `set_center`, whose `reconcile`
(`dock_area.rs:992`) drops every panel no longer in a live tree. The dropped `LocalSession`'s
`Drop` calls `pty_close()`, and `Conpty::drop` runs `ClosePseudoConsole`, terminating the host that
`cmd.exe` #1 is still attaching to → `STATUS_DLL_INIT_FAILED` (0xc0000142), raised by `csrss.exe`
as the dialog the owner screenshotted.

Implementation seam and shape, both public in `gpui-base`:

```rust
// PanelState { panel_name: String, children: Vec<PanelState>, info: PanelInfo }
// PanelInfo::stack(sizes: Vec<Pixels>, axis: Axis)
state.center = PanelState {
    panel_name: state.center.panel_name.clone(),   // keep the persisted container label
    children: Vec::new(),
    info: PanelInfo::stack(Vec::new(), Axis::Vertical),
};
```

That exact shape (`children: []`, `sizes: []`) was validated end to end before writing this
packet: hand-editing the real `docks.json` to it made 0.5.2 binary spawn exactly one
`cmd.exe` in four consecutive launches with no dialog, where the unmodified file gave one or two
with the dialog in three of eight. `load_layout` has exactly one production call site, so the
change cannot affect any other loader.

Version-mismatch prompt: unaffected. It reads `state.version`, and on "Yes" runs
`reset_default_layout`, which builds its own center either way.

## Plan

- [ ] Add the failing focused assertion first: register a counting builder for
  `panel_names::TERMINAL` in `layout_tests.rs` (an `Rc<Cell<usize>>` bumped per build, alongside
  the existing `NamedPanel`), load a document whose center holds two terminal panels, and assert
  the count is 0 after `load_layout` and 1 after `apply_center_reset`. Confirm it fails against
  current code with 2 / 3.
- [ ] Drop the center subtree in `load_layout` before `DockArea::load`.
- [ ] Update the `load_layout` doc comment and `docs/gui-layout.md` §Persistence.
- [ ] Re-run the focused test, `cargo test -p oneterm-workspace`, then `cargo test --workspace`.
- [ ] Ten-launch E2E on a release build with a saved center terminal; record per-launch child
  counts and dialog presence.
- [ ] `pwsh scripts/ci-local.ps1`.

## Decisions

None. The seam choice and the three rejected alternatives are recorded in the intake's
High-Level Design table; no consequential rationale here is new enough to need a `DEC` record.

## Verification Plan

Focused proof:

- The panel-build count assertion above, in
  `crates/workspace/src/layout/workspace/layout_tests.rs`. It must fail before the fix — a test
  that passes both ways would not pin this defect.
- `load_reset_center_and_save_round_trip` unchanged: still one center terminal after the reset,
  right dock still `(333., false, ssh_client_panel)`, `zoomed_panel` and `sftp_table_state` still
  preserved.
- `pre_migration_fixture_loads_and_saves_without_semantic_drift` unchanged and untouched.

Regression:

- `cargo test -p oneterm-workspace`, then `cargo test --workspace`.
- `pwsh scripts/ci-local.ps1` (fmt, clippy `-D warnings`, workspace tests, the Python policy
  checks).

E2E, on Windows with a real desktop session:

- Ten launches with a saved `docks.json` whose center holds `terminal`; per launch, count
  `cmd.exe` and `OpenConsole.exe` children of the app pid and enumerate visible `#32770` windows.
  Expect 1 / 1 / none every time. Sample at ≤ 100 ms — the process that dies is short-lived, so a
  coarse sampler under-counts.
- After each exit, assert no `cmd.exe /K chcp 65001 >nul` survives.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Not implemented. Nothing below is proof of the fix; it is the pre-implementation baseline the
acceptance is measured against, taken on
`dist/oneterm-x86_64-pc-windows-msvc/oneterm.exe` (FileVersion 0.5.2.0, built 2026-09-11).

| `docks.json` center | launches | new `cmd.exe` per launch | `#32770` dialog |
| --- | --- | --- | --- |
| holds `terminal` | 8 | 1–2 (two in 5) | 3 |
| `children: []` | 4 | 1 every launch | none |
| absent | 1 | 1 | none |

One affected launch observed directly: app pid 13360 with children `cmd.exe` 11696, `cmd.exe`
12688, `OpenConsole.exe` 2692; the dialog window belonged to `csrss.exe` pid 1464.

Known gaps at planning time:

- The 1–2 spread is a sampler artefact (20–60 ms polling misses a shell that dies between
  samples), not evidence that the second spawn is conditional. The E2E acceptance therefore
  asserts the steady state (one child, no dialog) rather than trying to catch the corpse.
- The baseline was taken on the packaged 0.5.2 binary; `HEAD` carries a newer bundled ConPTY pair
  (1.24.2607.10001) and the `crates/pty` transport, which change the race width but not the
  double spawn. The fix is verified on `HEAD`, so the ten-launch E2E is the honest comparison,
  not this table.
- No test can cover the race itself: it depends on Windows console-server attach timing. The
  focused test pins the cause (the extra build), and the E2E covers the symptom.

## Handoff

None — single-session packet. If the E2E cannot run (no interactive desktop), report the platform
proof as unrun rather than inferring it from the focused test, and record it here.
