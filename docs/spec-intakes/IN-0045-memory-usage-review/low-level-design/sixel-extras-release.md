# Low-Level Design: Sixel extras entries and the extras table's own sweep

Intake: IN-0045
HLD: [high-level-design.md](../high-level-design.md)
Topic: `oneterm-vt` extras table growth from a resent Sixel image, and how the table proves an
entry is safe to free
Date: 2026-09-28. Reworked 2026-09-28 after adversarial verification failed the first design
(`evidence/BUG-0081-verify.md`), reworked again the same day after a second adversarial pass
failed the mark-and-sweep design's own orchestration and trigger (`evidence/BUG-0081-verify.md`,
"Second pass"), and reworked a third time the same day after a third pass found the second
rework's own sweep-and-retry had no rate limit (`evidence/BUG-0081-verify.md`, "Third pass", T1).

## Concern

`BUG-0079` fixed the same extras table filling from repainted `OSC 8` links, by keying an implicit
link's id to its URI so a resend reuses the same `HyperlinkId`. Its own Evidence and Gaps (F7) found
that Sixel images share the collateral but not the fix: `place` interns a fresh `Extras { graphic:
Some(id), .. }` for **every** placement, because `id` itself (`GraphicId`) is a fresh counter value
every time, even when the resent bytes are byte-for-byte the same image. A program that redraws a
Sixel preview every frame — a file manager, an Ink-style TUI — therefore took one permanent extras
entry per resend, same as the hyperlink case, until the table filled at 65,535 entries and no later
link or image got its cells, until `RIS`.

## First design (rejected by adversarial verification)

The first attempt tracked, per placement, every extras id `place`/`stamp` interned for it
(`GraphicsState::extras_by_graphic`), and freed all of them when the placement was released — through
`sweep`'s row-derived detection or `MAX_PLACEMENTS` eviction. It shipped as `3796cef0`, and
`evidence/BUG-0081-verify.md` failed it (F1, High): **a placement's release does not mean its cells
are gone.** `evict_oldest` releases the *oldest* placement whenever 256 are live, whether or not its
cells are still on screen or in scrollback — "a stream that emits an image per line" is its own
documented use case, and those images sit in history. `assert_integrity`'s own doc already said a
cell outside a placement's tracked extent "can hold a stale id ... and that is inert" — `IL`/`SD`
splitting an image outside its anchor's tracked extent is exactly that, and freeing on release broke
the "inert" half of that sentence. A history trim that drops a tall image's anchor row while its
lower rows survive is the same mechanism. In every one of these, the freed id's slot went to
whatever was interned next — `place` interns the new image's own entry **before** `evict_oldest`
frees the old one, so the very next placement or link anywhere in the terminal could take it —
turning a released image's still-live cell into a link to an unrelated URI (V1, V8, V9), splitting
one link into two targets when the freed entry was a hyperlink-plus-graphic merge (V4), or letting a
recycled id alias a *different*, still-live image's id and hide it from the renderer's paint loop
(F2/V2). `graphics.md`'s own risk table already listed the release paths that do not mean "gone";
the first design's safety argument (`snapshot/row.rs` resolving `ExtrasId` under the lock) was true
but answered a question nobody was asking — the grid itself, not a snapshot, is the long-lived holder
the design never considered.

