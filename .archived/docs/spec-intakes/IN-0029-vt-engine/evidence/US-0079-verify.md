# Verification: US-0079 — Damage, render state and events

Intake: IN-0029
Packet: [`../US-0079-damage-and-render-state.md`](../US-0079-damage-and-render-state.md)
Under verification: branch `worktree-agent-aa7396b1407c94dca` @ `b080858`, base `83933b9`
Worktree: `D:\TrungKFC-Research\Rust\myTerm2\.claude\worktrees\agent-aa7396b1407c94dca`
Date: 2026-09-12
Verifier: independent (read-only on the main checkout; no fix applied, nothing committed)

**Verdict: merge after fixes.** One major defect in `begin_update`'s synchronized-output path
(three proven symptoms, one ~6-line fix), one major finding that belongs to `US-0075`, and five
cheap minors. Everything else — scope, the gate, the tri-state, watermarks, resolved values,
the event surface, the numbers — holds.

Scratch reproduction suite:
`C:\Users\trunglt\AppData\Local\Temp\claude\D--TrungKFC-Research-Rust-myTerm2\fa2d0135-9c28-4a69-a255-ba7479e604cb\scratchpad\US-0079-verifier-tests.rs`
(was `crates/vt/tests/zz_verify_scratch.rs` in the worktree while running; removed — the worktree
is clean at `b080858`).

Free space on `D:` at start: **68.9 GB** (limit 15 GB).

---

## 1. Pass / fail table

| # | Check | Result |
|---|---|---|
| 1 | Scope: only `crates/vt/src/render/**`, `src/events/**`, `src/lib.rs`, the packet | **PASS** |
| 2 | `pwsh scripts/ci-local.ps1` green; crate test time; release bench reproduced | **PASS** |
| 3a | `begin_update` bounded by changed rows | **PASS** |
| 3b | Tri-state semantics incl. deviation D5 | **PASS** (contract not changed) |
| 3c | Watermark independence for two consumers, monotonicity | **PASS** |
| 3d | The stale-row finding; is `RenderRow::allocated` the right fix? | **PASS with escalation** (correct for `RenderState`; the grid is still wrong — F2) |
| 3e | No interned id crosses the phase boundary; hyperlinks and graphics usable | **PASS** |
| 3f | `map_colors` needs nothing from the grid | **PASS** |
| 3g | Demand/yield handshake; the 250 us park; `FairMutex` stated | **PASS with note** (test accommodation, correctly labelled) |
| 3h | Mode 2026: 150 ms refresh and 1 s watchdog on injected time | **FAIL** — deadlines correct, the suppressed-frame path is not (F1) |
| 3i | Every `alacritty_terminal::Event` + patch has a home or an owner; spans cannot dangle | **PASS** (one doc overstatement, F7) |
| 4 | Can `RenderState` alone drive `frame.rs`? | **PARTIAL** — gap list in § 4; three gaps are US-0079's |
| 5 | Code quality: no `unsafe`/`unwrap`, `#[path]`, pub surface, allocation-free | **PASS** (two doc defects, F3/F7) |
| 6 | Packet completeness, DB row, commit trailer, wrong-base incident | **PASS** |

---

## 2. Raw evidence

### 2.1 Scope

```
$ git diff 83933b9...HEAD --stat
 crates/vt/src/events/batch.rs                      | 227 +++++++
 crates/vt/src/events/event_tests.rs                | 217 +++++++
 crates/vt/src/events/mod.rs                        |  18 +
 crates/vt/src/events/vt_event.rs                   |  93 +++
 crates/vt/src/lib.rs                               |  13 +
 crates/vt/src/render/demand.rs                     |  41 ++
 crates/vt/src/render/mod.rs                        |  31 +
 crates/vt/src/render/modes.rs                      |  80 +++
 crates/vt/src/render/palette.rs                    | 160 +++++
 crates/vt/src/render/render_bench.rs               |  91 +++
 crates/vt/src/render/render_tests.rs               | 714 +++++++++++++++++++++
 crates/vt/src/render/row.rs                        | 183 ++++++
 crates/vt/src/render/state.rs                      | 359 +++++++++++
 crates/vt/src/render/sync.rs                       |  77 +++
 crates/vt/src/render/sync_tests.rs                 | 118 ++++
 .../US-0079-damage-and-render-state.md             | 341 ++++++++++
 16 files changed, 2763 insertions(+)

$ git diff 83933b9...HEAD --stat -- docs/
 .../US-0079-damage-and-render-state.md             | 341 +++++++++++++++++++++
 1 file changed, 341 insertions(+)
```

No `crates/vt/src/grid/`, `parser/` or `reflow/` file. No `Cargo.toml`, no `Cargo.lock`. No LLD
edited. `git status --short` is empty. **PASS.**

**Spec currency.** The base `83933b9` predates `feat/vt-engine` @ `1ef1414`, but
`git diff 83933b9 1ef1414 -- low-level-design/damage-and-render-state.md events-and-api.md`
is empty — `1ef1414` touched only the grid, reflow, testing LLDs and the HLD. The implementer
read the current version of both owning LLDs.

### 2.2 The gate

