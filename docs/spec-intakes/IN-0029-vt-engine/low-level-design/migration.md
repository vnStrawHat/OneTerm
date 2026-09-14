# Low-Level Design: Migration

Intake: IN-0029
HLD: ../high-level-design.md
Topic: migration
Date: 2026-09-12

> One concern per file. How the new engine replaces the vendored fork without the product
> stopping working in between.

## Concern

The seam the swap happens behind, the packet-by-packet order, the compatibility shim that makes
each slice independently verifiable, the exact list of code that is deleted, and the documentation
and notices that must be reconciled.

The governing constraint: **the baseline stays runnable throughout.** `main` builds and runs the
vendored engine until the shim packet flips it, and both engines coexist from `US-0072` to
`US-0087` so the differential runner always has something to compare against.

## Design

### The seam

`TerminalSession` (`crates/terminal/src/session.rs:399-...`, four composed traits) and
`TerminalContent` (`crates/terminal/src/content.rs`) are the boundary, and they keep their names.
Everything above them — `crates/terminal-view`, `crates/sftp-ui`, `crates/state`,
`crates/workspace`, `crates/app` — is unaware of which engine is underneath.

Above the seam, four files in `crates/terminal-view` import `alacritty_terminal` (R-26):
`src/render/frame.rs`, `src/input/mouse.rs`, `src/input/mouse_tests.rs` and
`src/theme/palette.rs`. The narrower claim in `frame.rs:3-7` — "the only file under `render/`" —
is the true one, and the migration list below covers all four.

### Packet order

Two packets start immediately and in parallel, because neither depends on the engine design:

- **`US-0071` — `oneterm-pty` extraction.** The PTY and the engine ship in the same crate today,
  so the transport would disappear with the fork if it were not extracted first. About 1 660 lines
  with no dependency on the grid. Details: [`pty.md`](pty.md).
- **`US-0072` — benchmark and parity harness.** Records the baseline and **blesses both
  expectation files with the old engine** before any replacement code exists. Details:
  [`testing-and-bench.md`](testing-and-bench.md).

Then the engine, bottom-up:

```
US-0073 parser ───────┐
US-0074 cell/style ───┴─▶ US-0075 grid + anchors ──┬─▶ US-0076 dispatch  (PARITY GATE GREEN)
                                                    ├─▶ US-0077 reflow ──▶ US-0078 selection
                                                    └─▶ US-0079 damage/render/events
                                         US-0080 graphics (needs 0075 + 0076)
```

Then the migration proper, one verifiable slice at a time (R-42):

```
US-0081 engine behind the seam (shim)      workspace green, zero behaviour diff
   ├─▶ US-0082 crates/terminal goes native
   ├─▶ US-0083 crates/local-shell goes native
   ├─▶ US-0084 crates/ssh goes native
   └─▶ US-0085 crates/terminal-view goes native
US-0086 deferred deviations + extension-point hardening   (after the gate and the swap)
US-0087 decommission the fork
```

### `US-0081` — the shim, and why it is a real slice

The earlier plan had one packet rewrite `crates/terminal`, `crates/ssh`, `crates/local-shell` and
`crates/terminal-view`, delete eleven call-site constructs, rewrite about 45 tests and 662 lines of
test support, and be accepted by three GUI walks. `docs/HARNESS.md` requires one coherent,
independently verifiable outcome per packet, and "rollback is `git revert` of one commit" was an
admission that the packet was the whole migration.

The split is made real by a named shim:

```rust
// crates/terminal/src/engine_shim.rs  — exists only from US-0081 to US-0085
/// Produces today's TerminalContent from the new engine's RenderState.
pub(crate) struct LegacySnapshot {
    state: RenderState,
    row_of: Vec<RowId>,      // viewport index -> RowId, so the seam can translate both ways
}
impl LegacySnapshot {
    pub fn refill(&mut self, term: &mut Terminal, out: &mut TerminalContent, now: Instant);
    pub fn display_row(&self, id: RowId) -> Option<usize>;   // RowId -> display row
    pub fn row_id(&self, display_row: usize) -> Option<RowId>;
}
```

`LegacySnapshot::refill` calls `render_update`, then fills the existing `TerminalContent` fields —
`cells: Vec<IndexedCell>`, `cursor`, `selection`, `mode`, `display_offset`, `damage`, `graphics` —
so `crates/terminal-view` compiles and behaves **unchanged**. That is the shim's whole job, and it
is what makes `US-0081`'s acceptance provable: *workspace green, differential runner zero
divergence, three GUI walks reproduced, no consumer changed*.

The two-way translation (`display_row` / `row_id`) is the seam adapter the earlier draft mentioned
in prose without designing (R-45). It is `RowId` to viewport index and back, computed once per
snapshot from the render state, and it is what lets `crates/completion`, the gutter, search and
the agent panel keep speaking display rows until their own packet moves them. It is deleted with
the last of them at `US-0085`.

**What each slice needs:**

**Disjoint scopes (N-04).** `US-0081` must touch the two backends, because each constructs the
`Arc<FairMutex<Term>>` and nothing compiles until they hold `oneterm_vt::Terminal`. What it may
touch there is bounded to the construction lines, and everything else is explicitly the later
packet's:

