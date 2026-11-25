# Work: Keep the grid aligned with conhost on a ConPTY grow-resize

ID: BUG-0051
Intake: IN-0019
Created: 2026-09-09

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [ ] Planned
- [ ] In progress
- [x] Implemented
- [ ] Changed
- [x] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: bug
- Risk lane: normal (changes established resize behaviour of local sessions; no data, auth, or
  persistence impact)
- Spec Intake: IN-0019

## Outcome

After the viewport of a local ConPTY session grows (window maximize or split resize), shell
output and typed input appear on the rows the shell addresses; no stale rows or cursor fragments
remain after an alt-screen TUI exits; the prompt is the last visible row.

## Scope

- [x] In scope: grow-resize policy for `SessionKind::Local` on Windows in `crates/terminal`
      (`TerminalModel::resize_grid`) selected by `crates/local-shell`, a focused grid test,
      a manual repro, `docs/terminal-backend.md` update.
- [x] Out of scope: the view crate; SSH sessions; shrink-resize semantics; vendored-fork patches
      unless policy (c) is chosen.

## Acceptance

- [x] Repro script (`dir` to fill the screen, maximize, `echo desync-check`): the echo lands on
      the prompt row, not inside the listing (compare with
      `../IN-0018-rebuild-terminal-render-engine/evidence/rework2-bug2-resize-desync-new.png`).
      Evidence: `evidence/BUG-0051-after-echo.png`.
- [x] Owner flow (opentui-examples, maximize during the TUI, quit): no stale fragment, prompt on
      the last content row (blank rows below it — DEC-0008 tradeoff). Evidence:
      `evidence/BUG-0051-after-tui.png`.
- [x] Shrink-resize and SSH resize behaviour unchanged (existing backend tests pass;
      `keep_viewport_top_leaves_shrink_and_history_less_grow_to_alacritty`,
      `ssh_session_keeps_the_default_grow_policy`).
- [x] Focused test in `crates/terminal` proves the cursor row after a grow with history matches
      the chosen policy (`keep_viewport_top_grow_keeps_rows_cursor_and_history`,
      `default_grow_pulls_history_and_moves_the_cursor_down`).
- [ ] `pwsh scripts/ci-local.ps1` green — not run in this session (see Gaps); the individual
      gates it wraps were run.

## Documentation

### Owning Docs Reviewed

- `docs/terminal-backend.md` — session resize ordering (`pty_resize` before `resize_grid`).
- `docs/spec-intakes/IN-0019-conpty-resize-scrollback-desync/high-level-design.md` — policy
  candidates.
- `docs/spec-intakes/IN-0018-rebuild-terminal-render-engine/US-0049-terminal-view.md`
  ("Acceptance Rework 2", bug 2) — triage evidence that the view is not the cause.
- `vendor/README.md` — how vendored-fork patches are made, if policy (c) is chosen.

### Documentation Action

Update required: `docs/terminal-backend.md` must describe the chosen grow policy for local
sessions and why it differs from alacritty's default.

Reason: resize semantics are an operator-visible backend contract.

### Reconciliation

- `docs/terminal-backend.md` §5.3: resize ordering corrected (`pty_resize` runs before
  `resize_grid`) and the grow-resize policy, its invariants, tests and tradeoff added; §6.3
  Resize bullet points at it.
- `docs/agents/persistence.md`: reviewed, no change — nothing persisted changes.
- `docs/decisions/DEC-0008-local-conpty-grow-resize-keeps-viewport-top.md`: consequences
  confirmed.
- `IN-0019.md`: packet and first action ticked.

## Context

- `vendor/alacritty_terminal/src/grid/resize.rs:43-69` (`grow_lines`) pulls `from_history =
  min(history_size, lines_added)` rows and moves the cursor down by that amount.
- conhost keeps the viewport top on `ResizePseudoConsole` and addresses output with absolute
  `CUP` in its own coordinates (observed in the raw PTY dump during triage: `ESC[34;65H` while
  the grid prompt was on row 51).
- Both engines (old and new view) render the same picture; the desync is terminal state.

## Plan

- [x] Owner picked policy (a): no scrollback pull on local grow (DEC-0008).
- [x] Implement behind `SessionKind::Local`, Windows only.
- [x] Focused grid test + manual repro screenshots into `evidence/`.
- [x] Update `docs/terminal-backend.md`.