**Content-hash reuse** (`BUG-0079`'s fix, generalised to image bytes) was considered and rejected too,
independently of the above: `GraphicsState::placement` resolves an id to a placement with
`.find(|p| p.id == id)` — the **first** match — so if a program ever shows the same image at two
positions **at once** (a real case: two identical icons side by side), sharing a `GraphicId` would
silently give the second occurrence the first one's anchor, cols and rows. Hyperlinks tolerate two
simultaneous occurrences of one URI merging (`osc8-interning.md` accepts merged hover groups as a UX
simplification); graphics do not tolerate two simultaneous placements merging into one, because the
painter would draw the wrong rectangle. This stays rejected in the rework: `GraphicId` keeps its
fresh-per-placement identity unchanged.

## Second design flaw (S1/S2, second adversarial pass)

The mark-and-sweep direction survived the second pass (`evidence/BUG-0081-verify.md`, "Second
pass", S4: the live set is complete -- every holder the first pass named, including the
anchor-trim and saved-cursor-screen cases the first pass only reasoned about, survives a sweep).
Two defects in this design's own orchestration and trigger did not, each on its own a FAIL:

**S1: `State::intern_extras` swept *after* interning, freeing the very id it had just handed back.**
The value that pushes `since_sweep` to the interval is not yet held by any cell or pen at the point
`intern_extras` is called -- both callers write the id it returns right after, never before. Sweeping
after interning built the live set before that write happened, found the brand-new id unreferenced,
and freed it: the caller then wrote a freed id into a cell or a pen, which the paranoid `is_free`
check this same rework added catches immediately in debug, and which resolves silently to whatever
is interned next in release. This happened deterministically on every 4,096th new value, and none of
the committed regressions (`v1`-`v9`, the double-free test, the paranoid regression) reached it,
because none of them creates enough distinct values to cross the trigger (S3, below). Fixed by
sweeping *before* interning: at that point no fresh value exists yet to be mis-swept, and every id
already handed out is provably in a cell or on a pen.

**S2: `since_sweep` counted only pushes, so the table filled and then stopped sweeping forever.**
A sweep frees `F` ids; the next `F` new values all reuse them via the free-list branch of `intern`,
which did not touch `since_sweep`. Only once that reuse is exhausted do pushes resume, and only 4,096
of *those* trigger the next sweep. Each cycle therefore raised `entries()` by exactly the interval and
lasted `F + interval` interns, with `F` growing every cycle -- not a ratchet that slows toward a
plateau, but one where the table grows as the square root of the resend count and the resends needed
grow quadratically in the cycle count (the reverse of what the first version of this note claimed).
Measured: the table reached 65,535 (full) at about 558,000 resends of one image, with `since_sweep`
stalled at 4,094 -- below the interval, so `needs_sweep` could never become true again, and every
later new link or image fell back to id 0 until `RIS`. That is the exact symptom `BUG-0081` exists to
remove, just delayed by a factor of about eight. Fixed by counting every index miss in `since_sweep`
-- a free-list reuse exactly like a push -- so a sweep runs once per `TABLE_SWEEP_INTERVAL` *new
values*, whatever slot they land in, and the table settles to roughly `interval + live` instead of
climbing forever. `State::intern_extras` also sweeps once and retries before accepting the table-full
fallback, so a counter that happens to fall a little short of the interval right as the table
genuinely fills can never strand it there for the rest of the session.

**S3: the committed regressions never actually reached a sweep.** None of `v1`/`v2`/`v4`/`v8`/`v9`,
the double-free test or the intern unit tests creates 4,096 distinct values, so `sweep_unreferenced`
never ran through any of them -- a mutation that made the live-set collector return nothing still
passed the whole suite. `R1`-`R4` (Verification, below) are the discriminating regressions this pass
added: each was confirmed to fail when the live-set collector is stubbed to return an empty set
(mutation A), and to pass when `needs_sweep` is stubbed to always return `false` (mutation B, which
disables the sweep and therefore cannot free anything prematurely) -- proof that they exercise the
sweep itself, not merely that release does nothing (which `v1`/`v8` already covered). The two bound
tests (70,000 and 1,500,000 resends) are the mirror image: they pass under mutation A (freeing
everything on every sweep does not break their specific single-cell check) and fail under mutation B
(with no sweep at all, the table fills and the last resend stops placing) -- they are what actually
proves S2 fixed, not R1-R4.

## Third design flaw (T1, third adversarial pass)

S1, S2 and S3 from the second pass held: sweeping before interning is correct on every path,
including `stamp`'s merged path, and counting reuses keeps the table at 4,098 entries at both
70,000 and 1,500,000 resends. A third pass found one more High finding, this time in the
sweep-and-retry itself, added as S2's own fix: **it had no rate limit.**

`State::intern_extras`, on the miss where `try_intern` returns `None`, swept and retried
unconditionally. When the table is full of entries a real cell still references — 65,534 distinct
live hyperlinks or images, hostile-reachable in one burst, or plausible from a long session that
prints many distinct `OSC 8` URLs — a sweep frees nothing, the retry fails the same way, and the
*next* miss does the exact same unconditional sweep again. `try_intern`'s own failure path did not
touch `since_sweep` before this fix, so nothing amortised it: every single failing intern became a
full `O(history)` scan. Measured (third pass, `evidence/BUG-0081-verify.md`): 5.7 ms per image at
the default 10,000-row scrollback, 47.7 ms at 100,000 rows -- 10,000 resends took 57 s where `main`
took `O(1)` per resend, and a hostile 64 KB read of minimal Sixels (about 3,200 images) held the
terminal lock for roughly 18 s at the default scrollback, or about 150 s at 100,000 rows. The
unconditional retry was this rework's own previous suggestion ("sweep once and retry"); the "once"
was never rate-limited against repetition.

**Fixed: `try_intern` counts its own failure toward `since_sweep`, and the retry runs only when
`needs_sweep` says another sweep is due.** Both changes together, not either alone: counting the
failure is what lets a later attempt's `needs_sweep` check ever become true again after a run of
nothing but failures (without it, gating the retry on `needs_sweep` would let the very first failure
after a sweep permanently skip every future retry, since `since_sweep` would sit at a small number
forever); gating the retry on that count is what turns "an unrated retry on every failing call" into
"one extra scan at most per `TABLE_SWEEP_INTERVAL` failed attempts." Measured after the fix, same
shape: 3 sweeps for 10,000 resends at the default scrollback, 25.3 ms total (was 60.6 s measured on
this rework's own re-run, matching the third pass's 57 s); `exhausted()` reads exactly one count per
resend, not two, because the retry now happens through the same `try_intern` the first attempt used,
with `InternTable::record_exhausted` called exactly once regardless of how many `try_intern` calls a
single `intern_extras` invocation makes. The "never permanently stranded" property from the second
rework still holds: it is now "not stranded for more than `TABLE_SWEEP_INTERVAL` attempts at a time"
rather than "immediately," which is the correct reading of what `needs_sweep` was already promising
everywhere else in this design.

**The per-`intern_extras` cost when the table is genuinely live-full is therefore `O(1)` amortised,
not `O(1)` worst-case.** A single failing call can still cost a full scan — whichever one crosses the
interval — but that cost is paid once per `TABLE_SWEEP_INTERVAL` (4,096) attempts, giving roughly
1.4 µs amortised per intern at the default 10,000-row scrollback and about 12 µs at 100,000 rows (the
measured per-scan cost divided by the interval). This is worse than `main`'s true `O(1)` fallback, and
that is an accepted trade: `main` could never recover once its own hyperlink and extras tables filled
with dead content, and this design's whole point is that it can, at the cost of an occasional scan
instead of none, ever.

## Design (rework)

**Free an id only once a scan of the whole grid proves nothing references it.** Nothing short of that
proves it, per the first design's failure: not a placement's release, not `sweep`'s row-flag
approximation, not any one call site's own bookkeeping about what it touched.

`InternTable<T>` gains:

- `free: FxHashSet<u16>` — ids freed, waiting for `intern` to hand them back out. `intern` checks it
  before growing the table.
- `since_sweep: u32` — new entries handed out since the last sweep, incremented on **every** index
  miss of `intern` — the free-list-reuse branch as much as the growth branch, and not a cache hit
  (S2: counting growth alone let the table fill and then never sweep again).
- `needs_sweep(&self) -> bool` — `since_sweep >= TABLE_SWEEP_INTERVAL` (4,096, an absolute count for
  the same reason `GRAPHEME_SWEEP_ENTRIES` is one: keeps a sweep rare for an ordinary session while
  still bounding a hostile one). Generic on `InternTable<T>`, but nothing calls the sweep for styles
  today.
- `sweep_unreferenced(&mut self, live: &FxHashSet<u16>)` — frees every allocated id **not** in `live`
  and resets `since_sweep`. `O(entries)`. Nothing is renumbered: a kept id keeps its slot; a freed one
  is cleared to the default and pushed onto the free list, same as calling `free` once per id.
- `is_free(&self, id: u16) -> bool` — for the new paranoid check (below).
- `free(&mut self, id: u16)` is now idempotent in every build, not only under `debug_assert!`
  (`F5`): a second free of the same still-outstanding id is a no-op, so a bug in the caller cannot
  hand the same id out twice from the free list.

**Building `live` is the caller's job, and it must be complete.** `InternTable` has no access to a
grid. `Screen::collect_live_extras_ids` walks the screen's **whole** history (`oldest..=newest`, not
`integrity_lo`'s O(rows) batch-touched approximation — this is a rare, O(history) operation by design,
not a per-`feed` one) plus the pen (`template()`) and erase cell of both the active and the saved
cursor, and `TerminalGrid::live_extras_ids` runs it for both screens. This list had to be exactly as
complete as the first design's failure demanded: every holder `evidence/BUG-0081-verify.md` F1 named
(scrollback, a split extent, the pen/erase cell of both cursors) is included; the only excluded field
is `Screen::spare` (the one row kept only for its cell allocation after a history trim, "never read as
content" — outside the ring `Screen::row` walks, so nothing observable can resolve through it).

**Orchestration: swept before interning, checked after every hot intern, not once per `feed`.**
`state.interner.extras(..)` has three call sites. Checking `needs_sweep()` once at the end of
`Terminal::feed` was the first instinct, and it is wrong on its own: a single `feed` can carry a
whole screen's worth of resends (a full repaint arriving in one read), so `since_sweep` could run
into the tens of thousands before the end-of-batch check ever looked. `State::intern_extras` wraps
`place`'s graphic-only entry and `set_hyperlink`'s call with an immediate `needs_sweep`/sweep check
**before** calling `interner.extras`, not after (S1) — the value about to be interned does not exist
yet at that point, so a sweep run there can never mis-free it, and every id already handed out by an
earlier call is provably written to a cell or a pen by the time this one's live-set scan reads the
grid, because each caller writes its id back synchronously before anything else can call through
`intern_extras` again. On the rare miss where the table is still full even after that pre-sweep,
`intern_extras` sweeps once more and retries before accepting the id-0 fallback, so a counter that
happens to fall short of the interval right as the table fills cannot strand it. `stamp`'s rare
merged hyperlink-and-graphic entry keeps calling `interner.extras` directly: it runs inside a loop
already borrowing `state.grid` for the row it is writing, and a live-set scan needs `state.grid` on
its own, so the two cannot interleave without restructuring that loop for a path that needs many
pre-existing distinct hyperlinks under one about-to-be-placed image to matter, and it always writes
its id to the cell it just read from in the same loop iteration, before anything else can call
`intern_extras` and possibly sweep. `Terminal::feed`'s own end-of-batch check stays as that path's
backstop.

**The bound is `O(interval + live)`, not a high-water mark that climbs forever.** Freeing never
shrinks `entries()` (a kept id must never move, so there is no lower slot to move a survivor to), so
`entries()` itself never decreases — but with `since_sweep` counting every reuse as well as every
push (S2), a sweep runs once per `TABLE_SWEEP_INTERVAL` *new values handed out*, whichever slot they
land in, so the table's high-water mark settles once the live set stops changing rather than growing
every cycle. Measured (see Measurements below): 70,000 resends and 1,500,000 resends of one image
both hold 4,098 entries, not the 258 the first design measured unsafely, and nowhere near 65,535. The
first version of this note claimed the opposite — a bound that "ratchets up... indefinitely" under
sustained load, needing "a compacting sweep that renumbers ids" for anything better — which had the
relationship backwards (S2's own measurement disproves it) and was true only of the un-reworked S2
defect, not of a design that counts reuses.

**`F4`, scoped while here.** `graphics::assert_integrity`'s debug check walked every cell of a live
placement's **row**, not its own columns. Two placements sharing a row (one narrower than the screen
next to another) meant a live placement's check could reach an evicted neighbour's columns and assert
on its id — a pre-existing false positive the first design's bug happened to mask (a recycled id read
back as "no graphic" or as some live id instead of the evicted one). Scoped to `pos.col ..
pos.col + placement.cols` now.

