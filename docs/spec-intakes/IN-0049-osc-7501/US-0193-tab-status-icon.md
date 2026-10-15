# Work: Tab status icon

ID: US-0193
Intake: IN-0049
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
- Change type: feature (UI)
- Risk lane: high-risk (shares the intake's lane); the change itself is display-only.
- Spec Intake, when required: IN-0049

## Outcome

A single-Space Terminal Tab shows a rotating loader for working, a pulsing bell for blocked, a check for done and a cross for error, from its terminal's 7501 summary; done / error clear when acknowledged; a split tab shows none.

## Scope

- [x] In scope: `low-level-design/tab-status-icon.md`; `docs/gui-layout.md` tab-strip description.
- [x] Out of scope: notifications or sounds on a state change; icons on split tabs; any other surface.

## Acceptance

- [x] Every edge case in the tab-icon LLD has a test or a recorded manual check.
- [ ] Screenshot of each state in a real window. Partial: `working` captured; the workstation locked before the others (see Gaps).
- [x] Reduced motion stops both animations.
- [x] Full `scripts/ci-local.sh` passes.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0049-osc-7501/high-level-design.md` and its `low-level-design/` — the design.
- `docs/decisions/DEC-0022-osc-7501-replaces-osc-20308-and-the.md` — the ruling.
- `docs/gui-layout.md` — the tab strip's contents.

### Documentation Action

Choose one and explain it before implementation:

- Update required: `docs/gui-layout.md` gains the status icon in the tab-strip description.

Reason: a new element in the tab strip.

### Reconciliation

Before completion, list docs changed or confirm the recorded no-change reason remains valid.

## Context

Add only relevant code, decisions, dependencies, and invariants not already clear from the owning docs.

## Plan

- [x] View state + acknowledgement + event.
- [x] Panel `tab_status` + icon element.
- [x] Tests, docs. Screenshots partial.

## Decisions

Link consequential decisions; do not repeat their rationale here.

## Verification Plan

`cargo test -p oneterm-terminal-view`; manual `printf` reports per state with screenshots; Claude Code run; full gate.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

- View: `program_status` (idle stored as none), `StatusChanged` on change, exit drops `Working` / `Blocked`, `acknowledge_program_status` from `clear_bell` (keys, mouse reports) and the IME commit; the panel acknowledges every terminal leaf on `set_active(true)`. Panel: `tab_status` (single Space only); `tab_status_icon` after the chips and the recording dot, theme colours, per-panel animation ids, tooltip.
- Tests (`oneterm-terminal-view`, 402 -> 407): stored-without-idle + announced once per change; exit rule for all four states; typing (IME text and a key) acknowledges a result but not `Blocked`; the CORR-02 regression re-added as `exited_behind_an_output_batch_still_ends_the_status`; split tab shows none while the terminal keeps its status, and selecting the tab acknowledges; `the_live_status_icons_animate_unless_motion_is_reduced` (working / blocked ask for the next frame, done / error do not, nothing does under reduced motion).
- Theme: the bell draws `warning` on the tab surfaces, so `check-theme-contrast.py` now measures `warning.background` on `tab_bar.background`, `tab.background`, `tab.active.background`. Three light variants were under 4.5:1 and took a lightness-only correction of their own hue: Everforest Light `#8b6927` -> `#876626` (4.47 -> 4.68), macOS Classic Light `#816e00` -> `#796700` (4.15 -> 4.61), Solarized Light `#706a54` -> `#6d6752` (4.42 -> 4.62). Darker on a light surface, so every other surface the token is measured on only gains.
- Walk (debug build from `target/walk-0193`, its own config dir): `emit.ps1 state=working` -> loader icon in the tab (`us0193-working-icon.png`); after the program ended the cmd prompt cleared it. The icon was static on screen: this machine has Windows animation effects off (`SPI_GETCLIENTAREAANIMATION` = false), which gpui reads as reduced motion — the designed behaviour; the motion itself is proven by the frame-request test above.
- Docs: `gui-layout.md` (new "Program status icon" section), README feature section, AGENTS.md project introduction.
- `CARGO_BUILD_JOBS=6 bash scripts/ci-local.sh --full` on Windows, 2026-10-15, after all three packets: all 32 checks passed, 141 test suites / 5045 passed / 0 failed; `cargo deny`: advisories, bans, licenses ok.
- Gaps: screenshots of `blocked`, `done`, `error`, a split tab and a background tab were not taken: the workstation locked mid-walk (`SendKeys` access denied, `LogonUI` running) and the walk was stopped rather than unlock it. Each of those behaviours has a unit test. Claude Code end to end is the owner's run.

## Handoff

Use only across actors or sessions: current state, next owner/action, and blockers.
