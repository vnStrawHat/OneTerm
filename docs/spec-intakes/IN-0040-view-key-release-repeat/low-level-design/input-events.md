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
// Only a press or a repeat carries text. The engine refuses it on a release
// anyway; not sending it keeps the two halves agreeing rather than relying on
// one of them to clean up after the other.
if kind != KeyEventKind::Release {
    event.text = ks.key_char.clone().filter(|text| !text.is_empty());
}
Some(event)
```

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
/// Keys whose press actually reached the PTY, by the GPUI key name.
///
/// A release is sent only for a key in here, which is what keeps a chord the
/// view swallowed (zoom, copy, the completion overlay, a dead key, a printable
/// key the IME owns) from producing a release the program never saw a press
/// for. Drained on blur so a window the user left cannot strand a held key.
held_keys: HashSet<SharedString>,
```

Bounded by the number of physically held keys -- a handful -- so no eviction policy and no cap.
Keyed by `Keystroke::key`, the chord name, not by `key_char`: the name is what both the down and
the up event carry, and it is stable under a modifier changing between the two (holding `a`, then
pressing and releasing `Shift`, then releasing `a` still produces a key-up whose `key` is `a`).

Three write sites and no others:

| Site | Action |
| --- | --- |
| `on_key_down`, after `send_key` returned `Some` | `insert(ks.key.clone())` |
| `on_key_up`, before building the event | `remove(&ks.key)`; `false` means drop the event |
| `on_blur` | drain, one `Release` per name, then clear |

`send_key` returning `None` (a chord with no encoding, `Ctrl` plus a non-ASCII character) must
**not** insert: nothing was written, so nothing is owed a release.

### The key-up path

`on_key_up` is deliberately **not** `classify_key`. The classification table is full of view-side
shortcuts, and running it on a release would risk toggling the search bar or zooming on a key-up.
The release path is four steps and nothing else:

```rust
pub(super) fn on_key_up(&mut self, e: &KeyUpEvent, _window: &mut Window, cx: &mut Context<Self>) {
    if !self.held_keys.remove(&e.keystroke.key) {
        return; // the press never reached the PTY, so neither does the release
    }
    let Some(event) = map_key(&e.keystroke, KeyEventKind::Release) else {
        return;
    };
    let modes = self.render_state.borrow().frame.modes();
    send_key(&self.session, &event, &modes, cx);
    // No fan-out, and no `cx.stop_propagation()`.
}
```

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
| `TerminalView::held_keys` | does not exist | `HashSet<SharedString>` |
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
- [ ] **`Ctrl+C`.** `KeyAction::Interrupt` sends `SIGINT` and never encodes. Expected: no insert
      into the held set and therefore no release, which keeps the press and the release consistent
      with each other. Pre-existing divergence; intake open decision 1.
- [ ] **A held key while the broadcast channel is active.** Expected: presses and repeats fan out
      exactly as today; a release never does. A peer pane whose program negotiated nothing must not
      receive a `:3` sequence, which is a byte form it has never seen -- unlike the pre-existing
      `app_cursor` and kitty-rung divergence, which at least produces bytes the peer's program
      recognises as a key.
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
      implementing session. Instrument: a short Python script inside an OneTerm local shell that
      pushes `CSI > 2 u`, echoes the bytes it receives, and pops with `CSI < u` on exit --
      `kitty +kitten show_key -m kitty` is not available on Windows, and `US-0105`'s walk specifies
      the same instrument. Expected: holding an arrow prints a stream of `:2` events at the user's
      own repeat rate, releasing it prints one `:3`, alt-tabbing away while holding it prints the
      `:3` rather than nothing, and typing in a plain shell with the script not running produces
      exactly the characters typed.