| Slice | May touch | Must not touch |
| --- | --- | --- |
| `US-0081` shim | all of `crates/terminal`; in `crates/local-shell` and `crates/ssh` **only** the type of the shared terminal, its construction call and the `Cargo.toml` line that adds `oneterm-vt` | their read loops, their transports, their resize paths, their tests |
| `US-0082` `crates/terminal` native | `model.rs`, `session.rs`, `content.rs`, `search.rs`, `url*.rs`, `palette.rs`, `osc_color.rs`, `logging.rs`, `test_support.rs`, `backend/` | any other crate |
| `US-0083` `local-shell` native | `take_render_demand()` at the chunk boundary and the guard dropped when it answers `true` — the loop that holds the lock across reads, so the one that needs it; the `oneterm-pty` token constants replace the last local ones | `crates/terminal`; **not** the manifest line (see below) |
| `US-0084` `ssh` native | the same yield check for the tokio task | `crates/terminal`; **not** the manifest line |

**Neither backend can delete its `alacritty_terminal` manifest line, and `US-0084` measured why.**
Removing it fails with five `E0433`s whose span is the `impl_pty_terminal_session!` *invocation*:
the macro, in `crates/terminal/src/session.rs`, expands `::alacritty_terminal` paths into each
backend for the trait's own signatures — four `vte::ansi::Rgb` parameters of `set_default_colors`
(`:515-518`) and `selection::SelectionType` in `mouse_down` (`:611`). No hand-written impl escapes
them, and **no `crates/ssh` or `crates/local-shell` source file names the fork**: the only two hits
per crate are the manifest line and its comment.

The same macro blocks the resize policy: it generates
`fn resize_policy(&self) -> $crate::model::ResizePolicy`, so although `TerminalModel::new` already
takes `impl Into<oneterm_vt::ResizePolicy>`, passing the engine's enum is **not** one token in the
backend — the macro's return type has to change first, in `crates/terminal`.

Both are therefore **`US-0085`'s**, which is where the compatibility surface goes. The backends'
final act is one line each, deleting the manifest entry once the macro stops expanding the name:
**assigned to `US-0085`'s Handoff as a two-line follow-up**, not to `US-0087`, so the fork's last
references leave with the surface that forced them rather than waiting for the decommission.
| `US-0085` `terminal-view` native | `render/frame.rs`, `plan_cache`, `input/mouse.rs`, `mouse_tests.rs`, `theme/palette.rs`; deletes `engine_shim.rs` | the backends |

None of the five can run behind the old engine, which is exactly why `US-0081` exists: it is the
one flip, it changes no behaviour, and every later slice is a refactor with the differential
runner still available.

### What the view still needs from the engine, and who owns it

Taken from the `US-0079` verification, which inventoried `crates/terminal/src/content.rs` and
`crates/terminal-view/src/render/frame.rs` against the shipped `RenderState`. **`RenderState`
alone cannot drive `frame.rs` yet**; every gap has an owner and none of them is the adapter's to
invent.

| Gap | View site | Owner |
| --- | --- | --- |
| **Cursor shape** — Block / Beam / Underline / HollowBlock / Hidden | `render/frame.rs:421-440`, `render/cursor.rs:54-55` | **`US-0076`** — `CursorStyle` is part of the dispatch packet (`DECSCUSR`) |
| **Selection range** — `start`, `end`, `is_block` | `render/frame.rs:451-463`, `:547-560`, `render/overlay.rs:32-67` | **`US-0078`** — [`selection.md`](selection.md) |
| **Per-cell graphic offset** — `RenderCell.graphic` carries the id only; the painter needs the `(col, row)` offset *inside the image's cell grid*, plus `width` / `height` / `rgba` and the virtual cell | `render/frame.rs:264-271`, `:313-317`, `render/element.rs:332-375` | **`US-0080`** — the `Placement` table must be able to reproduce that offset; see [`graphics.md`](graphics.md) § "Ownership" |
| **Absolute output-line count** for the gutter | `terminal_view/gutter_timestamps.rs:66`, `:84` | **`US-0076`** — `Terminal::lines_produced()` |
| **`size()` on the render state** — the view reads `GridSize { rows, cols }` | `render/frame.rs:577-582`, `render/element.rs:108-118`, `render/overlay.rs:36-44` | **`US-0079`** — a public accessor over the field it already stores |
| **Dim colour rule** — see below | `render/row_plan.rs:197-199` | **`US-0079`** — the `Palette` must be able to express it |
| **`last_content_line` / the clear epoch** | `terminal_view/gutter_timestamps.rs:91` | **`US-0081`** — adapter-side, read off the grid, not the render state |

**The dim rule is OneTerm's, not the generic one.** `Palette` must reproduce
`crates/terminal/src/palette.rs:128-137`: a dim colour is a **50 % mix with the background**, not a
fixed fraction toward black. The view then applies its own `fg.a *= 0.7`
(`render/row_plan.rs:197-199`) on top, and that stays where it is. A `Palette` whose `named()`
hard-codes a dim derivation with no override hook cannot express this, so the type takes the dim
colours from the adapter like every other themed colour.

### Per-crate swap detail

**`US-0082` — `crates/terminal`.** `model.rs`, `session.rs`, `content.rs`, `search.rs`, `url.rs`,
`url_policy.rs`, `palette.rs`, `color_classification.rs`, `osc_color.rs`, `logging.rs`,
`test_support.rs` and the whole `backend/` module move onto the native API: `RowId`,
`render_update`, the batch drain, `ColorKey`, `ModeSnapshot`.

