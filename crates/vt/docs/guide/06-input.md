# 6. Input: keys and mouse to bytes

The `input` module is the other direction: a key press or a mouse click becomes
the bytes the program on the far end expects. The engine answers this because
only the engine knows the modes the answer depends on -- `DECCKM` for the cursor
keys, `? 1005` and `? 1006` for the mouse report.

Both encoders are pure. They return bytes and have no side effects at all:
scrolling the viewport back to the live screen, clearing a selection, tracking a
held shift and swallowing a shortcut of your own stay with your input handling,
where the platform event lives.

The types are framework-neutral on purpose. You map your own key events onto
`KeySpec`, `KeyMods`, `NamedKey`, `TerminalMouseButton` and `MouseModifiers`;
nothing here depends on a UI toolkit or a keyboard crate.

## Keys

```rust
use oneterm_vt::input::{KeyMods, KeySpec, NamedKey, encode_key};
use oneterm_vt::{Config, Size, Terminal};

let term = Terminal::new(Size { rows: 24, cols: 80 }, Config::default());

// `Terminal::encode_key` uses this terminal's own modes, which is what you
// want unless you are encoding against a snapshot you already hold.
let up = term.encode_key(&KeySpec::Named(NamedKey::ArrowUp), KeyMods::default());
assert_eq!(up.as_deref(), Some(b"\x1b[A".as_slice()));

// Ctrl+C is the xterm control table, not a special case in your code.
let mods = KeyMods { ctrl: true, ..KeyMods::default() };
let interrupt = term.encode_key(&KeySpec::Character("c".into()), mods);
assert_eq!(interrupt.as_deref(), Some(b"\x03".as_slice()));

// The free function is the same encoder against modes you supply yourself.
let modes = term.mode_snapshot();
assert_eq!(
    encode_key(&KeySpec::Named(NamedKey::ArrowUp), KeyMods::default(), &modes),
    up
);
```

`None` means the chord has no terminal encoding at all -- `Ctrl` with a
non-ASCII character, or multi-codepoint text -- and the event should be dropped
rather than sent as something else.

The legacy rules the encoder follows when no program has asked for anything
richer -- which is the common case, and the one the section below extends:

- a character with `ctrl` goes through the xterm control table; with `alt` it is
  prefixed by `ESC`, and `Ctrl+Alt+a` is the `ESC` prefix on the control byte;
- `Enter` with shift or ctrl is `CSI 13 ; <mod> u`, and plain is `\r`;
- `Backspace` is `0x08` with ctrl, `ESC DEL` with alt, and `0x7f` plain;
- an arrow with shift or ctrl is `CSI 1 ; <mod> <final>`; plain it is `ESC O
  <final>` when the program has enabled application cursor keys, and `CSI
  <final>` otherwise;
- `Home` and `End` follow the same split, with finals `H` and `F`.

On that path the only mode the key encoder reads is `app_cursor`, the terminal's
`DECCKM` state. Programs such as vim, less and man set it, and sending `CSI A`
to one of them when it asked for `ESC O A` is the classic "arrow keys do
nothing" bug.

## The by-reference and by-value asymmetry

Note the signatures:

```text
encode_key(&KeySpec, KeyMods, &ModeSnapshot) -> Option<Vec<u8>>
encode_mouse_press(usize, usize, TerminalMouseButton, ModeSnapshot, MouseModifiers) -> Vec<u8>
```

`encode_key` takes `&ModeSnapshot`; every mouse encoder takes it by value. That
is a historical inconsistency in an otherwise symmetric family, not a
distinction with meaning: `ModeSnapshot` is a small `Copy` struct, so the two
forms compile to the same thing and `&term.mode_snapshot()` or
`term.mode_snapshot()` are equally cheap. It is recorded here rather than
quietly changed because changing a signature is a minor version bump under
chapter 12's promise, and this one is not worth one on its own. Write `&modes`
for keys and `modes` for the mouse, and the compiler will tell you if you
mix them up.

## Mouse

Four encoders: press, release, move and wheel. All take `row` and `col` as
zero-based cell coordinates -- use `Terminal::hit_test` to turn a pointer
position into them, so that a click and a drag agree about what a half-covered
wide glyph means.

```rust
use oneterm_vt::input::{MouseModifiers, TerminalMouseButton, encode_mouse_press};
use oneterm_vt::{Config, EventBatch, Size, Terminal};
use std::time::Instant;

let mut term = Terminal::new(Size { rows: 24, cols: 80 }, Config::default());
let mut batch = EventBatch::new();

// No program has asked for mouse reports yet, so there is nothing to send.
assert!(term.mouse_reporting().is_none());

// `CSI ? 1000 h` (click reporting) plus `CSI ? 1006 h` (SGR encoding).
term.feed(b"\x1b[?1000h\x1b[?1006h", &mut batch, Instant::now());
let protocol = term.mouse_reporting().expect("the program asked for reports");

let bytes = encode_mouse_press(
    0,
    0,
    TerminalMouseButton::Left,
    term.mode_snapshot(),
    MouseModifiers::default(),
);
assert_eq!(bytes, b"\x1b[<0;1;1M");
# let _ = protocol;
```

