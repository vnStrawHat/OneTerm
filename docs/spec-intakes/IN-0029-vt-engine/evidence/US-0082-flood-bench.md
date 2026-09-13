# US-0082 — the flood bench, before and after

Recorded, never gated (`IN-0029.md` acceptance: "no packet's exit criteria contain a
performance number"). This is a **measurement**, not a claim of improvement over the
engine being replaced — the new engine is still slower on this workload, and the intake
says plainly that the case for the rewrite is extension cost, not speed.

## How

`crates/terminal/tests/us0081_parity.rs::flood_bench`, the in-process old-versus-new
bench the `US-0081` verification round adopted into the workspace. Both engines are fed
the **same** 4 MiB of coloured text in 4 KiB chunks, grid 120x30, scrollback 10 000, with
a snapshot after every chunk, and `feed` and `snapshot` are timed separately:

```
cargo test -p oneterm-terminal --profile fast-dev --test us0081_parity \
    -- --ignored flood_bench --nocapture
```

`fast-dev` is the profile the project runs the app in: `debug_assertions` on, first-party
crates at `opt-level = 3`. One machine, three runs per column, median reported. The
"before" column was measured on this worktree at `f9af66c` before any file was edited.

## Result

| | before (`f9af66c`) | after (`US-0082`) | old engine |
| --- | ---: | ---: | ---: |
| feed / advance | 93 ms (avg 91 us/chunk) | 93 ms (avg 91 us) | 55 ms (avg 54 us) |
| **snapshot** | **75 ms (avg 73 us)** | **56 ms (avg 55 us)** | 27 ms (avg 26 us) |
| total | 168.8 ms | **150.7 ms** | 83.3 ms |
| ratio to old | 2.07x | **1.81x** | — |

Raw, three runs after:

```
NEW total 150.7114ms   new feed 93 ms (avg 91 us, p95 108)   new snapshot 56 ms (avg 54 us, p95 59)
NEW total 150.1010ms   new feed 92 ms (avg 90 us, p95 121)   new snapshot 56 ms (avg 54 us, p95 57)
NEW total 151.1777ms   new feed 93 ms (avg 91 us, p95 121)   new snapshot 56 ms (avg 55 us, p95 58)
OLD total  84.6782ms   old advance 55 ms (avg 54 us)         old snapshot 28 ms (avg 27 us)
OLD total  82.2625ms   old advance 54 ms (avg 52 us)         old snapshot 27 ms (avg 26 us)
OLD total  83.0682ms   old advance 55 ms (avg 54 us)         old snapshot 26 ms (avg 25 us)
```

## What the numbers say

**1. The snapshot half — the part this packet owns — is down a quarter,** 75 ms to 56 ms.
`US-0081`'s evidence named the residue precisely: "what is left is the shim's own
full-viewport legacy-cell rebuild, which `US-0082` deletes." It is not deleted, because
`crates/terminal-view` still reads that shape and moving it is `US-0085`; what this packet
removed is the *waste* inside it:

- the resolved `StyleRun`s are converted **once per run** instead of once per cell — at
  120 columns that is about three conversions per row rather than a hundred and twenty,
  and the conversion is the expensive part (two colour matches and an eleven-entry
  attribute walk per call);
- `IndexedCell::point` is a pure function of the display row and the scroll offset, so a
  vector already laid out at this geometry keeps its positions and the refresh writes
  cells only. That is exactly the flood's case: it scrolls further than the viewport is
  tall, so the render state reports `Full` every chunk while the offset stays at the
  sticky bottom;
- on a `Partial` that did not scroll only `RenderState::changed()` is rebuilt, and on
  `Unchanged` nothing is touched at all. The flood exercises neither — every chunk is a
  `Full` — so the interactive gain (a keystroke rebuilds one row, not thirty) is **not**
  in this table.

**2. Feed is unchanged and is not this packet's.** 93 ms before and after, against the old
engine's 55 ms. The bench calls `Terminal::feed` directly, so nothing the adapter does
appears in that column; the gap is the engine's, and `IN-0029` records that a debug-build
gap of this size is expected (`fast-dev` keeps `debug_assertions` on, and the bounded
integrity tier R-28 shipped still runs). The adapter's own per-chunk cost did drop — the
`LineAccounting` heuristic scanned every chunk for `\n` under the lock and is gone — but
that cost sits in `TerminalPump::advance`, which this bench does not call.

**3. What is left in the snapshot is the compatibility surface itself:** one
`alacritty_terminal::Cell` built per visible cell, 3 600 of them per chunk here, so the
view can read the shape it reads today. `US-0085` replaces the read rather than the
build, and the column disappears with it.

## Caveats

- One machine, `fast-dev`, three runs, medians. Not a benchmark suite; `vt-bench` is.
- The flood is deliberately the worst case for the incremental paths: every chunk scrolls
  the whole viewport away, so `changed()` never helps and `Unchanged` never happens.
- `debug_assertions` is on in both columns, which is what the app's own development
  profile does; release numbers are not measured here and no packet gates on them.
