# Low-Level Design: Input (keyboard, IME, mouse, wheel, paste, scroll, hit-testing)

Intake: IN-0018
HLD: ../high-level-design.md
Topic: input
Date: 2026-09-08

> One concern per file. Keep this focused on implementation-level mechanics for a single area of the HLD so it stays reviewable. Do not restate the whole intake here.

## Concern

`src/input/{keys, mouse, menu, edit}.rs` and `src/terminal_view/ime.rs`: how GPUI events reach
`TerminalSession`, in what order the interceptors run, and the single coordinate contract
(`GridGeometry`) shared with the render element.

## Design

### Hit-testing contract

`RenderState.geometry: Option<GridGeometry>` is written by `TerminalElement::paint` and read by
every mouse handler on the next event. `pixel_to_grid(p) -> Option<(row: f32, col: f32)>` returns
fractional coordinates (`(p.y - origin.y) / line_height`, `(p.x - origin.x) / cell_width`),
`None` before the first paint or when `p` is outside `[origin, origin + size * cell)`. The
backend decides the cell and the side of a cell. `cell_at` floors and clamps for URL hover and
the scrollbar. Handlers are registered on the wrapper `div` in `terminal_view/render.rs`
(`track_focus(&focus)`, `on_key_down`, `on_modifiers_changed`, `on_mouse_down/up/move`,
`on_scroll_wheel`, `context_menu` when `show_context_menu`), so GPUI's hitbox of the wrapper
gates them; the element itself registers no listeners.

### Keyboard (`keys.rs`)

```rust
pub(crate) enum KeyAction {
    ToggleSearch, SwallowInSearch, TriggerCompletion, Completion(CompletionKey),
    ZoomIn, ZoomOut, ZoomReset,
    ScrollLines(i32), ScrollPages(i32), ScrollTop, ScrollBottom,
    Copy, Paste, Interrupt, Send(KeySpec, KeyMods), Ignore, Unhandled,
}
pub(crate) enum CompletionKey { SelectFirst, SelectNext, SelectPrev, Accept, Dismiss }
pub(crate) struct KeyContext {
    search_focused: bool, alt_screen: bool,
    completion_visible: bool, completion_selected: bool, completion_accept_tab: bool,
}
pub(crate) fn classify_key(ks: &Keystroke, prefer_char: bool, ctx: KeyContext) -> KeyAction;
pub(crate) fn map_key(ks: &Keystroke) -> Option<(KeySpec, KeyMods)>;
pub(crate) fn send_key(session, spec: &KeySpec, mods: KeyMods, app_cursor: bool, cx) -> bool;
pub(crate) fn interrupt(session, cx);
```

`P` = platform modifier (`cmd` on macOS, `ctrl` elsewhere via `Modifiers::secondary()`);
`plain` = no ctrl/alt/platform. Order is significant; the first matching row wins:

| # | Chord | Action | Notes |
| --- | --- | --- | --- |
| 0 | completion overlay visible | interception (below) | before every row |
| 1 | `P+f` | `ToggleSearch` | stop propagation |
| 2 | `enter` while `search_focused` | `SwallowInSearch` | stop propagation; bar handles Enter |
| 3 | `ctrl+shift+space` | `TriggerCompletion` | |
| 4 | `P+-` | `ZoomOut` | |
| 5 | `P+=` / `P++` | `ZoomIn` | |
| 6 | `P+0` | `ZoomReset` | |
| 7 | `shift+pageup` / `shift+pagedown` | `ScrollPages(+1 / −1)` | |
| 8 | `shift+home` / `shift+end` | `ScrollTop` / `ScrollBottom` | |
| 9 | `P+shift+up` / `P+shift+down` | `ScrollLines(+1 / −1)` | |
| 10 | `ctrl+shift+c` (non-mac) / `cmd+c` (mac) | `Copy` | |
| 11 | `ctrl+shift+v` (non-mac) / `cmd+v` (mac) / `shift+insert` | `Paste` | |
| 12 | printable `key_char`, `plain`, `!alt_screen` | `Ignore` | IME path delivers it (`replace_text_in_range`); no stop_propagation |
| 13 | `cfg!(windows) \|\| prefer_char`, ctrl+alt, printable `key_char` (AltGr) | `Ignore` | WM_CHAR delivers it |
| 14 | `ctrl+c` (no shift) | `Interrupt` | `send_ctrl_c`, scroll to bottom, regardless of selection |
| 15 | `map_key(ks) == Some(..)` | `Send(spec, mods)` | `encode_key(spec, mods, app_cursor)`, write, scroll to bottom, clear bell |
| 16 | otherwise | `Unhandled` | no stop_propagation |