## Interfaces

No public signature changes. `InternTable::free`, `is_free`, `needs_sweep`, `sweep_unreferenced`,
`try_intern`, `record_exhausted` are `pub(crate)`; `swept` is `pub(crate)` and additionally
`#[cfg(test)]` — no production caller needs the sweep count today, only
`crates/vt/src/terminal/terminal_tests.rs`, so it does not carry as an unused method in a normal
build. `Screen::collect_live_extras_ids` and
`TerminalGrid::live_extras_ids` are `pub(crate)`. `State::intern_extras` is `pub(crate)`.
`GraphicsState::extras_by_graphic` / `track_extras` from the first design are removed;
`graphics/mod.rs` and `graphics/placement.rs` are otherwise unchanged from `main`. `InternTable::intern`
keeps its existing public signature and behaviour, now implemented as `try_intern` plus
`record_exhausted` rather than one method that did both inline.

## Edge Cases and Failure Modes

- [x] `V1`/`V8`/`V9`/`V4`: a released placement's surviving cell (evicted while on screen, evicted
  while in scrollback, split outside its tracked extent by `IL`/`SD`, or a merged hyperlink-and-graphic
  entry) is never recycled into an unrelated value, because nothing frees an id the live-set scan
  still finds.
- [x] `V2`/F2: a recycled id can never alias a *different*, still-live image, for the same reason —
  every live placement is reachable through some cell of its own.
