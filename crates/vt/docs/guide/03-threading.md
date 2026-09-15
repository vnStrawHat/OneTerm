# 3. Threading, locking and when to draw

The engine is synchronous by construction. It has no `Mutex`, no `RefCell`, no
atomic and no global.

`Terminal` is both `Send` and `Sync`, and both are automatic: it is a plain
value holding a parser and its state, with no interior mutability anywhere in
it, so the compiler grants both. Neither is what makes the engine safe to share,
and it is worth being exact about what each one buys.

`Send` is what lets the terminal live on the thread that owns it -- typically a
reader thread -- rather than on the one that created it.

`Sync` means a `&Terminal` may be held on several threads at once, which is
useful for the read-only half of the API: `size`, `viewport`, `mode_snapshot`,
`cursor_style`, `title`, `stats` and `encode_key` all take `&self`, so a UI
thread can call them behind a read lock while another thread holds a read lock
of its own.

What `Sync` does **not** buy is concurrent use of the engine. `feed`,
`resize` and `snapshot_update` all take `&mut self`, so exactly one caller can
be mutating at a time, and the type system says so without needing to know
anything about your lock. That is the sense in which the embedder owns the
lock: the engine does not serialise anything, it simply cannot be mutated by two
callers at once, and choosing what kind of lock enforces that is your decision.

There is no atomic in the crate on purpose. A "something changed, come and draw"
flag belongs with the lock policy that reads it, and that policy is yours.

## The embedder owns the lock

The usual shape is one owner thread that reads the transport and one UI thread
that draws:

```text
  reader thread                      UI thread
  -------------                      ---------
  read bytes from the PTY
  lock
    term.feed(bytes, &mut batch, now)
    drain batch  (cheap, no I/O)     lock
  unlock                               term.snapshot_update(&mut state, now)
  wake the UI                        unlock
                                     state.map_colors(&palette)   <- no lock
                                     draw from state.rows()       <- no lock
```

Two properties make that safe without any help from the crate.

**Nothing of yours runs inside `feed`.** Events are values appended to a batch,
never callbacks. There is no re-entrancy to reason about, so holding your lock
across `feed` cannot deadlock against your own code.

**The snapshot hand-off is two phases.** `Terminal::snapshot_update` is phase
one: it copies the rows whose sequence number moved into your `SnapshotState`
and refreshes the cursor, the modes, the selection and the image placements. It
is cheap enough to run under the lock. Phase two -- `SnapshotState::map_colors`
and the drawing itself -- reads only your own state and must happen after the
lock is released.

Drain the batch under the same lock that fed it, or copy what you need out of
it: the payloads are spans into the batch's arena and the next `feed` clears
that arena.

## When to draw

`feed` appends at most one `VtEvent::Repaint` per batch, last, whenever the
chunk dispatched anything at all. It is a hint, not damage: it says "there is
something to draw", not what changed. Coalesce repaints on a frame timer rather
than drawing once per chunk -- a program that prints a progress bar can produce
thousands of chunks a second, and every one of them is one `Repaint`.

What actually changed is the return value of the next `snapshot_update`:

```rust
use std::time::Instant;
use oneterm_vt::{Config, EventBatch, Size, SnapshotState, SnapshotUpdate, Terminal};

let mut term = Terminal::new(Size { rows: 24, cols: 80 }, Config::default());
let mut batch = EventBatch::new();
let mut state = SnapshotState::new();

// The first ask always rebuilds: the state has read nothing yet.
assert_eq!(
    term.snapshot_update(&mut state, Instant::now()),
    SnapshotUpdate::Full
);

// Nothing was fed in between, so there is nothing to redraw.
assert_eq!(
    term.snapshot_update(&mut state, Instant::now()),
    SnapshotUpdate::Unchanged
);

term.feed(b"hello", &mut batch, Instant::now());
match term.snapshot_update(&mut state, Instant::now()) {
    // Shift your own row cache by `scrolled` viewport rows, then rebuild only
    // the viewport rows `state.changed()` names.
    SnapshotUpdate::Partial { scrolled } => {
        assert_eq!(scrolled, 0);
        assert_eq!(state.changed(), &[0u16]);
    }
    SnapshotUpdate::Full => {}
    SnapshotUpdate::Unchanged => unreachable!("a printed row is a change"),
}
```

`state.rows()` always holds the full viewport, for every result including
`Unchanged`, so a renderer can index it without a special case. `changed()` is
the subset this update copied; `rows_copied()` is the running total, and it is
the evidence that an idle frame costs no copy at all.

## Synchronised output

A program can ask the terminal to hold its paint with `CSI ? 2026 h` and release
it with `CSI ? 2026 l`, so a full-screen redraw is not shown half-finished. The
engine applies the bytes immediately either way -- the grid is always current --
and it is the snapshot that waits: while the mode is set, `snapshot_update`
returns `Unchanged` and copies nothing.

The suppression has a timeout, so a program that sets the mode and then dies
cannot freeze your renderer. That is why `feed` and `snapshot_update` take an
`Instant`: the engine keeps no clock of its own, because a clock read inside the
engine is a syscall you cannot batch and a value your tests cannot control.

## Threads in the crate

None, with one exception: the `pty` feature's transport runs its own internal
threads -- two on Windows, one on Unix -- none of which calls into your code,
and none of which is joined on drop. Build with `--no-default-features` and the
crate spawns no thread at all. Chapter 13 has the detail, including why not
joining them is deliberate and what it means for your shutdown ordering.
