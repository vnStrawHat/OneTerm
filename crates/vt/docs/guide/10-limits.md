# 10. Hostile input, ceilings and counters

Terminal input is untrusted. The bytes arriving from a PTY were written by a
program you did not choose, possibly over a network, possibly by somebody who
would like your terminal to allocate a gigabyte or stop responding.

The engine's answer is one rule: **no byte sequence may panic the engine, and
none may cost unbounded memory.** Nothing here returns a `Result` to you,
because there is no decision for you to make -- a malformed or hostile sequence
is dropped, truncated, or degraded to a documented fallback, and counted.

## The ceilings

| Ceiling | Value | What happens past it |
| --- | --- | --- |
| OSC payload, inline | 2 KiB | truncated; the event carries `truncated: true` |
| OSC payload, claimed `large` | 8 MiB | truncated the same way |
| OSC parameters kept | 16 | bytes past the last one accumulate into it, which is what `OSC 8`'s `;`-joined URIs rely on |
| CSI parameters and sub-parameters | 32 | the sequence is marked ignored and dispatched as unhandled |
| Intermediate bytes | 2 | a third makes the sequence unhandled |
| DCS or APC payload | 16 MiB | the sequence is aborted and any partial image discarded |
| Image dimension | 4096 per axis | pixels past it are dropped rather than allocated |
| Scrollback rows | 1 000 000 hard, 10 000 by default | the oldest rows are trimmed |
| Viewport | 1024 rows, 2048 columns | clamped by `Size::clamped`, which `Terminal::new` and `resize` call for you |
| Grapheme arena | 65 536 entries, 1 048 576 chars | a sweep reclaims what no cell references |
| Shell marks tracked | 1024 | the oldest is released |
| Live image placements | bounded | the oldest is released, and you get `GraphicReleased` |

The inline and large OSC numbers are `parser::OSC_INLINE` and
`parser::OSC_LARGE`; the parameter and intermediate counts are
`parser::MAX_PARAMS`, `parser::MAX_OSC_PARAMS` and `parser::MAX_INTERMEDIATES`;
the DCS ceiling is `parser::DCS_MAX_BYTES`; the scrollback pair is
`grid::DEFAULT_SCROLLBACK` and `grid::SCROLLBACK_MAX`. They are published so
that a test of yours can assert against the same number the engine uses.

The large OSC ceiling is bought **per number**, so claiming it for your own
protocol does not let a stream spend 8 MiB under a number nobody reads. Chapter
5 has the call.

One more bound worth knowing: a batch whose arena grew past 1 MiB shrinks back
when it is cleared, so a single hostile clipboard write does not keep its
megabytes for the rest of the session.

## Where the last three of those live

Three of the ceilings above belong to `oneterm_vt::intern`, the tables that let
a cell be 64 bits: a cell stores a style id, a grapheme id and a hyperlink id,
and `oneterm_vt::intern` holds the values behind them. That is why those
ceilings are the odd ones out -- they bound a *table*, not a sequence.

A stream that prints a million distinct emoji sequences fills the grapheme
arena; a sweep reclaims every entry no live cell references, and an over-long
cluster is truncated rather than allowed to grow one. A stream that emits a
`OSC 8` link per cell fills the hyperlink table; further links are dropped,
counted in `FeedStats::hyperlink_table_exhausted`, and the text still renders
without them.

You normally never name any of it. The snapshot resolves a cell's hyperlink for
you under the lock -- `SnapshotState::hyperlink` -- and a row's clusters are
copied into the row itself. `Terminal::interner` is there for a consumer reading
the grid directly rather than through a snapshot.

## The counters

`feed` returns a `FeedStats` describing exactly that call -- never cumulative.
`Terminal::stats()` reads the last one back without feeding anything.

```rust
use std::time::Instant;
use oneterm_vt::{Config, EventBatch, Size, Terminal};

let mut term = Terminal::new(Size { rows: 24, cols: 80 }, Config::default());
let mut batch = EventBatch::new();

// A private-use CSI the engine does not implement.
let stats = term.feed(b"\x1b[?9999z", &mut batch, Instant::now());
assert_eq!(stats.bytes, 8);
assert_eq!(stats.unhandled_sequences, 1);
assert_eq!(stats.malformed_sequences, 0);

// Per call, so the next feed starts from zero.
let stats = term.feed(b"plain text", &mut batch, Instant::now());
assert_eq!(stats.unhandled_sequences, 0);
assert_eq!(term.stats().bytes, 10);
```

What each one means:

- `bytes` -- exactly the length of the slice you passed.
- `rows_scrolled` -- rows this call pushed into scrollback. Rows that scrolled
  inside the screen without reaching history are not counted.
- `unhandled_sequences` -- sequences the engine parsed but does not implement. A
  rising count on a real workload is the signal that a sequence is worth
  implementing, and it is the number to watch when a program misbehaves.
- `malformed_sequences` -- routed but unusable: today, an `OSC 52` body that is
  not valid base64 or not valid UTF-8.
- `truncated_osc` -- payloads that hit a size ceiling and lost their tail.
- `aborted_dcs` -- `DCS` sequences abandoned by `CAN` or `SUB`, or for running
  past the payload ceiling.
- `hyperlink_table_exhausted` -- `OSC 8` links dropped because the hyperlink
  table was full. The text still renders; the link is simply not clickable.
- `grapheme_truncated` and `style_table_exhausted` -- reserved, and always `0`
  today. The interner and the style table count their own exhaustion internally
  and do not yet report it here. They are in the struct so that wiring them up
  later is not a breaking change.

Every counter's meaning is part of the crate's contract: one that started
counting a different thing would be a minor version bump and a changelog entry,
because a monitor keyed to it would silently change meaning otherwise.

## Degradation, not rejection

The engine truncates an over-long OSC rather than rejecting it, because
rejecting one would break `OSC 52` for a legitimate large clipboard write. The
`truncated` flag on the event exists so that you can make the opposite choice
for your own protocol: a cut payload is a wrong payload, and dropping it is
usually right for anything with a checksum or a schema.

Similarly, an over-long grapheme cluster is truncated rather than dropped, an
image past the pixel clamp loses its tail rather than failing to decode, and a
scrollback past its limit loses its oldest rows rather than refusing to grow.
In every case the visible behaviour is degradation you can measure, never an
error you have to handle and never an allocation you did not ask for.

## What you still own

The engine's ceilings protect the engine. They do not protect your process from
your own choices: a `scrollback_limit` of a million rows is a legal
configuration and it is your memory. Nor do they make a payload safe -- a
forwarded OSC is bytes a hostile program wrote, and validating your own protocol
is your job. What the engine promises is narrower and worth stating exactly: no
input makes it panic, and no input makes it allocate without a bound you can
look up in the table above.