## Implementation

`crates/terminal/src/model.rs`: `ResizePolicy { Default, KeepViewportTop }` is a field of
`TerminalModel`; `resize_grid` keeps alacritty's semantics for `Default` and, for
`KeepViewportTop`, runs `resize_keeping_viewport_top` on every resize: `conhost_cursor_row`
copies the viewport rows from the top down to the cursor row into a history-less scratch
`Grid<Cell>` (plus a few blank rows above so `shrink_columns` cannot clamp the scratch cursor
at `Line(0)`), reflows it to the new width with the vendored `Grid::resize` and reads back the
cursor's distance from the top row (`history_size + line`, so split rows count); then
`Term::resize` runs and the viewport is shifted by `cursor row - measured row`: a positive
shift is `Grid::scroll_up` over the whole screen (top rows rotate back into history, bottom
rows cleared, cursor and saved cursor moved up), a negative shift grows the rows by that
amount (pulling the split rows back from history) and shrinks them again (dropping the blank
bottom rows). The pre-resize `display_offset` is restored (clamped) and the selection dropped
when rows moved. While the alt screen is active the primary grid is `Term::inactive_grid` (no
accessor): the alt grid is parked in a local behind a placeholder, `swap_alt` makes the
primary active for the measurement, the resize and the correction, `swap_alt` returns (its
clear-and-copy-cursor step hits the placeholder only) and the parked alt grid, resized with
`Grid::resize(false, ..)` as `Term::resize` would have, is put back. `Term::resize` marks the
whole terminal damaged, so no extra damage call is needed.
The policy is a new argument of `impl_pty_terminal_session!`
(`crates/terminal/src/session.rs`): `crates/local-shell/src/session_terminal.rs` passes
`KeepViewportTop` under `cfg!(windows)` (ConPTY) and `Default` elsewhere;
`crates/ssh/src/session_terminal.rs` passes `Default`. `vendor/` untouched.

## Decisions

- `docs/decisions/DEC-0008-local-conpty-grow-resize-keeps-viewport-top.md`.

## Verification Plan

- Focused: `cargo test -p oneterm-terminal resize`
- Unit: `cargo test --workspace`
- Integration: `cargo test -p oneterm-local-shell`
- E2E: manual Windows repro (both scripts above) with `PrintWindow` screenshots.
- Gate: `pwsh scripts/ci-local.ps1`

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [x] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Run on 2026-09-09, branch `refactor/terminal-render-engine`, Windows 11, isolated
`CARGO_TARGET_DIR=target/bug51-target` for the manual build (removed afterwards).

- `cargo fmt --all` → exit 0.
- `cargo clippy --workspace --all-targets -- -D warnings` → `Finished `dev` profile
  [unoptimized + debuginfo] target(s)`, exit 0.
- `cargo test -p oneterm-terminal` → `test result: ok. 238 passed; 0 failed; 0 ignored`.
- `cargo test -p oneterm-local-shell` → `test result: ok. 30 passed; 0 failed; 0 ignored`
  (includes `local_session_grow_policy_matches_conpty`).
- `cargo test -p oneterm-ssh` → `test result: ok. 50 passed; 0 failed; 0 ignored`
  (includes `ssh_session_keeps_the_default_grow_policy`).
- `cargo test --workspace` → 44 suites `test result: ok`, 1047 passed, 0 failed, 5 ignored.
- `python scripts/check-english.py` → `English contributor-text check passed for 545 files.`
- `python scripts/check-doc-paths.py` → `Doc path check passed for 149 current paths in 10
  documents.`
- Manual script 1 (`dir C:\Windows\System32`, `ShowWindow(SW_MAXIMIZE)`, `echo desync-check`
  via `PostMessage` `WM_CHAR`/`WM_KEYDOWN`): `evidence/BUG-0051-after-echo.png` — the echo,
  its output and the next prompt sit directly under the listing; the grown area below is blank.
