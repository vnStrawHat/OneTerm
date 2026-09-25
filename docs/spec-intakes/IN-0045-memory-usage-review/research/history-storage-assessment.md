# IN-0045: assessment of US-0138 (trimmed history rows) and US-0139 (growing ring)

Date: 2026-09-25. The owner asked (ruling 3, 2026-09-25): "assess whether it should be
fixed". The owner's scrollback limit is 10,000, the default.

## Verdict

| Packet | Recommendation | Saving at the owner's settings | Trigger to revisit |
| --- | --- | --- | --- |
| `US-0138` trimmed rows | **Fix later, with a trigger** | 3.2 MB per busy tab (6.4 MB of the owner's 346) | The default limit goes above 10,000, or a report comes from a user with a large limit, or a consumer of `oneterm-vt` asks |
| `US-0139` growing ring | **Do not fix** | 0.75 MB per tab, and only until the tab's history first fills | The default limit reaches 100,000 or more, or tabs that never scroll become a measured cost |

Both are real savings for users who set a large scrollback. At the default they are small
next to the per-view glyph table (`BUG-0078`: 23.6 MB of commit per view) and the ballast
(`DEC-0020`: 48 MB). `US-0138` would change a documented `oneterm-vt` read contract, and
`vt-public-api.py` cannot see that change.

## 1. What a history row costs today

**Sizes.** Measured with a throwaway release test in `crates/vt/src/grid/grid_tests.rs`, since
removed. The test filled the history with 51-character lines and read
`Screen::heap_bytes`:

- `size_of::<Cell>()` = 8.
- `size_of::<Row>()` = `size_of::<Option<Row>>()` = 48: a 24-byte header plus the `Vec`.
- The ring slot holds the row inline.

| Columns | Cells per row | Ring share per row at 10,000 / 100,000 | Measured per history row at 10,000 | at 100,000 |
| --- | --- | --- | --- | --- |
| 80 | 640 B | 78.6 / 62.9 B | 721.5 B | 703.2 B |
| 88 | 704 B | 78.6 / 62.9 B | 785.7 B | 767.2 B |
| 120 | 960 B | 78.6 / 62.9 B | 1,042.9 B | 1,023.3 B |
| 200 | 1,600 B | 78.6 / 62.9 B | 1,685.7 B | 1,663.6 B |

`heap_bytes` counts capacities only. The Windows heap adds about 16 B per row allocation.

**Formula:** bytes per row = ring share + 8 × columns.

- The row stores every column however short its line is (`crates/vt/src/grid/row.rs:97`).
- The ring share is `(limit + 1024).next_power_of_two() × 48 / limit`.

**The process agrees.** In S6 at 1280x800 (89 columns), commit rises by 122.8 MB between the
10,000 and 100,000 limits. That is two tabs, each holding about 80,000 extra rows, so
about 800 B per row: 760 B from the formula, plus heap overhead.

**Blank lines are already cheap.** A row that is never written stays a `None` slot, which
costs 48 B. An empty line that is simply scrolled past therefore costs only its slot.

## 2. How much of a history row is used

This is an **estimate from the mimic's line lengths, not instrumented.** In S6 the mimic
wrote 90,527 history lines per tab. Their mean length was 49.2 characters on an 89-column
grid, so **55 %** of the cells are used.

Other loads fill rows differently:

| Load | Cells used |
| --- | --- |
| The phase 1 S3 load (`line N`) | 11 of 88 (13 %) |
| Prose wrapped by the terminal | the full width on every row but the last |
| Real `claude` output | not measured; its history is mostly wrapped prose and code, so 40 to 70 % is a reasonable range |

## 3. What each change would save

**`US-0138`.** A trimmed row costs its ring share plus 8 bytes per used cell (Windows rounds
the allocation to 16 bytes). At 89 columns and 10,000 lines, a full-width row is
78.6 + 712 = 791 B.

| Line length | Trimmed row | Saving per row | Per full tab at 10,000 | Per full tab at 100,000 |
| --- | --- | --- | --- | --- |
| 49 of 89 (S6 mimic) | 473 B | 318 B (40 %) | **3.2 MB** | 31.8 MB |
| 11 of 88 (S3) | 167 B | 624 B (79 %) | 6.2 MB | 62 MB |
| 49 of 200 (wide window) | 473 B | 1,208 B (72 %) | 12.1 MB | 121 MB |
| wrapped prose (full rows) | 791 B | 0 | 0 | 0 |

**`US-0139`.** The ring is allocated at `Screen::new`:

| Limit | Ring size | Allocation seen in the S6 probe |
| --- | --- | --- |
| 10,000 | 16,384 slots × 48 B | 786,432 B |
| 50,000 | 65,536 slots × 48 B | 3,145,728 B |
| 100,000 | 131,072 slots × 48 B | 6,291,456 B |
| 1,000,000 (the maximum) | 1,048,576 slots × 48 B | 50.3 MB |

Growing the ring lazily saves these amounts only while a tab's history is shorter than its
limit. In S6 the first 10,000-line burst filled the history within seconds, so the saving
there was 0. For a tab that sits idle it is the whole ring, 0.75 MB at the default.

**Against the owner's 346 MB.** Two tabs at 10,000 lines:

- `US-0138` saves about 6.4 MB (1.8 %).
- `US-0139` saves 0 once both tabs have scrolled.

**Against S6 at 100,000 lines** (360.6 MB): `US-0138` saves about 64 MB, the largest
single item there.

## 4. What `US-0138` would change in `oneterm-vt`

### 4.1 Public API and guide

**No signature changes.** `python scripts/vt-public-api.py --check` would still pass. That
is the risk: the change is semantic, and the tool cannot see it.

**Two read methods change meaning.**

- `RowRef::cells() -> &'a [Cell]` is documented as "every cell of the row, left to right;
  blanks for an unwritten row". An unwritten row already returns a full-width static blank
  slice (`BLANK_CELLS`).
- `Row::cells()` and `RowMut::cells()` are public too.

A trimmed row cannot hand out a contiguous full-width slice without copying it. Each way
out has a cost:

| Option | Cost |
| --- | --- |
| a. `cells()` returns the trimmed slice; callers pad | An embedder that indexes `cells()[col]` panics, and one that iterates it sees a short row. Padding reads already exist: `RowRef::cell(col)` returns `Cell::EMPTY` past the end, and `RowRef::occ()` is public |
| b. Keep full width by copying on read | Allocates on the render and search paths, and gives back the memory it saved |
| c. Add a padding view, and trim only through it | `cells_padded()`, or an indexable type with `len() == cols`. The new method is a patch bump. `cells()` itself still has to change meaning or stay full width, so this only helps if `cells()` is changed as in option a |

Row access by index stays O(1) in every option: `Screen::row(id)` is a ring lookup either
way.

**Versioning.** `crates/vt/docs/guide/12-versioning.md` says grid internals reachable
through `grid::Screen` and allocation behaviour are not promised. But `RowRef` is
re-exported at the crate root, and its documented length is what embedders loop over.

- Treat the change as a **minor bump**.
- Add a `CHANGELOG.md` entry naming `RowRef::cells`, `Row::cells` and `RowMut::cells`.
- Add a sentence to the guide's grid chapter.
- `Row::bytes`, `Screen::heap_bytes` and vt-bench tier 5 (`rss`) report smaller numbers.
  That is allowed: allocation behaviour is not promised.

**Callers to adapt in this workspace.**

- Outside `crates/vt`: 12 `.cells()` call lines. The production ones are
  `crates/terminal/src/content.rs:96` and `crates/terminal/src/model.rs:423-424`.
- Inside `crates/vt/src`, non-test: 25 lines. These include snapshot, search, selection,
  reflow and the integrity walk.

### 4.2 CPU

**Scrolling.**

- *Today.* A row that leaves the screen keeps its `Vec`. The evicted oldest row is freed,
  and the new bottom row is allocated when first written. That is one allocation and one
  free per scrolled line (`Screen::push_rows_with`, `crates/vt/src/grid/screen.rs:560`).
- *With `US-0138`, per line entering history:*
  - one scan for the last cell that is not `Cell::EMPTY`. `occ` cannot be used: it only
    over-approximates, and a reset leaves styled blanks below it;
  - one shrinking `realloc`, which on Windows usually means a copy into another heap
    bucket.

  **Estimate:** 50 to 150 ns per line. On vt-bench's `scrolling` fixture (about 80 MiB/s of
  161-byte lines, 520,000 lines/s) that is 3 to 8 % of throughput. The bench would have to
  confirm it.

