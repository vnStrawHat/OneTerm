# BUG-0081 adversarial verification

- Subject: `3796cef0` `fix(vt): free a Sixel placement's extras entries on release`, on top of
  `main` @`eed33058`.
- Packet: [`../BUG-0081-sixel-image-permanent-extras-entry.md`](../BUG-0081-sixel-image-permanent-extras-entry.md)
- LLD: [`../low-level-design/sixel-extras-release.md`](../low-level-design/sixel-extras-release.md)
- Origin: [`BUG-0079-verify.md`](BUG-0079-verify.md) F7.
- Date: 2026-09-28. Host: Windows 11, MSVC, `CARGO_BUILD_JOBS=3`, worktree-local target dir,
  `target/release` deleted before the gate.
- "Before" figures were built from `eed33058`'s `crates/vt/src/intern.rs`, `intern_tests.rs`,
  `graphics/mod.rs` and `graphics/placement.rs` checked out over the fix, then restored. Every
  probe below (a counting-allocator example, nine extra tests in `graphics_tests.rs`) was a
  throwaway and was removed before the gate ran. The probe bodies are quoted in "Reproductions"
  so the rework can turn them into regression tests.

## Verdict: FAIL

The growth is gone: 70,000 resends of one image hold 258 extras entries instead of 65,535, and the
last resend still places. But the fix frees extras ids that cells in the grid **still carry**, and
the free list hands them to the next unrelated `intern`. The packet and the LLD argue safety only
from the snapshot side (`SnapshotRow` resolves under the lock, which is true). They never ask
whether the grid itself still holds the id after a placement's release. It does, on at least three
ordinary paths: `MAX_PLACEMENTS` eviction of an image that is still on screen or in scrollback, a
sweep after `IL`/`SD` split an image outside its own extent, and a history trim that drops an
image's anchor row while its lower rows survive. After the free, those cells resolve to whatever
is interned next. The probes show an old image's cells turning into a clickable link to an
unrelated URI (F1). They show cells of a hyperlink that an image covered switching to a different
link (F1). They show a live image going unpainted, because a stale cell above it now names it
(F2). The same probes pass on `main`, where a released image's cells stay inert. The rest of the
work is sound: the double-free, tracking, RIS and API checks all pass. What is broken is the
premise "release means nothing references the entry".

## Findings

### F1 (High, regression, blocking): a released placement's extras id is freed while cells still carry it

`release` (`graphics/placement.rs`) frees every id in `extras_by_graphic` for the dying placement.
The rustdoc says "nothing resolves to those entries any more". Before this commit, release never
promised that. It meant only "the view may drop the texture". The engine's own code documents the
survivors:

- `evict_oldest` releases the **oldest** placement whenever 256 are live, whether or not its cells
  are still on screen or in history. "A stream that emits an image per line" is its stated use
  case, and those images sit in scrollback.
- `assert_integrity`'s doc: "A cell *outside* every extent can hold a stale id -- an `SD` or `IL`
  can push part of an image below its own anchor's extent -- and that is inert rather than wrong:
  the painter resolves the id through the placement table and paints nothing when it is gone."
  The fix breaks that "inert". A recycled id no longer names a gone placement. It names someone
  else's entry.
- A history trim drops rows oldest first. For an image taller than one row, the anchor (top) row
  goes first, `is_live` returns false, and the image's lower rows stay in history carrying the
  freed id. This one is reasoned from `is_live` and was not run separately. It is the same
  mechanism as the `IL` probe.

`InternTable::free` resets the slot to the default until the next `intern`. With `intern` popping
the free list first, "until" means the very next link or image anywhere in the terminal. In
`place`, the new image's own entry is interned **before** `evict_oldest` frees the oldest one, so
the next image takes that slot at once.

Every probe below passes on `eed33058` (release build; see F4 for the debug build) and fails at
`3796cef0`:

