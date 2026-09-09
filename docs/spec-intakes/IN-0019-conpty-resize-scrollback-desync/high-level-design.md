# High-Level Design: ConPTY grow-resize alignment

Intake: IN-0019
Lane: normal
Date: 2026-09-09

## Idea

When a local ConPTY session's viewport grows, `alacritty_terminal::Grid::grow_lines` pulls up to
`lines_added` rows out of scrollback into the top of the viewport and moves the cursor down by
that amount. conhost, which owns the real console buffer behind ConPTY, keeps its viewport top
where it was and simply extends downward, then addresses later output with absolute cursor moves
(`CUP`) in its own coordinates. The two models disagree by `from_history` rows, so typed input
lands above the prompt and, after an alt-screen TUI exits, stale primary-screen rows and a cursor
fragment stay visible. The fix keeps both sides on the same origin for local sessions.

## Diagram

```text
 before grow (36 rows)         alacritty after grow_lines(52)      conhost after resize (52)
 ┌───────────────┐             ┌───────────────┐ ← 16 rows pulled   ┌───────────────┐ ← same top
 │ scrollback…   │             │ from history  │   from history     │ old row 0     │
 │ old row 0     │             │ …             │                    │ …             │
 │ …             │             │ old row 0     │                    │ prompt (35)   │ ← CUP 35
 │ prompt (35) █ │             │ …             │                    │ (blank)       │
 └───────────────┘             │ prompt (51) █ │                    │ …             │
                               └───────────────┘                    └───────────────┘
```

## UI Wireframe

N/A — no UI surface changes; the terminal grid simply stays consistent with the shell.

## Data Flow

1. `TerminalElement::prepaint` computes a new `(rows, cols)` and calls `TerminalInput::resize`.
2. The session calls `PtyTransport::pty_resize` (ConPTY `ResizePseudoConsole`) and then
   `TerminalModel::resize_grid` (`Term::resize` → `Grid::resize` → `grow_lines`).
3. Policy (candidate a): for `SessionKind::Local` on Windows, a grow does not pull scrollback;
   new rows are appended below and the cursor row is unchanged, matching conhost. Shrink keeps
   today's behaviour. Alternatives (b) request a conhost repaint after resize; (c) patch the
   vendored grid so that `from_history` is a caller option.
4. Output that follows uses the shell's absolute coordinates and lands on the expected rows.

## Detail Design

- [ ] Detail design: not needed
- Reason: the change is a single resize-policy switch in the backend once the owner picks the
  policy; the packet records the chosen variant and its test.
