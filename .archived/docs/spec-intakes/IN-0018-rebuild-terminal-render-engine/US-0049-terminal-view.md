# Work: TerminalView entity and the swap of the old view

ID: US-0049
Intake: IN-0018
Created: 2026-09-08

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [x] In progress
- [x] Implemented
- [ ] Changed
- [x] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: existing-contract change (the running app switches to the new engine)
- Risk lane: normal (broad behavior, but no persisted schema, no security policy change; policies stay in `crates/terminal`)
- Spec Intake: IN-0018

## Outcome

`src/terminal_view/` provides `TerminalView` (rename of `LocalTerminalView`) built on
`render/` and `input/`: events pump with Output coalescing, blink task, focus, OSC-to-UI
handling, IME, search, scrollbar, gutter timestamps, completion glue, agent status, theme/
settings reactivity, bell, SSH-closed banner, progress bar. The old `panel/` and `space/` are
pointed at it, and `src/view/`, `src/element/`, `src/layout/`, `src/box_drawing/`,
`src/handlers/` are deleted. The app renders through the new engine.

## Scope

- [x] In scope: `src/terminal_view/{mod, view, render, ime, search, scrollbar,
      gutter_timestamps, completion, agent_status}.rs` and tests; minimal edits in old
      `panel/*.rs`, `space/*.rs`, `agent.rs`, `status.rs` (type rename `LocalTerminalView →
      TerminalView`, `view::` → `terminal_view::` paths); `completion/` and `url/hover.rs`
      adaptations; `theme/` cleanup of old entry points; deletion of the five old module trees;
      `lib.rs` declarations.
- [x] Out of scope: rewriting `panel/` and `space/` (US-0050); removing `#[allow(dead_code)]`
      from `render`/`input` roots (US-0050, once nothing old remains); behavior changes beyond the
      HLD deviations.

## Acceptance

- [x] HLD parity items US-0049 (§2.9–2.17, 2.19–2.21) are implemented; each old `view/*`
      unit test listed in inventory §4 exists under `terminal_view/` with the same intent.
- [x] `TerminalView` exposes exactly the pub(crate) surface in the HLD (`session`,
      `duplicate_config`, `split_ctx`, `focus`, `new`, `shutdown`, `toggle_search`,
      `handle_event`, `render_diagnostics`, `TerminalViewEvent::TitleChanged`, `TerminalDeps`).
- [x] `phase0_renderer_baseline_counts_dirty_and_idle_frames` (view-level) passes on the new
      engine; `exited_behind_output_batch_marks_agent_ended` (panel test) still passes.
- [x] `ssh_close_shows_banner_once`, `notification_queue_is_bounded`,
      `blink_tick_gated_by_focus_and_setting`, `ime_disabled_on_alt_screen`,
      `ime_bounds_at_cursor_cell`, `clipboard_read_reply_gated_by_setting` pass.
- [x] `grep -rn "alacritty_terminal" crates/terminal-view/src` lists only `render/frame.rs`
      and `theme/palette.rs` (plus `input/mouse.rs`: `SelectionType` is part of the
      `TerminalInput::mouse_down` signature and cannot be avoided; recorded in the HLD).
- [ ] Manual Windows run (`--profile fast-dev`): typing, IME (Vietnamese/CJK composition on
      the primary screen), selection/copy/paste, Ctrl+F search, completion overlay, scrollbar
      drag and fade, gutter toggle, bell badge, OSC 9;4 progress, SSH close banner, box drawing
      sample (`─│┌┐└┘├┤┬┴┼═║╔╗╚╝╬╒╘╓╙╭╮╰╯▀▄█░▒▓⣿ E0B0–E0BF`) look correct; findings recorded.