`map_key`: named keys (`enter backspace delete tab escape up down left right home end pageup
pagedown insert f1..f24`) → `KeySpec::Named`; `"space"` → `Character(" ")` so `ctrl+space`
encodes NUL; else `key_char` if present, else the single-char `key`; multi-char unknown names
without modifiers → `None`. `KeyMods { shift, ctrl, alt }` copy from `Modifiers`.

Completion interception (row 0), only while the overlay is visible: `down`/`ctrl+n` → next
**if something is selected**, else fall through; `up`/`ctrl+p` → prev likewise; `escape` →
dismiss; `enter` → accept if selected (write `accept_bytes`, dismiss) else fall through (and
history capture happens in the fall-through Send path); `tab` when `accept_tab` → select first
if nothing selected, else accept; `tab` when `!accept_tab` → fall through; anything else falls
through. Accept and Interrupt bypass `encode_key`.

Send path: `session.scroll_to_bottom(); if let Some(bytes) = encode_key(&spec, mods,
frame_app_cursor) { report_generated_input("key", session.write(&bytes)) }`; `has_bell = false`;
scrollbar `mark_scrolled`; `cx.notify()`.

### IME (`terminal_view/ime.rs`, `EntityInputHandler for TerminalView`)

| Method | Behavior |
| --- | --- |
| `accepts_text_input` / `selected_text_range` | `false` / `None` when `session.is_alt_screen()`; else `Some(0..0)` (empty document) |
| `marked_text_range` | `session.marked_text().map(|t| 0..t.len())` |
| `unmark_text` | `session.clear_marked_text()` |
| `replace_text_in_range(_, text)` | `scroll_to_bottom(); session.commit_text(text); has_bell = false; notify` |
| `replace_and_mark_text_in_range(_, text, _)` | empty → `clear_marked_text`, else `set_marked_text(text)` |
| `bounds_for_range(range)` | cursor cell from `query_state()` (`cursor_line`, `cursor_col`), offset by `range.start` columns, sized by `GridGeometry`; `None` before the first paint |
| `text_for_range`, `character_index_for_point` | `None` |

The handler is installed by the element in paint (`spec.ime` closure) only while `focus` is
focused; `on_key_down` row 12 keeps plain chars out of the PTY so the IME path never double
types. Alt-screen apps get no composition (kept from inventory wart 8).

### Mouse (`mouse.rs`)

```rust
pub(crate) enum Drag { None, Selecting, Scrollbar }
pub(crate) struct MouseState { drag: Drag }          // last cell / ctrl live in `UrlHover`
pub(crate) struct MouseInputs {                      // what the view knows, per event
    geometry: Option<GridGeometry>, over_scrollbar: bool,
    show_context_menu: bool, copy_on_select: bool, scroll_multiplier: f32,
}
pub(crate) enum MouseOutcome {
    Ignored,                                  // did not reach the grid — no repaint
    Handled,                                  // mark_scrolled + notify
    ScrollbarDrag { track_y: f32 },           // view applies `scrollbar.drag_to`
    OpenUrl(UrlOpen),                         // { url: DetectedUrl, decision: TargetDecision }
    CopySelection,                            // view runs `edit::copy_selection`
}
// down / moved / up / wheel take (&event, &session, &MouseInputs, [&mut UrlHover], &mut App).
pub(crate) fn selection_type(click_count: usize, alt: bool) -> SelectionType; // Alt → Block; 1 Simple, 2 Semantic, ≥3 Lines
pub(crate) fn to_mods(m: &Modifiers) -> MouseModifiers; pub(crate) fn to_button(b: MouseButton) -> Option<TerminalMouseButton>;
pub(crate) fn wheel_lines(delta: ScrollDelta, line_height: Pixels, multiplier: f32) -> f32;
```

State machine (all transitions end with `scrollbar.mark_scrolled(); cx.notify()`):

