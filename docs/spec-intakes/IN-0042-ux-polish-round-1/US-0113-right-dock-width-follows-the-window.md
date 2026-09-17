# Work: The right dock width follows the window

ID: US-0113
Intake: IN-0042
Created: 2026-09-17

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

- Change type: existing-contract change
- Risk lane: normal
- Spec Intake, when required: `IN-0042` — `docs/spec-intakes/IN-0042-ux-polish-round-1/IN-0042.md`

## Outcome

The terminal keeps the majority of the window at laptop widths. The right dock's width is
bounded by a share of the window rather than being a fixed pixel count, so narrowing the
window narrows the dock instead of squeezing the terminal.

## Findings and proposals covered

`P15` (effort M) — *"Make the right dock width proportional (or clamp it to ~35 % of the
window) and auto-collapse below a threshold, so the terminal keeps the majority at laptop
widths."*

Addresses `F9` (**high**), quoted from `research/ux-walkthrough-2026-09-16.md`:

> | F9 | narrow window (~900 px) | The right dock keeps its absolute ~490 px, so the terminal
> is left ~410 px — under half the window — while the dock shows a two-row session list and
> "No SFTP connection." | Density: the primary surface loses to two mostly-empty panels on any
> laptop-width window. | **high** | 50 |

## Scope

- [ ] In scope:
  - A pure clamp function in `crates/workspace`, `(requested px, window px) -> px`, plus the
    decision of what it returns when the window is too narrow for both surfaces.
  - Applying it at the four seams that already set the dock width:
    `crates/workspace/src/layout/workspace/layout.rs:41-45` (centre reset),
    `layout.rs:76` (default layout),
    `crates/workspace/src/layout/workspace/actions.rs:169-172` (mode swap), and on window
    resize.
  - `crates/state/src/dock_persistence.rs` — read to confirm no schema change is needed
    (see Context); changed only if that turns out to be wrong.
  - The `docs/gui-layout.md` §Dock composition sentence about preserving the width.
- [ ] Out of scope:
  - The dock's contents, its mode toggle (`BUG-0067`) and its collapse button.
  - Left or bottom docks — OneTerm has neither.
  - The SFTP table's column widths inside the dock; that is `US-0124`, which depends on the
    width this packet settles.
  - Remembering a different width per window size, or per monitor.

## Acceptance

- [ ] At a ~900 px window the terminal has more than half the width. Measured on the
      re-captured frame, not asserted.
- [ ] At a wide window (~1900 px) the user's own dragged width is honoured up to the ceiling;
      widening the window does not shrink the dock.
- [ ] Dragging the splitter still works and still feels absolute within the allowed range —
      the clamp bounds the width, it does not fight the drag.
- [ ] Resizing the window from wide to narrow narrows the dock; resizing back does not lose
      the user's preferred width.
- [ ] Below the threshold where both surfaces cannot be useful, the behaviour is the one the
      packet chose (auto-collapse or minimum width), it is stated in this packet, and it is
      recoverable in one click.
- [ ] A restart with a saved width above the ceiling applies the ceiling, and nothing in
      `docks.json` is corrupted or quarantined.
- [ ] The clamp is covered by a focused test at several window widths.
- [ ] `pwsh scripts/ci-local.ps1` ends with "ci-local: all checks passed".

## Documentation

### Owning Docs Reviewed

- `docs/gui-layout.md` §Dock composition — *"Selecting SSH Client or Agent mode builds the
  registered panel, replaces the right-dock layout, **preserves its width**, and opens it"*
  and *"Layout construction uses `DockLayout::{tabs,v_split}` and
  `DockArea::{set_center,set_dock,set_dock_size,set_dock_collapsible}`."* Both describe the
  width as something simply preserved. **Update required:** state the ceiling and what happens
  below the threshold.
- `docs/agents/persistence.md` — read before touching anything that reaches `docks.json`, to
  confirm whether a behaviour change that alters what is written needs a schema version bump.
  Record the answer in Reconciliation.
- `crates/state/src/dock_persistence.rs:30-45` — `DockDocument` flattens the kit's layout
  fields into `dock_fields: BTreeMap<String, Value>`; the width is one of them and OneTerm
  does not name it. **No change expected** — the clamp changes what is applied, not the
  document's shape.
- `crates/workspace/src/layout/workspace/mod.rs:33` — `DEFAULT_RIGHT_DOCK_WIDTH = px(480.)`,
  used only when no saved layout provides one. It stays as the first-launch value; the clamp
  applies to it like any other requested width.
- `docs/PROJECT.md` — read for standing invariants. **No change.**

### Documentation Action

Update required: `docs/gui-layout.md` §Dock composition gains the width rule — the ceiling as
a share of the window, the below-threshold behaviour, and the statement that the user's drag
is honoured within that range.

Reason: "preserves its width" stops being the whole truth once a ceiling exists, and the next
person reading that sentence would otherwise reintroduce the bug.

### Reconciliation

Before completion, list docs changed or confirm the recorded no-change reason remains valid.
Specifically: confirm whether `docks.json` needed a schema bump, and record the answer either
way.

## Context