- [ ] `pwsh scripts/ci-local.ps1` green.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0018-rebuild-terminal-render-engine/high-level-design.md` — data flow, threading, interfaces, deviations, parity US-0049.
- `docs/spec-intakes/IN-0018-rebuild-terminal-render-engine/low-level-design/render-pipeline.md` — `RenderInputs`, element spec, IME install hook.
- `docs/spec-intakes/IN-0018-rebuild-terminal-render-engine/low-level-design/input.md` — IME rules, listener wiring on the wrapper div.
- `docs/PROJECT.md`, `docs/architecture.md` — boundaries (OSC/paste/URL policies in `crates/terminal`; `AppServices` hooks).
- `docs/osc-agent-status.md` — OSC 9;7 lifecycle (ended vs closed vs removed).
- `docs/osc-sequences-checklist.md` — which OSCs reach the UI (Cwd/ShellIntegration stay unhandled).
- `docs/sftp-follow-terminal-cwd/README.md` — cwd is read on demand via capabilities, not through `SessionEvent::Cwd`.
- `docs/terminal-backend.md` — snapshot/damage and event channel contract.

### Documentation Action

Update required: `docs/osc-sequences-checklist.md` only if a UI-visible OSC behavior changes
(none planned); `high-level-design.md` interfaces section if the pub(crate) surface changes.
Old rendering docs are marked historical in US-0050, not here.

Reason: this packet changes implementation, not the documented behavior; the HLD is the owning
contract and must stay accurate about the surface `panel/`/`space/` consume.

### Reconciliation

Before completion, confirm the HLD "Public and pub(crate) Interfaces" block matches
`terminal_view/view.rs` and list the deleted directories.

Done (2026-09-09):

- HLD interface block vs `terminal_view/view.rs`: `session`, `duplicate_config`, `split_ctx`,
  `focus` (pub(crate) fields), `new`, `shutdown`, `toggle_search`, `handle_event`,
  `TerminalViewEvent::TitleChanged`, `TerminalDeps::from_globals`, `Focusable`, `EventEmitter`
  match. Two recorded differences (HLD "Amendments (US-0049 implementation)" 1–2):
  `render_diagnostics` is `#[cfg(test)]` (dead under the feature alone); `alive`,
  `event_task`, `blink_task` are `pub(crate)` for the retained `panel/tests.rs`.
- Deleted directories: `crates/terminal-view/src/view/`, `src/element/`, `src/layout/`,
  `src/box_drawing/`, `src/handlers/`. `lib.rs` declares `terminal_view` and no
  `#[allow(dead_code)]` root remains (the US-0050 item is already done).
- Retained-module cleanup: `theme::resolve_cell_color`, `url::url_masks_wrapped`,
  `SemanticOverlay::scan` removed; `url/detect.rs`, `url/tests.rs`, `theme/tests.rs`,
  `input/mouse_tests.rs` no longer name `alacritty_terminal`; `input::menu::split_items` is
  shared with the placeholder menu (was `handlers::menu::split_items`).
- LLD amendments: `low-level-design/input.md` "Amendments (US-0049 implementation)" (key send
  does not mark the scrollbar, hover moves repaint only on change, scrollbar hit test in the
  view, `on_mouse_exit`, IME row = display row, alt-screen completion dismiss, `split_items`);
  `low-level-design/render-pipeline.md` "Amendments (US-0049 implementation)" (test-only
  seams, removed accessors, `Cell::from_indexed` / `hyperlink_uri` bridge).
- `oneterm_terminal::test_support::FakeSessionProbe::set_alt_screen` added (additive) for the
  IME tests.

## Context

- Old `panel/` and `space/` reference `LocalTerminalView::new`, `.session`, `.duplicate_config`,
  `.split_ctx`, `.focus`, `focus_handle()`, `shutdown()`, `toggle_search()`,
  `TerminalViewEvent`, `TerminalDeps` — the field names are kept so the edit is a rename.
- `SplitContext` stays at `crate::space::SplitContext` (old file now, rewritten in US-0050).
- Events pump: `take_events()` once; drain `Output` with `try_recv`; a trailing `Exited` in
  the same batch must still be processed (CORR-02 regression test in `panel/tests.rs`).
- Settings observation lives in the panel (`cx.observe(&deps.settings, ..)`); the view re-reads
  settings each render and rebuilds `Font`/`TerminalTheme` only when inputs differ.
- Blink task and events task are held as `Task<()>`; `shutdown` drops them, closes the
  session, and removes the agent card/nav.
- IME handler installation happens inside the element's paint via `spec.ime`; the wrapper div
  owns `track_focus` and all listeners.
- Scrollbar `pending_offset` is applied at render start against `terminal_info()` (the one
  `terminal_info` read of the frame; gutter stamping reads it again on `Output`).
- Completion reads the cursor row through `query_line_range_cells`, never the frame.

## Plan

- [x] `terminal_view/view.rs`: struct, `TerminalDeps`, `new` (events pump, blink task, focus
      subs, `set_nav`), `handle_event` table, `shutdown`, notification queue, progress, bell,
      `reply_clipboard_read`.
- [x] `terminal_view/render.rs`: `RenderInputs` refresh (font cache, theme cache, palette push,
      metrics inputs, cursor config, gutter stamps copy, search highlights), wrapper div with
      listeners delegating to `input/`, `TerminalElementSpec`, bars/badges/banner/progress/
      completion overlay/scrollbar element, notification drain.
- [x] `ime.rs`, `search.rs`, `scrollbar.rs`, `gutter_timestamps.rs`, `completion.rs`,
      `agent_status.rs` ported to the new types with their tests (+ `input.rs` for the wrapper's
      listeners, see HLD amendment 3).