**An encoder returns an empty `Vec` for an event the current mode does not
report**, and you write nothing. That covers every event when no program asked
for reports at all, and everything but a press under `? 9`. The engine is the
one place that knows, so forgetting to check cannot send mouse bytes into a
program that never asked for them.

Check `Terminal::mouse_reporting()` anyway, because an empty answer is not the
same as no decision: `None` means the event belongs to **your** UI -- selection,
scrolling, a context menu -- and you should act on it rather than drop it.

`Some(protocol)` carries two independent choices the program made.

`reporting` says which events it wants:

| Mode | `MouseReporting` | Sends |
| --- | --- | --- |
| `? 9` | `X10` | button press only: no modifiers, no release, no motion |
| `? 1000` | `Normal` | press and release |
| `? 1002` | `ButtonEvent` | press, release, and motion while a button is down |
| `? 1003` | `AnyEvent` | the above plus bare motion |

`encoding` says how a report is spelled:

| Mode | `MouseEncoding` | Report |
| --- | --- | --- |
| none | `Default` | `CSI M` then three raw bytes, each offset by 32 |
| `? 1005` | `Utf8` | the same values, UTF-8 encoded |
| `? 1015` | `Urxvt` | `CSI Cb ; Cx ; Cy M` -- the same values as decimal parameters, `Cb` keeping its `+ 32` offset |
| `? 1006` | `Sgr` | `CSI < Cb ; Cx ; Cy M`, with `m` for a release |

Send only what `reporting` asked for; a flood of hover events to a program that
asked for `Normal` is a bug you will see as latency.

Two asymmetries the reference has and this engine reproduces. **`? 9` outranks
the extended encodings**: with `? 9` and `? 1006` both set the report is the
legacy form, because the X10 protocol predates SGR and has no SGR shape defined
for it. And **setting any reporting mode clears the other three**, while
unsetting clears only the one named; the same holds for the three encodings.

The legacy encoding cannot express a coordinate past 223, which is what `? 1015`
and `? 1006` each fix in their own way. The engine encodes what the program
chose rather than what is best, because a terminal that silently upgrades an
encoding is a terminal that breaks the program that chose it.

## Kitty keyboard flags and modifyOtherKeys

Two protocols let a program ask for unambiguous key reporting, and the engine
both tracks them and **encodes them**. A program that negotiates one and is told
"yes" gets the bytes that protocol defines; there is nothing for you to
implement on top.

- `Terminal::keyboard_flags()` is the live kitty keyboard flag set, which the
  program pushes and pops with `CSI > flags u` and `CSI < u`. The flags are
  `DISAMBIGUATE_ESC_CODES`, `REPORT_EVENT_TYPES`, `REPORT_ALTERNATE_KEYS`,
  `REPORT_ALL_KEYS_AS_ESC` and `REPORT_ASSOCIATED_TEXT`;
- `Terminal::modify_other_keys()` is the `CSI > 4 ; Ps m` level, `0`, `1` or `2`.

Both reach the bytes through `ModeSnapshot`, so `encode_key` already honours
them. **The bytes it returns therefore change once a program has negotiated
either protocol** -- that is the point -- and with both at their defaults they
are the legacy encoding above, byte for byte.

```rust
use oneterm_vt::input::{KeyEvent, KeyMods, KeySpec, NamedKey};
use oneterm_vt::{Config, EventBatch, Size, Terminal};
use std::time::Instant;

let mut term = Terminal::new(Size { rows: 24, cols: 80 }, Config::default());
let mut batch = EventBatch::new();
let escape = KeySpec::Named(NamedKey::Escape);

assert_eq!(term.encode_key(&escape, KeyMods::default()).as_deref(), Some(b"\x1b".as_slice()));

// The program pushes `DISAMBIGUATE_ESC_CODES` and Escape stops being ambiguous
// with the start of an escape sequence.
term.feed(b"\x1b[>1u", &mut batch, Instant::now());
assert_eq!(
    term.encode_key(&escape, KeyMods::default()).as_deref(),
    Some(b"\x1b[27u".as_slice())
);
```

### The decision ladder

`encode_key_event` picks exactly one encoding, and the first rung that applies
wins:

1. a release event with no `REPORT_EVENT_TYPES` to ask for it → `None`;
2. any kitty flag that applies to this key → the kitty form;
3. a non-zero `modifyOtherKeys` level and a modified "other" key →
   `CSI 27 ; modifier ; code ~`;