```
$ pwsh scripts/ci-local.ps1
... ci-local: all checks passed.      (exit 0)

$ grep -c "test result:"                 -> 54
$ aggregate of every "test result: ok."  -> passed=1310 failed=0 ignored=6
```

Exactly the packet's claim (54 sections / 1310 / 0 / 6). All seven python checks passed
(dependency graph 21 packages, doc paths 122, English 697 files, catalogs, notices).

```
$ cargo test -p oneterm-vt
running 133 tests
test result: ok. 132 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.59s
```

0.59 s against the packet's 0.56 s — within budget, and there is no sleeping test outside the
two-thread fairness case.

**Bench.** `-- --ignored` runs nothing: the attribute is
`#[cfg_attr(debug_assertions, ignore = "...")]` (`render_bench.rs:22-27`), so in a release build
the test is *not* ignored and `--ignored` filters it out (`132 filtered out; 0 tests`). The
packet's own verification line (`--release -- --nocapture render::bench`) is the correct one.
Two consecutive release runs:

```
render-state build cost per frame, 200x50, 2000 frames each:
  unchanged frame :    0.115 us  (261x the old snapshot)      |    0.096 us  (311x)
  1 row changed   :    0.901 us  (33.2x)                      |    0.723 us  (41.3x)
  50 rows changed :   34.024 us  (0.9x)                       |   31.551 us  (0.9x)
  old full snapshot (alacritty_terminal, measured): 29.9 us
```

The packet's 0.098 / 0.760 / 30.202 us are reproduced to the order of magnitude and then some;
the spread is run-to-run noise on an unpinned clock. The 50-row parity claim is honest.

*Nit (F11):* this is **not** bench tier 3. `testing-and-bench.md:225` defines tier 3 as
"parse + grid + one `render_update` + `map_colors` per simulated frame" at 160x45; this times
`begin_update` + `map_colors` only, at 200x50. Both deviations are defensible (no parser exists
yet; 200x50 is the geometry `research/perf-baseline.md` § 4 measured the 29.9 us against), but
the packet does not name who builds tier 3.

---

## 3. Contract review, clause by clause

### (a) Is `begin_update` bounded by changed rows? — **PASS**

`copy_changed` (`state.rs:319-342`) walks all viewport rows but *copies* only where
`slot.id != id || row.seq() > watermark.0 || (slot.allocated && !row.is_allocated())`. The walk
is a header read per row; the copy is the cost. Measured with `RenderState::rows_copied()` on a
50-row viewport:

```
scroll  1 rows in a 50-row viewport -> Partial { scrolled: 1 },  copies=1,  changed=1
scroll 10 rows in a 50-row viewport -> Partial { scrolled: 10 }, copies=10, changed=10
scroll 49 rows in a 50-row viewport -> Partial { scrolled: 49 }, copies=49, changed=49
scroll 50 rows in a 50-row viewport -> Full,                     copies=50, changed=50
```

The 50-row case is `Full` by `needs_full` (`state.rs:272`: `delta.unsigned_abs() >= viewport.rows`)
and 50 copies is the floor — a whole-viewport scroll exposes 50 row ids the consumer has never
seen. The LLD says "`scrolled` larger than the viewport height" returns `Full`; the code uses
`>=`, which is the correct boundary (at exactly the height nothing survives) and not a regression.

**RowsScrolled consumption path.** `EventBatch::push_scroll_report` (`batch.rs:157-164`) emits
`RowsScrolled` only when `ScrollReport::scrolled` is `Some`, and `Screen::scroll_up_into_history`
returns `None` for a whole-viewport scroll — so `event::tests::a_whole_screen_scroll_emits_no_motion`
pins the "reports nothing" half and `an_in_region_scroll_moves_content_between_row_ids` the other.
This matches the `grid-and-scrollback.md` table restated in the damage LLD. The viewport delta in
`RenderUpdate::Partial { scrolled }` is a *different* quantity, computed independently from
`viewport.top` (`state.rs:277-282`); the two do not alias. Correct.

### (b) Tri-state, and deviation D5 "Full also when every row was copied" — **PASS, not a contract change**

`state.rs:168`: `if full || self.changed.len() == self.rows.len() { return Full }`.

The LLD's list reads "`Full` is returned by: …" — inclusive, not exhaustive. Semantically the
two are equivalent for a conforming consumer: `Partial { scrolled }` says "shift your cache by
`scrolled`, then rebuild the rows in `changed`", and when `changed` is every row there is nothing
left for the shift to preserve. Discarding `scrolled` loses no information a consumer could act
on. It also makes `RIS` and a full repaint reach the view as `Full` without depending on the
terminal remembering to bump the generation — which is the reason given, and it is a good one.

One consequence worth one line in the LLD when `US-0076` folds these readings in:
`changed().len() == rows().len()` is unreachable under `Partial`, so a consumer's "all rows
changed" branch is dead code.

Resize and alt-screen swap are additionally self-detected (`needs_full` compares `size`,
`state.rs:262-270`; `viewport_delta` returns `None` across the row-id lane boundary,
`state.rs:277-282`), so the `Full` guarantee does not rest on the generation alone. Good.

### (c) Watermark independence and monotonicity — **PASS**

