# DEC-0008 Local ConPTY sessions keep the viewport top on a grow-resize

Date: 2026-09-09

## Status

accepted

## Context

`alacritty_terminal::Grid::grow_lines` pulls rows out of scrollback into the top of a grown
viewport and moves the cursor down by the same amount. conhost, which owns the console buffer
behind ConPTY, keeps its viewport top and extends downward, then addresses output with absolute
cursor positions in its own coordinates. After a window maximize the two models disagree by the
number of pulled rows: typed input lands above the prompt and, after an alt-screen TUI exits,
stale rows and a cursor fragment stay visible (IN-0019).

## Decision

For `SessionKind::Local` sessions on Windows, a grow-resize never pulls scrollback into the
viewport: existing rows keep their row index, new rows are appended blank at the bottom, the
cursor row is unchanged, and the pulled rows stay in history. Shrink-resize and SSH sessions keep
alacritty's default behaviour (remote PTYs reflow and repaint on their side). The policy is
selected by the backend that constructs the terminal model, not by the view.

## Alternatives

- [x] Selected approach described above (owner choice, 2026-09-09).
- [ ] Force a conhost repaint after every resize: depends on undocumented conhost behaviour and
      still flashes stale rows.
- [ ] Patch the vendored grid to make the history pull optional: same effect, but a fork delta to
      maintain; kept as a fallback if the model-level correction proves fragile.

## Consequences

- [ ] Benefit to confirm: `echo` after a maximize lands on the prompt row; TUI exit after a
      maximize leaves no stale rows.
- [ ] Tradeoff: local sessions show blank rows at the bottom after a grow instead of recovered
      history; scrollback still holds those rows.