- [x] Two distinct images placed at once never share an extras entry (identity stays the fresh
  `GraphicId`, unaffected by either design).
- [x] `RIS` still empties the whole extras table at once (`InternTable::clear`, now also resetting
  `since_sweep` and clearing the free set).
- [x] `F5`: freeing the same id twice never hands it out twice — `free` is unconditionally idempotent.
- [x] `F3`: the paranoid whole-history walk (`Screen::assert_interned_ids_resolve`) now checks
  `!interner.extras.is_free(id)` for every cell's extras id **and** for the pen/erase cell of both
  cursors, not only `id < entries()` — a freed id always passed the old check, because freeing never
  shrinks the table. This is what caught S1 immediately once written: it panics in a plain debug
  build the first time the sweep-after-intern ordering mis-freed a fresh id.
- [x] S1: the value whose own creation crosses the sweep threshold keeps its own cell, on both hot
  call sites (`place`'s graphic-only entry, `set_hyperlink`'s pen write).
- [x] S2: the table settles near `interval + live` under sustained resends of one image, at both
  70,000 and 1,500,000 resends, and never falls back to id 0.
- [x] S4: a placement's surviving rows after an anchor trim, and a screen's cells and saved-cursor
  pen while the *other* screen is active, both survive sweeps triggered entirely elsewhere.