`Watermark` is per-`RenderState` (`state.rs:84`), set from the engine-wide `grid.seq()`
(`state.rs:133`, `:158`). Nothing writes back to the grid; `RowFlags::DIRTY` is never cleared
(the only `DIRTY` writes in `crates/vt/src/grid/` are inserts: `row.rs:84`, `:117`, `:124`,
`:132`, `:264`). `render::tests::two_consumers_keep_independent_watermarks` proves a view and a
search state each see row 6 exactly once and neither clears it for the other.

Monotonicity holds by construction (`grid.seq()` only increases) and is guarded by
`debug_assert!(seq >= self.watermark.0)` at `state.rs:134` plus
`render::tests::watermark_never_moves_backwards`.

Importantly the suppressed-frame early return (`state.rs:128-131`) happens **before**
`let seq = grid.seq()`, so a skipped frame does not advance the watermark and no row change is
lost to mode 2026. That part is right — see (h) for what is not.

### (d) The stale-row finding — **the flag is correct; the grid is still wrong**

Reproduced directly. Region scroll `SU` of 1 over rows 1..5, where row 1 was written and row 2
never was:

```
row1 after an in-region scroll: allocated=false seq=SeqNo(0) (watermark was Watermark(SeqNo(1)))
result=Partial { scrolled: 0 } changed=[1]
```

The row keeps its `RowId` (content moves between fixed ids), its `seq` reads `SeqNo(0)`
(`RowRef::seq` maps a `None` header to the default, `grid/row.rs:222-224`), and `0` is **below**
the consumer's watermark of `1`. `RenderRow::allocated` (`row.rs:65`, read at `state.rs:336`) is
the *only* reason the row is copied. Remove it and the consumer paints `"top"` forever.

Two producing paths, both in `crates/vt/src/grid/screen.rs`:

- `place_row` (`:445-449`) stores `None` when the source slot was unallocated — this is the one
  above, and it hits **rows in the middle of a region**, not just the blanked tail.
- `reset_row` (`:457-469`), else branch: an `Empty` erase template stores `None`.
  (Its `Some(row) if row.id() == id` branch *does* keep the allocation and stamp — so
  `reset_rows` on a live row is fine; I confirmed `reset_rows(3..4)` yields `allocated=true
  seq=SeqNo(2)`. Only the take-then-place path drops the header.)

**Is `RenderRow::allocated` a correct fix?** For `RenderState`, yes, and completely: the flag
mirrors exactly the state the header carries, every re-allocation stamps (`row_mut` `:425-435`,
`place_row` `:445-449` via `take_rehomed(id, seq)`), and `RowId`s are never reused. I could not
construct a case it misses.

**Does the grid need the stamp anyway?** Yes, and this is the part to escalate:

1. `DEC-0015` states damage as "a per-row sequence number plus a dirty bit in the row header,
   read through a watermark the consumer owns … so a second consumer becomes possible **without
   an engine change**." That is now false. Any consumer that reads `row.seq() > watermark` — the
   documented mechanism — misses the blanking. The fix lives in one consumer's private field.
2. `RowFlags::DIRTY` goes with the header. The damage LLD says `DIRTY` is the hint for the
   grapheme sweep and the graphics release scan, "where false positives are allowed and **false
   negatives are not**." A de-allocated row that held a `GraphemeId` or a `GraphicId` produces
   exactly a false negative — a leaked arena entry, or a `GraphicReleased` that never fires and a
   view texture that never evicts. Latent today (nothing reads `DIRTY` yet: grep over
   `crates/vt/src/grid/*.rs` + `intern.rs` + `render/*.rs` finds writes only), live at `US-0074`
   and `US-0080`.
3. `reset_row`'s own doc comment (`screen.rs:451-456`) already states the intended rule — "A row
   that *was* written keeps its allocation and gets the batch stamp, because the render state has
   to see that it changed" — and `place_row` defeats it.

Recommendation: **ship the flag** (it is cheap, correct, and tested), and file a BUG against
`US-0075` to make `place_row` / `reset_row` preserve the header, or stamp the blank, so the
documented damage contract is true again. Not a reason to hold this packet.

### (e) Resolved values, never ids — **PASS**

`grep "StyleId\|ExtrasId\|GraphemeId" render/row.rs render/state.rs` (non-test):

- `StyleId` — `row.rs:102`, a local `open_run` comparison inside `copy_from`. Never stored.
- `ExtrasId` — `row.rs:130`, compared against `ExtrasId::NONE`. Never stored.
- `GraphemeId` — resolved at `row.rs:119-127` into the row's own `clusters: Vec<char>`. Never stored.
- `StyleRun.style` is a `Style` **value** read through `interner.resolve_style` (`row.rs:112`).

`render::tests::a_copied_row_survives_a_grapheme_sweep` renumbers the arena under a copied row
and the cluster still reads `['a', '\u{0301}']`. Nothing an id could index survives the lock.

Two ids *are* stored, both correctly:

- `RenderCell.hyperlink: Option<HyperlinkId>` (`row.rs:41`) keys the state's **own** table
  (`state.rs:94`, filled by `resolve_link` `row.rs:171-183`). The view reads
  `state.hyperlink(id) -> Option<&Hyperlink>` with `id` and `uri` as `Box<str>` values
  (`intern.rs:364-368`) and needs no interner — proven by
  `hyperlink_strings_are_resolved_under_the_lock`. As a bonus, `HyperlinkTable` is append-only
  (`intern.rs`: `entries.push`, ids are indices, no replacement), so the LLD's "a link whose
  interned entry is later replaced" hazard does not exist here at all.