- [x] Point `panel/`, `space/`, `agent.rs`, `status.rs` at `terminal_view::TerminalView`.
- [x] Delete `src/view/`, `src/element/`, `src/layout/`, `src/box_drawing/`, `src/handlers/`;
      remove the old entry points left in `theme/`, `url/`, `highlight/`, `completion/`.
- [x] Port `view/tests.rs` (3), per-file tests (search 7, scrollbar 7, gutter 7, local_view 3,
      render 4, completion glue 10, grid 4 → `metrics`, key 6 → `input/keys`).
- [ ] Manual Windows run per Acceptance; capture screenshots into
      `docs/spec-intakes/IN-0018-rebuild-terminal-render-engine/evidence/`.
- [ ] `pwsh scripts/ci-local.ps1`.

## Decisions

- `docs/decisions/DEC-0007-terminal-render-engine-cell-glyph-cache-and-quad-shapes.md`.

## Verification Plan

- Focused: `cargo test -p oneterm-terminal-view terminal_view` — ported tests plus
  `phase0_renderer_baseline_counts_dirty_and_idle_frames`,
  `completion_overlay_shows_when_typing_d_at_cmd_prompt`,
  `completion_resumes_after_initial_non_prompt_render`, `ssh_close_shows_banner_once`,
  `notification_queue_is_bounded`, `blink_tick_gated_by_focus_and_setting`,
  `output_batch_coalesces_and_keeps_exited`, `ime_disabled_on_alt_screen`,
  `ime_commit_scrolls_and_clears_bell`, `ime_bounds_at_cursor_cell`,
  `clipboard_read_reply_gated_by_setting`, `title_event_emits_title_changed`,
  `progress_bar_color_by_variant`, `bell_badge_requires_setting`.
- `cargo test -p oneterm-terminal-view` (whole crate: panel 11, space 15, url 13, theme 10,
  highlight 7, completion 15, shapes, render, input).
- Regression: `cargo test --workspace`.
- E2E: manual Windows checklist in Acceptance (`cargo run -p oneterm-app --profile fast-dev`).
- Gate: `pwsh scripts/ci-local.ps1`.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Verbatim results (2026-09-09, Windows 11, branch `refactor/terminal-render-engine`):

- `cargo clippy -p oneterm-terminal-view --all-targets -- -D warnings` → exit 0.
- `cargo clippy -p oneterm-terminal-view --all-targets --features terminal-diagnostics -- -D warnings`
  → exit 0.
- `cargo clippy --workspace --all-targets -- -D warnings` → exit 0.
- `cargo fmt --all -- --check` → exit 0.
- `cargo test -p oneterm-terminal-view` →
  `test result: ok. 259 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out`
  (panel 11, space 15, url 13, theme 11, highlight, completion, render, input, and
  `terminal_view` 49: view_tests 10, ime 3, render 6, input 1, search 7, scrollbar 9,
  gutter 6, completion 7).
- `cargo test --workspace` → exit 0; every crate `test result: ok` (terminal-view
  `259 passed; 0 failed; 1 ignored`).
- `grep -rn "alacritty_terminal" crates/terminal-view/src` → `input/mouse.rs`,
  `render/frame.rs`, `theme/palette.rs`.
- Smoke run: `cargo build -p oneterm-app --profile fast-dev` → exit 0; `target/fast-dev/oneterm.exe`
  started, `PrintWindow` capture after 10 s → `target/US-0049-smoke.png` shows the cmd prompt
  `C:\Users\trunglt>` with semantic path colours and a `#528BFF` block cursor (Zed One Dark
  caret) in the cell after `>`; the process was stopped afterwards. With the
  `terminal-diagnostics` build the 5 s log reported `2 layers`, `rows 1/45 candidate, 0 planned,
  0 shaped` on idle frames and the cursor toggling `painted=true/false` every 500 ms.

Gaps:

- Manual Windows parity walk (typing, IME, selection, search, completion, scrollbar, gutter,
  bell, OSC 9;4, SSH close, box-drawing sample) not run yet — interactive step; the smoke run
  only confirms the prompt and cursor render.
- `pwsh scripts/ci-local.ps1` not run in this packet (fmt / clippy / tests were run
  individually; the Python doc/graph checks remain for the gate run).
- `PrintWindow` captures of a blinking cursor consistently sampled the hidden phase (each
  readback forces a synchronous redraw); the cursor was verified with `cursor.blink = false`
  in the debug config (`target/terminal.json`, restored afterwards) and through the
  diagnostics log.

## Handoff