4. otherwise the legacy table.

Rung 2 comes before rung 3 because the kitty flags are the superseding
negotiation: a program that pushed flags *and* set `modifyOtherKeys` gets kitty.
Once rung 2 applies, `app_cursor` is not read at all -- an unmodified arrow
under `DISAMBIGUATE_ESC_CODES` is `CSI A` and never `ESC O A`, even in
application cursor key mode, because the kitty form is not the cursor-key form.

Which flag makes rung 2 apply:

| Flag | Applies to | Does not apply to |
| --- | --- | --- |
| `DISAMBIGUATE_ESC_CODES` | `Escape`, every named key, and any character chord with ctrl or alt -- every event that generates no text | **`Enter`, `Tab`, `Backspace`**, the specification's own exception, so that you can still type `reset` after a program crashes with the flags set |
| `REPORT_EVENT_TYPES` | nothing on its own; it adds the `:event-type` sub-field and turns a release from "no bytes" into bytes | `Enter`, `Tab` and `Backspace` still send no release unless `REPORT_ALL_KEYS_AS_ESC` is set too |
| `REPORT_ALTERNATE_KEYS` | nothing on its own; it adds the `:shifted:base` sub-fields to a form another flag already chose | -- |
| `REPORT_ALL_KEYS_AS_ESC` | **every** key, `Enter`, `Tab` and `Backspace` included. Text is no longer sent as text | -- |
| `REPORT_ASSOCIATED_TEXT` | nothing on its own; it adds the trailing `;text-codepoints` field. The specification calls it undefined without `REPORT_ALL_KEYS_AS_ESC`, and this engine treats it as inert there rather than guessing | -- |

### The richer entry point

`KeyEvent` carries what the flags can report and `encode_key` cannot express: a
release, a repeat, the shifted and base-layout keys, and the text the event
would insert. Build it with `KeyEvent::new` and fill in what your platform
knows; a field left `None` omits its sub-field, which the protocol allows.

```rust
use oneterm_vt::input::{KeyEvent, KeyEventKind, KeyMods, KeySpec, encode_key_event};
use oneterm_vt::{Config, EventBatch, Size, Terminal};
use std::time::Instant;

let mut term = Terminal::new(Size { rows: 24, cols: 80 }, Config::default());
let mut batch = EventBatch::new();
// Report event types, and report every key as an escape code.
term.feed(b"\x1b[>10u", &mut batch, Instant::now());

let mut event = KeyEvent::new(KeySpec::Character("a".into()), KeyMods::default());
event.kind = KeyEventKind::Release;
assert_eq!(
    encode_key_event(&event, &term.mode_snapshot()).as_deref(),
    Some(b"\x1b[97;1:3u".as_slice())
);
```

`None` from either function means the event sends nothing, and you drop it: a
release nobody asked to hear about, a key with no code point, or a chord with no
encoding at all.

### What this engine does not encode

Four ceilings, stated rather than discovered:

- **Modifier values `1` through `8` only.** `KeyMods` has shift, ctrl and alt.
  The protocol also defines super, hyper, meta, caps lock and num lock, and
  nothing here can supply them. `KeyMods` stays exhaustive, so the compiler
  will tell you the day it grows.
- **The private-use functional keys are not emitted.** `NamedKey` cannot name
  the keypad (`57399`-`57415`), the lock and system keys (`57358`-`57363`), the
  media keys (`57428`+) or the modifier keys themselves (`57441`+), so under
  `REPORT_ALL_KEYS_AS_ESC` a `Super` press has nothing to be delivered as.
  `NamedKey` is `#[non_exhaustive]`, so adding them is a patch release.
- **`F13`-`F24` keep xterm's shifted `F1`-`F12` forms** (`CSI 1 ; 2 P`,
  `CSI 15 ; 2 ~`, ...) rather than the specification's `57376`-`57387`, because
  that is what the legacy path already sends and the two paths agreeing matters
  more here than the private-use spelling.
- **The un-shifted key code is the lower-cased text.** The protocol wants the
  un-shifted code point (`ctrl+shift+a` is `97`, never `65`) and gets it for
  letters; shifted punctuation cannot be un-shifted without a platform key map,
  so `shift+4` reports `$`. Supply `base_layout` when your platform knows
  better.

And one disagreement between the two rungs, deliberate and recorded: the legacy
encoder ignores `alt` on `Insert`, `Tab` and `F1`-`F24`, which is a defect
predating this. The legacy rung keeps that behaviour byte for byte; the kitty
rung does not have it, because there the modifier is a field rather than a table
lookup. So `Alt+F5` is `CSI 15 ~` on rung 4 and `CSI 15 ; 3 ~` on rung 2.
