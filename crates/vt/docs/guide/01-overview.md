# 1. What this crate is

`oneterm-vt` is the middle of a terminal emulator and nothing else: bytes go in, a
grid with scrollback comes out, and everything that touches a screen, a process,
a clipboard or a window stays yours.

```text
   your PTY / socket / fixture
              |
              v  feed(&[u8], &mut EventBatch, Instant)
        +-----------------+
        |   oneterm-vt    |   parser -> dispatch -> grid -> damage
        +-----------------+
           |           |
           |           `--> EventBatch: titles, replies, OSC, marks
           v  snapshot_update(&mut SnapshotState, Instant)
     rows, styles, cursor, placements  ->  your renderer
```

The whole engine is one synchronous object. It holds no lock, spawns no thread,
has no interior mutability, returns no `Result`, and never calls back into your
code. That is the property every other design decision in this guide follows
from: you can hold your own lock across a `feed`, drain the events afterwards,
and know that nothing ran in between that you did not write.

```rust
use std::time::Instant;
use oneterm_vt::{Config, EventBatch, Size, Terminal, VtEvent};

let mut term = Terminal::new(Size { rows: 24, cols: 80 }, Config::default());
let mut batch = EventBatch::new();

// Nothing of yours runs during this call. The events are a value you read
// after it returns, in the order the bytes produced them.
term.feed(b"\x1b]0;a title\x07hi", &mut batch, Instant::now());
assert!(matches!(batch.events().first(), Some(VtEvent::Title(_))));
```

## What is deliberately out

- **No renderer.** The engine produces a snapshot of cells, style runs, a
  cursor and image placements. Nothing here draws a pixel, measures a font or
  knows what a colour looks like on your display.
- **No policy.** Whether a program may write your clipboard, open a URL, raise a
  notification or set your window title is your decision. The engine hands you
  the request as a typed event and stays out of it.
- **No window.** No event loop, no focus tracking, no platform event handling.
  The `input` module turns a key press into the bytes the program expects; you
  deliver the key press and you own every side effect it has on your own view.
- **A transport you can switch off.** A pseudo-console ships in `pty`, behind a
  feature that is on by default. The engine itself never spawns a process and
  never reads a file descriptor. Chapter 13 has the detail.

## Against the two reference cores

Read from the live manifests and dispatch sites of `alacritty_terminal` 0.26 and
`rio-vt` 0.5 on 2026-09-15. The last row is the one that matters most, and it is
the reason this crate exists at all.

| | `alacritty_terminal` | `rio-vt` | `oneterm-vt` |
| --- | --- | --- | --- |
| Licence | Apache-2.0 | MIT | Apache-2.0 |
| Edition / MSRV | 2024 / 1.85.0 | 2021 / 1.96.1 | 2024 / 1.96.0 |
| Parser | delegates to `vte`, a second crate | its own | its own |
| Unconditional dependencies | 11 plus three platform sets | 15 plus platform sets | **6** |
| Dependencies with the transport off | not possible, the PTY is unconditional | still 15 | **6**, and a CI target |
| PTY in the core | yes, and not removable | yes, on by default | yes, on by default, removable |
| Clipboard / renderer / window in the core | no / no / no | optional / optional / optional | no / no / no |
| Event delivery | `EventListener` callback, invoked inside the terminal | `EventListener`, four callbacks, events carry a pane id | **values**: `feed` fills an `EventBatch`; nothing runs inside the engine |
| Event count | 13 variants | about 60 variants | 20 variants |
| OSC numbers handled | 14 | 17 | 18, plus any number you claim |
| **OSC extensibility** | **none**: a fixed `match` in another crate; unknown numbers are logged and dropped | **none**: a fixed `match`; unknown numbers are dropped | **`OscRoutes`**: per number, `Builtin` / `BuiltinAndForward` / `Forward` / `Drop` |
| Locks in the core | `parking_lot` | `parking_lot` | none |

Two things follow from that table.

**Neither reference core is extensible for OSC.** Both match the OSC number in a
compile-time `match` and silently drop what they do not know, and in one of them
that `match` lives in a different crate. Supporting a new OSC number there means
forking. Here it is one `route()` call and a `match` arm of your own, and chapter
5 shows both halves.

**The dependency budget is the smallest of the three by a factor of two**, and
the number a VT-only embedder pays -- `--no-default-features`, six leaf crates,
no platform code, no build script, no proc macro, no `unsafe` in the engine -- is
a number neither of the others can offer at any feature setting.

## How to read the rest

Chapter 2 embeds the crate in one program. Chapter 3 is the threading and
locking model, which is short because there is so little of it. Chapters 4 and 5
are the two halves of the output side: the events, and the OSC routing that
decides which events exist. Chapter 6 is the input side. Chapters 7 to 10 are
one subsystem each -- search, images, resize, and the ceilings that keep a
hostile stream from costing you memory. Chapter 11 says what is conformant and
what is missing, chapter 12 what a version number promises, and chapter 13 the
transport.

The crate is not published to a registry. Depend on it by git, pinned to a
commit; the top of `README.md` has the three lines.
