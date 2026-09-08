# Work: Input (keyboard classification, mouse state machine, wheel, edit, context menu)

ID: US-0048
Intake: IN-0018
Created: 2026-09-08

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [ ] In progress
- [x] Implemented
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

- [x] In scope: `src/input/{mod, keys, mouse, menu, edit}.rs` with sibling tests; small
      additions to `url/hover.rs` (`update_if_needed(cell, ctrl)` taking a `FrameRow` reader)
      if the current signature cannot be reused. `url/hover.rs` needed **no** change:
      `needs_detection(position, cell, ctrl)` + `set(..)` already is that contract.
- [x] Out of scope: `EntityInputHandler` (IME lives on `TerminalView`, US-0049), the search
      bar's own key handling, `TerminalPanel` action handlers, scrollbar geometry, deleting
      `handlers/`.

## Acceptance

- [x] HLD parity items US-0048 (§2.4 1–21 and key handoff, §2.5 1–15, §2.6, §2.7, §2.8) are
      implemented and covered by the tests below. §2.7.2 and §2.7.5–6 are carried by
      `send_key` / `interrupt` / the paste path (`scroll_to_bottom` before the write) and by
      the `KeyAction::Scroll*` variants the view applies.
- [x] `classify_key` table in `low-level-design/input.md` matches the code row for row; each
      row has a test.
- [x] Plain printable chars on the primary screen are `Ignore` (no PTY write) and on the alt
      screen are `Send`; Windows AltGr chords with `key_char` are `Ignore` under `cfg(windows)`.
- [x] `ctrl+c` → `Interrupt` (`send_ctrl_c`) regardless of selection; `ctrl+shift+c` → `Copy`.
- [x] Scrollbar drag is checked before selection on move; Middle button forwards to the session
      and never pastes; Right forwards only when `show_context_menu == false`.
- [x] Wheel uses `pixel_delta(line_height) / line_height * multiplier` with the `0.001`
      threshold.
- [x] Paste failures produce a Warning notification and a `log::warn!`; copy without a
      selection is a silent no-op.
- [x] Context menu order and guards match §2.6 (16 steps) — the order is reviewed against the
      LLD (no unit-test seam: `PopupMenu::menu_items` is private to `gpui-component`); the
      Close Space and Duplicate-destination guards are unit-tested.
- [ ] `pwsh scripts/ci-local.ps1` green — **not run** in this packet (outside its assignment);
      `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`
      and `cargo test --workspace` are green (see Evidence).

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

- [x] `input/mod.rs` (declared in `lib.rs` under `#[allow(dead_code, unused_imports)]` — the
      re-exported entry points have no caller until US-0049).
- [x] `keys.rs`: `KeyAction`, `KeyContext`, `map_key`, `classify_key` with the completion
      interception first, plus `send_key` / `interrupt`; tests per table row.
- [x] `mouse.rs`: `MouseState`, `Drag`, `selection_type`, modifier/button conversion, `down`/
      `moved`/`up`/`wheel`/`modifiers_changed`/`exit` returning `MouseOutcome` (session calls
      performed inside, URL open and copy returned to the caller as outcomes); tests with
      `FakeTerminalSession` probe writes and recorded input calls.
- [x] `edit.rs`: four commands + paste error handling; tests.
- [x] `menu.rs`: `build_menu(menu, &MenuContext, window, cx)`; guards tested.
- [ ] `pwsh scripts/ci-local.ps1` — deferred to the intake's integration packet.

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
- [x] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

### Evidence

Run on `refactor/terminal-render-engine` (Windows 11); nothing committed.

```
cargo fmt --all -- --check
(no output, exit 0)

cargo clippy -p oneterm-terminal-view --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.14s

cargo test -p oneterm-terminal-view input
test result: ok. 42 passed; 0 failed; 0 ignored; 0 measured; 275 filtered out; finished in 0.03s

cargo test -p oneterm-terminal-view
running 317 tests
test result: ok. 316 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.10s

cargo clippy --workspace --all-targets -- -D warnings
(no diagnostics)

cargo test --workspace
44 suites, every one `test result: ok`; 1083 passed, 4 ignored
```

Files added under `crates/terminal-view/src/input/`: `mod.rs` 29, `keys.rs` 322,
`keys_tests.rs` 382, `mouse.rs` 380, `mouse_tests.rs` 493, `menu.rs` 361, `menu_tests.rs` 38,
`edit.rs` 75, `edit_tests.rs` 93 lines. Also changed: `crates/terminal-view/src/lib.rs`
(module declaration) and `crates/terminal/src/test_support.rs` (additive probe extension).

Tests added: every name in the LLD "Verification" list for `keys_tests`, `mouse_tests` and
`edit_tests`, plus `keys_tests::unmapped_chord_writes_nothing`,
`mouse_tests::{buttons_and_modifiers_convert, modifiers_changed_redetects_at_the_last_cell,
selection_drag_continues_outside_the_grid}` and
`menu_tests::{close_space_only_with_siblings, duplicate_destination_labels}`.

### Gaps

- `menu_tests::{menu_item_order, log_submenu_only_with_logging_capability,
  duplicate_submenu_only_inside_space}` from the verification plan were not written:
  `PopupMenu::menu_items` is `pub(crate)` inside `gpui-component`, so a built menu cannot be
  inspected from this crate. The guards behind those items are unit-tested instead
  (`can_close_space`, `duplicate_label`) and the order is reviewed against the LLD. Closing
  this needs an upstream accessor or a UI-level test in US-0049.
- `pwsh scripts/ci-local.ps1` was not run (excluded from this packet's assignment).
- `crates/terminal/src/test_support.rs` gained `FakeInputCall`,
  `FakeSessionProbe::{input_calls, take_input_calls, set_selection}`, recording
  `TerminalInput` implementations and a selection-backed `selection_text` / `has_selection`.
  The fake previously captured byte writes only, so "assert what reached the session" was
  impossible for mouse, wheel and viewport calls. The change is additive; the one behavior
  difference is that `clear_selection` now clears the stored selection.
- The module is dead code until US-0049 wires it: nothing here is reachable from a running app.

## Reconciliation Result

`low-level-design/input.md` was amended — a new "Amendments (US-0048 implementation)" section
plus updated `KeyAction` / `KeyContext` / `MouseState` listings and rows 12–13. The
`classify_key` match arms run in the table's order: row 0 completion interception, 1 `P+f`,
2 Enter in search, 3 `ctrl+shift+space`, 4–6 zoom, 7–9 scroll, 10–11 copy/paste and
`shift+insert`, 12 plain printable, 13 AltGr, 14 `ctrl+c`, 15 `map_key` → `Send`,
16 `Unhandled`.

## Handoff

Depends on US-0047 (`GridGeometry`, `RenderState`). Blocks US-0049.