**`mouse_encode.rs` moves at `US-0085`, not here.** It takes `TermMode`, and so does
`TerminalQueryState`, which the crate publishes: converting the encoder while the vocabulary around
it is still the old one leaves two representations live and rewrites the encoder's fixtures twice.
It goes when the compatibility surface goes.

**Search, URL detection and the `GridText` snapshot (R-19).** `crates/terminal/src/search.rs:70-148`
copies every cell of history plus viewport under the lock, once, so the search itself runs
unlocked; `url.rs` and `url_policy.rs` read the same snapshot. The new API offers
`row_text(RowId, &mut String)` per row, which would mean looping under the lock. **`GridText`
stays**, rebuilt adapter-side: one lock, `for id in term.row_range() { term.row_text(id, &mut s) }`
into the existing owned structure, then unlocked matching exactly as today. It is the same
O(rows x cols) lock-held copy the design argues against elsewhere, and it is kept deliberately
because search is user-initiated and rare, unlike a per-frame snapshot. A later packet may make it
incremental off the row sequence numbers; that is not this intake.

**`US-0085` — `crates/terminal-view`.** `render/frame.rs` consumes `RenderRow` and `RenderCell`;
`plan_cache` keys on `(RowId, SeqNo)`; `theme/palette.rs` and `input/mouse.rs` move to the engine's
`Rgb` and `SelectionKind`; `mouse_tests.rs` follows.

### Snapshot deviations the shim declares (S1-S5)

`LegacySnapshot` produces the old engine's `TerminalContent`, and the differential proves it does so
exactly — **except** in five ways. The intake's `C` and `D` tables cannot express them, because
those compare `grid.expect` and `state.expect`, not the snapshot, so they live here. Each has no
reader today, and each names the packet that may remove it.

| # | Delta | Why it is not user-visible | Removed by |
| --- | --- | --- | --- |
| S1 | `TermMode::LINE_WRAP` and `URGENCY_HINTS` are never set (all 81 streams) | `ModeSnapshot` does not carry them, and `research/api-surface.md` § 3.5 lists both under the unused mode bits OneTerm never queries. The only reader is the corpus dumper in `crates/tools` | `US-0085`, when the view stops reading `TermMode` at all |
| S2 | `TermMode::ORIGIN` is never set (15 streams) | Same list, same absence of a reader. `DECOM` is honoured inside the engine; only the snapshot bit is missing | `US-0085` |
| S3 | A hyperlink with no `id=` gets `1`, `2`, … where the reference gave `0_alacritty` | The implicit-id counter is per terminal by design ([`cell-and-style.md`](cell-and-style.md)), not the reference's process-global atomic. The view hashes `id + "\x00" + uri`, so only distinctness matters, and an explicit `id=` passes through verbatim | `US-0085`, when the view keys on `HyperlinkId` |
| S4 | `total_lines` is one smaller after a Sixel (`sixel_basic`: 10 versus 9) | Engine-level, not the shim: the image's history depth. It reaches the user as a scrollbar one row short in a session that printed an image | `US-0080` follow-up |
| S5 | Damage is **narrower** | The reference damages a row on any write, the engine on an actual change. `us0081_parity::damage_soundness_detail` proves the property that matters — every row whose rendered content changed, plus a visible cursor's row, is always in the new `Partial` list — with **zero violations over all 81 streams**. Nothing is under-damaged, so no stale row survives a frame | never; this one is an improvement |

One engine-level answer also changed and is recorded here because the shim is where it became
observable: **DA2 replies `ESC [ > 0 ; 502 ; 1 c` instead of `ESC [ > 0 ; 2601 ; 1 c`**. The formula
is unchanged — it encodes `CARGO_PKG_VERSION`, which is now the workspace's version rather than the
fork's. Programs read DA2 to identify the terminal, so its long-term home is
[`dispatch-and-modes.md`](dispatch-and-modes.md) § "Answers".

**The differential that proves all of this** is `crates/terminal/tests/us0081_parity.rs`: 81 byte
streams fed to both engines, snapshots compared field by field, with **only the S-kinds above
allow-listed**. Anything else fails. It is the seam-level counterpart to `vt-diff` and, like the
old-engine paths it drives, it **retires at `US-0087`**.

### The event-order rule

Exactly **one** repaint hint per batch, emitted **after** that batch's reliable events. The pump
owns it: `TerminalPump::finish_batch` posts the single `SessionEvent::Output` once the deferred
reliable events have been flushed, which is what `docs/terminal-backend.md` promises — "reliable
events emitted during a batch, then that batch's `Output`".

**The engine's `VtEvent::Repaint` is therefore not forwarded.** Routing it to
`SessionEvent::Output` in `OscRouter::drain` produces a second, *earlier* hint: it arrives while the
batch is still draining, before the reliable flush, which both doubles the repaint hints under load
and inverts the documented order. The engine still emits `Repaint` — it is the engine's statement
that something changed — but at the seam it is dropped, because the pump already knows.

### The adapter contract, as `US-0082` shipped it

The shim's `LegacySnapshot` is gone; these four shapes are what `US-0083`, `US-0084` and `US-0085`
build against.

**`TerminalHandle` — the lock and the demand flag in one place.** `SharedTerminal =
Arc<TerminalHandle>` wraps `FairMutex<Engine>` plus one `Demand`
([`damage-and-render-state.md`](damage-and-render-state.md) § "Fairness and reply latency"):

- `Demand` is a **count of waiting renderers**, not a one-shot flag: `raise()` before blocking,
  `release()` once the lock is held, `is_raised()` to look. `Demand::take` is deleted.