| Event | Guard | Action |
| --- | --- | --- |
| down Left | over scrollbar thumb/track | `drag = Scrollbar`; `scrollbar.drag_to(track_y)`; stop propagation |
| down Left | `(ctrl \|\| platform)` and `url::detect_url_at(cell)` is `Some` | `open_url(policy)`; no selection |
| down Left | `pixel_to_grid` is `Some` | `session.mouse_down(row, col, Left, selection_type(click_count, alt), mods)`; `drag = Selecting` |
| down Middle | grid | `session.mouse_down(.., Middle, Simple, mods)` (mouse-mode passthrough; no paste) |
| down Right | `show_context_menu` | handled by `context_menu` on the wrapper; not forwarded |
| down Right | `!show_context_menu` | `session.mouse_down(.., Right, ..)` |
| move | `drag == Scrollbar` | `scrollbar.drag_to(track_y)`; return |
| move | `pressed_button == Some(Left)` and `drag == Selecting` | `session.mouse_drag(row, col, mods)` |
| move | otherwise, grid | `session.mouse_move(row, col, mods)` |
| move | always | `url_hover.update_if_needed(cell, ctrl)` (re-detects only when the cell or ctrl changed); pointer cursor while hovering |
| modifiers changed | | `url_hover.update_if_needed(last_cell, ctrl)` |
| up any | `drag == Scrollbar` | `drag = None` |
| up Left/Right/Middle | grid | `session.mouse_up(row, col, button, mods)`; if Left and `copy_on_select` and `session.has_selection()` → `edit::copy_selection`; `drag = None` |
| exit | | `url_hover.clear()` |

URL open: `validate_target_with_display(url, display) → Allow: cx.open_url; Confirm(reason):
confirm dialog then open; Deny(reason): log::info!`.

### Wheel

```rust
let px_delta = event.delta.pixel_delta(line_height).y;            // Lines → lines * line_height
let lines = f32::from(px_delta) / f32::from(line_height) * settings.scroll_multiplier;
if lines.abs() >= 0.001 { session.wheel(lines as f64, row, col, mods) }   // backend: mouse mode / alt-scroll / scrollback
```

Only fires when the wheel position maps into the grid; alt-scroll arrow emulation is backend
side (`alternate_scroll` setting is read by the session).

### Scroll chords and scrollbar

`ScrollPages(n)` → `session.scroll(n * rows)`; `ScrollLines(n)` → `session.scroll(n)`;
`ScrollTop`/`ScrollBottom` → `scroll_to_top()`/`scroll_to_bottom()`. Scrollbar drag maps track
y to a display-offset fraction and stores `pending_offset`; the next `render` applies it as
`session.scroll(delta)` against the freshly read `terminal_info()`. Alt-screen no-ops are
backend side.

### Edit and paste (`edit.rs`)

```rust
pub(crate) type EditCommand = fn(&Entity<Box<dyn TerminalSession>>, &mut Window, &mut App);
pub(crate) fn copy_selection(..)  // selection_text() non-empty → write_to_clipboard; else silent
pub(crate) fn paste_clipboard(..) // read_from_clipboard text → scroll_to_bottom → session.paste(&text);
                                  // Err(PasteError) → log::warn + Warning toast (ERR-04); bracketed/plain chosen by the backend
pub(crate) fn select_all(..)      // session.select_all()
pub(crate) fn clear_screen(..)    // session.clear()
```

Reached identically from keys (rows 10–11 and the panel actions), the context menu, and
`TerminalPanel`'s `on_action` handlers.

### Context menu (`menu.rs`)

`build_menu(menu: PopupMenu, ctx: &MenuContext, window, cx) -> PopupMenu` produces, in order: New
Terminal (`AddPanel`); Duplicate Session submenu (only with `split_ctx`: In New Tab, Into Space #N
per empty destination, separator, Split Right, Split Down); separator (with `split_ctx`); Split
Right/Left/Up/Down; separator; Copy (disabled without selection); Paste; Select All; Clear;
separator; Log submenu (only when `capabilities().logging` is `Some`: Start disabled while
running, Stop disabled while not; async with a notification), separator; Close Terminal Tab
(`ClosePanel`); Close Space (`CloseSpace`, only when `leaf_count() > 1`). The wrapper div gets a
stable `ElementId` so the menu state survives re-render.

## Interfaces

Listed inline above; the view-facing calls are `classify_key`, `map_key`, `MouseState::{down,
move, up, wheel}` (taking `&GridGeometry`, `&ScrollbarState`, `&UrlHover`, settings flags),
`edit::*`, and `menu::build_menu`.

## Edge Cases and Failure Modes

- [ ] Event before the first paint (`geometry == None`) → mouse events are ignored; keys still work.
- [ ] Mouse leaves the grid while selecting → `mouse_drag` keeps receiving clamped fractional
      coordinates (backend clamps); selection continues until up.
