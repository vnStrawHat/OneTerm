# Work: Keep empty Space numbers stable after duplicate fill

ID: BUG-0009
Intake: IN-0006
Created: 2026-08-12

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [ ] Planned
- [ ] In progress
- [ ] Implemented
- [x] Changed
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: bug / clarified existing contract
- Risk lane: tiny
- Spec Intake: `IN-0006`

## Outcome

An empty Space displays its raw `SpaceId` for the lifetime of that Space. If `Space #1` is filled by Duplicate Session, an existing `Space #2` remains labeled `Space #2` and the menu continues to show `Into Space #2`; remaining empty Spaces are not compacted or renumbered.

## Scope

- [x] In scope: SpaceId-derived labels, the `SpaceId(0)` initial-terminal invariant, matching placeholder/menu/Agent Panel labels, a separate visual-order field for Agent Panel sorting, simplified destination APIs, exhaustive duplicate dispatch, and regression tests.
- [x] Out of scope: persistence across application restart, renumber command, displaying numbers on occupied Spaces, or changing stable `SpaceId` identity.

## Acceptance

- [x] Filling `Space #1` leaves another empty `Space #2` numbered `#2`.
- [x] Closing or filling any Space does not renumber surviving Spaces.
- [x] A newly created empty Space receives the next unused `SpaceId` for that tab; IDs are not reused during the tab lifetime.
- [x] Placeholder and `Into Space #N` menu labels use the destination's raw `SpaceId` value.
- [x] Space actions still target `SpaceId`, not a separately allocated display number.
- [x] Agent Panel displays the same raw `SpaceId` as terminal placeholders/menu items while sorting cards by a separate depth-first order field.
- [x] Empty destination APIs return only `SpaceId`; rendering derives the label directly without tuple duplication, linear lookup, or a fallback number.
- [x] Duplicate destination dispatch exhaustively matches all enum variants without a production panic path.
- [x] Focused regression tests and mandatory workspace format, Clippy, and build gates pass after review remediation; the full workspace test command was attempted but hit a Windows linker resource failure, with affected crates then passing individually.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0006-duplicate-sessions-into-empty-spaces/IN-0006.md` — currently states empty Spaces are derived/renumbered in visual order.
- `docs/spec-intakes/IN-0006-duplicate-sessions-into-empty-spaces/high-level-design.md` — currently treats numbers as recomputed display data.
- `docs/terminal-split/01-architecture.md` — Space leaf identity and empty-destination query.
- `docs/terminal-split/04-context-menu.md` — `Into Space #N` labels and stable `SpaceId` targeting.
- `docs/terminal-split/05-rendering-theme.md` — placeholder number presentation.
- `docs/agent-panel-display.md` — Agent Panel Space labels and within-tab ordering.

### Documentation Action

Update required: revise IN-0006, its HLD, and terminal-split architecture/menu/rendering contracts from compact derived numbering to stable per-Space numbering for the tab lifetime.

Reason: the clarified user expectation changes the prior accepted numbering lifecycle; leaving the owning docs unchanged would explicitly preserve the reported bug.

### Reconciliation

Updated IN-0006, its HLD, and `docs/terminal-split/01-architecture.md`, `04-context-menu.md`, and `05-rendering-theme.md` to specify SpaceId-derived, monotonic, non-compacting labels. Removed the intermediate display-number state after the user selected the simpler SpaceId design.

## Context

The original implementation enumerated only currently empty leaves on every render/menu open. Filling the first empty leaf removed it from that list, so the second empty leaf shifted from index 2 to index 1. The clarified design uses the already stable, monotonic `SpaceId` directly: ID 0 belongs to the initial terminal, and empty destinations start at ID 1.

## Plan

- [x] Use the existing monotonic, non-reused `SpaceId` as the visible number and reserve ID 0 for the initial terminal.
- [x] Make the spawn-failure recovery empty placeholder start at `SpaceId(1)` so no user-visible empty Space is `#0`.
- [x] Initially returned `(SpaceId, SpaceId::display_number())`; code-style review showed the second tuple value duplicated identity and should be removed.
- [x] Return only `SpaceId` for empty destinations and render labels directly from the leaf ID.
- [x] Give Agent Panel separate `space_number` (raw `SpaceId`) and `space_order` (depth-first sorting) fields.
- [x] Replace the duplicate placement `unreachable!()` branch with exhaustive enum dispatch.
- [x] Add/update regression coverage, reconcile owning docs, and run focused plus mandatory verification.

## Decisions

No separate decision record is needed; the user clarification is bounded to IN-0006 and is captured in its owning contract/HLD.

## Verification Plan

- Focused SpaceTree tests: two empty numbered leaves; fill `#1`; assert surviving empty destination remains `#2`; close/fill does not compact; new empty receives the next unused number.
- Agent state/UI tests: preserve depth-first card ordering while displaying raw SpaceId labels.
- Source review: no derived-number destination tuple, render lookup/fallback, or duplicate-dispatch `unreachable!()` remains.
- Feature regression: `cargo test -p oneterm-terminal-view`; affected state and Agent UI tests.
- Platform: `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo build --workspace`.
- Manual GUI: duplicate into `Space #1` with `Space #2` present and confirm `#2` remains, if interactive GUI execution is available.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

- Focused `filling_space_one_does_not_renumber_space_two`: passed.
- Focused SpaceTree stable-number allocation/close test: passed.
- `cargo test -p oneterm-terminal-view`: 103 passed after review remediation.
- `cargo test -p oneterm-state`: 15 passed, 1 ignored.
- `cargo test -p oneterm-agent-ui`: 3 passed.
- Affected multi-crate test run before the full gate: 120 passed, 1 ignored.
- `cargo test --workspace`: attempted; Windows `link.exe` failed with status `0xc000012d` while linking test binaries (resource exhaustion), not a Rust test or compilation diagnostic. Affected crates passed separately as listed above.
- `cargo fmt --all -- --check`: passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed with no issues.
- `cargo build --workspace`: passed.
- `git diff --check`: passed.
- `srcwalk review --scope crates`: completed; no remaining reviewed tuple/fallback/duplicate-dispatch patterns were found.
- Manual GUI duplicate flow was not run in this non-interactive session; the exact fill/renumber sequence is covered by the panel regression test.

## Handoff

Implementation, contract reconciliation, and automated verification are complete. Recommended smoke test: create `#1` and `#2`, duplicate into `#1`, and confirm the remaining placeholder/menu entry stays `#2`.