- `lock_for_render()` is **raise, lock, release** — the waiter clears its own demand, on
  acquisition. The render side is already wired: `TerminalModel::snapshot` and `snapshot_into` go
  through it.
- `take_render_demand() -> bool` is the **pump's** yield check: call it at a chunk boundary,
  **after the batch's replies have left** (R-37), and drop the guard when it answers `true`. It
  **reads without clearing** — the name is kept so both pump loops read unchanged — so a standing
  demand survives more than one ask and a pump may yield at several consecutive boundaries while a
  frame is queued. `render_demand_raised()` is the same read, for diagnostics.
- `lock()`, `lock_unfair()` and `try_lock_unfair()` still compile at today's call sites.

It is measured, and the difference is not marginal. Re-measured after the count fix, one pump thread
feeding 4 MiB as 1024 chunks and checking at each boundary: honoured, **1 batch / about 0.9 ms**;
ignored, **1016 batches / about 850 ms**, 3 of 3 runs each. (The earlier `1 batch / 157 us` against
`3800 / 354 ms` is the same shape at a different chunk size and optimisation level; the ratio —
one batch against a thousand — is the property.)

**`US-0083` wired the local-shell half, and it is the one that needed it.** That loop holds the
engine lock across reads, so the flag alone is not enough: the read itself is capped at
**`MAX_LOCKED_READ = 64 KiB`**, which bounds the bytes handed to one `pump.advance` and therefore
one lock hold. `READ_BUFFER_SIZE` is deliberately unchanged — it is what the contended path
accumulates into, and shrinking it spun a test binary at 100 % CPU.

| Transport | Bytes per lock hold | Worst frame wait |
| --- | --- | --- |
| Real ConPTY, `fast-dev` — what ships | p50 **82 B**, max 9.6 KB | **52-59 us** |
| Loopback TCP fixture, before the cap | p50 ~634 KB | 86-125 ms |
| Loopback TCP fixture, after the cap | at most 64 KiB | **6-17 ms** |
| Yield disabled (negative control) | — | **never arrives** |

Throughput improved with the cap rather than regressing: **52.6-54.9 MiB/s**, independently
reproduced as 41.4 → 54.2 MiB/s.

**A yield is a batch boundary.** Capping reads meant a flooded socket never runs dry, so the inner
read loop stopped exiting — and `finish_batch` only ran when it exited. The first capped run
processed zero lines: no line count, no repaint hint and no title, cwd or OSC event reached the UI
for the length of the flood. ConPTY hides this because it runs dry tens of thousands of times a
second; a socket does not. The yield therefore calls `finish_batch_blocking(true)`, which is safe
because the guard is already dropped (CORR-01).