- [x] T1: a table full of entries a real cell still references costs at most one scan per
  `TABLE_SWEEP_INTERVAL` failed `intern_extras` calls, not one scan per call.
- [x] T5: that same table leaves a new image genuinely unplaced (the ladder's fallback, warned
  once), and `RIS`, not the sweep-and-retry, is what recovers it.
- [x] `exhausted()` counts exactly once per failed `intern_extras` call, including the ones that
  went through a sweep-and-retry.

## Measurements

Headless, release profile, throwaway counting allocator (removed before commit) for the entries/live
bytes rows, throwaway `#[ignore]`d timing tests (removed before commit) for the live-full rows, one
20x4 terminal unless noted, one 1x6 Sixel resent with `CSI H` + the same `DCS q`:

| | Extras entries at 70,000 resends | Extras entries at 1,500,000 resends |
| --- | --- | --- |
| `eed33058` (main, unfixed) | 65,535 (full) | see `evidence/BUG-0081-verify.md` |
| `3796cef0` (first design, rejected: frees on release) | 258 | 65,535 (full, stalled) |
| This rework (S1/S2/T1 fixed) | recorded in the packet's Evidence and Gaps | recorded in the packet's Evidence and Gaps |

**Live-full case, 200 columns, default (10,000-row) scrollback, the table filled with 65,535 entries
each written onto a real cell, 10,000 further image resends:**