- `RenderCell.graphic: Option<GraphicId>` (`row.rs:42`) — an opaque per-terminal id that is never
  reused, exactly what the LLD prescribes. See § 4 for what is still missing around it.

### (f) `map_colors` outside the lock — **PASS**

`Palette` (`palette.rs:21-32`) is a plain `Copy` value: `[Rgb; 256]` plus five `Rgb`/`Option<Rgb>`
fields. `Palette::resolve` (`:61-67`) and `named` (`:69-101`) are pure table lookups and integer
arithmetic. `RenderRow::map_colors` (`row.rs:156-165`) touches only `self.runs[*].style`.
Nothing reaches the grid, the interner or the engine. Idempotent by construction (`Color::Rgb`
passes through), proven by `map_colors_resolves_named_colours_and_is_idempotent`.

`RenderState::map_colors` (`state.rs:183-203`) maps only `changed` when the palette epoch is
unchanged and every row when it is not — and a palette-epoch change forces `Full` anyway
(`state.rs:268`), so every row is in `changed` on that path too. Correct either way.

### (g) The demand/yield handshake — **PASS with note**

`Demand` (`demand.rs`) is a one-bit `Arc<AtomicBool>`: `raise` = `store(Release)`, `take` =
`swap(false, AcqRel)`. Correct orderings for a flag handed between two threads.

The two-thread test (`render_tests.rs`, `pump_yields_to_the_render_demand_within_one_chunk`)
runs a pump rewriting 24 rows per chunk in a loop and a renderer that raises the flag and takes
the lock. It asserts `waited < 2 s` and `chunks_waited <= 8` — not "one chunk", despite the name.

**Is the 250 us park a fairness guarantee?** No, and the test says so in its own comment: it is
there because `std::sync::Mutex` is unfair, so the test's pump parks instead of spinning straight
back in. What the test actually proves is that the `Demand` primitive works and that a pump which
honours it lets a waiter in. It proves nothing about the production loop.