**Staying in the read loop is now only a preference.** It was a workaround for the Windows ring
delivering readiness edge-once behind a `PollMode::Level` registration; `US-0071`'s rework made the
ring genuinely level-triggered on both platforms ([`pty.md`](pty.md) § "Readiness is
level-triggered"), so a loop that returns to the poller with bytes still buffered is woken again.
Draining before going back is one fewer completion packet, not a correctness requirement.

**Gap 6 — the demand flag is one-shot, and that is a race.** `take_render_demand` clears by asking,
so a frame whose demand is consumed inside `lock_for_render`'s own raise-then-block window is not
served and starves for the length of the flood. Every frame that *asks* is served; this one asked
and lost its flag to the pump. The fix is in `crates/terminal`, not in either backend: **the demand
must stay observable until the renderer actually acquires the lock**, so the flag is cleared by the
acquisition rather than by the question. It is owned as a `US-0082` rework.

**`US-0084` wired the SSH half.** `ssh_main_task` calls `take_render_demand()` at the chunk
boundary, after `finish_batch` has sent the batch's events and after the replies have left, and
yields rather than dropping a guard — it locks per chunk, so it has no guard to drop, which is a
strictly stronger answer. Measured under a loopback flood, a waiting frame gets the engine in
**394 us**, the same order as the 157 us the handshake bench predicts. The yield is **insurance** in
that shape: the 354 ms starvation case belongs to the loop that holds the lock across reads, which
is `crates/local-shell`'s — so **`US-0083` is where the flag stops being insurance and starts being
the fix**. A mutation check proves the call is load-bearing: commenting it out fails
`task_tests::the_task_yields_the_engine_to_a_waiting_frame` on the "never took the render demand"
assertion.

`US-0084` also pinned the resize policy by behaviour rather than by name: a test asserts the
grid **anchors the bottom row** on a grow, mutation-checked, instead of asserting which enum
variant was passed.

**`TerminalModel::new(term, impl Into<oneterm_vt::ResizePolicy>)`.** It already accepts the
engine's enum, but the backends cannot yet pass it: `impl_pty_terminal_session!` generates
`fn resize_policy(&self) -> $crate::model::ResizePolicy`, so the macro's return type changes first,
in `crates/terminal`, at `US-0085`. Only then are `crate::model::ResizePolicy` and its `From`
deleted. Until then each backend keeps its adapter-enum token, and its *behaviour* is pinned by a
mutation-checked test rather than by the token's name.

**`OscRouter::drain(&batch, &mut Vec<SessionEvent>)`.** The deferred/reliable sink is gone.
`TerminalPump::advance` feeds and drains under the lock; `finish_batch[_blocking](repaint)` sends
afterwards. There is nothing left to move out from under the lock, so the backend loops keep their
current shape.

**`TerminalContent` owns the `RenderState`, and is no longer `Clone`.** Read the frame through
`update()`, `rows()` (always the full viewport), `changed()`, `size()`, `render_cursor()`,
`modes()`, `selection_range()`, `placements()`, `hyperlink(id)`, `scroll_offset()`, and the two-way
`row_id(display_row)` / `display_row(RowId)`. Losing `Clone` is deliberate: per-view ownership is
what `DEC-0015` asks for, and it removes the shim-era hazard of two views sharing one watermark.
Nothing outside `crates/terminal` cloned a content.

### Debug-build cost at the shim, measured by `US-0081`

The flip is where a debug-build cost becomes visible, because `fast-dev` is how the app is actually
run during development. Measured on a flood workload:

In-process, 4 MiB of coloured text through 4 KiB chunks, grid 120x30, scrollback 10 000, snapshot
after every chunk, `fast-dev`:

| | new | old | ratio |
| --- | --- | --- | --- |
| `feed` / `advance` | 97 ms | 54 ms | 1.8x |
| snapshot | 78 ms | 24 ms | 3.3x |
| **total** | **177 ms** | **79 ms** | **2.2x** |

Release: **90 ms against 43 ms**. The residue is not the engine: it is **the shim's own
full-viewport legacy-cell rebuild**, which exists only to produce the old `TerminalContent` shape
and which `US-0082` deletes. It is well inside a frame budget, and the intake forbids a performance
number as an exit criterion, so this is recorded, never gated.

**After `US-0082`** (same bench, median of three): total **150.7 ms** new against **83.3 ms** old
(feed 93 / snapshot 56 versus 55 / 27). With debug assertions off — the release shape — it is
**100 ms against 78 ms, a ratio of 1.29x**, roughly half the shim era's 2.1x. The gap decomposes as:

| Cost | Size | Owner |
| --- | ---: | --- |
| Bounded integrity walk and debug asserts, in `feed` | ~25 ms | not a shippable cost; off in release |
| Integrity walk inside `render_update` | ~21 ms | same |
| Engine parse and dispatch above the old engine | ~13 ms | the intake's own engine cost |
| **The legacy `Cell` rebuild that remains** | **~9 ms** | **`US-0085`** — the only line item a later packet in this intake can still delete |

That last row is the number `US-0085` inherits: about 9 ms, not the 28 ms a reading of the snapshot
column alone would suggest.

The path to it, kept because it is how two real defects were found:

| Stage | Per-frame cost |
| --- | --- |
| Old engine | ~200 us |
| New engine, first measurement | 44 ms |
| After adding `oneterm-vt` to the `fast-dev` `opt-level = 3` list | 6.5 ms |
| After the bounded-integrity rework (`US-0075` / `US-0079`) | the table above |

Two things follow. **`oneterm-vt` now sits in the `[profile.fast-dev]` opt-level-3 list in the root
`Cargo.toml`**, beside the other hot-path crates, for the same reason they are there: a debug-level
VT engine makes a full-screen TUI unusable. And the residual 6.5 ms was the unbounded
`assert_integrity` walk, which the rework reduced by three orders of magnitude
([`testing-and-bench.md`](testing-and-bench.md) § 1); `US-0081` re-measures and records the final
number, which is the one the GUI walks are performed against.

### Deletion list

Deleted outright (code), each in the packet named:

| What | Where | Packet |
| --- | --- | --- |
| `resize_keeping_viewport_top` | `crates/terminal/src/model.rs:481-527` | `US-0082` |
| `conhost_cursor_row` (the scratch-grid probe) | `crates/terminal/src/model.rs:528-541` | `US-0082` |
| `LineAccounting` — the whole file | `crates/terminal/src/backend/line_accounting.rs:1-49` | `US-0082` |
| The deferred/reliable tier of `SessionEventSink` and `flush_reliable[_blocking]` | `crates/terminal/src/backend/event_sink.rs`, driven from `pump.rs:163-178` | `US-0082` |
| `TermDamageInfo`'s display-line conversion | `crates/terminal/src/content.rs:66-106` | **`US-0085`** — it is part of the compatibility vocabulary the view still reads |
| The per-frame cell clone loop in `refill` | `crates/terminal/src/content.rs:173-222` | **`US-0085`** — `US-0082` moved it behind `RenderState`, but the legacy `Cell` rebuild survives until the view reads `RenderRow` directly (about 9 ms of the flood below) |
| The 256/257/258 colour index constants | `crates/terminal/src/osc_color.rs:23-27` | `US-0082` |
| `NamedColor` discriminant arithmetic | `crates/terminal/src/palette.rs:136`, `crates/terminal-view/src/render/frame.rs:118`, `:121` | `US-0082`, `US-0085` |
| The second `vte::Parser` in session logging | `crates/terminal/src/logging.rs:8`, `:57-83` | `US-0082` |
| The two display-offset fallbacks | `crates/terminal-view/src/render/frame.rs:514-532` | `US-0085` |
| `engine_shim.rs` (`LegacySnapshot`) | `crates/terminal/src/engine_shim.rs` | `US-0085` |
| The locally redeclared `PTY_CHILD_EVENT_TOKEN` and the two cfg'd child-pid helpers | `crates/local-shell/src/event_loop.rs:58-64`, `:168-179` | `US-0071` |
| `vt-diff` and every `--engine old` path | `crates/tools` | `US-0087` |

**Executed at `US-0087` (`c8d84ff`).** Everything below is gone; the list is kept as the record of
what the decommission covered.

Deleted outright (the fork and its scaffolding), all at `US-0087`:

| What | Where |
| --- | --- |
| The vendored trees | `vendor/vte/`, `vendor/alacritty_terminal/` |
| The patch series (823 lines, five patches) | `vendor/patches/` |
| The refresh tool and its readme | `vendor/refresh.sh`, `vendor/README.md` |
| The CI job proving the trees are pristine plus patches | `.github/workflows/ci.yml:65-68` |
| The `vendor/**` path triggers | `.github/workflows/ci.yml:23`, `:41` |
| The workspace exclusion of the vendored trees | `Cargo.toml:24-26` |
| The `alacritty_terminal` git dependency and its comment block | `Cargo.toml:68-76` |
| The profile overrides for the fork | `Cargo.toml:172`, `Cargo.toml:200` |
| The `[patch]` block | `Cargo.toml:234-244` |
**`US-0085` must precede `US-0087`.** `TerminalContent`, `SearchMatch` and `TerminalInfo` still
publish `alacritty_terminal` value types — the *compatibility surface* — and `crates/terminal-view`
reads them. They are public API, not an internal detail, so the fork cannot be deleted until the
view stops consuming them.

| `alacritty_terminal.workspace = true` — the **last** manifest lines. No backend source file names the fork; the lines survive only because `impl_pty_terminal_session!` expands `::alacritty_terminal` paths into each backend for the trait's own signatures, which is `crates/terminal`'s to stop. `crates/terminal` deletes its own line when the compatibility surface goes (`US-0085`), and the two backends then delete theirs — one line each, assigned to `US-0085`'s Handoff. `crates/terminal-view` at `US-0085`; `crates/tools` at `US-0087` with `vt-diff` | `crates/terminal/Cargo.toml:19`, `crates/local-shell/Cargo.toml:20`, `crates/ssh/Cargo.toml:20`, `crates/terminal-view/Cargo.toml:32`, `crates/tools/Cargo.toml:35` |
| The two fork rows in the notices header | `scripts/third-party-notices.py:85-86` |
| The `--full` step running `vendor/refresh.sh --check` | `scripts/ci-local.sh`, `scripts/ci-local.ps1` |
| The `vte` dev-dependency and the differential oracle | `crates/vt/Cargo.toml`, `crates/vt/tests/differential.rs` |
| **`vt-corpus bless` and the `US-0072` cross-check** — a scope extension, **ratified** | `crates/tools`. Not in the original list, and correctly taken: under R-58 only the **old** engine may bless, so with the fork gone there is no engine that may write an expectation. Keeping the subcommand would have meant shipping a writer with nothing behind it, and a `--engine new` bless is precisely the self-referential gate R-58 forbids. The rule is therefore no longer "`bless` refuses without `--deviation`" but the stronger **"nothing blesses"**: the 46 frozen expectations are read-only artefacts, and a genuine future change to them is a reviewed, hand-authored diff with its reason, not a tool run |

Rewritten, not deleted: `crates/terminal/src/test_support.rs` (662 lines) and
`crates/terminal-view/src/render/frame.rs`'s `FrameBuilder` (`frame.rs:586-757`).

### Tests that change

| Suite | File:lines | Disposition |
| --- | --- | --- |
| Resize / ConPTY policy, 10 tests | `crates/terminal/src/model.rs:616-910` | **Kept running against the OLD engine until `US-0082`** (R-44), while the engine-side equivalents are built in `US-0077`. Both suites must be green at `US-0082`; only then is the old one deleted. Those ten tests are the only written form of the `KeepViewportTop` contract, so translating them in the same packet that implements the new coordinate model would remove the independent check |
| Sixel, 10 tests | `crates/terminal/src/sixel_tests.rs:46-271` | **Move** into `oneterm-vt` as `graphics::tests::*` at `US-0080`; the old file stays until `US-0082` |
| Snapshot / damage, 5 tests | `crates/terminal/src/content.rs:269-324` | **Rewrite** at `US-0082`: "damage is Full until the first reset" becomes "the first `render_update` is `Full`"; "Partial with only the cursor row" becomes "`Partial` with no changed rows when only the cursor moved" |
| Search, 11 tests | `crates/terminal/src/search.rs:226-344` | **Keep** at `US-0082`, with `Line`/`display_offset` assertions replaced by `RowId`; the `display_row(display_offset)` conversion test is deleted with the conversion |
| Router / pump, about 25 tests | `crates/terminal/src/backend/backend_tests.rs:98-640` | **Keep** the event-mapping and ordering coverage at `US-0082`; **delete** the deferred-flush and CORR-01 deadlock cases, unreachable once events are values. Each deletion named in the packet with that reason |
| Frame conversions, 5 tests | `crates/terminal-view/src/render/frame.rs:764-890` | **Rewrite** at `US-0085` |
| Mouse tests | `crates/terminal-view/src/input/mouse_tests.rs:218`, `:242` | **Keep** at `US-0085`, on `ModeSnapshot` and `SelectionKind` |
| Loopback PTY loop | `crates/local-shell/src/event_loop_tests.rs:196-...` | **Move** to `oneterm-pty` at `US-0071`, unchanged in substance |

Every rewritten or deleted test names, in its packet, what it used to pin and what pins it now.

### Cleanup before decommission (`US-0087`) — done

Two small code items the `US-0086` verification surfaced. Neither was a defect in shipped behaviour
and neither justified its own packet; both were one-liners with a test, and `US-0087` was the last
packet to touch this code, so they rode with it. **Both shipped**, pinned by
`crates/vt/tests/us0087_cleanup_rows.rs`:

| Item | Rule |
| --- | --- |
| **Reverse wrap stays inside the region** — done | `BS` at column 0 with `? 45` set crosses into the previous row only when the cursor is **inside** the scroll region (and the origin-mode region when `DECOM` is set). Guard at `crates/vt/src/grid/screen.rs:~667`. Pinned by `verify_reverse_wrap_blocked_at_a_non_zero_region_top` and `verify_reverse_wrap_crosses_once_the_region_is_dropped`. **The guard also refuses a cursor parked *above* the region** — see the note below; that is intended |
| **LNM answers through `inert_state`** — done | `? 20` is tracked and inert (deviation D9), so it sits in the same table as `? 9001` and `DECRQM` answers `Reset` whatever a program sets. Pinned by `verify_decrqm_lnm_is_consistent_and_never_claims_set`, and by the `Mode::ANSI` walk that proves nothing was lost when LNM joined the table |

Recorded parity, **not** a cleanup item: `CUB` (`CSI D`) does not reverse-wrap even with `? 45`
set. The reference applies the mode to `BS` only, and so does this engine.

**A cursor at or above the region top cannot reverse-wrap at all — intended, recorded here
(`US-0087` verifier note 8).** The guard refuses the crossing not only when the cursor sits *on*
the region's top row but also when it is parked *above* the region entirely, which is wider than
the cleanup row literally asked for. It is kept, for three reasons. The row an out-of-region cursor
would wrap into is not the region's to write, so refusing is the conservative direction and its
failure mode is the mode's own default: `BS` does nothing. Reverse wrap is an xterm extension that
is **off by default** and that no recording exercises — `grep-deviations` finds `? 45` in six
recordings and every occurrence is a *reset* — so the wider refusal is unmeasurable and cannot move
the gate. And narrowing it would mean editing shipped engine code in the last packet before the
merge, with no evidence that any program wants the wider behaviour. Pinned deliberately by
`verify_reverse_wrap_above_the_region_is_also_blocked`, and stated in the deviation table's D12 row.
Revisit only if a real program is observed relying on reverse wrap outside the margins.

### Deleted tests, and what pins them now

`US-0082` deleted 26 adapter tests that drove the **old** engine. Nothing was lost; each has a named
successor, and R-44's condition was met first — the old and the new suite were green in the same
commit before the deletion.

| Deleted | Count | What pins it now |
| --- | ---: | --- |
| `legacy_resize.rs` (`keep_viewport_top_*`, `default_grow_*`) | 15 | `oneterm_vt::reflow::tests::keep_viewport_top_*` — 20 tests, a superset by name, against the new engine. The adapter's own share (that the backend's policy reaches the engine) is `model_tests::resize_grid_applies_the_backend_policy` |
| `sixel_tests.rs` | 11 | `oneterm_vt::graphics::tests` — 26 tests, landed at `US-0080`. The adapter's share is `model_tests::a_sixel_reaches_the_snapshot_once_with_per_cell_offsets` |

