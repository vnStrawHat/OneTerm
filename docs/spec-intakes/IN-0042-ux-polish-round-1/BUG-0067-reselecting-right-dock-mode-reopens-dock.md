# Work: Reselecting the current right-dock mode reopens a collapsed dock

ID: BUG-0067
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

- Change type: bug
- Risk lane: normal
- Spec Intake, when required: `IN-0042` — `docs/spec-intakes/IN-0042-ux-polish-round-1/IN-0042.md`

## Outcome

Closing the right dock with the tab bar's dock button and then clicking the title bar's
already-selected mode reopens the dock. The title bar and the dock never disagree about
whether the dock is open, and there is no state the user can reach that needs a detour
through another mode to escape.

## Findings and proposals covered

`P2` — *"Drop the `ix != current_ix` guard in the mode toggle's `on_click` and make
`SetRightDockMode` re-open the dock for the already-selected mode; or have the dock-collapse
button write `RightDockMode::None` to `UiConfig`."*

Addresses `F2` (**high**), quoted from `research/ux-walkthrough-2026-09-16.md`:

> | F2 | title bar <-> tab bar | The tab bar's 4th trailing button closes the right dock, but
> the title-bar segmented control still shows **SSH Client** selected. Clicking **SSH Client**
> then does nothing — the dock stays closed. Recovery requires clicking Agent or None first,
> then SSH Client. | State consistency / dead end. Root cause in code: `mode_toggle_group`
> reads the persisted `UiConfig.right_dock_mode` and its `on_click` only dispatches
> `when ix != current_ix` (`crates/workspace/src/layout/title_bar.rs:141-154`); the
> dock-collapse button never updates that config. | **high** | 21, 24a, 24b |

## Scope

- [ ] In scope:
  - `crates/workspace/src/layout/title_bar.rs:141-154` — the `on_click` guard that swallows a
    click on the already-selected toggle.
  - Whatever the chosen fix needs on the action side
    (`crates/workspace/src/layout/workspace/actions.rs`) and at the dock-collapse button.
  - A focused regression test for the state machine.
- [ ] Out of scope:
  - The set of modes, their order, their labels and their widths.
  - The dock width (`US-0113`) and what the dock contains.
  - The `RightDockMode` persistence format in `ui_config.json`.

## Acceptance

- [ ] Close the right dock with the tab bar's dock button, then click the title bar's
      already-highlighted mode: the dock reopens with the same content and the same width.
- [ ] After closing the dock with that button, the title bar no longer claims a mode is
      showing when it is not — either the toggle reflects the closed dock, or clicking it
      reopens the dock, and the packet says which rule it implements.
- [ ] Clicking a mode that is already selected while the dock is **open** does not close it,
      toggle it, or rebuild its panel. The only effect of an explicit mode click is "show me
      this", never "hide this".
- [ ] Clicking **None** while None is selected leaves the dock closed.
- [ ] The persisted `right_dock_mode` after each of the above is what the title bar shows on
      the next launch.
- [ ] `pwsh scripts/ci-local.ps1` ends with "ci-local: all checks passed".

## Documentation

### Owning Docs Reviewed

- `docs/gui-layout.md` §Dock composition — *"`RightDockMode::None` closes the existing right
  dock without replacing its content. Selecting SSH Client or Agent mode builds the registered
  panel, replaces the right-dock layout, preserves its width, and opens it."* This describes
  the intended behaviour correctly and is what the bug violates; whether it needs a sentence
  about the collapse button depends on which fix is chosen. **Decide during implementation**
  and record it here.
- `crates/workspace/src/layout/workspace/actions.rs:113-139` — `on_action_set_right_dock_mode`
  **already** handles the same-mode case: *"Same mode clicked — still apply the requested
  visibility: hide for None, force-open for SSH Client / Agent (the click is explicit)."* The
  action is correct; the click never reaches it. That is the root cause, and it narrows the
  fix considerably. **No change needed to the action's contract.**
- `crates/workspace/src/layout/workspace/layout_tests.rs:132-160` —
  `switch_right_dock_mode_swaps_panel_and_keeps_width` is the existing regression home for
  this behaviour, and where the new test belongs.
- `docs/PROJECT.md` — read for standing invariants. **No change.**

### Documentation Action

No contract change expected: `docs/gui-layout.md` already describes the behaviour this packet
restores. One sentence is added only if the fix changes what the **collapse button** means
(for example, if it starts writing `RightDockMode::None`), because that is behaviour the
document does not currently describe.

Reason: the bug is that code disagrees with the accepted contract, not that the contract is
wrong.

### Reconciliation

Before completion, list docs changed or confirm the recorded no-change reason remains valid.