- Manual script 2 (`dir`, `opentui-examples.exe`, wait 3 s, maximize, wait 2 s, `q`, Escape,
  Ctrl+C, wait 2 s): `evidence/BUG-0051-after-tui.png` — primary screen restored with no stale
  fragment; the prompt follows the TUI command line. Ctrl+C had to be delivered as
  `WM_KEYDOWN 'C'` with `VK_CONTROL` set through `AttachThreadInput` + `SetKeyboardState`
  (GPUI reads modifiers with `GetKeyState`; a `WM_CHAR 0x03` is not treated as Ctrl+C).

Gaps:

- `pwsh scripts/ci-local.ps1` was not run (another agent was building in the shared target
  dir); the gates it wraps were run individually above except `verify-dependency-graph.py`,
  `test_check_english.py`, `completion-catalog.py validate` and `third-party-notices.py`,
  none of which this change touches (no dependency, catalog or notice changes).
- The correction reaches the inactive primary grid through two `Term::swap_alt` calls around
  a parked alt grid because the vendored fork exposes no `inactive_grid` accessor. A
  three-line accessor patch under `vendor/patches/alacritty_terminal/` would remove that
  dance; kept out per the "no vendor change" scope.
- Column reflow on a width change is still alacritty's; conhost reflows its own buffer
  independently. Only the row pull is corrected (the decided scope). Superseded by the
  acceptance rework below.
- Linux/macOS local sessions keep `ResizePolicy::Default`; not exercised here.

## Handoff

Implemented and verified on Windows; awaiting owner acceptance of the two screenshots and the
blank-rows-below-the-prompt tradeoff recorded in DEC-0008.

## Acceptance Rework (2026-09-09, owner)

TUI flow passes. New failing case: `ls -lath` (Git for Windows `ls` under cmd) at a narrow
window so long lines wrap, maximize, type any character: the grid cursor stays on the prompt row
but the echoed character lands 4 rows above it (`evidence/owner-ls-lath-maximize-desync.png`):
conhost and alacritty reflow the wrapped rows differently on the column change.

### Measurements (raw PTY dump before the parser + grid dump around `resize_grid`, both
temporary and removed)

Windows 11, cmd.exe, fast-dev build, window restored to 900x700 (33x43 cells) then resized by
`ShowWindow`/`MoveWindow`; conhost's next output after the resize was the echo of one typed
character, addressed with `CUP`.

- (a) Long lines arrive as one continuous line (implicit wrap): `ls -lath` rows carry
  `WRAPLINE` in the grid, 86 flagged rows in a 162-row buffer, 21 of them inside the 33-row
  viewport. Only cmd's cooked-read echo writes a space past the margin and then `CUP`s to the
  next row, which also leaves `WRAPLINE` set.
- (b) After `ResizePseudoConsole` conhost emits nothing until the keystroke. Maximize
  33x43 -> 52x158: `ESC[12;18H` (row 11, 0-based) while the grid prompt was on row 32,
  i.e. 21 rows up = the 21 wrapped rows joined. Widen only 33x43 -> 33x132: `ESC[14;18H`
  (row 13, 19 joined; two lines still wrap at 132). Rows grow + columns shrink 33x43 -> 49x34:
  `ESC[38;18H` (row 37, 5 split rows). Seven synthetic `for /L` cases with the top row being
  the head, a middle row or the tail of a 149-char wrapped line at 71, 99 and 34 columns
  all fit one rule: conhost re-wraps the old viewport rows at the new width as if the top row
  started a line (the visible part of that line keeps `ceil(visible_chars / new_cols)` rows:
  149->3, 106->2, 63->1, 20->1 at 71 columns; 106->2, 63->1 at 99; 38->1 at 158; 106->4 at
  34), keeps that content on top, and the cursor row is its distance from the top. History
  above the viewport never enters the count.
