# Low-Level Design: platform key event to `KeyEvent`

Intake: [`IN-0040`](../IN-0040.md)
HLD: [`../high-level-design.md`](../high-level-design.md)
Topic: input-events
Date: 2026-09-16

> One concern: which GPUI keyboard event becomes which `oneterm_vt::input::KeyEvent`, what the view
> must remember between the two, and the five things that must never happen. The encoder this feeds
> is designed in
> [`IN-0039/low-level-design/kitty-keyboard.md`](../../IN-0039-vt-gaps-and-publish/low-level-design/kitty-keyboard.md)
> and is not restated here.

## Concern

`crates/terminal-view/src/input/keys.rs` and `crates/terminal-view/src/terminal_view/input.rs`:
the boundary where a `gpui::KeyDownEvent` or `gpui::KeyUpEvent` becomes the `(event, mode
snapshot)` pair that clause 6 of `oneterm-vt`'s semver promise calls a contract.

## Design

### The shape change

`KeyAction::Send` carries a `KeyEvent` instead of a `(KeySpec, KeyMods)` pair. Everything else in
the classification table is untouched, including the order of its rows, which
[`IN-0018/low-level-design/input.md`](../../IN-0018-rebuild-terminal-render-engine/low-level-design/input.md)
owns.

```rust
// crates/terminal-view/src/input/keys.rs
pub(crate) enum KeyAction {
    // ...
    /// Encode and write to the PTY (see [`send_key`]).
    Send(KeyEvent),
    // ...
}

/// Classify a key-down. `kind` is `Repeat` when GPUI reports the key held.
pub(crate) fn classify_key(
    ks: &Keystroke,
    prefer_char: bool,
    kind: KeyEventKind,
    ctx: KeyContext,
) -> KeyAction;

/// Map a GPUI [`Keystroke`] to a [`KeyEvent`] of the given kind.
pub(crate) fn map_key(ks: &Keystroke, kind: KeyEventKind) -> Option<KeyEvent>;

pub(crate) fn send_key(
    session: &Entity<Box<dyn TerminalSession>>,
    event: &KeyEvent,
    modes: &ModeSnapshot,
    cx: &mut App,
) -> Option<Vec<u8>>;
```

`map_key`'s body is unchanged except for its tail, which now builds the event rather than a tuple:

```rust
let mut event = KeyEvent::new(spec, key_mods);
event.kind = kind;
// Only a press or a repeat carries text, and only when the modifiers would
// have let that text reach the program: `Ctrl+A` produces `0x01`, and claiming
// an `a` here would tell the program it inserted one. That is the engine's own
// rule for its fallback, applied to the text this side supplies so the two
// halves agree rather than one cleaning up after the other.
if kind != KeyEventKind::Release && !(key_mods.ctrl || key_mods.alt) {
    event.text = ks.key_char.clone().filter(|text| !text.is_empty());
}
Some(event)
```

The modifier test is `US-0108`'s finding `F5`: the first draft supplied the text for every press,
which is inert on Windows (a control `key_char` never survives `process_key`) and wrong on a
backend that reports a printable one for a ctrl chord.

`KeyEvent` is `#[non_exhaustive]`, so it is built with `KeyEvent::new` and then assigned -- which
is what its own documentation and guide chapter 12 both prescribe.

`shifted` and `base_layout` stay `None`. GPUI's `Keystroke` is `{ modifiers, key, key_char }` and
carries neither, so `REPORT_ALTERNATE_KEYS` remains inert and the encoder omits the two sub-fields.
That is a ceiling stated here so that a reader does not look for the assignment.

`send_key` loses two parameters and gains one; its body is otherwise the same three lines, with
`encode_key(spec, mods, modes)` becoming `encode_key_event(event, modes)`.

### The three re-exports

`crates/terminal-view` has no `oneterm-vt` dependency and reaches the encoder through
`oneterm-terminal`'s shim. Three names join the existing `pub use oneterm_vt::input::{...}` block
in `crates/terminal/src/lib.rs`: `KeyEvent`, `KeyEventKind`, `encode_key_event`. The block's
comment already says it is a one-release convenience and that consumers should name the types from
`oneterm_vt::input`; the alternative -- adding `oneterm-vt` to `crates/terminal-view`'s manifest --
is the cleaner end state, is permitted by R1-R12, and is a manifest change in a `high_risk` packet
whose subject is not the crate graph. Three names now; the shim's removal gets one more consumer to
migrate, and that is a cheaper debt than a dependency edge added as a side effect.

### The held set

One field on `TerminalView`, beside the `focused` flag it already keeps:

