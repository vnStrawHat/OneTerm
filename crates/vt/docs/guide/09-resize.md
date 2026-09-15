# 9. Resize and reflow

`Terminal::resize` takes the new size and a policy, resizes both screens, and
returns what it did. The size is clamped for you -- 1 to 1024 rows, 1 to 2048
columns -- so an absurd one from a window manager is not an error.

```rust
use oneterm_vt::{Config, ResizePolicy, Size, Terminal};

let mut term = Terminal::new(Size { rows: 24, cols: 80 }, Config::default());

// A height-only change: no reflow, because the columns did not move.
let outcome = term.resize(Size { rows: 30, cols: 80 }, ResizePolicy::BottomAnchor);
assert!(!outcome.reflowed);
assert_eq!(term.size(), Size { rows: 30, cols: 80 });

// A width change re-lays out every row that was wrapped.
let outcome = term.resize(Size { rows: 30, cols: 100 }, ResizePolicy::BottomAnchor);
assert!(outcome.reflowed);
# let _ = outcome.rows_trimmed;
```

## The two policies

`ResizePolicy::BottomAnchor` is what a remote PTY expects and what most terminals
do: growing the window pulls scrollback down into the top of the viewport and
the cursor moves with it. Use it for SSH sessions and Unix local shells.

`ResizePolicy::KeepViewportTop` is what the Windows console host does behind a
ConPTY: the viewport keeps its top row, the cursor moves to the row that host
addresses, and the new rows are blank at the bottom. Use it for Windows local
shells.

Choosing the wrong one is not a crash, it is the bug where a resized window
shows the prompt in the wrong place and every subsequent absolute cursor move
compounds it. The policy belongs to the transport, so pick it where you pick the
shell.

## What reflow guarantees

**Row identity survives.** A `RowId` names the same content for as long as that
content is live, and that holds across a reflow. Caches keyed by row id stay
valid; caches keyed by a viewport index do not, which is why the engine
publishes ids and not indices.

**Everything tracked moves together.** The cursor, the saved cursor, the
selection ends, the shell marks and the image placements are all entries in one
anchor list, and a reflow remaps that list in the same pass that re-lays the
rows. There is no second fix-up step to forget.

**Wrapped lines are rejoined and re-split.** A row carries a `wrapped` flag, and
a run of wrapped rows is one logical line: widening rejoins it and re-splits it
at the new width, so text a program wrote as one line stays one line.

## What reflow does not guarantee

**The alternate screen is not reflowed.** A full-screen program owns its own
layout and repaints on `SIGWINCH`; re-wrapping its rows underneath it would
produce garbage that the repaint then has to overwrite. The alternate screen is
resized and cleared where it must be, and that is all.

**A selection can be destroyed.** A reflow kills the anchors of a selection whose
content no longer stands where it did, and a history trim kills anything that
fell off the oldest end. The engine notices and releases the selection, so it
reads as "nothing selected" afterwards rather than as a range pointing somewhere
wrong. Do not assume a drag survives a resize.

**A placement can be destroyed.** A reflow can eliminate the rows an image was
anchored to. The releases queue and are delivered as `GraphicReleased` on the
next `feed`, because `resize` has no batch of its own to report them in. If your
renderer needs them sooner, feed an empty slice: `feed(b"", &mut batch, now)` is
a legal call whose only job is to deliver queued releases.

**Rows can be lost.** Narrowing turns one row into several, and the scrollback
limit is a row count, so a reflow can push the oldest rows past it.
`outcome.rows_trimmed` says how many went, and `VtEvent::RowsTrimmed` names the
new oldest id.

## The cost

A reflow re-lays every row of the primary screen, history included. That is
linear in the scrollback depth, and at a large history it is milliseconds, not
microseconds. Two things follow for an interactive drag-resize:

- **Coalesce.** A drag that produces a resize per pixel produces a full reflow
  per pixel. Debounce to the end of the drag, or to a frame, and resize once.
- **Resize on the owner thread**, not in the middle of a paint. It takes `&mut
  Terminal`, so it needs the same lock `feed` does, and it invalidates every
  row: the next `snapshot_update` returns `Full` and every consumer rebuilds.

The generation counter behind that is how a snapshot state knows to rebuild; it
is bumped by `resize`, by `set_scrollback_limit` and by anything else that
invalidates the whole grid, and a consumer never has to track it.

## Telling the child

Resizing the engine does not tell the program anything. The child learns its new
size from the transport, and the two must stay in step: resize the terminal and
the pseudo-console together, from the same place, with the same numbers. Chapter
13 has the `OnResize` half.
