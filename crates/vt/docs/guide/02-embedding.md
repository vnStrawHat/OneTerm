# 2. Embedding it in ten minutes

This chapter walks `examples/headless.rs`, which is a complete terminal with no
window: it feeds one chunk of bytes, prints the events, and prints the screen.
Run it with `cargo run --example headless`.

The example is quoted here in numbered fragments rather than as one block. Both
copies are compiled -- the example by `cargo build --examples`, the fragments by
`cargo test --doc` -- so neither can rot silently, and quoting it in pieces makes
a divergence between them visible instead of plausible.

## Fragment 1: what you need in scope

```rust
use std::time::Instant;

use oneterm_vt::{
    CellWidth, Config, EventBatch, OscRoute, OscRoutes, Size, SnapshotContent, SnapshotRow,
    SnapshotState, Terminal, VtEvent,
};
```

Everything an embedder normally names is at the crate root. The modules --
`grid`, `input`, `intern`, `parser`, `search`, `pty` -- hold the families you
take wholesale rather than one type at a time.

## Fragment 2: the bytes

```rust
/// Everything below arrives in one `feed`: a title, a working directory, an
/// application-private OSC, colour, and two cursor moves.
const INPUT: &[u8] = b"\x1b]0;headless demo\x07\
    \x1b]7;file://localhost/tmp\x1b\\\
    \x1b]1337;SetUserVar=demo\x07\
    \x1b[1;32mhello\x1b[0m world\
    \x1b[3;3Hrow three";
# let _ = INPUT;
```

There is nothing special about where bytes come from. A PTY, a socket, a
recorded fixture and a `&[u8]` literal are indistinguishable to `feed`.

## Fragment 3: configuring the terminal

```rust
# use oneterm_vt::{Config, OscRoute, OscRoutes, Size, Terminal};
// One call adds an OSC number the engine has never heard of. Everything the
// engine implements -- the title, the working directory, colours, the
// clipboard -- already arrives as a typed event without any of this.
let mut routes = OscRoutes::new();
routes.route(1337, OscRoute::Forward);

let mut term = Terminal::new(
    Size { rows: 4, cols: 32 },
    Config {
        osc_routes: routes,
        // What `XTVERSION` and `DA2` will tell programs they are talking to.
        product_name: Some("headless-demo(1.0.0)".into()),
        ..Config::default()
    },
);
# let _ = &mut term;
```

`Config` is built with `Config::default()` plus the fields you care about. Every
knob it offers is read by the engine; one it could not honour is not offered.
The terminal keeps the config it was given and there is no setter: a route that
could change under a half-parsed sequence would be a race with no honest
description, so an embedder that wants different routes builds a new terminal.

## Fragment 4: feeding

```rust
# use std::time::Instant;
# use oneterm_vt::{Config, EventBatch, Size, Terminal};
# const INPUT: &[u8] = b"hello";
# let mut term = Terminal::new(Size { rows: 4, cols: 32 }, Config::default());
// The batch is reusable: clear it and feed again, and no allocation
// happens after the first few chunks.
let mut batch = EventBatch::new();
let stats = term.feed(INPUT, &mut batch, Instant::now());
# let _ = stats.bytes;
```

Keep one `EventBatch` for the life of the session. `feed` clears it first and
refills its one arena, so the steady state grows to a high-water mark and then
allocates nothing. A caller who has not drained the previous batch loses it --
that is a programming error, not a recoverable one.

`stats` is a [`FeedStats`](crate::FeedStats), per call and never cumulative: how
many bytes you passed, how many rows went into scrollback, and one counter for
each way the engine had to degrade. Chapter 10 reads them.

## Fragment 5: draining the events

```rust
# use std::time::Instant;
# use oneterm_vt::{Config, EventBatch, Size, Terminal, VtEvent};
# let mut term = Terminal::new(Size { rows: 4, cols: 32 }, Config::default());
# let mut batch = EventBatch::new();
# term.feed(b"\x1b]0;headless demo\x07", &mut batch, Instant::now());
for event in batch.iter() {
    match event {
        VtEvent::Title(span) => println!("title      {:?}", batch.str(*span)),
        VtEvent::Osc { code, params, .. } => {
            // Parameter 0 is the OSC number itself, which `code` already
            // carries, so the payload starts at 1.
            let text: Vec<String> = batch
                .params(*params)
                .skip(1)
                .map(|param| String::from_utf8_lossy(param).into_owned())
                .collect();
            println!("osc {code:<6} {:?}", text.join(";"));
        }
        other => println!("{other:?}"),
    }
}
```

A payload is a span into the batch's arena, not a `String`, so an OSC with
sixteen parameters costs no allocation per parameter. Resolve a span with
`batch.str`, `batch.bytes` or `batch.params`, all of which borrow the batch.
An event therefore cannot outlive the batch that produced it, which is what
stops a stale payload from silently reading the next chunk's bytes. Chapter 4
covers every variant.

## Fragment 6: taking a snapshot

```rust
# use std::time::Instant;
# use oneterm_vt::{Config, EventBatch, Size, SnapshotState, Terminal};
# let mut term = Terminal::new(Size { rows: 4, cols: 32 }, Config::default());
# term.feed(b"hello", &mut EventBatch::new(), Instant::now());
// Drawing is a pull: ask when you want to, and get back only what changed
// since this `SnapshotState` last asked.
let mut state = SnapshotState::new();
let update = term.snapshot_update(&mut state, Instant::now());
assert_eq!(state.rows().len(), 4);
# let _ = update;
```

`SnapshotState` is yours, one per consumer. The engine stamps a sequence number
per row and never clears it, so two consumers -- a renderer and a thumbnail, say
-- can each hold their own state over one terminal without clearing each other's
damage. `update` is `Full`, `Partial { scrolled }` or `Unchanged`; chapter 3 says
what to do with each.

## Fragment 7: reading a row

```rust
# use oneterm_vt::{CellWidth, SnapshotContent, SnapshotRow};
/// One row as plain text, wide-glyph spacers dropped and clusters expanded.
fn row_text(row: &SnapshotRow) -> String {
    let mut text = String::new();
    for cell in &row.cells {
        if cell.width == CellWidth::WideSpacer {
            continue;
        }
        match cell.content {
            SnapshotContent::Scalar(c) => text.push(c),
            SnapshotContent::Cluster { start, len } => text.extend(row.cluster(start, len)),
        }
    }
    text
}
# let _ = row_text;
```

A cell is a scalar or a span into the row's own cluster buffer, because most
cells are one `char` and paying a `String` for every one of them is what makes a
naive grid slow. A wide glyph occupies its own cell plus a `WideSpacer`, which a
renderer skips and a text extractor drops.

That is the whole embedding. Everything after this chapter is detail on one of
those seven fragments.