**Is `FairMutex` required and stated?** Yes, in three places, all outside this packet's scope:
`high-level-design.md:326` (the pump holds `FairMutex<Terminal>` for one `feed`),
`:330-333` ("`oneterm-vt` itself contains no lock, no atomic and no interior mutability …
The lock is `parking_lot::FairMutex`"), `:400`, and `damage-and-render-state.md` § "Fairness and
reply latency" with the five-step ordered loop (R-37). The adapter tests
(`backend::tests::replies_are_written_before_any_yield`,
`…::da1_is_answered_within_the_startup_budget`,
`…::pump_yields_to_the_render_demand_within_one_chunk`) are listed under `US-0081`/`US-0082`.
The packet's evidence text is honest about the bound. Acceptable; see F3 for where `Demand`
should live.

### (h) Mode 2026 — **FAIL**

The deadlines themselves are exactly right, on injected time, at both boundaries:

```
t0                     -> suppressed
t0 + 149 ms            -> suppressed
t0 + 150 ms            -> NOT suppressed (frame resumes), is_set() still true
refresh every 100 ms:
t0 + 100..900 ms       -> suppressed at every step
t0 + 1000 ms           -> NOT suppressed, is_set() == false (watchdog forced the mode off)
```

`SyncState::begin` refreshes `open_until` only and arms `watchdog` on the first open
(`sync.rs:43-48`); `suppresses_frame` closes on `now >= watchdog` before testing
`now < open_until` (`:63-72`). `is_set()` stays true past the refresh deadline, which is the
right answer for `DECRQM` — the mode *is* set, only the frames resumed.

*Observation, not a defect:* after the watchdog forces the mode off, the next `CSI ? 2026 h`
re-arms a fresh 1 s watchdog, so a hostile stream still gets roughly one frame per second
indefinitely. The LLD specifies no cooldown; worth a sentence in it.

**The defect is the suppressed-frame return path**, `state.rs:126-131`:

```rust
if sync.suppresses_frame(now) {
    self.changed.clear();
    return RenderUpdate::Unchanged;
}
```

It returns *after* `self.modes` and `self.cursor` have been overwritten (`:120-124`) and *before*
the viewport rows are built. Three proven symptoms:

1. **R-15 violated.** The first-ever `begin_update` on a `RenderState` that lands inside a sync
   block returns `Unchanged` with an **empty** `rows()`:
   `first-ever update inside a sync block: Unchanged, rows()=0`.
   The LLD's `rows_always_hold_the_full_viewport` asserts `rows().len() == viewport.rows` for
   `Unchanged` too, and the `debug_assert_eq!` at `state.rs:159-163` is bypassed on this path.
2. **A mode change inside a sync block is swallowed.** `?2026h` → `?25l` (DECTCEM off, no row
   touched) → `?2026l`. The skipped frame sets `self.modes.show_cursor = false` and reports
   `Unchanged`; the next real frame computes `modes_changed == false` and returns `Unchanged`
   again, so the view never repaints and keeps drawing the cursor:
   `frame after the sync block closed: Unchanged`.
3. **A cursor move inside a sync block is swallowed** the same way:
   `frame after the sync block closed (cursor): Unchanged`.

`sync_tests.rs:102-118` (`mode_snapshot_is_refreshed_even_when_the_frame_is_skipped`) proves the
snapshot *is* refreshed and stops one assertion short of catching this.

Narrow in practice — a sync block that touches no row is unusual — but it breaks the meaning of
`Unchanged`, and `Unchanged` is the one result that tells the element to skip layout and paint
entirely. Fix in § 5, F1.

### (i) `EventBatch` / `VtEvent` coverage — **PASS**

Every variant the fork + OneTerm's patches deliver (`research/api-surface.md` § 3.5, "all 15
matched exhaustively at `osc_router.rs:216-267`"):

| `alacritty_terminal::Event` | Home |
|---|---|
| `Wakeup` | `VtEvent::Repaint` (at most one per batch, `batch.rs:97-103`) |
| `Title(String)` / `ResetTitle` | `Title(StrSpan)` / `TitleReset` |
| `ClipboardStore(ClipboardType, String)` | `ClipboardStore { selection, text }`, base64 already decoded |
| `ClipboardLoad(_, closure)` | `ClipboardLoad { selection }` — the unused formatter closure correctly dropped |
| `PtyWrite(String)` | `Reply(ByteSpan)` |
| `Bell` | `Bell` |
| `Osc { params, bell_terminated }` (patch 0002) | `Osc { code, params: ParamSpans, terminator: StringTerm, truncated }` |
| `ClearScreen` (patch 0002) | `ScreenCleared` |
| `ColorRequest(usize, closure)` | `VtEvent::ColorQuery` — **deferred, owner `US-0076`** (packet Scope, LLD § events) |
| `ChildExit(ExitStatus)` | **not the engine's** — `pty.md` / `US-0071`; stated in events-and-api § "What is deliberately not an event" |
| `Exit` | ignored by OneTerm today (`osc_router.rs:245`); no variant, no loss |
| `MouseCursorDirty`, `CursorBlinkingChange`, `TextAreaSizeRequest` | deliberately not events (LLD, same section) |

Plus the two the new model adds: `RowsScrolled(RowsScrolled)`, `RowsTrimmed { oldest }`,
`GraphicReleased(GraphicId)` (the `I6` capability `api-surface.md` § 9.6 lists as "wanted,
missing today"). Mode changes and damage are deliberately not events (R-54, R-17). DCS
passthrough (patch `vte/0002`) is the parser's (`US-0073`, `R-35`), not this packet's.

**Span lifetimes.** `VtEvent` is deliberately `!Clone` (`vt_event.rs:34-37`) and `events()`
borrows the batch, so an *event* cannot outlive its batch — the borrow checker enforces it
because `clear()` takes `&mut self`. `str`/`bytes`/`params` bounds-check and return `""`/`&[]`
rather than panicking or slicing out of range (`batch.rs:169-192`). No dangling, no UB. One doc
defect: see F7.

---

## 4. What the view needs — gap list with owners

Inventory taken from `crates/terminal/src/content.rs` and
`crates/terminal-view/src/render/frame.rs` in the main checkout, cross-read against
`research/api-surface.md` § 9.

**Can `RenderState` alone drive `frame.rs` today? No** — but every gap has an owner, and only
three of them are US-0079's.

### Covered

| What the view reads | Where today | In `RenderState` |
|---|---|---|
| cell char + combining tail | `frame.rs:298-319` (`c.c`, `c.zerowidth()`) | `RenderContent::{Scalar, Cluster}` + `RenderRow::clusters` |
| fg / bg as {named, palette, truecolor} | `frame.rs:100-140` | `Color::{Named, Palette, Rgb}` in the resolved `StyleRun` |
| INVERSE BOLD ITALIC DIM HIDDEN STRIKEOUT, five underline kinds | `frame.rs:211-224`, `row_plan.rs:183-264` | `Attrs` bits 0-13 incl. `ALL_UNDERLINES` (`cell.rs:337-358`) |
| WIDE_CHAR / WIDE_CHAR_SPACER / LEADING_WIDE_CHAR_SPACER | `frame.rs:219-224`, `:290-293` | `CellWidth::{Wide, WideSpacer, LeadingWideSpacer}` — better: "wide with no spacer" is unrepresentable |
| WRAPLINE | `frame.rs:352-356` (last cell's flag) | `RenderRow.wrapped` (row-level, equivalent) |
| hyperlink id + uri | `frame.rs:306-312` (FNV of `id\0uri`), `:322-325`, `url/detect.rs:10,64` | `RenderCell.hyperlink` + `RenderState::hyperlink()` — a stable `HyperlinkId` is a **better** cache key than the hash |
| display offset | `frame.rs:495-497`, `:524-531`, `:541`, `:548` | `RenderState::scroll_offset()`; the two `display_offset` fallbacks at `frame.rs:514-532` disappear |
| damage | `content.rs:74-79,108` → `frame.rs:504-509` | `RenderUpdate` + `changed()` |
| mode bits | only `APP_CURSOR` at paint (`frame.rs:562-565`); the rest via `session.query_state()` | `ModeSnapshot` covers all of M1 (`api-surface.md` § 9.4) |
| cursor position + visibility | `frame.rs:534-545`, `cursor.rs:41-47` | `RenderCursor { id, col, row, visible }` |
| row-plan cache key | FNV over the whole row (`frame.rs:362-386`) | `RenderRow { id, seq }` — exactly what `DEC-0015` wants it keyed on |
| bold→bright mapping | **the view does none** (`row_plan.rs:207` picks a heavier font only) | nothing needed |

### Gaps

| Gap | View site | Owner |
|---|---|---|
| **Cursor shape** (Block / Beam / Underline / HollowBlock / Hidden) | `frame.rs:421-440`, `cursor.rs:54-55` | **US-0076** — declared deviation D2 (`CursorStyle` is US-0076's) |
| **Selection range** (`start`/`end` points + `is_block`) | `frame.rs:451-463`, `:547-560`, `overlay.rs:32-67` | **US-0078** — declared |
| **Graphics placement geometry**: `RenderCell.graphic` carries only the `GraphicId`; the view needs the per-cell `(col, row)` offset *inside the image's cell grid* plus `width`/`height`/`rgba` and `SIXEL_VIRTUAL_CELL` to place and scale | `frame.rs:264-271`, `:313-317`, `element.rs:332-375`, `graphics.rs:50-74` | **US-0080** — declared (`placements()` + `take_graphics`). Flagging the concrete requirement so US-0080 does not ship a placement table that cannot reproduce the per-cell offset. |
| **Absolute output-line count** for the gutter | `state.rs:404-428`, `gutter_timestamps.rs:66,84` | **US-0076** — `Terminal::lines_produced()` per `DEC-0015` clause 1 |
| **`last_content_line` / `clear_epoch`** | `gutter_timestamps.rs:91`, `state.rs:44-46` | **US-0081** (adapter; read off the grid, not the render state) |
| **No `size()` accessor.** `RenderState.size` is private (`state.rs:87`); the view needs `GridSize { rows, cols }` | `frame.rs:577-582`, `element.rs:108-118`, `overlay.rs:36-44`, `cursor.rs:46` | **US-0079** — 3 lines, see F5 |
| **`Palette` cannot express OneTerm's dim rule.** `palette.rs:110-117` dims by 2/3 toward black; today `crates/terminal/src/palette.rs:128-137` mixes 50 % with the **background**, and the view then also applies `fg.a *= 0.7` (`row_plan.rs:197-199`). `named()` hard-codes `dim(indexed[n])` with no override hook, so the adapter cannot supply the existing colours. | `palette.rs:128-140` (main), `row_plan.rs:197-199` | **US-0079** — see F4 |
| **`Style.underline_color` is carried and mapped** although `api-surface.md` § 9.6 says "`underline_color` is **not** needed" and § 9.9 lists per-cell underline colour under "do not build"; the view never reads it (`row_plan.rs:259`, `:263` use the cell fg) | — | **US-0074** (owns `Style`); note only |

---

## 5. Findings, ranked

### F1 — MAJOR — `begin_update` short-circuits the whole state on a suppressed frame

`crates/vt/src/render/state.rs:126-131`.

Three proven symptoms (§ 3h): `rows()` empty on a first update inside a sync block (R-15
violated, and the `debug_assert_eq!` at `:159-163` bypassed); a mode change inside a sync block
never reported; a cursor move inside a sync block never reported.

Proposed fix — one field, four touched lines:

```rust
// RenderState
meta_dirty: bool,

// begin_update, the suppressed path:
if sync.suppresses_frame(now) && !self.rows.is_empty() {
    self.meta_dirty |= modes_changed || cursor_changed;
    self.changed.clear();
    return RenderUpdate::Unchanged;
}
...
// the Unchanged test at :171
if self.changed.is_empty() && scrolled == Some(0)
    && !modes_changed && !cursor_changed && !self.meta_dirty { ... }
// and clear self.meta_dirty on every non-Unchanged return.
```

The `&& !self.rows.is_empty()` guard also closes the R-15 hole: a state that has never been
built gets built, then suppressed from the *next* frame on. Add the two tests
(`a_mode_change_inside_a_sync_block_reaches_the_next_frame`,
`rows_hold_the_full_viewport_on_a_first_update_inside_a_sync_block`) — both are in the scratch
file and ready to paste.

### F2 — MAJOR (not this packet's to fix) — the grid drops the batch stamp on de-allocation

`crates/vt/src/grid/screen.rs:445-449` (`place_row` storing `None`) and `:457-469`
(`reset_row`'s else branch). Full analysis in § 3d.

`RenderRow::allocated` is the right fix *for this packet* — keep it. But the `DEC-0015` damage
contract ("a second consumer without an engine change") is untrue while this stands, and
`RowFlags::DIRTY` is lost with the header, which the damage LLD says must never produce a false
negative (grapheme sweep, graphics release scan — latent until `US-0074`/`US-0080` consume it).
`reset_row`'s own doc comment at `screen.rs:451-456` states the rule `place_row` breaks.

**File a BUG against `US-0075`** to preserve the header (or stamp the blank) on de-allocation.
Do not hold `US-0079` for it; the packet already recorded the finding, correctly, and routed it
to the design owner.

### F3 — MINOR — `Demand` lives in the engine crate and falsifies the crate doc

`crates/vt/src/lib.rs:3-5` still claims the crate "holds no lock, atomic or interior
mutability"; `crates/vt/src/render/demand.rs:20` holds an `AtomicBool`. The LLD puts `Demand` in
`crates/terminal` explicitly (`// crates/terminal (the adapter owns this; the engine has no
atomics)`) and `high-level-design.md:330` repeats it. The placement is **not in the packet's
deviation list** (the Context section mentions the atomic but does not call the placement a
deviation).

Fix: amend `lib.rs:3-5` now and add the deviation, or move `demand.rs` to `crates/terminal` at
`US-0081`. Either is fine; the doc must stop being false.

### F4 — MINOR — `Palette` cannot express the existing dim rule

`crates/vt/src/render/palette.rs:110-117` (`dim()` = 2/3 toward black), used unconditionally by
`named()` for `DimBlack..DimWhite` and as the `dim_foreground` fallback (`:91`). OneTerm's live
behaviour is a 50 % mix with the background (`crates/terminal/src/palette.rs:128-137`).
A visible colour change at the seam with no way for the adapter to correct it.

Fix: add `dim_indexed: [Rgb; 8]` to `Palette` (or let the adapter pre-fill `indexed` and drop
`dim()` entirely), and let `US-0081` fill it from the theme.

### F5 — MINOR — no `size()` accessor

`crates/vt/src/render/state.rs:87` stores `size: Option<Size>` privately. The view needs
`GridSize { rows, cols }` every frame (`frame.rs:577-582`). `rows()[0].cells.len()` works but is
wrong at zero rows. Three lines: `pub fn size(&self) -> Size`.

### F6 — MINOR — two `#[path]` / visibility tricks that a rename removes

`crates/vt/src/lib.rs:16-17` — `#[path = "events/mod.rs"] pub mod event;` reconciles a directory
name with a module name the packet itself chose. Renaming `events/` to `event/` deletes the
attribute and the comment above it. Likewise `crates/vt/src/render/mod.rs:16` makes `sync` a
`pub mod` *and* re-exports its three items at `:23`, so the module is public only so the test
filter reads `sync::tests::`. Neither is harmful; both are cheaper to delete than to explain.
(The `#[path]` on the three `*_tests.rs` / `*_bench.rs` files is the crate's established
convention and is fine.)

### F7 — MINOR — the arena-span doc overstates its guarantee

`crates/vt/src/events/batch.rs:166-168`: "a consumer that stored raw indices past a `clear` gets
nothing, not a slice of the next batch." `StrSpan` / `ByteSpan` are `Copy` and untagged, so a
span copied out of an event and read back after the next `feed` bounds-checks against the
**new** arena and can return the new batch's bytes. Safe (no UB, UTF-8 re-validated) but not what
the comment promises.

Fix: reword, or add a `generation: u32` to `EventBatch` and to the spans and return empty on
mismatch (~4 lines, bumped in `clear`).

### F8 — MINOR — the fairness test's name does not match its assertion

`render_tests.rs`, `pump_yields_to_the_render_demand_within_one_chunk` asserts
`chunks_waited <= 8` and `waited < 2 s`. The packet's evidence is honest ("let in within 1 chunk,
bound asserted at 8"), but the test name is not. Rename to `…_within_a_bounded_number_of_chunks`,
or tighten the assertion. The real guarantee is the adapter's and is correctly deferred
(`US-0081`/`US-0082`).

### F9 — MINOR — the hyperlink table never shrinks, and both lookups are linear

`state.rs:238-242` (`hyperlink()`) and `row.rs:176` (`resolve_link`'s dedup) are `Vec` scans;
`resolve_link` runs one per decorated cell, so a row of distinct links is O(n²). The table is
cleared only on a `Full` rebuild (`state.rs:294`). Already recorded as a gap in the packet.
Fine at today's link counts; revisit if `US-0076`'s link capping lands.

### F10 — MINOR — public API with no consumer

`Demand::is_raised` (`demand.rs:38-40`, "for diagnostics and tests"), `RenderState::invalidate`,
`RenderRow::{style_of, cluster}`, `EventBatch::{iter, len, is_empty}` are used only by tests
today. All are the view's surface at `US-0081`, so this is not dead code — but `R-24`/`R-54`
deleted speculative surface elsewhere in this intake, so it is worth one pass at `US-0081` to
confirm each is reached. Several accessors also carry no doc comment
(`state.rs:215-233`, `batch.rs:70-84`).

### F11 — NIT — the bench is not tier 3

See § 2.2. Name the owner of the real tier 3 in the packet's Gaps.

---

## 6. Deviation judgments

| # | Deviation | Judgment |
|---|---|---|
| 1 | `EngineView` instead of `Terminal::render_update` | **Accept.** Borrows exactly the fields the LLD's `Terminal` lists; the shim is genuinely three lines. |
| 2 | selection / placements / `ColorQuery` / cursor shape not carried | **Accept.** Every one matches the LLD's own packet split, and each owner is named. |
| 3 | `ModeSnapshot` in `render::modes`, `SyncState` in `render::sync`; `event` module over `events/` | **Accept** the module homes (`mode.rs` is US-0076's). The `#[path]` is self-inflicted — F6. |
| 4 | `RowsScrolled(RowsScrolled)` reusing the grid struct | **Accept**, strictly better: one contract, one struct, no drift. |
| 5 | **`Full` also when every row was copied** | **Accept — not a contract change.** The LLD's list is inclusive; a consumer rebuilding every row has nothing for `scrolled` to preserve. Worth one line in the LLD that `changed().len() == rows().len()` is unreachable under `Partial`. |
| 6 | `pure_scroll_reports_a_delta_with_an_empty_changed_list` renamed | **Accept.** The LLD's name is provably impossible — a viewport scroll always exposes a row the consumer has never seen. The replacement asserts the stronger thing (one copy, not ten). |
| 7 | Allocation proven by capacity, not a counting allocator | **Accept.** `GlobalAlloc` is an unsafe trait and the crate forbids `unsafe`; `US-0075` set the precedent. Independently re-run at 1000 frames + `map_colors`, 200x50: per-row capacities `(256, 4, 0)` unchanged. |
| 8 | `std::sync::Mutex` + a 250 us park in the fairness test | **Accept as a test accommodation**, correctly labelled in the test's own comment. It is not a fairness guarantee and the packet does not claim it is. F8 is about the name only. |
| 9 | `Watermark` in `render::state`, not a `damage` module | **Accept.** R-24 / R-54 deleted that module. |
| — | **`Demand` in the engine crate** | **Undeclared deviation** — F3. The LLD and HLD both put it in the adapter. |

---

## 7. Packet, DB row, trailer, incident

**Packet.** Every heading in `docs/templates/work.md` is present and in order (Status,
Classification, Outcome, Scope, Acceptance, Documentation → Owning Docs Reviewed / Documentation
Action / Reconciliation, Context, Plan, Decisions, Verification Plan, PROOF, Evidence and Gaps,
Handoff), plus Evidence / Deviations / Gaps sub-headings. Status `Implemented`, PROOF: unit +
verify command, integration/E2E/platform correctly unticked with the reason ("nothing in the
workspace depends on `oneterm-vt` yet"). Documentation Action states no contract change with a
reason and a no-edit rule for `docs/terminal-backend.md` until `US-0081` — correct, and the
diff confirms no LLD was touched.

**DB row** (`harness.db`, read-only via python `sqlite3` `mode=ro`; the file is gitignored at
`.gitignore:28` and lives only in the main checkout):

```
id=US-0079 | title=Damage, render state and events | risk_lane=high_risk
contract_doc=.../low-level-design/damage-and-render-state.md
packet_doc=.../US-0079-damage-and-render-state.md
status=implemented | unit_proof=1 | integration_proof=0 | e2e_proof=0 | platform_proof=0
verify_command=pwsh scripts/ci-local.ps1 | last_verified_at=2026-09-12T23:59:00
last_verified_result=pass | intake_id=34
```

`evidence` and `notes` carry the full tri-state table, the watermark and resolved-value proofs,
the bench numbers, all nine deviations and every gap — they match the packet verbatim. Complete.

**Commit trailer** — exact, byte for byte (`git log -1 --format=%B | cat -A`):

```
Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>$
Claude-Session: https://claude.ai/code/session_018tAaHVGPnSzrEHLVcg3sx1$
```

**Wrong-base incident** — recorded in **both** places. Packet, "Evidence" § "Base correction":
"This worktree was branched from `main` (`c936ac0`), not from `feat/vt-engine`, so `crates/vt`
was missing entirely. The branch was `git reset --hard 83933b9` before any file was written."
DB `notes`: "BASE CORRECTION: the worktree was branched from main (c936ac0) without crates/vt;
reset --hard 83933b9 before any file was written." Verified: `git merge-base --is-ancestor
83933b9 HEAD` succeeds, and `git diff 83933b9...HEAD --stat` shows only this packet's 16 files —
no `main` residue.

---

## 8. Verdict

**Merge after fixes.**

Fix in this packet before merge:

1. **F1 (major)** — the suppressed-frame short-circuit in `state.rs:126-131`. ~6 lines plus two
   tests, both ready in the scratch file.
2. **F3 (minor)** — `lib.rs:3-5` is false; amend it and declare the `Demand` placement as a
   deviation (or move the file).
3. **F4 (minor)** — give `Palette` a way to express the existing dim colours.
4. **F5 (minor)** — add `RenderState::size()`.

File separately, do not hold this packet:

5. **F2 (major)** — a BUG against `US-0075`: `Screen::place_row` / `reset_row` must not drop a
   row's batch stamp when they de-allocate it. Keep `RenderRow::allocated` regardless.

F6-F11 are notes for `US-0081` and the design owner.

The core of the packet is right, and better than the LLD in two places (the `CellWidth` enum
making "wide with no spacer" unrepresentable; `HyperlinkId` as a stable cache key in place of the
view's FNV hash). The tri-state is bounded by change as designed, the phase boundary is clean of
interned ids, the watermark model is genuinely multi-consumer, the event surface accounts for
every variant the fork delivers, and the numbers reproduce.
