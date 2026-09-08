# Work: Input (keyboard classification, mouse state machine, wheel, edit, context menu)

ID: US-0048
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

- Change type: new capability (new `input/` module beside the old `handlers/`; wired by US-0049)
- Risk lane: normal
- Spec Intake: IN-0018

## Outcome

`src/input/` contains pure, unit-tested key classification (`classify_key`, `map_key`,
completion interception order), the mouse state machine (`MouseState`, selection type, URL
ctrl-click, copy-on-select, scrollbar drag precedence, mouse-mode passthrough), wheel math,
the edit commands with paste-error notification, and the context-menu builder — all consuming
`GridGeometry` from `render/metrics.rs` and the `TerminalSession` trait only. It is not yet
attached to a view.

## Scope

- [ ] In scope: `src/input/{mod, keys, mouse, menu, edit}.rs` with sibling tests; small
      additions to `url/hover.rs` (`update_if_needed(cell, ctrl)` taking a `FrameRow` reader)
      if the current signature cannot be reused.
- [ ] Out of scope: `EntityInputHandler` (IME lives on `TerminalView`, US-0049), the search
      bar's own key handling, `TerminalPanel` action handlers, scrollbar geometry, deleting
      `handlers/`.

## Acceptance

- [ ] HLD parity items US-0048 (§2.4 1–21 and key handoff, §2.5 1–15, §2.6, §2.7, §2.8) are
      implemented and covered by the tests below.
- [ ] `classify_key` table in `low-level-design/input.md` matches the code row for row; each
      row has a test.
- [ ] Plain printable chars on the primary screen are `Ignore` (no PTY write) and on the alt
      screen are `Send`; Windows AltGr chords with `key_char` are `Ignore` under `cfg(windows)`.
- [ ] `ctrl+c` → `Interrupt` (`send_ctrl_c`) regardless of selection; `ctrl+shift+c` → `Copy`.
- [ ] Scrollbar drag is checked before selection on move; Middle button forwards to the session
      and never pastes; Right forwards only when `show_context_menu == false`.
- [ ] Wheel uses `pixel_delta(line_height) / line_height * multiplier` with the `0.001`
      threshold.
- [ ] Paste failures produce a Warning notification and a `log::warn!`; copy without a
      selection is a silent no-op.
- [ ] Context menu order and guards match §2.6 (16 steps).
- [ ] `pwsh scripts/ci-local.ps1` green.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0018-rebuild-terminal-render-engine/high-level-design.md` — data flow step 7, interfaces, parity US-0048.
- `docs/spec-intakes/IN-0018-rebuild-terminal-render-engine/low-level-design/input.md` — the contract this packet implements.
- `docs/spec-intakes/IN-0018-rebuild-terminal-render-engine/low-level-design/render-pipeline.md` — `GridGeometry` hit-test contract.
- `docs/PROJECT.md` — terminal-controlled input and paste are sanitised in `crates/terminal` (the view calls `paste`, never encodes itself).
- `docs/architecture.md` — boundaries.
- `docs/agents/error-policy.md` — ERR-04 style notification on paste rejection; `report_generated_input` for key writes.
- `docs/terminal-split/04-context-menu.md` — historical description of the menu items (cross-checked against inventory §2.6).

### Documentation Action

No contract change: `input.md` describes the target behavior; amend its tables only if a chord
or guard must differ (record why).

Reason: behavior is parity with the inventory; the LLD already encodes it.

### Reconciliation

Before completion, confirm `input.md` classification table rows equal the `classify_key`
match arms in order.

## Context

- `Keystroke.key_char` is the literal text; `key` + `modifiers` map to control sequences;
  `KeyDownEvent.prefer_character_input` flags AltGr cases.
- `TerminalInput::mouse_*` already branch on mouse mode; the view forwards unconditionally.
- `url::detect_url_at` and `url_policy::validate_target_with_display` gate URL opening; the
  Confirm dialog is `gpui_component` modal built by the view (the mouse module returns an
  `UrlOpen { url, decision }` for the view to act on).
- `edit::EditCommand` is the fn-pointer type reused by `TerminalPanel` action handlers.
- Old `handlers/keyboard.rs` `classify_key` tests (Ctrl+C, unknown named key, plain char,
  Ctrl+Space, Ctrl+PrintScreen, Alt+C, Ctrl+Shift+C) are re-expressed against the new function.

## Plan

- [ ] `input/mod.rs` (declared in `lib.rs` under `#[allow(dead_code)]`).
- [ ] `keys.rs`: `KeyAction`, `KeyContext`, `map_key`, `classify_key` with the completion
      interception first; tests per table row.
- [ ] `mouse.rs`: `MouseState`, `Drag`, `selection_type`, modifier/button conversion, `down`/
      `move`/`up`/`wheel`/`modifiers_changed`/`exit` returning `MouseOutcome` (session calls
      performed inside, URL open and copy returned to the caller as outcomes); tests with
      `FakeTerminalSession` probe writes.
- [ ] `edit.rs`: four commands + paste error handling; tests.
- [ ] `menu.rs`: `build_menu`; test the item order and guards with a lightweight fake of the
      split context (leaf count, empty destinations, logging capability).
- [ ] `pwsh scripts/ci-local.ps1`.

## Decisions

- `docs/decisions/DEC-0007-terminal-render-engine-cell-glyph-cache-and-quad-shapes.md` (no new decision; input keeps parity).

## Verification Plan

- Focused: `cargo test -p oneterm-terminal-view input` — `keys_tests::{ctrl_f_toggles_search,
  enter_in_search_is_swallowed, ctrl_shift_space_triggers_completion, zoom_chords,
  shift_page_and_home_end_scroll, platform_shift_arrows_scroll_one_line,
  copy_paste_chords_per_platform, shift_insert_pastes, plain_char_on_primary_screen_is_ignored,
  plain_char_on_alt_screen_is_sent, altgr_char_on_windows_is_ignored, ctrl_c_interrupts,
  ctrl_space_encodes_nul, unknown_named_key_is_unhandled, alt_c_sends_escape_prefix,
  ctrl_shift_c_is_copy_not_send, completion_navigation_forwards_until_selected,
  completion_tab_select_then_accept, completion_tab_forwards_when_disabled}`,
  `mouse_tests::{click_count_selects_type, alt_click_is_block, ctrl_click_on_url_opens_not_selects,
  middle_click_forwards_to_session, right_click_forwards_when_menu_disabled,
  copy_on_select_on_left_up, scrollbar_drag_precedes_selection,
  wheel_delta_uses_multiplier_and_threshold, hover_redetects_only_on_cell_or_ctrl_change,
  pixel_to_grid_fractional_and_bounds, events_before_first_paint_are_ignored}`,
  `edit_tests::{copy_noop_without_selection, paste_scrolls_to_bottom_then_pastes,
  paste_too_large_notifies}`, `menu_tests::{menu_item_order, close_space_only_with_siblings,
  log_submenu_only_with_logging_capability, duplicate_submenu_only_inside_space}`.
- Regression: `cargo test --workspace`.
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

Depends on US-0047 (`GridGeometry`, `RenderState`). Blocks US-0049.
