# Work: Startup spawns exactly one local shell

ID: BUG-0054
Intake: IN-0031
Created: 2026-09-14

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

- [x] Loading a document whose center holds `terminal` panels builds **zero** of them; the
  subsequent `reset_center_only` builds exactly one. Asserted by build count, not by tree shape.
  `load_layout_builds_no_center_panel_and_the_reset_builds_one` — 2 / 3 before the fix, 0 / 1
  after (numbers in Evidence).
- [x] The right dock keeps its saved panel name, width and open state across
  load → reset → save, and `zoomed_panel` still restores by name.
  `load_reset_center_and_save_round_trip` passes unchanged.
- [x] `pre_migration_fixture_loads_and_saves_without_semantic_drift` still passes untouched. It
  drives `DockArea::load` directly and asserts the full center round-trips, which is the check
  that this fix did not reach into the dock crate's behaviour.
- [x] Ten consecutive launches of the built binary with a saved `docks.json` holding a center
  terminal: exactly one `cmd.exe` child per launch, no `#32770` "Application Error" dialog, and
  no `cmd.exe` surviving application exit. 10 / 10 clean; the same ten launches against a
  `HEAD` build without the fix gave 2 `cmd.exe`, 2 `OpenConsole.exe`, the dialog in 9 runs and
  2 orphans in 9 runs (tables in Evidence).
- [x] `pwsh scripts/ci-local.ps1` exits 0. 60 sections, 1932 passed / 0 failed / 13 ignored.

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

Docs changed:

- `docs/gui-layout.md` §Persistence — one sentence added after the existing "Startup intentionally
  resets the center" claim: `load_layout` drops the document's center subtree (container name
  kept, no children) before `DockArea::load`, because building a panel starts its session and a
  `terminal` panel spawns a local shell that the reset would discard.
- `crates/workspace/src/layout/workspace/persistence.rs` — `load_layout` doc comment says the same
  at the seam, and names `BUG-0054` and the `0xc0000142` consequence.

No change needed, and the recorded reason still holds:

- `crates/workspace/src/layout/workspace/mod.rs` — the `OneTermWorkspace::new` doc comment already
  states "load the old layout (keep right dock + settings), but reset the center". The
  implementation now matches that sentence instead of contradicting it, so the comment was left
  alone; the mechanism sentence went to the function that carries it.
- `docs/gui-layout.md` §Panel registration and presentation, `docs/terminal-backend.md` §6.2,
  `docs/agents/persistence.md` — reviewed, unchanged. No schema, panel-registration or session
  lifecycle behaviour moved.

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

- [x] Add the failing focused assertion first: register a counting builder for
  `panel_names::TERMINAL` in `layout_tests.rs` (an `Rc<Cell<usize>>` bumped per build, alongside
  the existing `NamedPanel`), load a document whose center holds two terminal panels, and assert
  the count is 0 after `load_layout` and 1 after `apply_center_reset`. Confirm it fails against
  current code with 2 / 3.
- [x] Drop the center subtree in `load_layout` before `DockArea::load`.
- [x] Update the `load_layout` doc comment and `docs/gui-layout.md` §Persistence.
- [x] Re-run the focused test, `cargo test -p oneterm-workspace`, then `cargo test --workspace`.
- [x] Ten-launch E2E on a built binary with a saved center terminal; record per-launch child
  counts and dialog presence. Run twice: with the fix and, as a `HEAD` baseline, without it.
- [x] `pwsh scripts/ci-local.ps1`.

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
- [x] Unit proof
- [x] Integration proof
- [x] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Implemented on `5b42247` ("fix(workspace): do not build the persisted center at startup"), on top
of `2f47366` on `feat/vt-engine`.

### Focused proof

`crates/workspace/src/layout/workspace/layout_tests.rs::load_layout_builds_no_center_panel_and_the_reset_builds_one`
re-registers `panel_names::TERMINAL` with a builder that bumps an `Rc<Cell<usize>>`, loads a
document whose center holds two terminals, and counts.

| | after `load_layout` | after `apply_center_reset` |
| --- | --- | --- |
| before the fix | **2** (assertion failed, expected 0) | **3** (assertion failed, expected 1) |
| after the fix | 0 | 1 |

Both pre-fix numbers were read from real failures, the second by temporarily relaxing the first
assertion to `2` and re-running, then restoring it. `load_reset_center_and_save_round_trip` and
`pre_migration_fixture_loads_and_saves_without_semantic_drift` pass unchanged and untouched.
`cargo test -p oneterm-workspace`: 12 passed / 0 failed.

### Gate

`pwsh scripts/ci-local.ps1` from the worktree, exit 0, all ten steps. Test totals across its two
cargo-test steps: **60 sections, 1932 passed / 0 failed / 13 ignored**.