- [ ] Scrollbar drag then release outside the window → next `up` of any button ends the drag.
- [ ] `click_count` accumulated by search-bar clicks → the bar stops left-click propagation.
- [ ] `key_char` absent for a printable (`cmd+s`) → row 12 does not match; falls to `map_key`.
- [ ] `encode_key` returns `None` (ctrl + non-ASCII) → nothing written, no error.
- [ ] Clipboard empty or non-text on paste → silent no-op.
- [ ] Paste larger than the policy → `PasteError::TooLarge` toast, nothing written.
- [ ] URL under Ctrl-click fails policy → logged, no selection started.
- [ ] Completion visible on the alt screen → dismissed before classification.

## Verification

- [ ] `keys_tests`: `ctrl_f_toggles_search`, `enter_in_search_is_swallowed`,
      `ctrl_shift_space_triggers_completion`, `zoom_chords`, `shift_page_and_home_end_scroll`,
      `platform_shift_arrows_scroll_one_line`, `copy_paste_chords_per_platform`,
      `shift_insert_pastes`, `plain_char_on_primary_screen_is_ignored`,
      `plain_char_on_alt_screen_is_sent`, `altgr_char_on_windows_is_ignored`,
      `ctrl_c_interrupts`, `ctrl_space_encodes_nul`, `unknown_named_key_is_unhandled`,
      `alt_c_sends_escape_prefix`, `ctrl_shift_c_is_copy_not_send`,
      `completion_navigation_forwards_until_selected`, `completion_tab_select_then_accept`,
      `completion_tab_forwards_when_disabled`.
- [ ] `mouse_tests`: `click_count_selects_type`, `alt_click_is_block`,
      `ctrl_click_on_url_opens_not_selects`, `middle_click_forwards_to_session`,
      `right_click_forwards_when_menu_disabled`, `copy_on_select_on_left_up`,
      `scrollbar_drag_precedes_selection`, `wheel_delta_uses_multiplier_and_threshold`,
      `hover_redetects_only_on_cell_or_ctrl_change`, `pixel_to_grid_fractional_and_bounds`,
      `events_before_first_paint_are_ignored`.
- [ ] `edit_tests`: `copy_noop_without_selection`, `paste_scrolls_to_bottom_then_pastes`,
      `paste_too_large_notifies`.
- [ ] `menu_tests`: `close_space_only_with_siblings`, `duplicate_destination_labels`.
      `PopupMenu::menu_items` is `pub(crate)` to `gpui-component`, so the 16-step order
      itself has no unit-test seam and is reviewed against this document.
- [ ] `ime_tests` (US-0049): `ime_disabled_on_alt_screen`, `ime_commit_scrolls_and_clears_bell`,
      `ime_bounds_at_cursor_cell`.

## Amendments (US-0048 implementation)

Recorded when the implementation had to differ from the text above; the tables and
signatures already carry the change.

1. **`KeyContext` carries the completion selection and `accept_tab`.** Row 0's rules are
   stated in terms of "if something is selected" and "when `accept_tab`", so the pure
   `classify_key` needs both facts. Row 0's outcome is `KeyAction::Completion(CompletionKey)`.
2. **Row 12 drops "single".** `key_char` is treated as layout text when it is non-empty and
   contains no control character (parity with the old `handlers/keyboard.rs`), so a
   dead-key composition of more than one `char` is also left to the IME path.
3. **Row 13 also fires on `prefer_char`.** GPUI sets `KeyDownEvent::prefer_character_input`
   exactly for the AltGr case, so the parameter is honoured in addition to `cfg!(windows)`.
4. **`Drag::Scrollbar` carries no `grab_offset`.** `ScrollbarState::drag_to(track_y)` maps
   the track position to a display offset by itself; there is no second anchor to keep.
5. **`MouseState` does not duplicate `last_cell` / `last_ctrl`.** `UrlHover` already owns
   them and already implements the cell-granular re-detect rule
   (`needs_detection(position, cell, ctrl)` + `set(..)`), which is what
   `update_if_needed` describes; no new entry point was added to `url/hover.rs`.
6. **Scrollbar hit-testing is an input, not a lookup.** The view answers
   `MouseInputs::over_scrollbar` and applies `MouseOutcome::ScrollbarDrag { track_y }`;
   the mouse module never sees `ScrollbarState`, which `US-0049` owns.
7. **URL opening is returned, not performed.** `MouseOutcome::OpenUrl` carries the
   `DetectedUrl` and the `validate_target_with_display` decision; the view opens, shows the
   confirmation dialog, or logs the denial, because all three need a `Window`.
8. **`oneterm_terminal::test_support` was extended** (additively) with
   `FakeInputCall`, `FakeSessionProbe::{input_calls, take_input_calls, set_selection}` and a
   settable `selection_text` / `has_selection`, so the mouse and edit tests can assert what
   reached the session. The fake previously recorded byte writes only.