Depends on US-0047 and US-0048. Blocks US-0050.

## Acceptance Rework (2026-09-09)

GUI parity walk (`evidence/US-0049-gui-walk.md`, 60 screenshots): 13 PASS, 2 PARTIAL
(`[--:--:--]` gutter fallback is a sub-second state; URL hover/IME/SSH banner/tab drag not
exercised on a locked workstation), 1 FAIL: closing the search bar (Esc, x, Ctrl+F) left the
terminal unfocused. The old view had the same gap. Fixed in `terminal_view/search.rs`
(`close_search` refocuses the view); regression test
`terminal_view::view_tests::closing_search_refocuses_the_terminal` fails without the fix.

## Acceptance Rework 2 (2026-09-09)

Two reports from the owner's second GUI pass (`evidence/05a-cursor-block-zoom.png`,
`evidence/06d-word-selection-zoom.png`, owner screenshot of the post-TUI screen).

**Bug 1 — block cursor and selection sit higher than the text.** Root cause:
`render/metrics.rs::measure` fed `TextSystem::descent` — a `FontMetrics` value that is
*negative* on DirectWrite (`gpui-pre-windows/src/direct_write.rs`, `descent: -(Base.descent)`)
and CoreText — into GPUI's `baseline_offset` formula. The old engine painted through
`ShapedLine::paint`, whose `paint_line` uses `LineLayout::descent` (*positive*: DirectWrite
`line height − baseline`), so the new baseline sat one descent (~4 px at 15 px Lilex) too low:
descenders hung below the cell, quads ended at the baseline, and the last row looked clipped by
the status bar. Fix: `measure` lays out a one-glyph probe line and takes `ascent`/`descent` from
its `LineLayout`; `CellMetrics::snapped` uses `descent.abs()`; the line-height floor is
unchanged, so cell heights and row counts are the same as before. Test:
`render::metrics::tests::baseline_centers_glyph_box_for_either_descent_sign`. Evidence:
`evidence/rework2-bug1-cursor-block-zoom.png` (block covers `g` with its descender, equal gaps
above and below); the per-scanline bright-pixel profile of the same terminal state rendered by
`main` and by this branch is identical (prompt row: all 22 scanlines equal), i.e. glyphs now land
where `ShapedLine::paint` put them.

**Bug 2 — stale fragment and cursor mid-screen after an alt-screen TUI exits following a
maximize.** Not a render-engine defect. A temporary per-frame snapshot dump showed the painted
rows equal the grid rows and the painted cursor equals the snapshot cursor; the stale text is
terminal state. On a grow-resize `alacritty_terminal::Grid::grow_lines` pulls the added rows from
scrollback and moves the cursor down (36 → 52 rows: cursor row 35 → 51) while ConPTY/conhost keeps
its viewport top and extends downward, then addresses later output with absolute `CUP` in its own
coordinates — raw PTY stream after the maximize: `ESC[34;65He ESC[34;66Hc …` while the grid's
prompt is on row 51 — so echoed input and prompt fragments land 18 rows above the prompt. The
old engine shows exactly the same picture: `evidence/rework2-bug2-resize-desync-{new,main}.png`
(same script, `echo desync-check` typed after the maximize lands inside the `dir` listing on
both). The owner's full sequence (opentui-examples, maximize during the TUI, Ctrl+C) rendered
clean on both builds here (`evidence/rework2-bug2-opentui-exit-{new,main}.png`); whether a
fragment appears depends on what conhost repaints after `?1049l`. Triage hypotheses (a)–(e)
were checked and hold: plans are keyed by the snapshot size and hash-verified on `Damage::Full`,
`Frame::row` slices densely, the element paints at most `plans.len()` rows, the cursor is
resolved from the snapshot every frame, the scrollbar offset is untouched by the alt screen. The
fix belongs to the terminal backend (`crates/terminal` / `crates/local-shell`: keep conhost and
the grid agreeing on the cursor row after a grow — do not pull scrollback for local ConPTY
sessions, or force a conhost repaint) and changes resize semantics, so it is left for an owner
decision; no render code changed.

Verification (branch tree, 2026-09-09): `cargo fmt --all` exit 0; `cargo clippy --workspace
--all-targets -- -D warnings` exit 0; `cargo clippy -p oneterm-terminal-view --all-targets
--features terminal-diagnostics -- -D warnings` exit 0; `cargo test -p oneterm-terminal-view`:
`test result: ok. 261 passed; 0 failed; 1 ignored`; `cargo test --workspace`: 46 suites ok,
1028 passed, 0 failed. `scripts/ci-local.ps1` not run in this rework (Python doc checks pending
for the gate run).