## Context

- Root cause, confirmed by reading both files:
  - `title_bar.rs:141-154` dispatches only `if checked && ix != current_ix`. A click on the
    already-selected toggle dispatches nothing at all.
  - `current_ix` comes from the persisted `UiConfig.right_dock_mode`, which the tab bar's
    dock-collapse button never touches. So after that button runs, the config still says
    `SshClient`, the toggle still shows `SshClient` selected, and the guard makes the click a
    no-op. The user is in a state only reachable by going through another mode.
- Two fixes, and they are not equivalent:
  - **(a) Drop the guard.** The action handler already does the right thing for both cases,
    so this is a one-line deletion plus the loop simplification around it. It leaves the
    collapse button and the persisted mode out of sync until the next mode click, but nothing
    reads that disagreement except the guard being removed.
  - **(b) Make the collapse button write `RightDockMode::None`.** Keeps the config honest, so
    the toggle visibly moves to "None" when the dock closes and the two controls always agree.
    It is a larger change (the button lives in the tab bar's trailing group, which does not
    own `UiConfig` today) and it changes what the button *means* — a mode change rather than
    a visibility toggle — which is why it would need the `gui-layout.md` sentence.
  - Ladder: (a) is the smaller diff and it fixes the dead end, which is the reported symptom
    *and* the root cause of the dead end. But it does not fix the second half of `F2` — "the
    title-bar segmented control still shows SSH Client selected" while the dock is closed,
    which is a lie the user can see. Implement (a); then decide whether the residual
    inconsistency is acceptable and, if it is not, do (b) in this packet rather than a
    follow-up. Record the call here.
- `ToggleGroup` is multi-select by nature, which is why the handler keys off *which* toggle
  was clicked (`title_bar.rs:105-108`). Removing the `ix != current_ix` half of the condition
  must not break the single-select behaviour the `checked` half provides; the focused test
  covers that.

## Plan

- [ ] Write the regression test first, against the current code, and watch it fail on the
      "collapse, then click the selected mode" sequence.
- [ ] Apply fix (a); re-run.
- [ ] Decide on the residual toggle-shows-a-lie half and record the decision here.
- [ ] Re-capture the three scenes.

## Decisions

None expected. If the packet ends up taking fix (b) — making the collapse button a mode
change — that redefines a control's meaning and is worth a line in `docs/gui-layout.md`, but
it is not a constraint future work must inherit, so it stays here rather than becoming a
`DEC`.

## Verification Plan

1. **Focused:** a new test in
   `crates/workspace/src/layout/workspace/layout_tests.rs` driving the exact sequence — open
   with SSH Client, collapse the dock, dispatch `SetRightDockMode(SshClient)`, assert the dock
   is open and the width survived. It must fail against the current `title_bar.rs`. The test
   exercises the action path; the click-to-dispatch half of the guard is not reachable from a
   headless test, so the guard removal itself is proven by the GUI walk — say so in Evidence
   rather than implying the unit test covers it.
2. **Unit:** `cargo test -p oneterm-workspace`.
3. **Integration:** `cargo test --workspace`.
4. **Platform:** `pwsh scripts/ci-local.ps1`.
5. **E2E (GUI walk, re-capture these scenes):**
   - `21-tabbar-4th-button.png` — the dock button before the click.
   - `24a-dock-collapsed-via-button.png` — the dock closed by that button, with the title bar
     visible in the same frame so the toggle state is readable.
   - `24b-after-clicking-sshclient-again.png` — the scene that proved the bug. The after frame
     must show the dock **open**.
   - `23-dock-none.png` — regression: None still closes the dock and stays closed when
     clicked again.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Risks

- **Turning the mode toggle into a hide toggle.** If the guard is removed carelessly, a
  second click on SSH Client could start closing the dock. The action handler is explicit that
  an explicit click means "show" (`actions.rs:124-127`), and the acceptance pins it, but this
  is the regression to watch.
- **Rebuilding the panel on every click.** `switch_right_dock_mode` rebuilds the right dock's
  panel. If a same-mode click routes through it instead of through the visibility-only branch,
  the user loses the panel's state (scroll position, SFTP connection) on a click that should
  do nothing. The handler already branches before that; keep it that way.
- **Width loss.** Both the rebuild and the reopen path snapshot and restore the dock width
  (`actions.rs:169-172`). The focused test asserts the width survives, because losing it here
  would be a silent regression against `US-0113`.

## Evidence and Gaps

After implementation, record commands, results, and anything skipped, unavailable, partial, or failing.

## Handoff

Use only across actors or sessions: current state, next owner/action, and blockers.
