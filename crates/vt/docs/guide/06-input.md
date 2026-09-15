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

The rules the encoder follows, in full:

- a character with `ctrl` goes through the xterm control table; with `alt` it is
  prefixed by `ESC`, and `Ctrl+Alt+a` is the `ESC` prefix on the control byte;
- `Enter` with shift or ctrl is `CSI 13 ; <mod> u`, and plain is `\r`;
- `Backspace` is `0x08` with ctrl, `ESC DEL` with alt, and `0x7f` plain;
- an arrow with shift or ctrl is `CSI 1 ; <mod> <final>`; plain it is `ESC O
  <final>` when the program has enabled application cursor keys, and `CSI
  <final>` otherwise;
- `Home` and `End` follow the same split, with finals `H` and `F`.

The only mode the key encoder reads is `app_cursor`, the terminal's `DECCKM`
state. Programs such as vim, less and man set it, and sending `CSI A` to one of
them when it asked for `ESC O A` is the classic "arrow keys do nothing" bug.

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

Check `Terminal::mouse_reporting()` before you send anything. `None` means no
program asked, and a mouse event then belongs to your own UI -- selection,
scrolling, a context menu. `Some(protocol)` carries two independent choices the
program made: `reporting` says which events it wants (`Normal` for press and
release, `ButtonEvent` to add drags, `AnyEvent` to add bare motion) and
`encoding` says how they are spelled (`Default` for the legacy X11 byte form,
`Utf8` for `? 1005`, `Sgr` for `? 1006`). Send only what `reporting` asked for;
a flood of hover events to a program that asked for `Normal` is a bug you will
see as latency.

The legacy encoding cannot express a coordinate past 223, which is why every
modern program asks for SGR. The engine encodes what the program chose rather
than what is best, because a terminal that silently upgrades an encoding is a
terminal that breaks the program that chose it.

## Kitty keyboard flags and modifyOtherKeys

The engine tracks the two protocols that let a program ask for unambiguous key
reporting, and reports their state:

- `Terminal::keyboard_flags()` is the live kitty keyboard flag set, which the
  program pushes and pops with `CSI > flags u` and `CSI < u`. The flags are
  `DISAMBIGUATE_ESC_CODES`, `REPORT_EVENT_TYPES`, `REPORT_ALTERNATE_KEYS`,
  `REPORT_ALL_KEYS_AS_ESC` and `REPORT_ASSOCIATED_TEXT`;
- `Terminal::modify_other_keys()` is the `CSI > 4 ; Ps m` level, `0`, `1` or `2`.

**`encode_key` does not yet honour either.** It reads `app_cursor` and nothing
else, so the bytes it returns are the legacy encoding regardless of what the
program asked for. The state is published so that an embedder who needs the
richer protocols can encode them itself from the flags; folding them into
`encode_key` is a change to what the function returns for a given input, and
that is a versioned change rather than a silent one.

Read the two accessors, and if both report their default -- empty flags and
level `0` -- `encode_key` is the whole answer.
