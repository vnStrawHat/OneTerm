# Work: Prevent terminal tab title descender clipping

ID: BUG-006
Created: 2026-08-11

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Keep operational status in `harness.db`.

## Classification

- Change type: bug
- Risk lane: tiny
- Spec Intake, when required: not required for a localized rendering correction

## Outcome

Terminal tab labels render lowercase descenders (including `p`, `g`, `q`, and `y`) at their full height instead of clipping their lower pixels.

## Scope

- In scope: terminal tab-title label overflow styling and a focused style regression test.
- Out of scope: tab-strip dimensions, fonts, title resolution, drag behavior, and gpui-component fork changes.

## Acceptance

- The terminal tab-title label has no label-level content mask, allowing glyph descenders to paint outside the line box.
- Long labels still shrink to available width and use ellipsis within the tab container.
- Existing rename, drag, and close interactions remain unchanged.
- A focused regression test protects the no-label-mask invariant.

## Documentation

### Owning Docs Reviewed

- `docs/terminal-split/03-drag-drop.md` — terminal tabs render a custom title element used as the drag source.
- `docs/terminal-split/06-integration.md` — `TerminalPanel::title()` owns the active terminal tab label while the shared dock/tab structure remains unchanged.
- `docs/PROJECT.md` — GPUI/gpui-component reference-first and verification invariants.

### Documentation Action

- No contract change: the accepted terminal-tab behavior is already correct; this change only corrects text clipping caused by implementation styling.

Reason: no user-visible workflow, architecture, persistence, or public contract changes.

### Reconciliation

The no-contract-change reason remains valid. The corrected implementation only removes the nested label's content mask and adds a local flex-width constraint; the documented tab workflow and shared dock structure are unchanged.

## Context

`TerminalPanel::title()` originally applied `overflow_hidden()` to the nested label that owns the ellipsis. The first correction changed this to `overflow_x_hidden()`, but the supplied screenshot still shows a flat clipping edge across the bottom of all tab-title glyphs. GPUI's `Style::overflow_mask` creates a rectangular content mask whenever either axis is non-visible; with no border, the horizontal-only mask still uses the label's full vertical bounds. Therefore any label-level overflow mask can clip glyph paint beyond its line box. The shared `Tab` already provides a taller bounded container, while `min_w_0()` can allow the nested label to shrink without setting overflow.

## Plan

1. Keep the nested label styling in a local helper so its mask and width constraints are directly testable.
2. Remove label-level overflow clipping and set an explicit zero minimum width so flex shrink and ellipsis remain available.
3. Run focused and workspace quality checks.

## Decisions

None; the fix follows existing GPUI overflow semantics and does not establish a new architectural choice.

## Verification Plan

- Focused unit test: assert the terminal title label leaves both overflow axes at the visible default, so GPUI does not create a label-level content mask.
- `cargo test -p oneterm-terminal-view`
- `cargo test --workspace`
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo build --workspace`
- Manual desktop rendering remains the final visual proof because the repository has no pixel/screenshot baseline for glyph paint output.

## Evidence and Gaps

- Screenshot evidence (`Screenshot 2026-08-11 171407.png`): the first horizontal-only correction remained clipped with a flat edge across the bottom of the title glyphs.
- GPUI source evidence: `Style::overflow_mask` creates a rectangular mask whenever either overflow axis is non-visible, explaining why `overflow_x_hidden()` did not fix glyph painting.
- `cargo test -p oneterm-terminal-view tab_title_label_does_not_create_a_content_mask -- --nocapture` — passed; 1 corrected regression test passed.
- `cargo test -p oneterm-terminal-view` — passed; 99 tests passed.
- `cargo test --workspace` — passed; 585 passed, 2 ignored.
- `cargo fmt --all -- --check` — passed.
- `cargo clippy --workspace --all-targets -- -D warnings` — passed with no issues.
- `cargo build --workspace` — passed.
- `git diff --check` — passed.
- Gap: no automated pixel/screenshot baseline exists for glyph painting. The regression test proves the clipping mask is absent; final desktop visual confirmation remains manual.

## Handoff

Not applicable; single-session change.