- Where the width lives today, all four read from the same place and write straight back:
  - `layout.rs:41-45` — `reset_center_only` snapshots `dock_size(Right)` and restores it.
  - `layout.rs:76` — `reset_default_layout` writes `DEFAULT_RIGHT_DOCK_WIDTH`.
  - `actions.rs:169-172` — `switch_right_dock_mode` snapshots and restores around the swap.
  - The kit owns the dragged value and persists it inside `dock_fields`.
  So the width is an absolute pixel count that OneTerm reads and writes at four seams and
  never compares with the window. One clamp applied at those four seams is the whole change —
  there is no fifth place a width can enter from.
- Ladder: a clamp, not a proportional width. Storing a fraction would mean owning a new
  persisted field, migrating the existing absolute values, and re-deriving pixels on every
  layout pass — for an outcome ("the terminal keeps the majority at 900 px") that a ceiling
  already delivers. Start at the ceiling; only go proportional if the re-captured frame says
  the ceiling is not enough, and say so here if it does.
- The threshold behaviour is the one real decision. Auto-collapse is what `P15` suggests, and
  it is recoverable in one click through the title bar's mode toggle — but only once
  `BUG-0067` is fixed, because today a collapsed dock plus an unchanged `right_dock_mode` is
  exactly the dead end `F2` describes. If `BUG-0067` has not landed, either land it first or
  choose a minimum width instead of auto-collapse. Record which, and why.
- Window resize: there is an `observe_in` on the dock area
  (`crates/workspace/src/layout/workspace/mod.rs:150-163`) that already fires on layout
  changes and saves. Check whether it fires on window resize before adding a second observer —
  reusing it is smaller than adding one.
- `research/before/50-narrow-900.png` (the ~900 px frame, terminal ~410 px) and
  `51-large-1900.png` are the before pictures.

## Plan

- [ ] Measure the current behaviour first: window widths against dock widths, so the ceiling
      is chosen from numbers rather than from a guess.
- [ ] Write the clamp and its focused tests.
- [ ] Apply it at the four seams; check whether the existing dock observer covers resize.
- [ ] Decide and implement the below-threshold behaviour; confirm `BUG-0067`'s status first.
- [ ] Update `docs/gui-layout.md`; confirm the `docks.json` no-change reason.
- [ ] Re-capture the scenes.

## Decisions

None expected. The clamp shape is a tuning choice inside an accepted contract, recorded here
rather than in a `DEC`. If the packet ends up auto-collapsing the dock — a behaviour the user
did not ask for, triggered by a window resize — reconsider: that *is* a rule future work
inherits, and it would deserve a decision record.

## Verification Plan

1. **Focused:** unit tests over the clamp in `crates/workspace`: a requested width below the
   ceiling is returned unchanged; one above is capped; a very narrow window returns the
   threshold behaviour; the first-launch default is clamped like any other value. Pure
   arithmetic, no gpui.
2. **Unit:** `cargo test -p oneterm-workspace`, including the existing
   `layout_tests.rs` width assertions
   (`switch_right_dock_mode_swaps_panel_and_keeps_width`, and the two tests that set
   `px(333.)` / `px(464.)`) — these must be reviewed rather than merely made green, because
   their fixtures may now be below or above the clamp.
3. **Integration:** `cargo test --workspace`.
4. **Platform:** `pwsh scripts/ci-local.ps1`.
5. **E2E (GUI walk, re-capture these scenes):**
   - `50-narrow-900.png` — the ~900 px window. Record the measured terminal and dock widths in
     Evidence, not just the picture.
   - `51-large-1900.png` — the wide window; the dragged width is honoured.
   - `01-first-launch.png` — the default window at first launch.
   - `23-dock-none.png` — regression: None still closes the dock.
   Plus a sequence the walkthrough did not take: drag the splitter wide, narrow the window,
   widen it again, and confirm the preferred width comes back. Capture the end state.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Risks

- **Fighting the user's drag.** A clamp that re-applies on every layout pass can snap the
  splitter back while the user is dragging it. Apply the clamp where the width is *set* by
  OneTerm, not on every frame, and walk a real drag before claiming this works.
- **Persisting the clamped value and losing the preference.** If the clamped width is written
  back to `docks.json`, a user who narrows the window once loses their wide dock forever.
  Decide explicitly whether the stored value is the user's preference or the applied width,
  and state it here. Preferring to store the preference and clamp on apply is the safer half.
- **Auto-collapse plus `BUG-0067`.** Collapsing the dock without updating
  `UiConfig.right_dock_mode` recreates the `F2` dead end at a new trigger. Covered in Context;
  do not implement auto-collapse before checking.
- **`docks.json` compatibility.** The width lives in the kit's flattened fields. Any change
  that makes OneTerm name that field starts a migration conversation this packet does not
  want. If the clamp cannot be applied without naming it, stop and re-scope.
- **The existing layout tests carry hard-coded widths.** Making them pass by editing the
  numbers would hide a real behaviour change. Review each, and say in Evidence which were
  adjusted and why.

## Evidence and Gaps

After implementation, record commands, results, and anything skipped, unavailable, partial, or failing.

## Handoff

Use only across actors or sessions: current state, next owner/action, and blockers.
