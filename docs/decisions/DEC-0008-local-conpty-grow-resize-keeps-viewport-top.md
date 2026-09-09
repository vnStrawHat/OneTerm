# DEC-0008 Local ConPTY sessions keep the viewport top on a grow-resize

Date: 2026-09-09

## Status

accepted

## Context

`alacritty_terminal` anchors the bottom row on a resize: `Grid::grow_lines` pulls rows out of
scrollback into the top of a grown viewport and moves the cursor down by the same amount, and
the column reflow lets history fill the rows freed by joining `WRAPLINE` rows (or pushes the top
rows out when it splits them) while the cursor keeps its index. conhost, which owns the console
buffer behind ConPTY, keeps its viewport top, re-wraps the old viewport rows at the new width as
if the top row started a line, repaints nothing and addresses output with absolute cursor
positions in its own coordinates (measured with raw PTY dumps, BUG-0051). After a window
maximize the two models disagree by the pulled rows plus the joined rows: typed input lands
above the prompt and, after an alt-screen TUI exits, stale rows and a cursor fragment stay
visible (IN-0019).

## Decision

For `SessionKind::Local` sessions on Windows every resize keeps the viewport top the way
conhost does: scrollback is never pulled into the viewport, the wrapped rows of the viewport
join or split as conhost re-wraps them, the cursor row is moved to the row conhost addresses
(measured by reflowing the viewport rows in a history-less scratch grid with the vendored
reflow), new rows are blank at the bottom and displaced rows stay in history. SSH sessions keep
alacritty's default behaviour (remote PTYs reflow and repaint on their side). The policy is
selected by the backend that constructs the terminal model, not by the view.

## Alternatives

- [x] Selected approach described above (owner choice, 2026-09-09).
- [ ] Force a conhost repaint after every resize: depends on undocumented conhost behaviour and
      still flashes stale rows.
- [ ] Patch the vendored grid to make the history pull optional: same effect, but a fork delta to
      maintain; kept as a fallback if the model-level correction proves fragile.

## Consequences

- [x] Benefit confirmed (BUG-0051, 2026-09-09): `echo` after a maximize lands on the prompt
      row; TUI exit after a maximize leaves no stale rows
      (`docs/spec-intakes/IN-0019-conpty-resize-scrollback-desync/evidence/`).
- [x] Amended after acceptance rework (2026-09-09): the row-only rule left `ls -lath` output
      (wrapped rows) desynchronised by the joined rows; the decision now covers the column
      reflow and the cursor row matches conhost's `CUP` in every measured flow.
- [x] Tradeoff: when the top row continued a wrapped line from history, the grid shows that
      line joined whole while conhost keeps it torn at the old top row; rows below agree.
- [x] Tradeoff: local sessions show blank rows at the bottom after a grow instead of recovered
      history; scrollback still holds those rows.
