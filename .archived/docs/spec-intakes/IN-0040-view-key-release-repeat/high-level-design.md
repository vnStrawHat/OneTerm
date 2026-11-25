# High-Level Design: OneTerm delivers key release and repeat to the engine

Intake: [`IN-0040`](IN-0040.md)
Lane: high_risk
Date: 2026-09-16

## Idea

`US-0105` taught `oneterm-vt` to encode the kitty keyboard protocol, including the `:2` repeat and
`:3` release event-type sub-fields that `REPORT_EVENT_TYPES` asks for. It could not make OneTerm
itself send them, because the application never observes a key-up and never distinguishes a
held-key repeat from a first press: `TerminalView` registers `on_key_down` alone, and `map_key`
returns `(KeySpec, KeyMods)` with no room for a kind.

This design closes that one link. The view starts observing key-up, reads GPUI's own `is_held` bit
for the repeat, and hands the engine a full `KeyEvent` instead of a key-and-modifiers pair. The
engine is unchanged.

The whole design is constrained by one sentence: **with no kitty flag pushed, the bytes must be
what they are today, byte for byte.** That is what makes a change to the input path of every
terminal in the application safe enough to make, and it is free -- the encoder's ladder already
answers a release with `None` and ignores the kind on its legacy rung. The work is entirely in
making sure the view does not send an event it should not have, which is where all three failure
modes live: a release for a key the view swallowed, a release that never arrives because focus
left, and a release for a dead key the IME consumed.

The instrument for all three is one set of key names whose press was actually written. A release is
sent only for a name in that set, and the set is drained on blur. Everything the view swallows --
zoom, copy, the completion overlay, a dead key, a printable key on the primary screen that belongs
to the IME -- never enters it, so its key-up is silently dropped without a single special case.

## Diagram

```text
  GPUI                     crates/terminal-view                       crates/terminal + vt
  ────                     ────────────────────                       ────────────────────

  KeyDownEvent ──────────► on_key_down
   { keystroke,              │
     is_held,                ├─ classify_key(keystroke, prefer_char, ctx)
     prefer_character }      │     │
                             │     ├─► ToggleSearch / Zoom / Copy / Completion / …
                             │     │        (view-side only; the key never enters `held`)
                             │     ├─► Ignore | Unhandled
                             │     │        (the IME or the platform owns it; same)
                             │     ├─► Interrupt ─────────────────────► send_ctrl_c()
                             │     │        (pre-existing; not encoded, so no release either)
                             │     └─► Send(KeyEvent)
                             │              kind = Repeat if is_held else Press
                             │              text = keystroke.key_char
                             │                │
                             ├─ held.insert(keystroke.key) ◄───────────┘
                             │                │
                             └────────────────┴──► send_key ──► encode_key_event(&event, &modes)
                                                                       │
  KeyUpEvent ────────────► on_key_up                                   ├─ rung 1: release with no
   { keystroke }             │                                        │   REPORT_EVENT_TYPES → None
                             ├─ held.remove(&keystroke.key)?           ├─ rung 2: kitty form, with
                             │     no  → drop, write nothing           │   `:2` / `:3` when asked
                             │     yes → KeyEvent { kind: Release,     ├─ rung 3: modifyOtherKeys
                             │              text: None, .. }           └─ rung 4: legacy table
                             └──────────────────► send_key                (kind is not read)
                                                        │
  blur ──────────────────► on_blur                      └──► session.write(bytes)
                             └─ drain `held`, one Release each              │
                                (never fanned out)                          ▼
                                                                        PTY / SSH channel
```

The broadcast fan-out sits beside `send_key` and is unchanged for a press and a repeat. A release
is **not** fanned out: a sibling pane whose program negotiated nothing would receive a byte
sequence it has never seen, and that is a new failure mode rather than the pre-existing divergence
(`app_cursor`, the kitty rung) that already applies to presses.

## UI Wireframe

`N/A -- no UI surface.` Nothing this intake changes is visible: no pixel, no control, no layout, no
setting. The only observable difference is the byte stream on a pseudo-console, and only for a
program that asked for it.

## Data Flow