`crates/terminal/tests/us0081_parity.rs` still passes over the same 81 streams with the same
five-difference allow-list, which is the proof that the **native** frame path produces the snapshot
the shim produced.

### Documentation reconciliation (R-46)

Each doc edit lands in the packet that changes the code it describes, because
`scripts/verify-dependency-graph.py` carries a manifest allow-list that must be edited in the same
commit as a new crate or CI fails — which made the earlier "all docs at the end" schedule
impossible anyway.

| Doc | Change | Packet |
| --- | --- | --- |
| `docs/agents/structure.md` §1, §3 | add `crates/pty` | `US-0071` |
| `scripts/verify-dependency-graph.py` allow-list | add `oneterm-pty` | `US-0071` |
| `docs/agents/dependencies.md` §3 | `polling`, `windows-sys`, `libc` under the new crate | `US-0071` |
| `.github/workflows/ci.yml` | the bench job | `US-0072` |
| `docs/agents/dependencies.md` §3 | `proptest`, the `vte` dev-oracle, the `cargo-fuzz` nightly exception | `US-0072` |
| `docs/agents/structure.md` §1, §3 + the allow-list | add `crates/vt` | `US-0073` |
| `docs/agents/crate-dependency-rules.md` R6/R7/R8 | stop naming `alacritty_terminal`; name the two new crates | `US-0073` |
| `docs/agents/dependencies.md` §3 | `memchr`, `unicode-width`, `unicode-segmentation`, `smallvec`, `bitflags`, `rustc-hash`, each pinned to its 2.x / current line (R-23, R-48) | `US-0073`, `US-0074` |
| `docs/osc-sequences-checklist.md` | real coverage, and the three stale statements fixed | `US-0076` |
| `docs/terminal-backend.md` §5 | `feed`-and-drain, watermark damage, the demand signal | `US-0081` |
| `docs/terminal-backend.md` §5.3 | the native `ResizePolicy` | `US-0082` |
| `docs/spec-intakes/IN-0018-.../high-level-design.md` | the frame pipeline consumes a render state | `US-0085` |
| `docs/spec-intakes/IN-0028-.../{high-level-design,low-level-design/vendor-graphics}.md` | superseded in place, pointing at [`graphics.md`](graphics.md) | `US-0080` |
| `docs/architecture.md` | the two new crates | `US-0071`, `US-0073` |
| `docs/terminal-backend.md` §4, `docs/agents/dependencies.md` §1, `docs/PROJECT.md`, `README.md`, `vendor/README.md`, `THIRD-PARTY-NOTICES.md`, `NOTICE` | **removal** rows only | `US-0087` |

