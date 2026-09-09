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
`TerminalModel`; `resize_grid` keeps alacritty's semantics for shrinks, column-only changes
and `Default`, and for a `KeepViewportTop` grow runs `grow_keeping_viewport_top`: compute
`pulled = min(history_size, lines_added)` on the primary grid, `Term::resize` the rows only,
then `Grid::scroll_up` over the whole screen by `pulled` (the pulled rows rotate back into
history, the bottom `pulled` rows are cleared, `history_size` returns to its old value),
move the cursor and saved cursor up by `pulled`, restore the pre-resize `display_offset`
(clamped), drop the selection, then resize the columns. While the alt screen is active the
primary grid is `Term::inactive_grid` (no accessor): the alt grid is parked in a local behind a
placeholder, `swap_alt` makes the primary active for the resize and the correction, `swap_alt`
returns (its clear-and-copy-cursor step hits the placeholder only) and the parked alt grid,
resized with `Grid::resize(false, ..)` as `Term::resize` would have, is put back.
`Term::resize` marks the whole terminal damaged, so no extra damage call is needed.
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
  independently. Only the row pull is corrected (the decided scope).
- Linux/macOS local sessions keep `ResizePolicy::Default`; not exercised here.

## Handoff

Implemented and verified on Windows; awaiting owner acceptance of the two screenshots and the
blank-rows-below-the-prompt tradeoff recorded in DEC-0008.

## Acceptance Rework (2026-09-09, owner)

TUI flow passes. New failing case: `ls -lath` (Git for Windows `ls` under cmd) at a narrow
window so long lines wrap, maximize, type any character: the grid cursor stays on the prompt row
but the echoed character lands 4 rows above it (`evidence/owner-ls-lath-maximize-desync.png`):
conhost and alacritty reflow the wrapped rows differently on the column change.