### E2E, ten launches each

Windows 11, interactive desktop. Binary: `target/fast-dev/oneterm.exe` built in this worktree
(debug profile, so `config_dir()` is `target/` next to it — the owner's `~/.OneTerm/docks.json`
was read once and never written). Before every launch the same fixture is copied to
`target/docks.json`: a copy of the owner's real file — `version` 3, center `StackPanel` →
`TabPanel` → two `terminal` leaves, right dock `agent_panel` 464 px closed. Each launch is
`Start-Process -PassThru`; only that pid's process subtree is sampled (`Win32_Process`
`ParentProcessId`, BFS) every 100 ms for 3 s, and only that pid is closed
(`CloseMainWindow`, then `Stop-Process -Id <mine>`). `#32770` windows are enumerated for
observation only and attributed by `GetWindowThreadProcessId`.

With the fix (`5b42247`), runs 1-10 identical:

| run | app pid | distinct `cmd.exe` | max live `cmd.exe` | `OpenConsole.exe` | `#32770` | survivors after exit |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | 3256 | 1 | 1 | 1 | none | 0 |
| 2 | 20332 | 1 | 1 | 1 | none | 0 |
| 3 | 23424 | 1 | 1 | 1 | none | 0 |
| 4 | 18132 | 1 | 1 | 1 | none | 0 |
| 5 | 10196 | 1 | 1 | 1 | none | 0 |
| 6 | 9384 | 1 | 1 | 1 | none | 0 |
| 7 | 7088 | 1 | 1 | 1 | none | 0 |
| 8 | 2836 | 1 | 1 | 1 | none | 0 |
| 9 | 23848 | 1 | 1 | 1 | none | 0 |
| 10 | 5476 | 1 | 1 | 1 | none | 0 |

`HEAD` baseline — the same ten launches against the same build with only the four-line center
drop removed from `load_layout` (edited in place, rebuilt, then restored from `5b42247`):

| run | app pid | distinct `cmd.exe` | max live `cmd.exe` | `OpenConsole.exe` | `#32770` | survivors after exit |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | 23104 | 2 | 2 | 2 | none | 2 |
| 2 | 21808 | 2 | 2 | 2 | `cmd.exe - Application Error` | 2 |
| 3 | 17656 | 2 | 2 | 2 | `cmd.exe - Application Error` | 2 |
| 4 | 20576 | 2 | 2 | 2 | `cmd.exe - Application Error` | 2 |
| 5 | 23740 | 2 | 2 | 2 | `cmd.exe - Application Error` | 2 |
| 6 | 17652 | 2 | 2 | 2 | `cmd.exe - Application Error` | 2 |
| 7 | 23296 | 2 | 2 | 2 | `cmd.exe - Application Error` | 2 |
| 8 | 12148 | 2 | 2 | 2 | `cmd.exe - Application Error` | 2 |
| 9 | 11944 | 2 | 2 | 2 | `cmd.exe - Application Error` | 2 |
| 10 | 21512 | 2 | 2 | 2 | `cmd.exe - Application Error` | 0 |

Every baseline dialog was owned by `csrss.exe` pid 1464, exactly as the intake recorded. Screenshot
of one: [`evidence/BUG-0054-prefix-application-error-dialog.png`](evidence/BUG-0054-prefix-application-error-dialog.png)
— "The application was unable to start correctly (0xc0000142). Click OK to close the application."
Baseline survivors were stopped by pid, each one first observed as a descendant of this script's
own launch; no process was ever matched by name or window title. Pre-existing `oneterm.exe`
processes were enumerated before each launch and left alone.

This upgrades the intake's planning baseline in two ways: the defect reproduces on `HEAD`
**deterministically** (10/10 double spawns, 9/10 dialogs), not intermittently as the 0.5.2
sampling suggested, and the two-shell count is steady rather than a 1-2 spread — the earlier
spread was the coarse sampler, as the packet predicted.

### Gaps

- `BUG-0055` is untouched and still open: in the baseline runs both shells survived the app's
  exit in 9 of 10 runs. With the fix there is no discarded session to leak, so the symptom does
  not appear, but the orphan mechanism itself is not fixed here.
- No test covers the race: it depends on Windows console-server attach timing. The focused test
  pins the cause (the extra build); the E2E covers the symptom.
- The E2E measures a `fast-dev` (debug) build, not the packaged release. The code path is the
  same and the defect reproduced on it; a packaged-binary run was not repeated.
- Non-Windows platforms take the same code path and waste the same shell, but only Windows was
  measured.

## Handoff

None — packet complete. `BUG-0055` continues independently.