| Probe | Path | At `eed33058` | At `3796cef0` |
| --- | --- | --- | --- |
| V1: 257 one-cell images on a 20x20 screen, then an `OSC 8` link elsewhere | eviction, cell on screen | cell (0,0) = `{graphic: G1}` (inert) | cell (0,0) = `{hyperlink: link to unrelated.example}` |
| V8: 257 thumbnails printed one per line (`lsix`/`chafa` shape), then a link | eviction, cell in scrollback | history cell = `{graphic: G1}` | history cell = `{hyperlink: unrelated}` |
| V9: 2-row image, `IL` inside it, region scroll drops the extent rows, then a link | sweep, one image, no eviction | n/a (inert stale id) | split row = `{hyperlink: unrelated}` |
| V4: link `id=a` over 10 cells, 2-column image placed over cols 0-1, 256 more images, then links `id=b`, `id=c` | eviction frees the **merged** `{a, G}` entry | cols 0-9 all resolve to link `a` | cols 0-1 resolve to link `b`; cols 2-9 to `a` |

What the user sees:

- V1/V8/V9: blank image cells in the scrollback or on screen become part of a link to a URI they
  never had. Hover underlines them, and Ctrl+click opens that URI. `terminal-view`'s hover groups by
  `HyperlinkId`, so those cells join the unrelated link's hover group.
- V4 answers the brief's merged-entry question. Freeing the hyperlink+graphic entry while the
  link's covered cells survive splits one link into two targets. Ctrl+click on the covered cells
  opens `other.example`, not the link the program wrote. The link-only cells (cols 2-9) and the
  pen stay correct. Their entry `{a, None}` is never tracked, so it is never freed (V5, below).