```rust
/// Keys whose press actually reached the PTY, in the canonical form
/// `canonical_key` produces.
///
/// A release is sent only for a key in here, which is what keeps a chord the
/// view swallowed (zoom, copy, the completion overlay, a dead key, a printable
/// key the IME owns) from producing a release the program never saw a press
/// for. Drained on blur so a window the user left cannot strand a held key.
held_keys: Vec<KeySpec>,
```

Bounded by the number of physically held keys -- a handful -- so no eviction policy and no cap,
and a linear scan rather than a `HashSet` (`KeySpec` is not `Hash`, and hashing a handful would
buy nothing).

**Not** keyed by `Keystroke::key`. That was this design's first answer and `US-0108`'s verification
found it wrong (`F1`): the Windows backend resolves a digit or an OEM punctuation key to its
**shifted** glyph while Shift is down and clears `shift` from the modifiers
(`get_keystroke_key` / `need_to_convert_to_shifted_key`), so `Shift+1` arrives as `key: "!"`, and a
user who lifts Shift before the digit produces a key-up named `"1"`. A set keyed on the raw name
misses, no release is written, and the program is left believing `!` is held. Letters are stable;
digits and `VK_OEM_*` are not.

The key is therefore the [`KeySpec`] `map_key` produces, canonicalized: a `Named` key as-is (which
also pairs `"enter"` with `"return"`), and a `Character` key folded through the same PC-101 shift
relation the encoder's own `unshifted` uses, so `"!"` and `"1"` are one key exactly as code point
`49` is one key to the encoder. A layout that pairs shift differently is the ceiling, and its worst
case is the missed release that is today's behaviour.

Three write sites and no others:

| Site | Action |
| --- | --- |
| `on_key_down`, after `send_key` returned `Some` | `hold_key(&event.key)` |
| `on_key_up`, before writing | remove the canonical key; absent means drop the event |
| `on_blur` | drain, one `Release` per entry, then clear |

`send_key` returning `None` (a chord with no encoding, `Ctrl` plus a non-ASCII character) must
**not** insert: nothing was written, so nothing is owed a release.

`hold_key` is idempotent, which is the one safety net against a stuck key that a test can reach: a
fresh press of a key already held -- a release the platform never delivered, or one lost to a name
this canonical form does not collapse -- leaves one entry rather than two, so a missed release
cannot compound.

### The key-up path

`on_key_up` is deliberately **not** `classify_key`. The classification table is full of view-side
shortcuts, and running it on a release would risk toggling the search bar or zooming on a key-up.
The release path is four steps and nothing else:

```rust
pub(super) fn on_key_up(&mut self, e: &KeyUpEvent, _window: &mut Window, cx: &mut Context<Self>) {
    let Some(event) = map_key(&e.keystroke, KeyEventKind::Release) else {
        return;
    };
    // The press never reached the PTY, so neither does the release.
    let canonical = canonical_key(&event.key);
    let Some(at) = self.held_keys.iter().position(|held| *held == canonical) else {
        return;
    };
    self.held_keys.remove(at);
    let modes = self.render_state.borrow().frame.modes();
    send_key(&self.session, &event, &modes, cx);
    // No fan-out, and no `cx.stop_propagation()`.
}
```

The event is built before the set is consulted, because the canonical key is derived from the
`KeySpec` rather than from the raw name.

Two absences are deliberate:

- **No `stop_propagation`.** The down path stops propagation because it consumed the key; the up
  path consumes nothing that another handler might want, and stopping a key-up the search input or
  a dock shortcut expects would be a regression outside this packet's subject.
- **No `clear_bell`, no `completion_capture_current`, no `scrolled`.** Those are press-time view
  effects. `send_key` still snaps the viewport to the live screen, which is inside it and which a
  release reaching the program should do for the same reason a press does.

### Where `kind` comes from

```rust
let kind = if e.is_held { KeyEventKind::Repeat } else { KeyEventKind::Press };
```

`is_held` is GPUI's, and on Windows it is `lparam.0 & (0x1 << 30)`, the `WM_KEYDOWN`
previous-key-state bit -- the operating system's own auto-repeat, including the user's own repeat
delay and rate. **Nothing in the view times, counts or synthesises a repeat.** If a platform
backend does not set the bit, every event is a `Press`, which is today's behaviour: the degradation
is to the status quo rather than to a wrong cadence.

## Interfaces

