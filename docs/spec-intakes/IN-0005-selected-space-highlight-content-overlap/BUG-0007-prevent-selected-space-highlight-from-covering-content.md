# Work: Prevent selected Space highlight from covering content

ID: BUG-0007
Intake: IN-0005
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

- Change type: bug
- Risk lane: tiny
- Spec Intake: `IN-0005`

## Outcome

Selecting a split Space shows a complete four-sided highlight without painting highlight chrome over terminal or placeholder content.

## Scope

- [x] In scope: active/inactive Space frame rendering, interaction with the resizable handle, and focused regression coverage.
- [x] Out of scope: resize behavior, focus selection, split-tree behavior, and terminal cell layout.

## Acceptance

- [x] The selected Space shows `table_active_border` on all four sides, including the edge shared with a resize handle.
- [x] Frame space is reserved in layout, so no highlight is painted over terminal or placeholder content.
- [x] Inactive Spaces retain the normal theme border and identical content insets.
- [x] No absolute highlight element is painted after the Space content.
- [x] A single unsplit terminal still uses the no-chrome fast path.

## Documentation

### Owning Docs Reviewed

- `docs/terminal-split/00-overview.md` — requires a distinct active Space border.
- `docs/terminal-split/02-split-and-close.md` — preserves active-Space tracking and the single-terminal no-border path.
- `docs/terminal-split/05-rendering-theme.md` — owns border and active-highlight rendering.
- `docs/PROJECT.md` — brownfield project context and verification record are currently skeletal.

### Documentation Action

- Update required: reconcile `docs/terminal-split/05-rendering-theme.md` with the implemented one-pixel wrapper frame and explicitly prohibit a content-overlaid highlight.

Reason: the owning design still describes an obsolete four-pixel proposal, while the renderer uses a one-pixel separator plus an inner overlay that causes the reported defect.

### Reconciliation

Updated `docs/terminal-split/00-overview.md`, `05-rendering-theme.md`, and `07-roadmap-risks.md` to describe the implemented one-pixel frame and prohibit a content-overlaid active ring.

## Context

Before this fix, `render_leaf` painted a neutral wrapper border, then appended an absolute inset border after the content. The first supplied screenshot shows that inner ring crossing the first terminal text row. GPUI paints later siblings above earlier siblings, so the ring covered glyphs at the pane edge.

The first correction reused only the wrapper border for selection. The second supplied screenshot (`Screenshot 2026-08-12 113125.png`) shows the selected right Space missing its left highlight edge. `gpui-component` positions the second panel's one-pixel resize handle over that shared outer edge and paints the neutral `theme.border` color, obscuring the active wrapper border. The corrected frame must therefore reserve an inner one-pixel gutter in normal layout; the resize handle may cover the outer pixel while the active gutter remains visible without covering content.

## Plan

- [x] Extract the active/inactive frame color choice into a small private rendering helper.
- [x] Render the frame as an outer border plus a layout-reserved inner gutter so a resize handle cannot erase the selected edge.
- [x] Keep the overlay child removed.
- [x] Extend regression coverage and update the owning rendering design.

## Decisions

No durable decision record is needed; the accepted design already assigns selected state to the Space border.

## Verification Plan

- Focused: run the new Space frame-color regression test.
- Unit: run `cargo test -p oneterm-terminal-view`.
- Regression: run `cargo test --workspace`.
- Quality gates: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo build --workspace`.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

- `cargo test -p oneterm-terminal-view selected_space_uses_active_gutter_color -- --nocapture` — 1 passed.
- `cargo test -p oneterm-terminal-view` — 100 passed.
- `cargo test --workspace` — 586 passed, 2 ignored.
- `cargo fmt --all -- --check` — passed.
- `cargo clippy --workspace --all-targets -- -D warnings` — passed with no issues.
- `cargo build --workspace` — passed.
- Gap: no automated screenshot/pixel assertion exists for GPUI Space rendering; the regression test verifies token selection, while structural review confirms the gutter is reserved by padding and the overlay child remains removed.

## Handoff

No handoff or blocker.