**Wrap, reflow and resize.**

- Wrapped rows are full by definition, so trimming neither helps nor costs there.
- Unwrapped rows get cheaper to refit: a trimmed row fits any width at or above its
  length, so `Row::into_refitted` has nothing to grow.
- Re-padding is needed where a history row returns to the screen. That happens when
  growing the window pulls history down into the viewport (`crates/vt/docs/guide/09-resize.md`). It
  costs one `realloc` per pulled row, bounded by the rows gained. Screen rows must stay
  full width, because every write path indexes them by column.

**Reads.** Only history rows are ever trimmed. The renderer reads them only when the user
has scrolled back, and the snapshot must then pad them: a copy of up to 45 × 160 cells per
frame, which it already does for the viewport.

Search (`GridText::from_terminal`) and selection copy text. They must give the same text,
and the same columns, whether the tail is stored or implied.

### 4.3 Correctness and `vt-paranoid`

**The integrity walk must change.** `Screen::assert_integrity` asserts
`row.cells().len() == cols` for every allocated row
(`crates/vt/src/grid/screen.rs:1841`). With `US-0138`:

- screen rows must still equal the width;
- history rows must be at most the width.

The walk gets cheaper per row, since there are fewer cells to scan.

**The dropped tail cannot be checked afterwards.** It is gone, so trimming is correct only
if it drops cells equal to `Cell::EMPTY` and nothing else. The cases that must stay
correct:

- **Styled blanks must stay.** Erase with a background colour (`BCE`) fills a row with them,
  and they carry the colour.
- **Wide pairs are safe.** A `WideSpacer` or `LeadingWideSpacer` is never `EMPTY`, so
  trimming cannot split a pair.

**The riskiest part.** Trimming must go through one function with its own tests, and the
`vt-paranoid` suite must run a BCE-and-scroll case. Screen-row and history-row width are
then two invariants instead of one.

### 4.4 BUG-0075's bench

`snapshot::bench::integrity_walk_cost_per_feed_and_snapshot_update` compares:

- the walk over a full 100,000-row history, against
- the walk over an almost empty one.

It fails if the ratio of the two reaches 20.

- **The bounded walk is unaffected.** It never visits history rows, so trimming does not
  touch it.
- **The bench's sensitivity drops, but stays sufficient.**
  - A regression back to an O(history) walk would walk shorter rows, so it would show a
    smaller ratio than the ~1,700 measured by `US-0075`.
  - The ratio scales with cells per row. The fixture's 160 columns would drop to its line
    length, which cuts the ratio by about 5 to 10 times.
  - That is still about 200, far above 20. No change to the bench is needed.
- **One test needs a new figure.** The memory test
  `grid::grid_tests::empty_scrollback_costs_only_its_ring_slots` prints a "written rows"
  figure, and that figure would change. Its assertions cover only the empty case.

## 5. What `US-0139` would change

- **No signature changes.** The only visible behaviour: `Screen::ring_len()` and
  `Screen::ring_mask()`, both public, would change during a session instead of staying
  constant.
- **A design rule has to be restated.** R-30 says "the ring's length is a session
  constant", and the grid module relies on it (`crates/vt/src/grid/mod.rs:33`: "a resize
  can never invalidate the mask"). Growth would restate it as "changes only on doubling,
  never inside a resize".
- **The re-slotting code already exists.** `Screen::set_scrollback_limit` re-slots every
  row in O(rows). Growth would call it at each doubling, so the amortized cost is small:
  moving 65,536 slots at the step from 65,536 to 131,072 takes under a millisecond.
- **Integrity checks.** The walk covers ids, not slots, so it would not change. A test must
  check that no `RowId` changes across a doubling. Embedders key their caches on `RowId`,
  and the guide promises that a `RowId` keeps naming the same content.
- **What it is worth.** 0.75 MB per tab at the default, and only until the tab's history
  first fills. Real work and a restated design rule for that saving is not worth it at the
  default.

## 6. Recommendation, in one place

**`US-0138`: fix later.** When the trigger is met:

1. Write `low-level-design/history-storage.md` first (the high-level design already asks
   for it).
2. Choose API option a (trimmed `cells()`, a minor bump).
3. Add a vt-bench `scrolling` measurement before and after, so the per-line cost from
   § 4.2 is measured.

Until then, the cheapest lever for users who want lower memory is the existing
`scrollback_history` setting:

| Limit | S6 commit (two tabs) |
| --- | --- |
| 10,000 | 237.8 MB |
| 100,000 | 360.6 MB |

**`US-0139`: do not fix.** Close it with this assessment as the reason. The trigger for
reopening it is in the verdict table.