| Item | Before | After |
| --- | --- | --- |
| `KeyAction::Send` | `Send(KeySpec, KeyMods)` | `Send(KeyEvent)` |
| `classify_key` | `(&Keystroke, bool, KeyContext) -> KeyAction` | `(&Keystroke, bool, KeyEventKind, KeyContext) -> KeyAction` |
| `map_key` | `(&Keystroke) -> Option<(KeySpec, KeyMods)>` | `(&Keystroke, KeyEventKind) -> Option<KeyEvent>` |
| `send_key` | `(session, &KeySpec, KeyMods, &ModeSnapshot, cx) -> Option<Vec<u8>>` | `(session, &KeyEvent, &ModeSnapshot, cx) -> Option<Vec<u8>>` |
| `TerminalView::on_key_up` | does not exist | `(&mut self, &KeyUpEvent, &mut Window, &mut Context<Self>)` |
| `TerminalView::held_keys` | does not exist | `Vec<KeySpec>`, canonicalized |
| `TerminalView::release_held_keys` | does not exist | `(&mut self, &mut App)`, the blur drain |
| `TerminalView::hold_key` | does not exist | `(&mut self, &KeySpec)`, the idempotent insert |
| `canonical_key` | does not exist | `(&KeySpec) -> KeySpec` |
| `KeyAction::Interrupt` | `Interrupt` | `Interrupt(Option<KeyEvent>)` |
| `KeyContext` | five fields | adds `ctrl_c_is_a_key` |
| `crates/terminal`'s shim | `{KeyMods, KeySpec, NamedKey, encode_key, ...}` | adds `KeyEvent`, `KeyEventKind`, `encode_key_event` |

All four `crates/terminal-view` items are `pub(crate)`; nothing outside the crate sees the change.
The `render.rs` registration gains one line:

```rust
.on_key_down(cx.listener(Self::on_key_down))
.on_key_up(cx.listener(Self::on_key_up))
```

## Edge Cases and Failure Modes

- [ ] **A release for a key the view swallowed.** `Ctrl+Shift+C` (copy), `Ctrl+=` (zoom),
      `Down` inside the completion overlay, `Ctrl+F` (search). Expected: the key-up writes nothing,
      because the name is not in the held set. This is the failure that a re-classifying release
      path could not prevent, since `ctx` can differ between the press and the release.
- [ ] **A release for a dead key or a composition key.** During IME composition GPUI routes text
      through `EntityInputHandler`; on the primary screen a printable key is `KeyAction::Ignore` by
      design so the IME is the only writer. Expected: neither ever enters the held set, so neither
      produces a release. **No IME-specific code is added** -- this falls out of the set.
- [ ] **A press with no encoding.** `Ctrl` plus a non-ASCII character returns `None` from the
      encoder. Expected: nothing written, nothing inserted, and the later key-up writes nothing.
- [ ] **Focus loss with a key held.** The user holds `Shift` (not a key event) and `j`, then
      alt-tabs. Expected: `on_blur` drains the set, the program receives the `j` release, and the
      set is empty. Without this a `vim` in kitty mode believes `j` is held forever.
- [ ] **A key-up with no matching key-down**, which the platform can deliver when the window gained
      focus while the key was already down. Expected: `remove` returns `false` and nothing is
      written. Symmetrical with the previous case and free from the same field.
- [ ] **A modifier key alone.** `Shift`, `Ctrl`, `Alt`, `Super` and `CapsLock` become
      `ModifiersChanged` on Windows and never reach either handler. Expected: unchanged behaviour;
      `REPORT_ALL_KEYS_AS_ESC` cannot report them, and that ceiling is the platform's rather than
      this design's.
- [ ] **`Ctrl+C`.** `KeyAction::Interrupt(None)` sends `SIGINT` and never encodes, and inserts
      nothing into the held set, so there is no release either. With a kitty flag that puts a ctrl
      chord on the `CSI u` rung -- `DISAMBIGUATE_ESC_CODES` or `REPORT_ALL_KEYS_AS_ESC` --
      `Interrupt(Some(event))` writes the encoded key to **this pane** instead and inserts it, so
      its release pairs like any other. Intake open decision 1, settled; `US-0108`'s `F4` is why
      the gate names both flags rather than only the second.
- [ ] **A held key while the broadcast channel is active.** Expected: presses and repeats fan out
      exactly as today; a release never does. A peer pane whose program negotiated nothing must not
      receive a `:3` sequence, which is a byte form it has never seen -- unlike the pre-existing
      `app_cursor` and kitty-rung divergence, which at least produces bytes the peer's program
      recognises as a key.
- [ ] **`Ctrl+C` while the broadcast channel is active.** Expected: whatever the **origin**
      negotiated, every peer receives `BroadcastInput::Interrupt` and never the encoded bytes. Same
      rule as the release, for the same reason, and `US-0108`'s `F2` is the draft that broke it: a
      user who broadcasts `Ctrl+C` to four panes to stop four runaway programs must not stop one
      and print `^[[99;5u` into the other three.
