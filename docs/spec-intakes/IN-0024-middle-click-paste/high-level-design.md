# High-Level Design: Middle-click paste

Intake: IN-0024
Lane: normal
Date: 2026-09-11

## Idea

The mouse state machine in `crates/terminal-view/src/input/mouse.rs` already turns presses
into outcomes the view executes. A middle press becomes a new `MouseOutcome::Paste` when the
`middle_click_paste` setting is on and the program has not claimed the mouse (mouse mode off,
or Shift held); the view runs the same `paste_clipboard` the Paste action uses. The setting is
one more boolean in the `mouse` group of `terminal.json`, mirrored into the live
`TerminalSettings` and shown as a switch next to "Copy on Select".

## Diagram

```text
 MouseDownEvent(Middle) ─▶ MouseState::down(event, session, inputs)
                              │ inputs.middle_click_paste (terminal.json mouse.middle_click_paste)
                              │ session.is_mouse_mode()  && !shift  ─▶ forward press to program (as today)
                              └─ else ─▶ MouseOutcome::Paste ─▶ view: paste_clipboard(session, origin)
                                                                       (sanitise, bracketed, fan-out, toast)
```

## UI Wireframe

Settings › Terminal › Mouse:

```text
+-- Mouse -----------------------------------------------------+
| Right-Click Context Menu   Show OneTerm right-click menu.  [on]  |
| Copy on Select             Copy the selection ... released. [on] |
| Middle-Click Paste         Paste the clipboard on middle click. [on] |
+--------------------------------------------------------------+
```

## Data Flow

1. `MouseConfig.middle_click_paste` (default `true`) loads from `terminal.json`; `apply.rs`
   copies it into `TerminalSettings.middle_click_paste`; `persist.rs` writes it back.
2. `TerminalView::mouse_inputs` copies the live value into `MouseInputs.middle_click_paste`.
3. `MouseState::down`: after the scrollbar and context-menu checks and the URL check, a
   `Middle` press with the setting on returns `MouseOutcome::Paste` unless
   `session.is_mouse_mode()` and Shift is not held, in which case the press is forwarded to
   the program exactly as today.
4. `apply_mouse_outcome` handles `Paste` by calling `paste_clipboard(&session, &origin, ..)`.
5. `TerminalSession::is_mouse_mode` is added beside `is_alt_screen`: the shared backend macro
   reads `TermMode::MOUSE_MODE` from the model, the test fake from its mode cell.

## Detail Design

- [ ] Detail design: not needed
- Reason: one boolean and one outcome variant on existing seams.