The BUG-0079 hyperlink maps are **not** a holder. `HyperlinkTable`'s implicit URI map and its
explicit `(id, uri)` map hold `HyperlinkId`, never an `ExtrasId`, so `free` cannot recycle
anything under them. The pen/template holds only `{hyperlink, None}` values (`set_hyperlink`
resolves the template's entry and replaces only the hyperlink). A graphic is never put on the pen,
so the pen never holds a tracked id. A saved cursor (`DECSC`) copies the cursor's template, so
the same holds for it. The erase cell is the default cell plus the template background and carries
no extras. Snapshots are safe as the LLD says:
`snapshot/row.rs` resolves `extras_id()` into `Option<HyperlinkId>`/`Option<GraphicId>` under the
lock, and `oneterm-terminal`'s `model.rs`/`content.rs` also resolve while they borrow the
`Terminal`. The one long-lived holder of raw `ExtrasId`s is the grid, which the LLD never
considered.

Suggested rework direction, left to the implementer. Freeing needs proof that no cell holds the
id, and the tracking map cannot give that proof. Options:

- a grapheme-arena-style collection. When the extras table passes a threshold, mark every
  `extras_id` held by both screens, all history, the template and the saved cursors. Free the
  unmarked ids. Nothing is renumbered, so no remap is needed.
- on release, walk the `HAS_GRAPHIC` rows (history included) and rewrite surviving cells to their
  entry with `graphic` cleared, then free.

The first also covers F3's missing paranoid check for free.

### F2 (High, part of F1): a recycled id can hide a live image

V2: 258 one-cell images on a 20x20 screen. Cell (0,0) belonged to evicted image #1. It now resolves
to `{graphic: G258}`, the **newest live** image. `terminal-view`'s `paint_graphics`
(`render/element.rs`) pushes an id into `seen` **before** it checks `graphic_offset`. The stale
cell is scanned first and has no offset inside G258's placement. G258 is therefore marked seen and
never painted. The probe emulated that loop on the snapshot: it painted 255 of 256 live placements,
and G258 was the one skipped. At `eed33058` it paints 256 of 256, because the stale cell names the
gone G1.

### F3 (Medium): the vt-paranoid acceptance claim is vacuous for this change

The packet's Acceptance says: "`--features vt-paranoid` ... still passes: a freed-and-reused
`ExtrasId` never leaves a cell resolving to a dangling placement." The whole-history walk
(`Screen::assert_interned_ids_resolve`) checks only `extras_id < interner.extras.entries()`, and a
freed id always passes that check. `graphics::assert_integrity` checks only rows inside a *live*
placement's extent, where a recycled `{graphic: G_live}` or `{hyperlink}` also passes. The suite is
green (below), but no check in it can see F1. A check that fails on F1 would assert that no cell
carries an id on the free list, or that no cell in `HAS_GRAPHIC` rows resolves to a graphic whose
placement does not cover it. That check belongs in the rework.

### F4 (Low, pre-existing, masked by the fix): debug builds panic on `main` when eviction leaves cells in a live placement's row

At `eed33058`, debug build, V1/V2/V6 panic in `assert_integrity` (`placement.rs:249`, "a cell
references a graphic with no live placement"). With 257 one-cell images, evicted #1 shares row 0
with live #2..#20. The check walks **every** cell of a live placement's rows, not only its
columns, and finds G1. Release builds are unaffected. At `3796cef0` the panic disappears only
because the freed slot reads as "no graphic" or as a live graphic, which is F1. This is a
pre-existing debug-invariant false positive: scope the check to the placement's columns. It should
not count as evidence for the fix.

### F5 (Low): `InternTable::free` guards against a double free only with `debug_assert!`

V7: `free(a); free(a);` then two `intern`s. A debug build panics ("freed twice"). A release build
hands out id 1 twice, which aliases. No production path reaches it. V6 ran 900 placements through
eviction, `ED 2`, `ED 3`, two resizes (reflow) and settle sweeps, with links merged under images,
then 600 fresh interns: all distinct. An id is tracked only under the `GraphicId` its value embeds,
and `extras_by_graphic.remove` runs once per placement. A release-mode guard (return early if the
slot is already the default, or if the id is on the free list, which holds at most ~258 ids) costs
nothing and removes the hazard.

### F6 (Low, docs): the release premise is stated as fact in five places

`release`'s rustdoc ("nothing resolves to those entries any more"), the `intern.rs` module doc, the
`IN-0029` `cell-and-style.md` and `graphics.md` additions, and the LLD's "Why this is safe" all rest
on the premise F1 disproves. The embedder guide `crates/vt/docs/guide/04-events.md` line 33/150 says
`GraphicReleased` fires when "the last cell referencing an image is gone". That was already
inaccurate on `main` for eviction and `IL` splits, and it is the sentence this design leaned on.
Correct it in the rework. The guide needs no note about id reuse: `GraphicId` is still never
reused, and the guide does not expose `ExtrasId` caching. That stays true only if the rework keeps
"never resolve an `ExtrasId` outside the borrow that read it". `Cell::extras_id()` is `pub`, so one
sentence in `10-limits.md` saying so would be cheap.

### F7 (Info): tracking completeness, RIS, free-list semantics are correct

- Every extras value with `graphic: Some(_)` is created in `place` (graphic-only) or `stamp`
  (merged), and both are tracked. The only other `interner.extras(..)` call site is
  `set_hyperlink` (`dispatch.rs`), which never sets a graphic. Reflow and resize copy cells without
  interning. A hyperlink can never be merged into a graphic cell later, because a write replaces the
  cell's extras with the pen's. No leak was found: each placement's set is removed at its one
  release, and `reset()` clears the map only on `RIS`, which empties the table right after.
- `ED 3`, `clear_history`, `set_scrollback_limit` and alt-screen leave kill anchors, so the next
  `sweep` releases the placement and frees its ids. That is correct bookkeeping, but it inherits F1
  wherever image rows outlive the anchor.
- `free` resets the slot to `T::default()` and does not re-index it. `intern(&default)` still
  returns 0 from the index, so a freed slot is never confused with the default id. The invariant
  `entries.len() == index.len() + free.len()` holds, and the 65,535 cap is unchanged (the free list
  is consumed before the table grows).
- Two distinct simultaneous images keep distinct `GraphicId`s. Rejecting content-hash reuse is
  right, and the first-match `placement()` argument is accurate.

## Reproductions (throwaway, appended to `crates/vt/src/graphics/graphics_tests.rs`)

```rust
const ONE: &str = "#0;2;100;0;0#0~"; // 1x6 px -> one cell at the 10x20 fallback

// V1 (and V2 with 258): one image per cell, 20 per row
let mut s = Session::new(20, 20);
for i in 0..257u16 {
    s.feed(format!("\x1b[{};{}H", i / 20 + 1, i % 20 + 1).as_bytes());
    s.feed(&sixel(ONE));
}
s.feed(b"\x1b[20;1H\x1b]8;;http://unrelated.example\x07x\x1b]8;;\x07");
// resolve_extras(cell(row 0, col 0)).hyperlink == None expected; got Some(..)

// V8: one thumbnail per line into scrollback
let mut s = Session::new(20, 5);
let top = s.row_id(0);
for _ in 0..257 { s.feed(&sixel(ONE)); s.feed(b"\r\n"); }
s.feed(b"\x1b]8;;http://unrelated.example\x07link\x1b]8;;\x07\r\n");
// resolve_extras(cell(top, 0)).hyperlink == None expected; got Some(..)

// V9: sweep path, one image
let mut s = Session::new(10, 8);
s.feed(b"\x1b[2;1H");
s.feed(&sixel("\"1;1;10;40#0~-~-~-~-~-~-~")); // 2 rows at index 1..2
s.feed(b"\x1b[3;1H\x1b[L");                  // IL: image row 2 -> index 3
s.feed(b"\x1b[2;3r\x1b[2S\x1b[r");           // region scroll discards index 1..2
s.feed(b"\x1b[8;1H\x1b]8;;http://unrelated.example\x07x\x1b]8;;\x07");
// resolve_extras(cell(index 3, col 0)).hyperlink == None expected; got Some(..)

// V4: merged link+graphic entry
s.feed(b"\x1b[1;1H\x1b]8;id=a;http://original.example\x07AAAAAAAAAA\x1b]8;;\x07\x1b[1;1H");
s.feed(&sixel("\"1;1;20;6#0~~")); // 2 columns over cols 0-1
// ...256 one-cell images on rows 2+, then links id=b and id=c:
// cols 0-1 resolve to link b instead of a
```

V5 (passes at `3796cef0`): a link open on the pen, an image stamped over linked cells, `ED 2` on
the alternate screen releasing it, a new link interned, then the old link resumed. The pen and the
resumed cells still resolve to link `a`. This is the path where release **is** safe, because the
row reset wiped every holder.

## Measurements

Headless, release profile, 20x4 terminal, `Config::default()`, one 1x6 Sixel resent with
`CSI H` + the same `DCS q` 70,000 times (70 feeds of 1,000), throwaway counting global allocator.
"Undrained" never calls `take_graphics()`, which matches the packet's figures. "Drained" calls it
after every feed, as an embedder does.

| | Extras entries | Placements | Live bytes, undrained | Live bytes, drained | Last resend placed |
| --- | --- | --- | --- | --- | --- |
| `eed33058` | 65,535 (full) | 256 | 13.45 MB | 6.81 MB | No |
| `3796cef0` | 258 | 256 | 7.62 MB | 0.97 MB | Yes (`GraphicId(70000)`) |

The packet reports 13.42 MB before and 7.85 MB after. The entry counts match exactly, and the byte
figures are within about 3%. 258 = 256 live placements + the default entry + one transient slot,
because `place` interns before `evict_oldest` frees. That ordering is also why the next image
immediately takes the evicted image's slot (F1/F2).

## Checks

- `cargo test -p oneterm-vt`, `--features vt-paranoid`, `--features regex` and
  `--no-default-features`: green inside the gate below. See F3 for why paranoid being green says
  nothing about F1.
- `python scripts/vt-public-api.py --check --no-doc` and `--diff-platforms`: unchanged. `free`,
  `track_extras` and `extras_by_graphic` are crate-private. `ExtrasId`, `Interner::extras` and
  `InternTable::entries` keep their signatures.
- Rustdoc citation grep (gate): clean. No `US-`/`BUG-`/`IN-`/`DEC-` citation and no bare
  `crates/` or `docs/` path in the `crates/vt` diff.
- CHANGELOG "Fixed" entry: the numbers and the list of release paths are accurate. "Frees every
  extras entry its cells resolved to, so the next image reuses the slot" describes F1's mechanism
  without its hazard. Rewrite it with the rework.
- Packet: follows `docs/templates/work.md`, Created 2026-09-28, with owning docs reviewed,
  Evidence and Gaps, and a Handoff. Its safety Context bullet (`snapshot/row.rs`) is accurate but
  incomplete (F1). Its Acceptance vt-paranoid bullet is vacuous (F3).

Gate: `pwsh scripts/ci-local.ps1`, `CARGO_BUILD_JOBS=3`, at `3796cef0` with the throwaways
removed. Final line:

```
ci-local: all checks passed.
```

## Gaps

- No GUI run: F2's hidden image comes from emulating `paint_graphics`'s `seen` loop on the snapshot,
  not from a screenshot.
- The history-trim path (a multi-row image whose anchor row is trimmed first) is reasoned from
  `is_live`, not run.
- No real Sixel program (`lsix`, `chafa`, `yazi`) and no live `measure.ps1` re-measure, the same
  gap the packet records.

## Handoff

FAIL: rework `BUG-0081` (acceptance rework of the owning packet, not a new BUG). Keep the free list,
the tracking and the measurement. Replace "free on release" with a proof that no cell holds the id
(F1 options). Add V1/V4/V8/V9 as regression tests and a paranoid check that can fail on F1 (F3).
Guard `free` in release builds (F5). Correct the five docs and the guide's `GraphicReleased`
sentence (F6). Optionally scope `assert_integrity` to the placement's columns (F4).

---

# Second pass: the rework `5b45add0` (2026-09-28)

- Subject: `5b45add0` `fix(vt): rework BUG-0081 -- free extras ids by a proven whole-grid sweep`,
  on `50eba848` (the first pass above, committed verbatim) on `3796cef0`.
- Host and method as above: Windows 11, MSVC, `CARGO_BUILD_JOBS=3`, worktree-local target dir,
  `target/release` deleted before the gate. Every probe (a counting-allocator example, throwaway
  tests appended to `graphics_tests.rs`, two local source mutations) was removed before the gate.

## Verdict: FAIL

The design direction is right, and the live set is complete. Every holder the first pass named
survives a sweep, including the anchor-trim and saved-cursor-screen cases that were not run
before. Two defects remain, each on its own a FAIL:

1. **The sweep frees the id it was triggered by (S1, High).** `State::intern_extras` interns first
   and sweeps second. The value that pushes `since_sweep` to 4,096 is not yet held by any cell or
   pen, so the sweep frees it, and the caller writes the freed id into the pen or the image's cells
   anyway. This happens on every 4,096th new extras value, deterministically. The rework's own
   paranoid check fires on it in a plain debug build.
2. **The trigger counts only table growth, so the table still fills, and then never sweeps again
   (S2, High).** `since_sweep` counts pushes, not free-list reuses. Each cycle drains the previous
   sweep's free list and then grows the table by exactly 4,096. At about 558,000 resends of one image
   the table reaches 65,535. It stops at `since_sweep = 4,094`, below the interval, so no sweep ever
   runs again. From then on every new link and image gets no cells until `RIS`. That is the exact
   symptom `BUG-0081` exists to remove, 8x later. At 30 frames a second that is about 5.2 hours.

The committed regressions cannot see either defect (S3): none of them reaches a sweep.

## Findings

### S1 (High, blocking): `intern_extras` sweeps after interning, freeing the fresh id before it is used

`terminal/mod.rs`:

```rust
pub(crate) fn intern_extras(&mut self, value: &Extras) -> ExtrasId {
    let id = self.interner.extras(value);            // push #4,096: since_sweep hits the interval
    if self.interner.extras.needs_sweep() {
        let live = self.grid.live_extras_ids();      // `id` is in no cell and no pen yet
        self.interner.extras.sweep_unreferenced(&live); // ...so it is freed here
    }
    id                                               // and returned to be written anyway
}
```

The sweep trigger is always a push, because only a push increments `since_sweep`. The value that
triggers the sweep is therefore always brand new, and nothing can hold it yet. Both callers write it
right after:

- `set_hyperlink` puts it on the pen.
- `place` stamps it on the image's cells.

The slot now reads as the default until some later `intern` pops it from the free set, which is
hash order, not LIFO. At that point the cells resolve to that unrelated value.

Throwaway probes, each step in its own `feed` so the per-feed integrity walk sees it:

| Probe | Debug build | Release build |
| --- | --- | --- |
| R1: 4,200 distinct explicit links, one cell each | panics: `extras id ExtrasId(4096) is on the free list but a cell or pen still names it` | cell #4,095 (link `x/4095`) resolves to `http://x/4096`, the next link's URI; every other cell is right |
| R2: 4,095 distinct links written into one dead cell, then one image | same panic | the image's cell resolves to `{hyperlink: None, graphic: None}` right after `place` and stays that way: the image is never painted |

The 70,000-resend test crosses the trigger five times without noticing. Each feed carries 10,000
frames, so the freed id is overwritten by the next resend and reused from the free set before the
end-of-feed walk. The fix is one line: sweep **before** interning. At that point no fresh value
exists yet, and every id already handed out is in a cell or on a pen. Alternatively, insert `id`
into `live`.

### S2 (High, blocking): the growth law and the stall

`needs_sweep()` is `since_sweep >= 4,096`, and `since_sweep` increments only on the push branch of
`intern`. After a sweep frees `F` ids, the next `F` new values reuse them without counting. Only
then do 4,096 pushes grow the table to the next trigger. So each cycle raises `entries()` by exactly
4,096 and lasts `F + 4,096` interns. `F` grows with the table, so after `k` cycles
`entries = 1 + 4,096 k`, and the resends needed are about `4,096 k (k + 1) / 2`. The table grows
as `sqrt(2 * 4,096 * N)`. It is not a ratchet that slows toward a plateau.

Measured (release, 20x4, one 1x6 image resent at `CSI H`, feeds of 1,000, drained):

| Resends | Entries |
| --- | --- |
| 25,000 | 12,289 |
| 41,000 | 16,385 |
| 62,000 | 20,481 (the packet's figure; 5 sweeps so far) |
| 320,000 | 49,153 |
| 431,000 | 57,345 |
| 492,000 | 61,441 (15th and last sweep) |
| 558,000 | 65,535 (full) |
| 1,500,000 | 65,535, `exhausted` = 942,977, last resend **not placed** |

At 61,441 the 15th sweep fires, and the table can grow only 4,094 more before the cap. `since_sweep`
stops at 4,094, so `needs_sweep()` is never true again and the free set stays empty. Every later
new value falls to id 0: no link, no image, until `RIS`. The packet calls the bound "a high-water
mark that ratchets up ... indefinitely" and says "a true constant bound would need a compacting
sweep that renumbers live ids". The first half understates the problem, because the table does not
creep, it fills and freezes. The second half is wrong.

**The fix keeps the same scan cost.** Count every index miss (a free-list reuse or a push) in
`since_sweep`, not only pushes. A sweep then runs once per 4,096 **new values**, whatever slot they
land in. After a sweep the table is at most `live + 4,096 + 1`. It grows only while the live set
itself exceeds what the free list covers, so the bound is `O(interval + live)`. Measured with that
one-line change applied locally (then reverted): **4,098 entries at 70,000 and at 1,500,000
resends, 1.40 MB drained, last resend placed.** The scan frequency is the one the packet already
accepted, one scan per 4,096 new values. The shipped rule reaches the same frequency and then stops
scanning forever. "Sweep when the free list is empty and the interval has elapsed" is equivalent
once the interval counts reuses. With pushes only, it is the shipped rule. It is also worth making
step 3 (table full) sweep once and retry through `State` before it falls back to id 0, so that no
counter state can strand the table.

Verdict on the shipped rule: **FAIL.** The packet's Outcome is that the table does not fill without
bound under resends, and it does fill. The fix is a one-line counter change that costs no extra
scans.

### S3 (Medium): the committed regressions are not discriminating for the sweep

None of `v1`/`v2`/`v4`/`v8`/`v9`, `integrity_rejects_an_extras_id_freed_while_a_cell_still_names_it`
or the intern unit tests creates 4,096 new values, so none of them ever runs `sweep_unreferenced`
through the terminal. Two local mutations, each reverted:

| Mutation | Committed `oneterm-vt` suite | Round-2 throwaways |
| --- | --- | --- |
| A: `live_extras_ids` returns an empty set (the sweep frees **every** live id) | **all 579 pass** (`v1`/`v8` included) | R1-R4 fail |
| B: `needs_sweep()` always false (no sweep) | 583 pass; only `repainting_a_sixel_image_does_not_grow_the_extras_table` fails (65,535 > 30,000) | R1/R2 pass (no sweep, no premature free) |

So `v1`/`v8` fail neither when the sweep is disabled nor when it frees everything. They pin that
release does not free, which is correct, but they are not evidence that the sweep is safe. A
discriminating regression has to force a sweep while a released image's cells survive. R3 and R4
below do that and fail under mutation A. It also has to run each step in its own `feed`, so the
integrity walk sees the id before a reuse hides it. R1 and R2 do that.

### S4 (Info, PASS): live-set completeness

`TerminalGrid::live_extras_ids` walks, for **both** screens, `oldest..=newest`, which is the whole
ring including history, plus `cursor` and `saved_cursor`, each with `template()` (the pen) and
`erase()`. I found no other holder of a raw `ExtrasId` that survives the sweep:

- `State` holds no other `Cell`.
- The `DECSC`/`1049` saved cursor is `Screen::saved_cursor`. It is walked.
- The erase cell carries no extras.
- `Screen::spare` is reset before it is reused.
- The BUG-0079 hyperlink maps hold `HyperlinkId`.
- `snapshot/row.rs`, `oneterm-terminal`'s `model.rs`/`content.rs` and `corpus_replay` all resolve
  while they borrow the `Terminal`.
- `terminal-view` receives resolved `SnapshotCell`s.
- `stamp` interns without sweeping, and `feed`'s end-of-batch sweep runs after every write of the
  batch. The only in-flight id a sweep can meet is S1's.

Probes, release build (they fail in debug only through S1, when their own link spam crosses the
trigger):

| Probe | Result |
| --- | --- |
| R3: 3-row image, scrollback 3, newlines until the anchor row is trimmed and the placement is released while 2 lower rows survive in history; then 4,200 distinct links (sweeps run) | both survivors still resolve to `{graphic: G1}`, no link |
| R4: link `keep` open on the primary pen and cells, `1049h`, 9,000 distinct links on the alternate screen (two sweeps), `1049l`, write `C` | primary cols 0-2 all resolve to `http://keep.example` |
| First-pass V1/V2/V4/V8/V9 (now committed as `v1`...`v9`) | pass |

### S5 (Info): numbers

| | Extras entries at 70k | Undrained | Drained | Last placed |
| --- | --- | --- | --- | --- |
| `eed33058` (first pass) | 65,535 | 13.45 MB | 6.81 MB | No |
| `5b45add0` | 20,481 | 9.52 MB | 2.87 MB | Yes |
| `5b45add0` + counting reuses (local, reverted) | 4,098 | -- | 1.40 MB | Yes |

The packet reports 9.080 MB and 2.740 MB, within 5% of these figures. Sweep frequency under the
70,000 test is 5 sweeps, at pushes 4,096 / 8,192 / 12,288 / 16,384 / 20,480. Under 1.5M it is 15
sweeps, then none.

`live_extras_ids` scan cost, release profile, median of 7, history filled with 200-column text:

| Shape | Scan | Scan + `sweep_unreferenced` |
| --- | --- | --- |
| 10,000 x 200 (the default scrollback) | 3.0 ms | 3.2 ms |
| 100,000 x 200 | 31.6 ms | 33.4 ms |

This agrees with the packet's 32.4 ms. At one scan per 4,096 new values the cost is acceptable
either way.

### S6 (Low, records)

- The CHANGELOG, `10-limits.md`, the IN-0029 `graphics.md` and `cell-and-style.md` rows, the
  `terminal-backend.md` risk row, and the packet's Decisions all describe the bound as a slow
  ratchet. They must say, or better stop being true, that the table fills after about 558k resends
  and then never sweeps again (S2). The packet's "reaches the 65,535 ceiling eventually ...
  (quadratically, in the number of resends)" has the relationship backwards: the resends needed grow
  quadratically in the cycle count, and the table grows as the square root of resends. Its "true
  constant bound would need a compacting sweep" is disproved by S2's measurement.
- The `04-events.md` `GraphicReleased` rewrite is accurate now (F6 closed). The `10-limits.md`
  "resolve an `ExtrasId` only while you hold the borrow, never cache it" note is right and
  sufficient for embedders. `GraphicId` is still never reused.
- The LLD's "First design (rejected by adversarial verification)" section is accurate. Its
  orchestration paragraph states `intern_extras` "check[s] after every hot intern" without seeing
  that sweeping after the intern frees that intern's own id (S1).
- The packet's Handoff asks for exactly this pass. Its gap "history-trim path reasoned, not run" is
  now closed by R3, which passes.

## Checks

- Committed tests at `5b45add0` (inside the gate): `cargo test -p oneterm-vt`, `--features
  vt-paranoid`, `--features regex` and `--no-default-features` all green. The green result means
  little (S3).
- `vt-public-api.py --check --no-doc` and `--diff-platforms`: unchanged. Every new item is
  `pub(crate)`.

Gate: `pwsh scripts/ci-local.ps1`, `CARGO_BUILD_JOBS=3`, at `5b45add0` with every throwaway
removed. Final line:

```
ci-local: all checks passed.
```

## Gaps

- No GUI run and no real Sixel program, as in the first pass.
- S2's corrected rule was measured with only its counter change. The step-3 sweep-and-retry
  suggestion and the S1 reorder were not built or measured together.

## Handoff

FAIL. Rework `BUG-0081` again, as acceptance rework of the owning packet:

1. In `State::intern_extras`, sweep before interning (S1).
2. Count free-list reuses in `since_sweep`, and consider a sweep-and-retry at step 3 (S2). Re-measure
   at 70,000 and at 1,000,000+ resends.
3. Add discriminating regressions: R1/R2 (the trigger intern keeps its cells, one feed per step), and
   R3/R4 or `v1`/`v8` variants that force a sweep. Confirm each fails under mutation A.
4. Correct the bound wording everywhere S6 lists.

The live-set collector, the idempotent `free`, the paranoid `is_free` check and the F4 scoping can
stay as they are.