`python scripts/check-doc-paths.py` covers `docs/architecture.md`, `docs/agents/*.md`,
`docs/README.md`, `README.md` and `AGENTS.md`, so every `vendor/...` path in those five must be
gone before `US-0087` can pass CI.

### Notices and licensing

| Change | Detail | Packet |
| --- | --- | --- |
| Added | the vendored ref-test recordings: alacritty, Apache-2.0, revision recorded, "test data, unmodified" | `US-0072` |
| Added | the ported `avt` reflow iterator: Apache-2.0, with the § 4(b) "state changes" notice in the source header, `NOTICE` and the notices file | `US-0077` |
| Removed | the two `THIRD-PARTY-NOTICES.md` § 2 rows and the "pristine upstream plus the listed patch set" prose in `scripts/third-party-notices.py:85-86`, and the matching claim in `NOTICE` | `US-0087` |
| Unchanged | `THIRD-PARTY-NOTICES.md` § 1 — the bundled `conpty.dll` and `x64/OpenConsole.exe` rows with their SHA-256 hashes. Only § 2 is removed; the console-host bundle is owned by IN-0030 and is not touched by this intake | — |
| Unchanged | `deny.toml`'s allow-list: every new direct dependency is already permitted | — |
| Verified by | `python scripts/third-party-notices.py --check` in CI | `US-0087` |