- (c) alacritty with the shipped rows-then-columns fix: the row step restored the cursor to
  row 32; the column step joined the 21 rows above the cursor and, being bottom-anchored,
  pulled 21 history rows into the top while the cursor stayed on row 32, so the grid was 21
  rows below conhost (4 in the owner's wider window). A single combined `Term::resize`
  gives the same picture (cursor 51 after the pull, still 21 rows off after the join).

### Root cause

DEC-0008 only undid the history pull of `grow_lines`. The column reflow moves rows too:
alacritty keeps the cursor index and fills the freed rows from history (or pushes the top
rows out when it splits rows); conhost keeps its top row and lets the cursor move with the
joined or split rows. Every wrapped row above the cursor therefore desynchronised the two by
one row, on any width change, not only on a maximize.

### Fix

`resize_keeping_viewport_top` (see Implementation) applies to every resize and measures
conhost's cursor row with the vendored reflow itself (`conhost_cursor_row`), so joins,
partial joins, splits, the cursor's own row and wide-character spacers are decided by the
same code that reflows the real grid; the viewport is then shifted by the measured difference.
No vendor patch. Rows below the top row that start at column 0 hold the same text as in
conhost; when the top row continued a wrapped line from history the grid shows that line
joined whole while conhost keeps it torn (top rows only, recorded in DEC-0008).

### Tests (`crates/terminal/src/model.rs`)

`keep_viewport_top_widen_joins_wrapped_rows_and_keeps_the_top_row` (the `ls -lath` shape:
rows grow and columns grow, two joined rows, cursor row 4 -> 2, joined rows back to history,
`WRAPLINE` cleared), `keep_viewport_top_widen_without_row_change_moves_the_cursor_up`,
`keep_viewport_top_top_row_continuing_a_history_line_keeps_the_cursor_row`,
`keep_viewport_top_widen_joins_the_cursor_row` (cursor moves up one row and right by the
joined width), `keep_viewport_top_narrow_with_a_mid_screen_cursor_pulls_split_rows_back`
(negative shift), `keep_viewport_top_narrow_with_the_cursor_at_the_bottom_matches_alacritty`,
`keep_viewport_top_widen_during_alt_screen_joins_the_primary_rows`; the eight earlier policy
tests are unchanged and still pass.

### Verification

Run on 2026-09-09, branch `refactor/terminal-render-engine`, Windows 11.

- `cargo fmt --all -- --check` -> exit 0.
- `cargo clippy --workspace --all-targets -- -D warnings` -> `Finished `dev` profile
  [unoptimized + debuginfo] target(s) in 6.68s`.
- `cargo test -p oneterm-terminal` -> `test result: ok. 245 passed; 0 failed; 0 ignored`.
- `cargo test -p oneterm-local-shell` -> `test result: ok. 30 passed; 0 failed; 0 ignored`
  (one earlier run, concurrent with the GUI captures, failed
  `mouse_drag_updates_selection_not_mouse_move` on its 2 s real-PTY wait; two isolated runs
  passed; the crate is untouched).
- `cargo test -p oneterm-ssh` -> `test result: ok. 50 passed; 0 failed; 0 ignored`.
- `cargo test --workspace` -> every suite `test result: ok`, 0 failed, 3 ignored.
- `python scripts/check-english.py` -> `English contributor-text check passed for 536 files.`
- `python scripts/check-doc-paths.py` -> `Doc path check passed for 146 current paths in 10
  documents.`
- Instrumented fast-dev build, conhost `CUP` row vs grid cursor row after the resize:
  `ls -lath` maximize 11/11, `for` maximize 12/12, `ls` widen 13/13, `ls` 49x34 37/37,
  `for` chain widen 30/30, `dir` maximize 17/17 (all match).
- Clean fast-dev build, `PostMessage` driver (`WM_CHAR`, `WM_KEYDOWN` Enter, `ShowWindow(3)`,
  `PrintWindow`): `evidence/BUG-0051-rework-ls-lath.png` (the typed `l` echoes on the prompt
  row after the maximize), `evidence/BUG-0051-rework-dir-echo.png` (`echo desync-check` under
  the listing), `evidence/BUG-0051-rework-tui.png` (prompt directly under the TUI command
  line, no fragment).

### Gaps

- The TUI flow was run six times; five put the prompt directly under the command line, one
  (before the alt-screen path had been unit-tested) put it 18 blank rows lower with no stale
  fragment and no dump to explain it. The alt-screen path is covered by
  `keep_viewport_top_widen_during_alt_screen_joins_the_primary_rows`; not reproduced since.
- `pwsh scripts/ci-local.ps1` was not run (out of scope for the rework); the gates it wraps
  were run individually above except the dependency, catalog and notice checks, which this
  change does not touch.
- A row shrink still relies on alacritty's `shrink_lines` matching conhost (cursor kept
  unless it falls off the bottom); the measured shift is zero there and no counter-example
  was seen, but conhost's row-shrink behaviour was not dumped.