1. **A first press.** GPUI delivers `KeyDownEvent { keystroke, is_held: false, .. }`.
   `classify_key` returns `KeyAction::Send(event)` with `kind = Press` and
   `text = keystroke.key_char`. The view records `keystroke.key` in its held set, encodes against
   the frame's `ModeSnapshot`, writes, and fans out to the broadcast channel exactly as today.
2. **A held repeat.** The OS repeats `WM_KEYDOWN`; GPUI sets `is_held` from the previous-key-state
   bit. The kind becomes `Repeat`. The held set already contains the name, so the insert is a
   no-op. With no flag pushed the encoder's legacy rung does not read the kind, so the bytes are
   the same repeated bytes OneTerm sends today; with `REPORT_EVENT_TYPES` the kitty form carries
   `:2`. **Nothing is synthesised**: the repeat cadence is the OS's, and the view neither times nor
   counts anything.
3. **A release.** GPUI delivers `KeyUpEvent { keystroke }`. The view removes the name from the held
   set; if it was not there, the key's press never reached the engine and the release is dropped
   without a write. If it was, the view builds `KeyEvent { kind: Release, text: None, .. }` and
   calls the same `send_key`. With no flag the encoder returns `None` at rung 1 and nothing is
   written -- so a release costs one function call and no bytes in the common case. With
   `REPORT_EVENT_TYPES` the kitty form carries `:3`.
4. **A key the view swallows.** `Ctrl+Shift+C`, `Ctrl+=`, an arrow inside the completion overlay, a
   printable key on the primary screen (the IME's), a dead key mid-composition: `classify_key`
   returns something other than `Send`, so the name never enters the held set, so step 3 drops its
   key-up. No special case, no IME hook, no composition state.
5. **Focus leaves.** The `on_blur` subscription the view already owns drains the held set and sends
   one release per name before clearing it. A window the user alt-tabbed away from cannot leave a
   program believing a key is still down. These releases are not fanned out, for the same reason
   step 3's are not.

## Detail Design

- [x] Detail design: **required (high-risk)**
- Reason: the lane is `high_risk` on the "broad established behaviour" trigger, so
  `docs/HARNESS.md` blocks `story create` until at least one concern file exists. One file is
  enough here, because there is one concern: which platform event becomes which `KeyEvent`, and
  what must never happen.
  [`low-level-design/input-events.md`](low-level-design/input-events.md).

## What this design deliberately does not do

- **It does not touch `crates/vt`.** Not one file. The engine shipped complete in `US-0105`; if
  this design needs an engine change, that is a finding and a different packet.
- **It does not add a crate dependency.** `crates/terminal-view` keeps reaching the encoder through
  `oneterm-terminal`'s existing re-export shim rather than taking a direct `oneterm-vt` edge. The
  shim's own comment says it exists "for one release" and that consumers should name the types from
  `oneterm_vt::input`; taking the edge is the cleaner end state and is a larger diff in a
  higher-risk packet than this one, so three names join the shim instead. Recorded so the shim's
  eventual removal knows it has one more consumer.
- **It does not supply `shifted` or `base_layout`.** GPUI's `Keystroke` does not carry them, so
  `REPORT_ALTERNATE_KEYS` stays inert and the encoder omits the sub-fields, which the protocol
  allows. Reading a keyboard layout per platform is its own intake.
- **It does not report the modifier keys themselves.** Windows converts `VK_SHIFT` and friends to
  `ModifiersChanged` before a key event exists, so `REPORT_ALL_KEYS_AS_ESC` cannot deliver a
  `Super` press no matter what this design does. The engine states the same ceiling from its side.
- **It does not re-encode broadcast bytes per target pane.** That divergence is pre-existing; this
  design's only obligation is not to widen it, which it meets by never fanning out a release.
- **It does not change `Ctrl+C`.** `KeyAction::Interrupt` keeps sending `SIGINT` rather than an
  encoded byte, which means a program that pushed `REPORT_ALL_KEYS_AS_ESC` still cannot see
  `Ctrl+C` as a key. Pre-existing, owner-owned, and recorded as open decision 1 in the intake.
- **It does not synthesise a repeat.** If a platform ever stops setting `is_held`, the result is
  that repeats look like presses -- which is exactly today's behaviour -- rather than a timer in
  the view inventing a cadence the OS did not ask for.