- [ ] **The session died.** `send_key` goes through `session.write`, which the alive gate already
      guards; a release takes the same path as a press and needs no separate check.
- [ ] **A repeat arriving with no preceding press**, if a backend sets `is_held` on a first event.
      Expected: the `Repeat` is encoded as a repeat and the name is inserted, so the eventual
      release still pairs. The set is keyed by name and does not model a state machine, which is
      why this is harmless rather than a case to handle.

## Verification

- [ ] **Kind mapping, focused and view-level.** `key_down("a", ..)` with `is_held: false` produces
      a `Press`; the same with `is_held: true` produces a `Repeat`; a `key_up("a", ..)` after a
      sent press produces a `Release`; a `key_up` after a swallowed press produces nothing. Four
      assertions against the classification and the held set, with no engine involved.
- [ ] **Byte identity with no flag pushed.** Drive a `FakeTerminalSession` through a press, three
      repeats and a release for each of `enter`, `a`, `up`, `escape`, `f5` and `Ctrl+A`, and assert
      `probe.writes()` equals what the same sequence of `on_key_down` calls writes on `main` --
      which, since a release wrote nothing on `main`, means the release contributes no entry at
      all. This is the criterion the lane exists for and it is the one that must not be weakened.
- [ ] **The `:2` and `:3` bytes, integration.** `probe.feed(b"\x1b[>2u")` to push
      `REPORT_EVENT_TYPES` on the real `Terminal` behind the fake session, force a repaint so the
      frame's `ModeSnapshot` carries the flags (the view reads modes from the last painted frame,
      so a test that skips the draw reads stale flags and silently proves nothing), then drive
      press / held / up on a key the flag applies to -- an arrow or `Escape`, not a text key, since
      the specification exempts text keys from event types unless `REPORT_ALL_KEYS_AS_ESC` is also
      set -- and assert the written bytes carry `:2` and `:3`.
- [ ] **Blur drains the set.** Press a key, blur the view, assert a release was written and the set
      is empty; blur again and assert nothing further is written.
- [ ] **The broadcast test still passes unchanged.**
      `member_input_reaches_the_channel_peers_only` asserts an exact write list on four sessions;
      it must pass without being edited, which is the cheapest available proof that presses still
      fan out identically.
- [ ] **A manual Windows walk**, recorded as the E2E criterion and runnable only outside the
      implementing session. It is the **only** proof of the blur drain: `US-0108`'s verification
      showed (`F3`) that no view test can reach the `on_blur` subscription, because GPUI raises
      focus events only inside a draw and builds the event's `previous_focus_path` only
      `if previous_window_active`, while a test window's `is_active` is hard-coded `false`.
      `view_tests::verify_the_blur_drain_is_unprovable_in_a_test_window` demonstrates that rather
      than asserting it. The drain is therefore untested in *both* places until this walk is run.

      Instrument: a short Python script inside an OneTerm local shell that pushes `CSI > 2 u`,
      echoes the bytes it receives, and pops with `CSI < u` on exit -- `kitty +kitten show_key -m
      kitty` is not available on Windows, and `US-0105`'s walk specifies the same instrument.

      Steps, each with its expected result:

      1. Open a local shell, run the script. Type a few letters: **exactly the characters typed**
         appear, and no escape codes -- `REPORT_EVENT_TYPES` alone exempts text keys.
      2. Tap an arrow key once: one press event, no `:2`, no `:3` (a press stays on the legacy
         rung under this flag alone).
      3. **Hold** the arrow for about two seconds: a stream of `\x1b[1;1:2A` at the user's own
         repeat rate -- the OS rate, not a fixed cadence. Release it: exactly one `\x1b[1;1:3A`.
      4. Hold `Shift` and tap `1`, then release `Shift` **before** releasing `1`: the release is
         reported and carries the same code point (`49`) as the press. This is `F1`, and the
         reason the set is not keyed on the platform's key name.
      5. **The blur drain.** Hold the arrow down and alt-tab away while still holding it. Expect a
         `\x1b[1;1:3A` at the moment focus leaves. Release the key outside the window, alt-tab
         back, and expect **no further `:2` and no second `:3`** -- a repeated `:2` after refocus,
         or silence where the `:3` should be, is the drain not firing and is a defect to report.
      6. Quit the script (it pops with `CSI < u`) and type in the plain shell: exactly the
         characters typed, proving the flags were popped and nothing leaked.