Design ideas carry no obligation. kitty (GPL-3.0) and Warp (AGPL-3.0) were read for design only;
no line is transcribed, and commit messages keep that separation explicit.

## Interfaces

`LegacySnapshot`, above, is the only new interface this file owns. It is `pub(crate)` in
`crates/terminal` and lives for four packets.

## Edge Cases and Failure Modes

- [ ] **A packet lands half-swapped** — each compiles and tests green on its own; `US-0081` is the
  single flip and changes no behaviour.
- [ ] **The differential runner finds a divergence late** — it runs from `US-0072`, so it is
  available from the first engine packet.
- [ ] **The parity gate goes red after the flip** — the fork is still vendored until `US-0087`, so
  the comparison is available and `git revert` restores a working build.
- [ ] **A GUI walk regresses** — it is `US-0081`'s acceptance evidence; a failure reopens that
  packet rather than opening a new BUG (`docs/HARNESS.md` routing table).
- [ ] **`vendor/refresh.sh --check` failing mid-migration** because someone edits the vendored tree
  to work around a difference — forbidden. The fork is frozen from `US-0073`; a difference is fixed
  in the new engine or recorded as a deviation.
- [ ] **The unbounded `osc_raw` in the still-shipping fork** stays exploitable from any SSH session
  until `US-0081`. Pre-existing, and an explicit owner decision in `IN-0029.md`.
- [ ] **`crates/completion`, the gutter and the agent panel still speak display rows** after
  `US-0081` — `LegacySnapshot`'s two-way translation covers them until `US-0082` and `US-0085`.

## Verification

- [x] `US-0071`: **met** (`2095c87`, rework `2bd247c`) — `grep -rn "alacritty_terminal::tty" crates/` empty; `cargo test -p oneterm-pty`
  green; a local shell opens, resizes and exits on Windows.
- [x] `US-0081`: **met** (`f9af66c`) — `cargo test --workspace` green with the new engine behind the shim; `vt-diff`
  reports no divergence over the 45 recordings and a captured session; the three GUI walks **plus
  IN-0028's Sixel walk** reproduced with fresh screenshots (N-02); no file outside
  `crates/terminal` changed except the bounded backend lines listed in the scope table above, and
  no backend test changed (N-04).
- [x] `US-0082`: **met** (`d3c537b`, rework `b0c82e1`) — `grep -rn "resize_keeping_viewport_top\|conhost_cursor_row\|LineAccounting" crates/`
  empty; the old `model.rs` resize suite and the new `reflow::tests` suite both green in the same
  commit, then the old one deleted (R-44).
- [x] `US-0085`: **met** (`ee9057a`) — `engine_shim.rs` deleted; `plan_cache` keyed on `(RowId, SeqNo)`.
- [x] `US-0087`: **met** (`c8d84ff`) — the two cleanup rows above are done, each with its test; `test -d vendor` fails; `grep -rn "alacritty_terminal\|vendor/" Cargo.toml .github/workflows/ci.yml scripts/`
  empty; `python scripts/third-party-notices.py --check`, `python scripts/check-doc-paths.py` and
  `pwsh scripts/ci-local.ps1` all green.

### Status of the migration at the merge candidate

`feat/vt-engine` @ `88de99e`. Every packet in the order above is merged and independently
verified; the seam, the shim and the fork are all gone, so nothing in this file is still pending
as *migration* work. What is left before the branch reaches `main` is listed once, in
[`../IN-0029.md`](../IN-0029.md) § "What remains before the merge to main": the tab-title FAIL
from the acceptance walk (root-caused elsewhere, outside the engine on the evidence so far), a
green `pwsh scripts/ci-local.ps1` on the merge candidate, and owner sign-off. The rollback plan
above expires with the fork: from `US-0087` on, the only way back is `git revert` of the merge
commits, which is why the branch is not squashed.