| | Total time | Sweeps |
| --- | --- | --- |
| Before this rework (unrated retry, T1) | 60.6 s | 10,001 |
| After this rework | 25.3 ms | 3 |

Live bytes and the scan cost (`Screen::collect_live_extras_ids` for one screen, release profile, at
the coordinator's requested worst-case shape of 100,000 rows x 200 columns and at the test
terminal's own small shape) are recorded in the packet's Evidence and Gaps.

## Verification

- [x] `intern::tests`: a freed id is the next one `intern` reuses; freeing id 0 is a no-op; freeing
  the same id twice does not alias it; `sweep_unreferenced` frees exactly the ids missing from `live`.
- [x] `graphics::tests`: `v1`/`v8`/`v9`/`v4` (released-but-still-live cells are never recycled);
  `v2` (every live placement is reachable through its own cells); `two_different_images_stay_distinct`;
  `r3` (an anchor-trimmed placement's surviving lower rows are not recycled by a later sweep); a
  paranoid-check regression (`integrity_rejects_an_extras_id_freed_while_a_cell_still_names_it`).
- [x] `terminal::tests`: `r1`/`r2` (the value that triggers a sweep keeps its own cell, on both hot
  call sites); `r4` (the other screen's cells and saved-cursor pen survive sweeps on the active
  screen); 70,000 and 1,500,000 resends of one Sixel image at a fixed cursor position keep the extras
  table near the sweep interval, the last resend still places, and a following explicit link and a
  distinct image both still get their cells; a table stuffed with **dead** entries recovers via the
  sweep-and-retry at the exhausted step, without waiting for `RIS`
  (`a_table_full_of_dead_entries_recovers_via_sweep_and_retry`); a table stuffed with **live**
  entries leaves a new image unplaced and only `RIS` recovers it
  (`a_table_full_of_live_entries_leaves_an_image_unplaced_until_ris`, T5); that same live-full table
  bounds its sweep count to `ceil(attempts / TABLE_SWEEP_INTERVAL) + 1` over 1,000 further failing
  attempts, not one sweep per attempt (`intern_extras_bounds_its_scan_rate_when_the_table_is_live_full`,
  T1) -- confirmed to fail (1,001 sweeps instead of at most 2) when the `needs_sweep` gate on the
  retry is removed, and to pass with it.
- [x] Each of `r1`-`r4` was confirmed to fail when the live-set collector is stubbed to return an
  empty set (mutation A) and to pass when the sweep is disabled (mutation B); the 70,000- and
  1,500,000-resend tests were confirmed to pass under mutation A and fail under mutation B. See the
  packet's Evidence and Gaps for the full table.
