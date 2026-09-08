# Work: TerminalView entity and the swap of the old view

ID: US-0049
Intake: IN-0018
Created: 2026-09-08

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [ ] In progress
- [ ] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
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

- [ ] In scope: `src/terminal_view/{mod, view, render, ime, search, scrollbar,
      gutter_timestamps, completion, agent_status}.rs` and tests; minimal edits in old
      `panel/*.rs`, `space/*.rs`, `agent.rs`, `status.rs` (type rename `LocalTerminalView →
      TerminalView`, `view::` → `terminal_view::` paths); `completion/` and `url/hover.rs`
      adaptations; `theme/` cleanup of old entry points; deletion of the five old module trees;
      `lib.rs` declarations.
- [ ] Out of scope: rewriting `panel/` and `space/` (US-0050); removing `#[allow(dead_code)]`
      from `render`/`input` roots (US-0050, once nothing old remains); behavior changes beyond the
      HLD deviations.

## Acceptance

- [ ] HLD parity items US-0049 (§2.9–2.17, 2.19–2.21) are implemented; each old `view/*`
      unit test listed in inventory §4 exists under `terminal_view/` with the same intent.
- [ ] `TerminalView` exposes exactly the pub(crate) surface in the HLD (`session`,
      `duplicate_config`, `split_ctx`, `focus`, `new`, `shutdown`, `toggle_search`,
      `handle_event`, `render_diagnostics`, `TerminalViewEvent::TitleChanged`, `TerminalDeps`).
- [ ] `phase0_renderer_baseline_counts_dirty_and_idle_frames` (view-level) passes on the new
      engine; `exited_behind_output_batch_marks_agent_ended` (panel test) still passes.
- [ ] `ssh_close_shows_banner_once`, `notification_queue_is_bounded`,
      `blink_tick_gated_by_focus_and_setting`, `ime_disabled_on_alt_screen`,
      `ime_bounds_at_cursor_cell`, `clipboard_read_reply_gated_by_setting` pass.
- [ ] `grep -rn "alacritty_terminal" crates/terminal-view/src` lists only `render/frame.rs`
      and `theme/palette.rs`.
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
- `docs/terminal-rendering-optimization.md`, `docs/terminal-gap-analysis.md` — historical; consulted for PERF/CORR tags only.

### Documentation Action

Update required: `docs/osc-sequences-checklist.md` only if a UI-visible OSC behavior changes
(none planned); `high-level-design.md` interfaces section if the pub(crate) surface changes.
Old rendering docs are marked historical in US-0050, not here.

Reason: this packet changes implementation, not the documented behavior; the HLD is the owning
contract and must stay accurate about the surface `panel/`/`space/` consume.

### Reconciliation

Before completion, confirm the HLD "Public and pub(crate) Interfaces" block matches
`terminal_view/view.rs` and list the deleted directories.

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

- [ ] `terminal_view/view.rs`: struct, `TerminalDeps`, `new` (events pump, blink task, focus
      subs, `set_nav`), `handle_event` table, `shutdown`, notification queue, progress, bell,
      `reply_clipboard_read`.
- [ ] `terminal_view/render.rs`: `RenderInputs` refresh (font cache, theme cache, palette push,
      metrics inputs, cursor config, gutter stamps copy, search highlights), wrapper div with
      listeners delegating to `input/`, `TerminalElementSpec`, bars/badges/banner/progress/
      completion overlay/scrollbar element, notification drain.
- [ ] `ime.rs`, `search.rs`, `scrollbar.rs`, `gutter_timestamps.rs`, `completion.rs`,
      `agent_status.rs` ported to the new types with their tests.
- [ ] Point `panel/`, `space/`, `agent.rs`, `status.rs` at `terminal_view::TerminalView`.
- [ ] Delete `src/view/`, `src/element/`, `src/layout/`, `src/box_drawing/`, `src/handlers/`;
      remove the old entry points left in `theme/`, `url/`, `highlight/`, `completion/`.
- [ ] Port `view/tests.rs` (3), per-file tests (search 7, scrollbar 7, gutter 7, local_view 3,
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
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

## Handoff

Depends on US-0047 and US-0048. Blocks US-0050.
