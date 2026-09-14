# Adversarial design review: IN-0029 VT engine rewrite

Date: 2026-09-12
Reviewer: independent design review, documents only (no code run, no project file changed except this one)
Scope reviewed: `AGENTS.md`, `docs/HARNESS.md`, `docs/agents/{structure,crate-dependency-rules,dependencies,code-style,error-policy}.md`,
`docs/terminal-backend.md`, `research/{api-surface,engine-semantics,prior-art,perf-baseline}.md`,
`docs/decisions/DEC-0014`, `DEC-0015`, `IN-0029.md`, `high-level-design.md`, and all eleven files under `low-level-design/`.
Packet numbering is reviewed as written (`US-0071`…`US-0082`); the announced +1 shift is ignored.

Findings are ranked by severity. **Blocker** = the design cannot produce correct packets until it changes.
**Major** = the packet will be built wrong or unverifiably. **Minor** = a documentation or scoping defect.

## Summary

| Severity | Count |
| --- | ---: |
| Blocker | 10 |
| Major | 27 |
| Minor | 21 |
| **Total** | **58** |

By dimension:

| # | Dimension | Blocker | Major | Minor |
| --- | --- | ---: | ---: | ---: |
| 1 | Correctness against current engine semantics | 2 | 4 | 6 |
| 2 | Internal consistency | 3 | 5 | 5 |
| 3 | Feasibility in Rust | 0 | 3 | 4 |
| 4 | ConPTY reality | 0 | 4 | 2 |
| 5 | Migration risk | 2 | 3 | 1 |
| 6 | Harness and process compliance | 0 | 2 | 3 |
| 7 | Over-engineering | 0 | 3 | 3 |
| 8 | Test strategy | 2 | 3 | 3 |
| 9 | Missing pieces (accounted for in 1–8) | 1 | 0 | 4 |

Two things the review confirms as **sound**, so no packet re-litigates them:

- The 8-byte cell bit budget **fits**: 21 (content) + 1 (is_grapheme) + 2 (width) + 2 (semantic) + 1 (protected)
  + 1 (has_extras) + 4 (reserved) + 16 (style_id) + 16 (extras_id) = 64, and 2^21 = 2 097 152 > 0x10FFFF.
  Four bits spare.
- Nothing in the design assumes a Unix-style PTY. `pty.md` preserves the bundled-host resolution order, the IOCP
  pipe threads, the `ClosePseudoConsole` drop-order invariant and the `RegisterWaitForSingleObject` child watcher.

---

## Dimension 1 — Correctness against the current engine's semantics

Traps checked line by line against the LLD text: 1–22, 24–43, 46–48 (43 of 48; 23, 44, 45 are parser/harness items
covered in dimensions 2 and 8). The trap table in `testing-and-bench.md` § "Trap list — master map" is complete and
each row names a test. The defects below are cases where the **text specifies the wrong behaviour**, not where a
test is merely named.

### R-01 · Blocker · The viewport's relationship to the bottom row is never defined

**Location:** `grid-and-scrollback.md:37-39`, `:159-172`, `:213`; `DEC-0015` § "Decision 1".

The design states `Viewport { top: RowId, rows, cols }`, "scrolling the viewport moves `top`", and that `scroll_up`
with `region.top == 0` is "`newest += n`". It never says what happens to `viewport_top` on that scroll. If
`viewport_top` is a plain absolute value, ordinary output freezes the view; if it tracks the bottom, the design must
carry the "sticky bottom" state that `display_offset == 0` encodes in the reference, and every operation must say
which it does.

The visible symptom is trap 9. `grid-and-scrollback.md:213` says `ED 2` "keeps the content, leaves `viewport_top`
where it was". The reference (`engine-semantics.md` § 2.7, trap 9) preserves **`display_offset`** while the bottom
moves down by `positions`, which in absolute space is `viewport_top += positions`. As written, a scrolled-back user
sees the pre-clear rows frozen where alacritty scrolls them; at the bottom, the viewport would stop following output
entirely. The test `grid::tests::ed2_keeps_the_scrolled_back_viewport_position` cannot be written from this text.

**Why it matters:** this is the core of `DEC-0015`. Every scroll, every `ED`, every trim and every resize touches it,
and it is the one thing the whole viewport model rests on.

**Fix:** add a "Viewport anchoring" subsection to `grid-and-scrollback.md` defining a single invariant — e.g.
`viewport` is `(distance_from_newest: u32, sticky_bottom: bool)` internally and `top: RowId` only in the API — and
restate `scroll_up`, `scroll_down`, `clear_viewport`, `clear_history`, `trim` and `resize` against it. State the
`display_offset` equivalence for every trap that references it (9, 10, 30, 35).

**Effort:** M

### R-02 · Blocker · `RowId` stability breaks under scroll regions, `IL`, `DL` and `SD`, and nothing reports it

**Location:** `grid-and-scrollback.md:59` (`slot(id) = id & mask`), `:159-172`; `DEC-0015` § "Decision 1"
("Stable row identity is a first-class contract, relied on by OSC 133 prompt marks, search results, Sixel
placements, selection anchors and the agent panel"); `graphics.md:53-57`, `:184-186`.

Because the ring index *is* the row id, ids must stay positionally ordered and contiguous. Therefore any scroll that
is not a whole-viewport scroll — `region.top != 0`, `region.bottom != rows`, `IL`, `DL`, `SD`, `RI` at the region
top — must **copy content between fixed ids**. A mark, a search hit, a selection anchor or a Sixel placement that
names `RowId(N)` then silently refers to different content. That is the normal case inside tmux, vim, htop and any
full-screen TUI.

`graphics.md:184-186` asserts the opposite: "the placement's `top` follows the row, because it is a `RowId`". It does
not, for any in-region scroll. `VtEvent` has `RowsTrimmed` but no shift event, so the embedder cannot compensate.

**Why it matters:** the contract this intake exists to establish is wrong for the workloads the intake benchmarks
(`tmux_htop`, `vim_large_window_scroll`). Retrofitting it after `US-0075` costs the grid, the graphics placement
model and every consumer.

**Fix:** decide and write down which of the two meanings `RowId` has. Recommended: `RowId` names a **position in the
output stream**, not a piece of content; then say so in `DEC-0015`, delete the "anchors survive" claim for in-region
motion, and either (a) emit `VtEvent::RowsShifted { region, delta }` so anchored consumers re-anchor, or (b) keep an
engine-side anchor registry (marks, placements) that the scroll primitives update. Option (b) is what `Placement`
already half is.

**Effort:** L

### R-03 · Major · `scroll_up` with `region.top == 0` but `region.bottom < rows` is unspecified

**Location:** `grid-and-scrollback.md:161-166`.

"`newest += n`, `n` fresh rows at the bottom of the region" moves the whole viewport, including the rows **below**
`region.bottom`, which must not move. The reference rotates the storage and then swaps the below-region rows back
(`grid/mod.rs:252-307`). The table has no row for this case.

**Fix:** add the sub-case explicitly, including which rows keep their ids (see R-02).
**Effort:** M

### R-04 · Major · The alternate grid's `RowId` space is undefined

**Location:** `grid-and-scrollback.md:43-57` (each `Grid` owns `newest`, `oldest`, `mask`), `:178-186`
(`alt.cursor = primary.cursor`).

Each `Grid` carries its own `newest`/`oldest`, so either ids are allocated per grid — and are therefore **reused**
across the two screens, violating `DEC-0015`'s "never reused" and making `Terminal::row(id)` ambiguous — or they come
from one counter, in which case the alternate grid's ids are not contiguous with its own ring and `slot = id & mask`
breaks. The `swap_alt` pseudocode copies a `Pos { row: RowId }` from the primary cursor into the alternate cursor,
which is meaningless under either reading.

**Fix:** state that ids are allocated from one terminal-wide counter, that each grid keeps its own contiguous run,
and that `swap_alt` sets the alternate cursor's **row index**, not its `RowId`. Add `assert_integrity` coverage that
the two grids' live ranges never overlap.
**Effort:** M

### R-05 · Major · `rows_produced()` is not a replacement for `LineAccounting`

**Location:** `DEC-0015` § "Consequences"; `high-level-design.md:337`; `migration.md:97`; against
`crates/terminal/src/backend/line_accounting.rs:16-48` and `crates/terminal-view/src/terminal_view/gutter_timestamps.rs:62-83`.

`LineAccounting` counts **output lines** (it adds the `\n` count of a batch once history is full) and **restarts**
when `total_lines` shrinks — a clear, an alt-screen swap or a resize. `rows_produced() = newest + 1` counts **rows
created**: inflated by every wrapped line, changed by every reflow (which allocates a fresh id per new row,
`reflow-and-resize.md:91`), and never reset, by design (`DEC-0015`: ids survive `RIS` and `ED 2`). The gutter's
line numbers therefore change behaviour after a `clear`, after a window resize and on any long wrapped output.

This is a user-visible change presented as a pure deletion, and it is not in any deviation table.

**Fix:** either keep a separate `lines_produced: u64` counter incremented on `linefeed`/`NEL`/region scroll (not on
wrap), or record the gutter behaviour change as a deviation with the owner's agreement. State which in
`grid-and-scrollback.md`.
**Effort:** S (doc) / M (behaviour)

### R-06 · Major · Trap 30 is restated as something the design cannot do

**Location:** `reflow-and-resize.md:114-116`, `:203-204`, `:241-242`.

"`viewport_top` is preserved across both (clamped to the row range)" is not the reference behaviour. The reference
adjusts `display_offset` by ±1 per row created or destroyed **above the viewport** and finishes with
`min(display_offset, history_size())` (`engine-semantics.md` trap 30). Worse, after a column reflow the old top row
may not exist — step 5 gives every row a fresh id — so "the top row is unchanged", which
`reflow::tests::viewport_top_is_preserved_across_resizes` asserts, is not a well-formed statement.

**Fix:** make the viewport anchor a `TrackPoint` (kind `ViewportTop`) remapped through the same iterator as the
cursor, and restate the test as "the character at the old viewport top is still the first visible character".
**Effort:** M

### R-07 · Major · `KeepViewportTop` is specified twice, incompatibly, and the alt-screen case is ambiguous

**Location:** `reflow-and-resize.md:109-116` (table) versus `:141-154` (procedure).

The table says the policy keeps the viewport top, appends rows at the bottom and keeps the cursor's row. The
procedure says it measures `conhost_row`, then runs `rows_only(BottomAnchor)`, then shifts by
`cursor_row_index - conhost_row`, then restores the pre-resize `viewport_top`. These are different algorithms with
different outputs. Additionally, `conhost_cursor_row` reads `self.viewport_top` and `self.cursor`
(`:135`) without saying which grid, while `docs/terminal-backend.md` § 5.3 is explicit that the correction applies
to the **primary (inactive)** grid while the alternate screen is active, and that the alternate grid is left alone.

**Fix:** delete the table row for `KeepViewportTop` and keep only the procedure; add the grid selection explicitly
(`primary` always) and say which cursor `measure_rows` uses when the alternate screen is active.
**Effort:** M

### R-08 · Minor · Trap 1 and deviation D12 contradict each other

**Location:** `grid-and-scrollback.md:287-289` ("`BS` at column 0 is a no-op: there is no reverse wrap even when the
previous row is `WRAPPED`") versus `dispatch-and-modes.md:162` (D12: `? 45` implemented, "`BS` at column 0 moves to
the previous row's last column when that row is `WRAPPED`").

Reconcilable only by gating the grid rule on `Mode::ReverseWrap` (default reset), which neither file says.
**Fix:** one sentence in `grid-and-scrollback.md`. **Effort:** S

### R-09 · Minor · Deviation G3 is observable, not invisible

**Location:** `grid-and-scrollback.md:125-130`, `:247`.

With `DECAWM` off the reference leaves `input_needs_wrap` set forever (trap 4). That flag is read by trap 2
(`EL 0` erases nothing while set) and trap 3 (`HT` consumes the wrap). Under G3 the flag is never set, so a program
that turns off `DECAWM`, fills the last column and then sends `CSI K` or `HT` gets **different** behaviour from the
engine being replaced. "Same visible result" is only true for the printing path.

**Fix:** say so in the G3 row and add the two combination tests.
**Effort:** S

### R-10 · Minor · Trap 36 (`occ`) is specified two ways

**Location:** `cell-and-style.md:43` ("a zeroed row is a blank row and `Row::reset` is a `memset`") versus
`grid-and-scrollback.md:100-104` (`Row::reset` clears `0..occ` only when the background matches the template,
otherwise everything; `occ` promoted from hint to asserted content invariant).

A BCE fill with a non-default background cannot be a `memset` of `Cell(0)`, and making `occ` exact contradicts its
definition ("highest column touched since reset"): the reference deliberately allows it to over-report.
**Fix:** pick one; keep `occ` as an over-approximating hint and drop it from `assert_integrity`.
**Effort:** S

### R-11 · Minor · Mode 2026 reads the wall clock inside the engine

**Location:** `damage-and-render-state.md:146-158` (`Instant`, `SYNC_REFRESH`, `SYNC_WATCHDOG` evaluated inside
`render_update`).

The engine is otherwise pure, I/O-free and replay-deterministic, which is what makes the parity gate, the
differential runner and the fuzz targets possible. Reading `Instant::now()` inside it makes a replay's output
time-dependent, and the three `sync::tests::*` cannot be deterministic.

**Fix:** pass `now` into `render_update` (and into `feed` if the watchdog must fire without a frame). One parameter.
**Effort:** S

### R-12 · Minor · `put_tab` writes a literal `\t` into the cell with no downstream rule

**Location:** `grid-and-scrollback.md:230-232` (reference parity, correct) — but `row_text`, `is_blank`,
`is_erasable`, the corpus snapshot and the search text path are never told what a `\t` cell means. The
`tab_rendering` recording pins it.
**Fix:** one row in the `cell-and-style.md` predicate section. **Effort:** S

### R-13 · Minor · `ED 2`'s occupancy scan and graphic-only rows

**Location:** `grid-and-scrollback.md:216-218` (the scan uses `is_erasable()`, "the reference's looser predicate")
versus `cell-and-style.md:123-125` (which notes the Sixel patch had to extend two such predicates).

A cell that holds only a `GraphicRef` is a space with the default style; under the reference's `is_empty` rule as
transcribed it counts as erasable, so `CSI 2 J` over an image would decide the image rows are unoccupied and not
scroll them into history.
**Fix:** state that `is_erasable()` is false when `has_extras` carries a `GraphicRef`, and test it.
**Effort:** S

---

## Dimension 2 — Internal consistency

### R-14 · Blocker · `RenderState::resolve` cannot resolve anything outside the lock

**Location:** `damage-and-render-state.md:99` (`RenderCell` carries `style_id`), `:108-117` (phase 1 copies
"style **ids**, not colours"; phase 2 runs "outside the lock"), `:182-191` (`fn resolve(&mut self, palette: &Palette)`);
`cell-and-style.md:236-249` (`Interner` is owned by the `Grid`); `DEC-0015` § "Decision 2".

`resolve` is handed only a `Palette`. Expanding a `StyleId` requires the grid's `StyleSet`; expanding an `ExtrasId`
(hyperlink, graphic) requires its `ExtrasTable`; both live inside the `Grid` behind the mutex. Worse,
`cell-and-style.md:101-103` runs a **sweep at the end of `feed()`** that renumbers every id — so ids captured by
`render_update` can be renumbered by the pump thread before the render thread calls `resolve`, and the frame paints
the wrong colours with no assertion able to catch it.

**Why it matters:** this is the central claim of `DEC-0015` decision 2 and of the whole `US-0078` packet, and it is
unimplementable as written.

**Fix:** choose one and write it down. (a) `render_update` copies resolved `Style` values into `RenderCell` under the
lock — drops the "ids not colours" claim, costs ~20 B/cell for changed rows only, and removes the hazard entirely.
(b) `render_update` also copies (or `Arc`-clones a copy-on-write snapshot of) the style/extras tables, and the sweep
is forbidden while any watermark is outstanding. (a) is smaller and should be the default; note that (a) also
removes most of R-53's motivation.
**Effort:** M

### R-15 · Blocker · `RenderState::rows()` is specified as two incompatible things

**Location:** `damage-and-render-state.md:88` (`rows: Vec<RenderRow>, // reused; indexed by viewport row`) versus
`:66-68` ("`Partial { scrolled }` — rows in `RenderState::rows()` are the ones that changed") and `:239-240`
(`pure_scroll_returns_a_delta_without_copying_rows` asserts `rows().len() == 0`).

A viewport-indexed array always has `rows().len() == viewport.rows`. The renderer needs the **whole** viewport every
frame (it paints all of it) plus the list of what changed.

**Fix:** two fields: `rows: Vec<RenderRow>` indexed by viewport row (always full) and `changed: Vec<u16>` (viewport
row indices copied this update); `Partial { scrolled }` then means "shift your cache by `scrolled`, rebuild the rows
in `changed`". Update the three tests.
**Effort:** S

### R-16 · Major · Graphics are drained by two different owners

**Location:** `high-level-design.md:186` and `graphics.md:161` (`Terminal::take_graphics()` — "drain, oldest first,
**once**") versus `damage-and-render-state.md:91`, `:112-113`, `:188` (`render_update` copies "the drained graphics"
into the render state; `RenderState::take_graphics`).

If `render_update` drains, the adapter's `take_graphics` sees nothing; and with per-consumer render states
(`DEC-0015`: "a per-consumer watermark", `high-level-design.md:236`: "any other consumer … holds its own watermark")
only the first `render_update` caller ever receives an image.

**Fix:** one owner. Recommended: `Terminal::take_graphics()` is the only drain, called by the adapter, and
`RenderState` never touches pixels; the view's store is keyed by `GraphicId` and fed from the adapter.
**Effort:** S

### R-17 · Major · The view loses its mode snapshot

**Location:** `damage-and-render-state.md:84-93` (`RenderState` fields) versus `crates/terminal/src/content.rs:97`
(`TerminalContent.mode: TermMode`), `crates/terminal-view/src/render/frame.rs:564` (`content.mode.contains(APP_CURSOR)`),
`crates/terminal/src/model.rs:192-405` (~10 reads of `ALT_SCREEN`, `MOUSE_MODE`, `MOUSE_MOTION`, `APP_CURSOR`).

`RenderState` carries generation, watermark, viewport top, rows, cursor, selection and graphics — no modes. The
`high-level-design.md` consumer map does not mention `TermMode` at all. `Terminal::mode()` and `mouse_reporting()`
exist but require the lock, which the view does not hold at paint time.

**Fix:** add a `modes: ModeSnapshot` (a small `Copy` struct: alt screen, app cursor, app keypad, bracketed paste,
mouse protocol, show cursor, insert) to `RenderState`, refreshed on every `render_update` including `Unchanged`.
**Effort:** S

### R-18 · Blocker · Selection is public API with no design and no owning packet

**Location:** `high-level-design.md:177-183` (`selection_start/update/range/text/clear/select_all`, `SelectionKind`,
`Side`), `events-and-api.md:207` (`pub use selection::{SelectionKind, SelectionRange, Side}`),
`events-and-api.md:139` (`Config::semantic_escape_chars`) — and no `low-level-design/selection.md`, no mention in
`US-0074`/`US-0075`/`US-0078`, and no line in the trap-list map.

The product depends on four selection kinds with distinct semantics (`crates/terminal-view/src/input/mouse.rs:300-307`:
`Block`, `Semantic`, `Lines`, `Simple`), fractional-coordinate hit testing
(`mouse.rs:346`), `Selection::new(kind, point, side)` (`crates/terminal/src/model.rs:248-260`), rotation on scroll and
trim, invalidation on erase (`grid-and-scrollback.md:220-222` assumes selection rules exist), clearing on resize, and
`selection_text` across wrapped logical lines, wide pairs and block rectangles.

**Why it matters:** `US-0079` cannot compile without it, and no packet is scoped to build it. This is the largest
single omission in the design.

**Fix:** add `low-level-design/selection.md` (kinds, expansion rules with `semantic_escape_chars`, side handling,
anchors as `RowId` + column, invalidation matrix per operation, `selection_text` rules for wrap/wide/block) and add
`US-00xx — selection` between `US-0075` and `US-0079`, or scope it explicitly into `US-0075` with its own acceptance
clause.
**Effort:** M

### R-19 · Major · Search is assumed to port, but its lock-held snapshot is removed

**Location:** `high-level-design.md:343` and `migration.md:136` versus `crates/terminal/src/search.rs:70-148`
(`GridText::from_term` copies every cell of history + viewport under the lock, once, so the search runs unlocked).

The new API offers `row_text(RowId, &mut String)` per row. Eleven tests are listed as "Keep". Nothing says whether
the `GridText` snapshot survives (an O(rows×cols) lock-held copy the design elsewhere argues against) or whether
search now loops `row_text` under the lock. `crates/terminal/src/url.rs` and `url_policy.rs`, which read the same
snapshot, are not mentioned anywhere in the migration.

**Fix:** one paragraph in `migration.md`: keep `GridText` as an adapter-side snapshot built from `row_text` over
`row_range()`, or state the alternative. Add `url.rs`, `url_policy.rs` and `color_classification.rs` to the per-file
swap list.
**Effort:** S

### R-20 · Major · `Interner` ownership does not borrow-check

**Location:** `cell-and-style.md:236` ("`pub struct Interner { … } // one per grid; grid owns it") and `:247`
(`pub fn sweep_all(&mut self, grids: [&mut Grid; 2]) -> SweepStats`), `:75` ("primary and alternate each own one"),
`:101` ("walk every row of **both** grids").

If the interner lives inside a `Grid`, `grid.interner.sweep_all([&mut grid, &mut alt])` aliases. If there is one per
grid, sweeping both grids from one is wrong. `grid-and-scrollback.md:55` also places `interner: Interner` inside
`Grid`, while `reflow-and-resize.md` says "reflow moves it".

**Fix:** move `Interner` up to `Terminal` (one per terminal, shared by both screens — which also fixes the
alt-screen half of R-14), or make the sweep a free function taking `&mut Terminal` and destructuring its fields.
**Effort:** S

### R-21 · Blocker · Interning `Extras` is defeated by graphics and exhausts the table on one image

**Location:** `cell-and-style.md:112-121` (`Extras { hyperlink, graphic }`, interned, `u16` id, "same four-step
ladder"), `graphics.md:27` (`GraphicRef { id, col, row }` where `(col, row)` is the offset **inside the image**),
`high-level-design.md:281` (`ExtrasTable`, 65 535 entries).

Every covered cell of an image carries a *distinct* `(id, col, row)` triple, so interning deduplicates nothing and
each cell consumes one `ExtrasId`. A 4096×4096 Sixel at the 10×20 virtual cell covers ~410×205 = **84 050 cells** —
more than the whole `u16` id space, from a single image. The ladder's step 4 falls back to id 0, which means "no
extras", so the image silently loses its cells and `GraphicReleased` fires against a half-referenced placement.

**Fix:** do not intern per-cell-unique data. Either store the graphic reference out of band (Ghostty's per-page
`grapheme_map` shape: a map keyed by `(RowId, col)` on the row or grid, which `RowFlags::HAS_GRAPHIC` already lets
you skip), or pack `(col, row)` into the cell's spare bits plus a small per-image base so the interned entry is one
per image, not one per cell. Update `cell-and-style.md`, `graphics.md` and the HLD memory table together.
**Effort:** M

### R-22 · Major · `GraphicReleased` cannot be correct as specified

**Location:** `graphics.md:57-66` — `live_cells` is "decremented in exactly one place: the cell writer, and only
when the old cell had `has_extras`".

Cells also stop referencing an image through: `Row::reset` / `reset_rows` (a `memset`, `cell-and-style.md:43`),
scroll blanking (`grid-and-scrollback.md:164-166`), `clear_viewport`, `reset_all_rows` on alt-screen entry, the
uniform↔general coercion, and reflow's row destruction. None of these goes through the cell writer. `live_cells`
therefore never reaches zero for any image cleared by `CSI 2 J`, `clear`, or a TUI repaint — which is the common
case and precisely the capability being added.

**Fix:** derive liveness from the row rather than from a counter: on any row reset/destroy, if the row has
`HAS_GRAPHIC`, scan it and decrement. Or drop `live_cells` and release a placement when its `top..top+rows` range is
fully trimmed or fully non-`HAS_GRAPHIC`, checked lazily at the end of `feed()`.
**Effort:** M

### R-23 · Minor · The dependency list disagrees with itself in three places

**Location:** `IN-0029.md` § "Project Impact" (six: `parking_lot`, `unicode-width`, `unicode-segmentation`,
`smallvec`, `memchr`, `proptest`) versus `high-level-design.md:45` (adds `bitflags` as a `oneterm-vt` dependency and
drops `parking_lot`, which is the *adapter's*) versus `cell-and-style.md:78` (`HashMap<Style, u16> /* FxHash */` —
`rustc-hash`, a seventh) versus `testing-and-bench.md:133-141` (`cargo-fuzz` implies `libfuzzer-sys` and
`arbitrary`).

Verified in `Cargo.lock`: all are present, but `bitflags` and `rustc-hash` each resolve to **two** versions, and
`deny.toml:88` sets `multiple-versions = "warn"`.

**Fix:** one authoritative table in the HLD listing every new direct declaration, the version to pin, and the crate
that declares it. **Effort:** S

### R-24 · Minor · Two types for one watermark, and mismatched receivers

**Location:** `damage-and-render-state.md:86` (`RenderState.watermark: SeqNo`) versus `:194` (`pub struct Watermark(SeqNo)`),
and `:196` (`changed_rows(&self, …)`) versus `high-level-design.md:156` (`render_update(&mut self, …)`).
**Fix:** one type; say why `render_update` needs `&mut self` (it clears nothing — the dirty bit is a hint), or make it
`&self`. **Effort:** S

### R-25 · Minor · "Eleven" versus "twelve" detail-design files

`high-level-design.md:407-419` and `IN-0029.md` § "Candidate Product Contracts" say eleven; `pty.md:11-12` calls
itself "the shortest of the twelve" and says it is "not in the owner's original eleven". The folder holds eleven.
**Effort:** S

### R-26 · Minor · "The one file above the seam that names an engine type" is false

**Location:** `migration.md:29-32`. `crates/terminal-view/src/input/mouse.rs:16`,
`crates/terminal-view/src/input/mouse_tests.rs:218` and `crates/terminal-view/src/theme/palette.rs` also import
`alacritty_terminal`. The quoted header in `frame.rs:3-7` says "the only file under `render/`", which is true and
narrower. `migration.md` step 4 does list mouse and palette, so only the claim is wrong — but it is the claim the
migration risk argument rests on.
**Effort:** S

### R-27 · Minor · Grapheme sweep trigger is computed against the wrong id space

`cell-and-style.md:156` ("`spans.len() >= 32_768` (half the id space)") — the id space is the cell's 21 content bits
(2 097 152), not 65 536. `high-level-design.md:282` says "50 % of the id space", inheriting the error.
**Fix:** state the trigger as an absolute constant with a rationale. **Effort:** S

---

## Dimension 3 — Feasibility in Rust

### R-28 · Major · `assert_integrity()` at the end of every mutating public method is not affordable

**Location:** `events-and-api.md:186`, `cell-and-style.md:314-317`, `grid-and-scrollback.md:280-283`,
`testing-and-bench.md:44-47`.

`assert_integrity` walks both grids (wide-pair consistency, `occ`, row ids, row flags) and every interned id. Called
at the end of every mutating public method, in debug, over the `row_reset` recording (1200 rows), the `history`
recording (1000 rows), 45 replays, ~10 000 proptest cases and every unit test, it turns `cargo test --workspace` —
which is the CI gate and runs in debug — into a multi-minute-to-hours job. This is the mechanism Ghostty uses, but
Ghostty's is per *page*, not per grid.

**Fix:** scope it. Call it once per `feed()`/`resize()`/`render_update()` rather than per mutation; make the
full-grid walk opt-in behind a `vt-paranoid` feature used by the property tests and the fuzz targets; keep a cheap
O(1) invariant check on the hot path. Record the intended debug-suite runtime budget in `testing-and-bench.md`.
**Effort:** S

### R-29 · Major · The resize performance target is unachievable and contradicts the design's own algorithm

**Location:** `high-level-design.md:398` ("target: nearer Rio's 5 us than alacritty's 227 us, recorded not gated")
and `reflow-and-resize.md:284-287` (the bench resizes "a filled 80x24 grid with 100 000 rows of scrollback").

The algorithm reflows the whole scrollback, allocates a fresh `RowId` per new row and one `RowRemap` entry per old
row (`reflow-and-resize.md:37`, `:91`). That is inherently O(scrollback) — milliseconds for 100 000 rows, not
microseconds. `prior-art.md` § 11.5 itself says two contradictory Rio number sets exist and only the *ratio* is
trustworthy.

**Fix:** delete the numeric target from the phase plan; state the cost model (O(live rows × cols)) and the real
lever — reflow the viewport eagerly and the history lazily or incrementally, which is also what makes a drag-resize
usable. If lazy history reflow is out of scope, say so and set the expectation accordingly.
**Effort:** S

### R-30 · Major · Ring size versus a changing scrollback limit and a changing row count

**Location:** `grid-and-scrollback.md:44-59` (`slots` length is `next_power_of_two(scrollback + rows)`,
`slot(id) = id & mask`).

`rows` changes on every resize and `scrollback_limit` changes when the user edits `terminal.json`. Changing either
changes `mask`, which invalidates the position of **every** live row and forces a full rehome. Nothing in
`reflow-and-resize.md` mentions it, and the resize fast path (`:58`, "if `size == current`: return early") does not
cover it.

**Fix:** size the ring from `SCROLLBACK_MAX`-independent terms (e.g. `next_power_of_two(scrollback_limit + MAX_ROWS)`)
so `mask` is fixed for the session, or add an explicit rehome step to `resize` with its cost stated.
**Effort:** S

### R-31 · Minor · `RowRemap` is an O(scrollback) allocation per resize

`reflow-and-resize.md:37` — one `(RowId, RowId, u16)` entry per old row, allocated on every drag frame. The remap is
piecewise-monotonic by construction (`reflow::props::row_ids_stay_ordered`), so a run-compressed form is both
smaller and O(log n) to query.
**Effort:** S

### R-32 · Minor · The `Terminal` struct is never shown, and `Handler` is under-specified

`dispatch-and-modes.md:362` gives `Handler<'a> { grid, modes, out, … }`, but dispatch needs both grids, the interner,
the sync state, the graphics queue, the tab stops, the title stack, the keyboard stacks and the colour overrides,
while `parser::advance(&mut self, d: &mut D, …)` needs a disjoint `&mut Parser`. This is solvable by field
splitting, but no LLD shows the owning struct, so the implementer must invent the split — and the split determines
whether `Handler` can be built at all.
**Fix:** add the `Terminal` field list to `events-and-api.md` and show the `let Terminal { parser, screens, .. } = self;`
split in `feed`. **Effort:** S

### R-33 · Minor · `EventedReadWrite` uses RPITIT and leaks `polling` into a public contract

`pty.md:47-53` — `fn reader(&mut self) -> &mut impl Read` makes the trait non-object-safe and the reader type
unnameable; the crate being replaced uses associated types (`type Reader: Read`). `register`/`reregister`/`deregister`
put `polling::{Poller, Event, PollMode}` in the public API of a new crate, which pins `polling`'s version in
`oneterm-pty`'s contract.
**Fix:** associated types; and say explicitly that `polling` is a public dependency. **Effort:** S

### R-34 · Minor · Redundant cell bits

`cell-and-style.md:37-38` — `has_extras` duplicates `extras_id != 0`, and the reserved-bit comment lists "blink",
which is already an `Attrs` bit (`:67`). Cosmetic, but the four spare bits are the design's only headroom for Kitty
placeholders.
**Effort:** S

---

## Dimension 4 — ConPTY reality (Windows first)

### R-35 · Major · `Passthrough` has no destination, and the mechanism it copies does not apply to OneTerm

**Location:** `parser.md:194-210` (`SequenceEcho`, `ECHO_CAP`, `Handled::No`), `dispatch-and-modes.md:113`, `:180`,
`:353` (D16), `events-and-api.md:48`, `high-level-design.md:205`.

Windows Terminal's `FlushToTerminal` exists because conhost is a **relay**: bytes it does not understand must reach
the terminal behind it. OneTerm is the terminal at the end of the chain. No document says what the embedder does with
`VtEvent::Passthrough`. Writing the bytes back to the PTY would echo conhost's own `ESC [ ? 9001 h`,
`ESC [ ? 1004 h`, `ESC [ 6 n` and `ESC [ c` probes straight back at it; dropping them makes the whole mechanism a
no-op with a cost.

The cost is not small: a 4 KiB buffer that must survive chunk boundaries, a per-byte append on every escape sequence
on the hot path, and a `Handled` return value contaminating all ten `Dispatch` methods and every dispatch arm.

**Fix:** cut `Passthrough`, the echo buffer and `Handled` from v1; count unhandled sequences in `FeedStats` and log
at `debug`. Re-introduce it in `US-0081` **only if** a named consumer exists (SSH→ConPTY bridging is the only
candidate and is not in this intake). If it stays, state the consumer and the rule in `events-and-api.md`.
**Effort:** S (and a net simplification)

### R-36 · Major · `?9001` win32-input-mode must be swallowed, not re-emitted

**Location:** `dispatch-and-modes.md:182-186` (out of scope, "each re-emitting rather than vanishing").

Conhost sends `ESC [ ? 9001 h` unprompted at `VtIo::StartIfNeeded` and re-injects it after any DECRST
(`prior-art.md` § 5.2, § 5.4). Under the design it becomes an unknown private mode: `DECRQM NotSupported` plus a
`Passthrough` event on **every local Windows session, repeatedly**. The current engine silently drops it.

**Fix:** add `?9001` to the mode table as recognised-but-inert with a `real` DECRQM answer of `Reset` (the honest
answer: we do not implement the encoding), and never re-emit it. `prior-art.md` § 10 risk 11 is explicit that
half-adopting it corrupts F3, so "recognised and off" is the correct v1 state.
**Effort:** S

### R-37 · Major · The reply latency budget is not stated, and the demand/yield handshake can breach it

**Location:** `dispatch-and-modes.md:209-214` (conhost blocks up to 1 s for DA1) versus
`damage-and-render-state.md:139-142` (the pump drains the batch, then "if `demand.take()`, park briefly … or a 1 ms
timed wait"), and `events-and-api.md:110-113`.

Replies are values in the batch, drained after the lock is released. Nothing says the reply drain happens **before**
any yield, nor whether the colour-query answers keep today's one-batch deferral (`crates/terminal/src/backend/pump.rs:105-128`,
which the design preserves at `high-level-design.md:340`). Under sustained output with a raised demand flag, the DA1
answer could be delayed past conhost's window at session start — the worst possible moment, because it is the moment
the first client connects.

**Fix:** state in `damage-and-render-state.md` that `Reply` bytes are written to the transport before the yield
check, and add a startup-latency assertion to the adapter tests.
**Effort:** S

### R-38 · Major · The ConPTY glyph-width flag cannot follow mode 2027, so enabling 2027 makes ConPTY worse

**Location:** `cell-and-style.md:178-191`, `pty.md:154-161`.

The flag is chosen at `CreatePseudoConsole` time; mode 2027 is set by the program afterwards. The design records this
as a "known limitation" but not its consequence: a program that enables 2027 makes the engine measure clusters while
conhost keeps measuring `wcswidth`, which is exactly the column-drift failure that `prior-art.md` § 10 risk 3 names
as the most user-visible class of terminal bug — and it only occurs *because* 2027 was implemented.

**Fix:** either make 2027 a configuration choice applied at spawn (and have the engine refuse `CSI ? 2027 h` on a
session whose transport was spawned with `WcsWidth`), or defer mode 2027 entirely (see R-56). State the chosen
rule in both files.
**Effort:** S

### R-39 · Minor · The `measure_rows` fixtures need their host version recorded

`reflow-and-resize.md:260` ports three measured BUG-0051 numbers as fixtures. Those numbers are a property of a
specific `OpenConsole.exe` build (`DEC-0013` / IN-0030 owns the bundled pair and its version bump script). If the
bundle is bumped, the fixtures may no longer describe the shipping host.
**Fix:** record the host version alongside each fixture and add a note to IN-0030's bump checklist.
**Effort:** S

### R-40 · Minor · `cell_pixels` has two owners

`pty.md:45` (`WindowSize { …, cell_width, cell_height }`) and `events-and-api.md:140` (`Config::cell_pixels`, for
`CSI 14 t`). Two copies of the same fact, updated from different places.
**Effort:** S

---

## Dimension 5 — Migration risk

### R-41 · Blocker · The `vte` differential oracle is unobtainable while the fork is patched

**Location:** `parser.md:336-337` and `testing-and-bench.md:107-110` ("`vte 0.15` as a dev-dependency — the
unmodified crates.io release, not the fork") against `Cargo.toml:243-244`:

```toml
[patch.crates-io]
vte = { path = "vendor/vte" }
```

`[patch]` applies to the whole workspace and to every dependency kind, including dev-dependencies. A
`vte = "0.15"` dev-dependency of `oneterm-vt` resolves to the **patched** vendored copy for the entire window
`US-0073` → `US-0082` — which is exactly the window in which the differential runner is the primary proof.
`US-0073`'s exit criterion (`high-level-design.md:394`) depends on it.

**Fix:** pick one and write it into `parser.md`: (a) vendor a second, pristine `vte` under a different package name
(`vte-oracle`) with its own notice row; (b) put the differential test in a separate crate with its own workspace,
outside the patch; (c) accept the patched `vte` as the oracle and enumerate the patch's effects as differential
exclusions (`vte/0001` adds `report_osc`, `vte/0002` adds DCS forwarding — both are *additive* action-trace changes,
so this may in fact be the cheapest option). Do not leave it implicit.
**Effort:** M

### R-42 · Blocker · `US-0079` is not an independently verifiable outcome

**Location:** `migration.md:66-88`, `IN-0029.md` § "Candidate Work Packets", `high-level-design.md:400`.

`US-0079` rewrites `crates/terminal` (`model.rs`, `content.rs`, `search.rs`, `palette.rs`, `color_classification.rs`,
`osc_color.rs`, `mouse_encode.rs`, `logging.rs`, `session.rs`, `test_support.rs` and the whole `backend/` module),
`crates/ssh`, `crates/local-shell` and `crates/terminal-view` (`render/frame.rs`, `theme/palette.rs`,
`input/mouse.rs`, `plan_cache`); deletes eleven call-site constructs; rewrites ~25 backend tests, 11 search tests,
5 content tests, 5 frame tests and 662 lines of `test_support.rs`; and is accepted by three GUI walks plus the
differential runner. `docs/HARNESS.md` requires "one coherent, independently verifiable outcome" per packet.
"Rollback is `git revert` of one commit" is a statement that the packet is the whole migration.

**Fix:** split, with a named compatibility shim so each slice compiles and tests on its own:

1. `TerminalContent` reshaped over `RenderState` + the mode snapshot (R-17), old engine still underneath via a shim.
2. Pump and events: batch drain, delete the deferred/reliable tier and the CORR-01 cases.
3. Coordinates at the seam: `RowId` for search, gutter, marks, graphics anchors; delete `LineAccounting`.
4. `crates/terminal-view`: `render/frame.rs`, `plan_cache`, `input/mouse.rs`, `theme/palette.rs`.
5. Backend type swap: `crates/ssh`, `crates/local-shell`.

State in `migration.md` which of these can run behind the old engine and which require the new one; that is the
question that decides whether the split is real or cosmetic.
**Effort:** L

### R-43 · Major · The differential comparator cannot see the divergences it exists to catch

**Location:** `testing-and-bench.md:72-87` (the snapshot format) and `:112-117` (`vt-diff` "renders both to the
snapshot format").

The snapshot is rows of text with **trailing blanks trimmed**, plus cursor, non-default modes and a style run list
"for rows whose style is non-default". It therefore cannot see: BCE backgrounds on erased trailing cells (which is
what `vim_24bitcolors_bce` exists to pin), per-cell attributes in general, wrap flags, `occ`-driven scrollback
occupancy differences, or viewport position (the old engine has `display_offset`, the new has `RowId`).

**Fix:** make the snapshot cell-exact: per cell, content + width class + resolved style key; per row, the wrap flag;
per grid, the row count and the viewport offset expressed as distance-from-newest so both engines can produce it.
Do not trim trailing cells. Then re-check the size claim (it grows, but still far below 46 MB, and only the
recordings are committed).
**Effort:** M

### R-44 · Major · The ConPTY resize contract is moved and rewritten by the packet before the one that proves it

**Location:** `migration.md:133` (the ten `model.rs:616-910` tests "**Move** into `oneterm-vt`" at `US-0077`) and
`IN-0029.md` § "Acceptance".

Those ten tests are the only written form of the `KeepViewportTop` contract. Translating them into a different
coordinate model, in the packet that also implements that model, removes the independent check. Only the three
`measure_rows` fixtures (`reflow-and-resize.md:260`) carry a number that cannot be re-derived from the new code.

**Fix:** keep `crates/terminal/src/model.rs`'s suite running against the **old** engine until `US-0079`, and require
both suites green at `US-0079`. Cheap: the fork is still vendored until `US-0082`.
**Effort:** M

### R-45 · Minor · The seam adapter is on the critical path and is not designed

`migration.md:202-204` — "`crates/completion`, the gutter, search and the agent panel still speak display rows after
`US-0079`; the adapter translates at the seam". That translation is `RowId ± viewport_top`, the exact arithmetic
`DEC-0015` deletes, and it appears in no interface list.
**Effort:** S

---

## Dimension 6 — Harness and process compliance

Checked: R1–R12, `docs/agents/dependencies.md` § 1 and § 3, `docs/agents/error-policy.md`,
`docs/templates/{spec-intake,design,detail-design,decision,work}.md`, English-only, doc paths.

Compliant: the intake, the HLD and all eleven LLDs follow their templates' section structure; `DEC-0014` and
`DEC-0015` follow `decision.md` exactly; no work packets exist yet (correct — the intake is the gate); the HLD's
`UI Wireframe` is marked `N/A` with a reason; the high-risk lane's detail-design precondition is satisfied; all text
is English; no marker blocks are required in these document types and none are malformed.

### R-46 · Major · Rule and graph documents are scheduled later than the crates they govern

**Location:** `migration.md:150-153` — `structure.md` at `US-0071`/`US-0073`, `crate-dependency-rules.md` at
`US-0073`, `dependencies.md` at `US-0082`.

`oneterm-pty` lands at `US-0071`, but R6/R7/R8 still name `alacritty_terminal` until `US-0073`, and the six (or
seven, R-23) new direct declarations are not recorded in `dependencies.md` § 3 until `US-0082` — nine packets after
they are added. `scripts/verify-dependency-graph.py` carries a manifest allow-list that must be edited in the same
commit as each new crate or CI fails, which makes the schedule impossible anyway.

**Fix:** move each doc edit into the packet that changes the code it describes: `structure.md` + `verify-dependency-graph.py`
+ `dependencies.md` § 3 rows at `US-0071` and again at `US-0073`; `crate-dependency-rules.md` R6/R7/R8 wording at
`US-0073`; only the *removal* rows at `US-0082`.
**Effort:** S

### R-47 · Major · `cargo-fuzz` is a new dependency category, a nightly requirement, and not runnable as described

**Location:** `testing-and-bench.md:132-146`, `parser.md:348-351`, `high-level-design.md:394`
("`cargo fuzz run parser -- -rss_limit_mb=512` survives 10 minutes" as a `US-0073` **exit criterion**).

`docs/agents/dependencies.md` § 3 requires a design decision before a new dependency category. `cargo-fuzz` brings
`libfuzzer-sys` + `arbitrary` in a nested `fuzz/` crate, requires a nightly toolchain against a pinned
`rust-toolchain.toml`, and libFuzzer is not usable on `x86_64-pc-windows-msvc` — the project's primary target and
the only one with QA (`docs/PROJECT.md`, Platforms).

**Fix:** state the fuzzing host (Linux CI or WSL), record the toolchain exception, and remove the fuzz run from
`US-0073`'s exit criteria (keep it as a scheduled activity, which `testing-and-bench.md:145` already says). Same
treatment for the "separate Windows bench job" (`testing-and-bench.md:200-202`), which does not exist in
`.github/workflows/ci.yml` and has no owning packet.
**Effort:** S

### R-48 · Minor · Version pinning for the new direct declarations

`deny.toml:88` sets `multiple-versions = "warn"`; `Cargo.lock` already resolves two `bitflags` and two `rustc-hash`
versions. The new declarations must name the 2.x line explicitly so the graph does not gain a third.
**Effort:** S

### R-49 · Minor · Module layout conflicts with `code-style.md`

`docs/agents/code-style.md` § "Module organization": "Do not use `mod.rs` unless the module naturally contains
multiple related files" and "Avoid creating folders that contain only a single source file". The LLDs specify
`damage/mod.rs`, `render/mod.rs`, `reflow/mod.rs`, `graphics/mod.rs` and `cell/mod.rs` for what are described as
single-concern modules.
**Fix:** flat files (`damage.rs`, `render.rs`, `reflow.rs`) unless the file actually splits.
**Effort:** S

### R-50 · Minor · Template drift in the HLD and intake

The HLD's data-flow section is titled "Data flow, byte to pixel" where `docs/templates/design.md` says "Data Flow";
`IN-0029.md` adds two sections not in `spec-intake.md` ("Acceptance (intake level)", "Design summary and open
decisions for the owner"). Both are improvements in substance; note them so a generator does not clobber them.
**Effort:** S

---

## Dimension 7 — Over-engineering (and under-scoping)

### R-51 · Major · Dual-form rows are the largest correctness surface in the design and buy nothing measured

**Location:** `grid-and-scrollback.md:83-95`, `high-level-design.md:276-277`, `:305-307`.

Every grid operation must handle two representations, and the operations that matter — `ICH`, `DCH`, `ECH`,
`CUP`-into-the-middle, wide-pair repair, insert mode, graphic stamping — all coerce to `General` anyway
(`:91-92`). The `Uniform` form therefore survives only for append-only ASCII rows, where the win is memory, not
time. But the memory win the HLD actually claims (`:292-296`) comes from `Option<Row>` lazy allocation, which is
independent. And the intake forbids a throughput outcome. Meanwhile the recompression pass
(`grid::tests::rows_recompress_on_leaving_the_viewport`) is another mutation path that must interact correctly with
damage stamping (R-22), the interning sweep and reflow.

**Fix:** v1 = lazily allocated `Vec<Cell>` rows behind the `RowRef`/`RowMut` API. Add `Uniform` in a later packet,
gated on the tier-5 RSS measurement that `US-0072` will produce. `prior-art.md` § 9.2 recommends dual-form for a
design at Ghostty's scale; nothing in `perf-baseline.md` asks for it here.
**Effort:** S to remove; L to keep and get right

### R-52 · Major · The style sweep is a new silent-corruption class for an unreachable condition

**Location:** `cell-and-style.md:95-108`, `high-level-design.md:280-281`, `:382` (the design's own added risk 16).

Ghostty's telemetry says real workloads use fewer than 16 distinct styles; the ladder triggers at 49 152. Step 4
(fall back to id 0) is already safe, bounded and observationally degraded-but-correct. The sweep is a full mutating
walk of both grids that renumbers every cell — the mechanism behind R-14's stale-id hazard and behind the design's
own risk 16 ("a missed live reference corrupts colours or text silently").

**Fix:** keep the sweep for the **grapheme arena** only, where unbounded growth is attacker-reachable and real
(`prior-art.md` § 10 risk 6). For styles, keep steps 1, 2 and 4 and delete step 3. This also removes half of R-14
and half of R-20.
**Effort:** S

### R-53 · Major · Ten "nearly free correctness" deviations land in the packet whose exit criterion is parity

**Location:** `dispatch-and-modes.md:336-353` (D3, D5, D6, D7, D8, D10, D11, D12, D14, D15) and
`grid-and-scrollback.md:243-251` (G4, G5), all inside `US-0076`, whose exit is "**the 45-recording parity gate is
green**".

Each row's "Recording risk: none" is an **assertion, not a check** — nobody has grepped the recordings. `? 47` /
`? 1047` / `? 1048` (G4/D3) is the risky one: implementing sequences the reference silently ignores changes the grid
for any recording that sends them, and `wrapline_alt_toggle`, `alt_reset` and `saved_cursor_alt` are exactly the
recordings that exercise alt-screen toggling.

**Fix:** (a) as part of `US-0072`, grep the 45 recordings for every sequence each deviation touches and record the
result in the deviation table's "Recording risk" column — cheap and removes the guess; (b) move every deviation that
is not required for the product to `US-0081`, after the gate is green. A rewrite that changes behaviour and breaks
the gate in the same packet cannot tell the two apart, which is the intake's own stated principle
(`IN-0029.md` § "Design summary", item 2).
**Effort:** S

### R-54 · Minor · APIs with no named consumer

`VtEvent::ModeChanged` + `Config::mode_watch: ModeSet` (`events-and-api.md:51`, `:137`) — no consumer named anywhere.
The APC streaming sink that discards (`parser.md:148-149`). `Terminal::changed_rows()` + the multi-watermark
`damage` module (`damage-and-render-state.md:193-204`) — its named consumers (search index, semantic highlighter,
session logging) are not watermark consumers today and none is scoped in this intake. Keep `SeqNo` per row (free,
and the whole point) and add the second-consumer API when a second consumer exists.
**Effort:** S

### R-55 · Minor · Caps that are arbitrary or mutually inconsistent

`OSC_LARGE = 8 MiB` with a hard-coded spill allow-list `{8, 52, 99, 1337}` (`parser.md:106-124`) duplicates a policy
that `crates/terminal/src/security_policy.rs` already owns for OSC 52; `DCS_MAX_BYTES = 16 MiB`
(`parser.md:143`) versus a Sixel pixel buffer bounded at `4096*4096*4 = 64 MiB` (`graphics.md:100-103`) — the byte
cap makes the pixel cap unreachable, which is fine but should be stated rather than left as two independent numbers.
**Effort:** S

### R-56 · Minor · Mode 2027 is built in v1 although nothing asks for it and ConPTY cannot follow it

`cell-and-style.md:169-191`, `dispatch-and-modes.md:177`. The research asks only that the **storage** decision
(intern + cap) be made early (`prior-art.md` § 10 risk 3). The design goes further: a `GraphemeCursor` print path, a
cross-chunk pending-cluster buffer, a cluster-width function and a ConPTY flag axis that cannot actually follow the
mode (R-38).
**Fix:** implement the grapheme arena and `cluster_width()`; leave `? 2027` unimplemented with DECRQM
`NotSupported` until a packet asks for it. Removes R-38 entirely.
**Effort:** S

---

## Dimension 8 — Test strategy

**Can the 45 recordings be the parity gate, given § 7 says they skip cursor, modes and palette?** Partly, and only
after two fixes. They are the right *inputs* — real tmux/vim/zsh captures — but the design's comparison format is
weaker than upstream's on the axis the recordings were built for, and the blessing procedure is ambiguous.

### R-57 · Blocker · The parity snapshot compares less than upstream on exactly the axis the recordings pin

**Location:** `testing-and-bench.md:72-87` versus `engine-semantics.md` § 7.3 and `IN-0029.md` § "Acceptance".

The design's snapshot is text rows with **trailing blanks trimmed**, cursor, non-default modes and "styles: run list
per row for rows whose style is non-default". Upstream's `Grid::eq` compares every cell's `c`, `fg`, `bg`, `flags`
(including `WRAPLINE`) and `extra`, plus `columns`, `lines` and `display_offset`. The claim at `:82` that this
"deliberately compares **more** than the upstream harness" is true only for cursor and modes; it compares
**materially less** for per-cell attributes, wrap flags and erased-region backgrounds.

Trimming trailing blanks specifically destroys what `vim_24bitcolors_bce` exists to pin. There is no wrap-flag field
at all, although the intake's own acceptance clause requires "grid content, **attributes**, **wrap flags**, row count
and **display offset**" — so the LLD contradicts the intake acceptance.

**Fix:** cell-exact snapshot (see R-43); keep trailing cells; add a per-row wrap flag and a viewport offset line.
Then the claim at `:82` becomes true.
**Effort:** M

### R-58 · Blocker · Who blesses the expectations is stated two ways

**Location:** `testing-and-bench.md:64-67` ("Expectations are generated by **our own engine** and reviewed once")
versus `high-level-design.md:393` / `IN-0029.md` § "First Action" (`US-0072`: "all 45 recordings … replaying green
against the **current** engine through the new runner").

If the new engine blesses, the gate proves self-consistency and nothing else — and the rewrite's single cheapest
safety net evaporates.

**Fix:** state unambiguously in `testing-and-bench.md`: expectations are generated **by the old engine at `US-0072`**,
committed, and frozen; `vt-corpus bless` refuses to touch `corpus/alacritty-ref/` after `US-0072` without an explicit
`--deviation <row-id>` argument that names a deviation-table row.
**Effort:** S

### R-59 · Major · The corpus gaps the research names are not filled

**Location:** `engine-semantics.md` § 7.3 (the corpus never checks cursor, `input_needs_wrap`, modes, colour
overrides, title, tab stops, scroll region) and `testing-and-bench.md:100-103` ("our own recordings": Sixel,
OSC 9;7, OSC 133, a ConPTY resize capture, CJK/emoji, a captured session).

Adding cursor and modes to the *snapshot* does not add corpus *inputs* for the palette (OSC 4/10/11/12/104/110-112),
tab stops, the scroll region, hyperlink id matching (OSC 8 with and without `id=`), selection, search, or the
keyboard/mouse mode reports. Those are exactly the surfaces with zero corpus coverage and the most new code.

**Fix:** add six named recordings to `testing-and-bench.md`'s own list: a palette/OSC-colour stream, a tab-stop and
scroll-region stream (`vttest` menus 1 and 3 captured once), an OSC 8 hyperlink stream with shared ids, a
DECRQM/DA/XTVERSION query stream, a mouse-mode transition stream, and a wide-char/wrap boundary stream.
**Effort:** S–M

### R-60 · Major · The bench and fuzz story does not run where the project runs — see R-47

Restated here for the dimension: `cargo fuzz` is not usable on `x86_64-pc-windows-msvc`; the "separate Windows bench
job" has no owning packet and does not exist in `.github/workflows/ci.yml`; tier 5 (RSS) is specified but no method
is given for measuring RSS portably (`grid::tests::unwritten_slots_read_as_blanks_and_cost_nothing` is an "RSS
assertion" inside a unit test, which is not reliable under a test harness running other tests in parallel).
**Fix:** re-home fuzzing to Linux CI, give the bench job an owning packet, and make the RSS check a `vt-bench rss`
report rather than a `#[test]`.
**Effort:** S

### R-61 · Minor · `cross-check` reads a machine-specific path

`testing-and-bench.md:55`, `:93-97` — `C:\Users\trunglt\.cargo\git\checkouts\alacritty-…\tests\ref`. That path is
one developer's machine and disappears when the dependency is removed at `US-0082`.
**Fix:** copy the `grid.json` set into the scratch area once during `US-0072`, record its SHA-256 in the packet, and
point `cross-check` at a `--grid-json <dir>` the packet documents.
**Effort:** S

### R-62 · Minor · `terminal_from_text` marks every `\n` row as `WRAPPED`

`events-and-api.md:166-170` — "reproduces `mock_term`'s contract exactly: `\n` breaks the line **and** marks the
previous row `WRAPPED`". That is alacritty's mock convention, and it is fine to keep for the ported tests — but the
design has just made `WRAPPED` a row flag that is reflow's primary input (G1), so every fixture built this way is
one logical line and will be rejoined by any reflow test that uses it.
**Fix:** note it, and give the reflow tests a separate builder that takes explicit wrap flags.
**Effort:** S

### R-63 · Minor · The benchmark fixture geometry does not match the resize fixture

`testing-and-bench.md:167-186` fixes all fixtures at 160×45 "matching `pty-throughput`", but tier 4 resizes "a filled
80x24 grid with 100 000 rows of scrollback … to 100x40". Two geometries, one comparability claim.
**Effort:** S

---

## Dimension 9 — Missing pieces: the checklist

Each item from the brief, with where it lives (or does not):

| Item | Home | Verdict |
| --- | --- | --- |
| Search over scrollback | `high-level-design.md:343`, `migration.md:136` | Partial — R-19 |
| Selection semantics (semantic chars, block, lines, rotation) | none | **Missing — R-18 (blocker)** |
| Hyperlink OSC 8 id matching | `cell-and-style.md:127-131`, `dispatch-and-modes.md:259` | Covered, but per-grid ids + resolve-outside-lock breaks the view's identity hash — R-14 |
| OSC 52 policy hooks (`security_policy.rs`) | `dispatch-and-modes.md:262`, `IN-0029.md` § "Architecture" | Covered; overlap with `OSC_LARGE` — R-55 |
| `url.rs` / `url_policy.rs` | none | **Missing from the migration file list — R-19** |
| `logging.rs` escape stripper | `parser.md:274-277` (`strip.rs`), `migration.md:103` | Covered, good |
| Session restore | n/a | Not a OneTerm feature |
| Agent OSC 9;7 | `dispatch-and-modes.md:284-290` | Covered; the ConEmu sub-code collision is correctly deferred |
| Bell | `dispatch-and-modes.md:37`, `events-and-api.md:41` | Covered |
| Title stack | `dispatch-and-modes.md:294-298` (D14) | Covered |
| Clipboard limits | `high-level-design.md:283`, `dispatch-and-modes.md:262` | Covered |
| `TermMode` consumers in the view (mouse encoding) | `dispatch-and-modes.md:149-152` (engine side only) | **Missing on the view side — R-17** |
| Key encoding modes (kitty stack, modifyOtherKeys) | `dispatch-and-modes.md:300-316` | State covered; **`DECKPAM`/`DECKPNM` has no `Mode` entry — R-64** |
| Focus events (`? 1004`) | `dispatch-and-modes.md:166` | Covered |
| Bracketed paste (`? 2004`) | `dispatch-and-modes.md:175` | Covered |
| Alternate scroll (`? 1007`) | `dispatch-and-modes.md:169` | Covered engine-side; view needs it through R-17 |

### R-64 · Minor · `DECKPAM` / `DECKPNM` has no entry in the mode table

`dispatch-and-modes.md:63` handles `ESC =` / `ESC >` and calls the state `DECKPAM`, but `Mode` (`:154-180`) has no
`AppKeypad`, so `Terminal::mode()` cannot report it and `crates/terminal/src/key_encode.rs` (which encodes from
DECCKM and the keypad state) has no source.
**Fix:** one row in the mode table with `real` DECRQM and a `Mode::AppKeypad` variant.
**Effort:** S

---

## Verdict

**The design is not ready for packet creation.** It is unusually thorough — the trap map is complete, the deviation
tables are honest, the phase plan's exit criteria are commands rather than judgements, and the Windows specifics in
`pty.md` are correct. But four of its load-bearing claims do not hold as written, and two of them are the claims the
whole intake was created to establish.

The three that matter most:

1. **The coordinate model is incomplete** (R-01, R-02). `DEC-0015`'s stable-row-identity contract is contradicted by
   the design's own storage choice for every scroll region, `IL`, `DL` and `SD` — i.e. for tmux and vim. This must be
   settled in the decision record, not in a packet.
2. **The render hand-off does not close** (R-14, R-15). `resolve()` cannot resolve interned ids outside the lock,
   and `rows()` is defined as two incompatible things in one file. `US-0078` cannot be written from this text.
3. **The parity gate does not gate parity** (R-57, R-58). The snapshot format compares less than upstream on cell
   attributes and wrap flags, and the blessing procedure is stated two ways — one of which makes the gate
   self-referential.

`US-0071` (`oneterm-pty`) and `US-0072` (harness and baseline) are the exception: they are well-specified,
independent of everything above, and should be created now — with R-47 (fuzz/bench hosting) and R-46 (doc and graph
edits move into the packet that adds the crate) folded into their scope, and R-53's "grep the recordings for every
deviation" and R-58's "bless from the old engine" added to `US-0072`'s acceptance.

### Minimum set that must be fixed before any engine packet (`US-0073` onward) is created

| # | Finding | Owning document |
| --- | --- | --- |
| 1 | R-01 viewport anchoring / `display_offset` equivalence | `grid-and-scrollback.md` |
| 2 | R-02 what `RowId` actually names, and how in-region motion is reported | `DEC-0015`, `grid-and-scrollback.md`, `graphics.md` |
| 3 | R-14 how `resolve` gets the style table, and sweep-versus-watermark ordering | `damage-and-render-state.md`, `cell-and-style.md` |
| 4 | R-15 `RenderState::rows()` — full viewport plus a changed list | `damage-and-render-state.md` |
| 5 | R-18 selection: an LLD and an owning packet | new `low-level-design/selection.md`, `IN-0029.md` |
| 6 | R-21 stop interning per-cell-unique graphic references | `cell-and-style.md`, `graphics.md` |
| 7 | R-41 how the unmodified `vte` oracle is obtained under `[patch]` | `parser.md` |
| 8 | R-42 split `US-0079` into verifiable slices with a named shim | `migration.md`, `IN-0029.md` |
| 9 | R-57 cell-exact parity snapshot (content, attributes, wrap flags, offset) | `testing-and-bench.md` |
| 10 | R-58 expectations are blessed by the **old** engine at `US-0072` and frozen | `testing-and-bench.md` |

Strongly recommended in the same pass, because each is cheap and each removes a whole class of later rework:
**R-16** (one graphics drain owner), **R-17** (mode snapshot in `RenderState`), **R-22** (graphic release on row
reset), **R-05** (`rows_produced()` versus the gutter), **R-35** (cut `Passthrough` from v1), **R-51** (defer
dual-form rows), **R-52** (delete the style sweep), **R-53** (move the ten free-correctness deviations out of the
parity packet), **R-28** (`assert_integrity` budget).

The design's own stated principle — "a rewrite that improves behaviour and breaks the gate cannot tell the two
apart" (`IN-0029.md` § "Design summary", item 2) — is the right one, and four of the recommendations above are
simply that principle applied to places where the design did not apply it to itself.

---

# Pass 2 — verification of the rework

Date: 2026-09-12. Re-read: `IN-0029.md` (including the Review resolution table),
`high-level-design.md`, `DEC-0014`, `DEC-0015`, and all twelve files under `low-level-design/`
including the new `selection.md`. Each pass-1 finding was checked **against the design text**, not
against the resolution table. Packets re-planned to `US-0071`..`US-0087`.

Bookkeeping correction: pass 1's summary table said 10 / 27 / 21. The named findings are
10 blockers, **26** majors and **28** minors (R-01..R-64, gap-free). The named list is
authoritative; no finding was lost, the summary arithmetic was wrong.

## Status of every pass-1 finding

| R | Status | Reason where not `resolved` |
| --- | --- | --- |
| R-01 | resolved | `grid-and-scrollback.md` "Viewport anchoring": `offset` from the bottom, sticky at 0, with a 13-row table covering every row-moving operation; `DEC-0015` decision 1 restated |
| R-02 | resolved | `RowId` = position, stated in `DEC-0015` **and** in a per-operation table; `Anchors` list plus `VtEvent::RowsScrolled`; `graphics.md:59-63` retracts the old claim — but see N-01 and N-06 |
| R-03 | resolved | all four `scroll_up` cases tabulated with ids, offset and anchor effects |
| R-04 | resolved | one terminal-wide counter, disjoint runs asserted, `swap_alt` copies a row **index** |
| R-05 | resolved | separate `lines_produced` counter, incremented on line feeds only — stale HLD row at N-07 |
| R-06 | resolved | `AnchorKind::ViewportTop`; the test becomes `first_visible_character_survives_a_resize` — but see N-01 |
| R-07 | resolved | table row deleted, the procedure is the design, "PRIMARY screen always" is a stated precondition |
| R-08 | resolved | trap 1 gated on `Mode::ReverseWrap`, default reset, D12 deferred |
| R-09 | resolved | G3's observable scope (`EL 0`, `HT`) stated, with two tests |
| R-10 | resolved | `occ` is an over-approximation, excluded from `assert_integrity`; `memset` only for the default template |
| R-11 | resolved | `now: Instant` is a parameter of `feed` and `render_update` |
| R-12 | resolved | `text_char()` plus the tab-cell rule across `row_text`, search, the log and both predicates |
| R-13 | resolved | a cell carrying a `GraphicId` is not erasable; `ed2_over_an_image_keeps_the_image_rows` |
| R-14 | resolved | phase 1 copies `StyleRun`s of resolved `Style` values and resolved hyperlink strings under the lock; phase 2 is `map_colors` only; `DEC-0015` decision 2 rewritten to match |
| R-15 | resolved | `rows` (always the full viewport) **and** `changed` (indices), in the LLD, the HLD API sketch and `DEC-0015` |
| R-16 | resolved | `Terminal::take_graphics` is the only drain, stated in three files — but `graphics.md` still contradicts it once, N-05 |
| R-17 | resolved | `ModeSnapshot` in `RenderState`, refreshed even on `Unchanged`, consumer-map row added |
| R-18 | resolved | `low-level-design/selection.md` is substantive — four kinds, side rules, semantic escape chars, block extraction, a full invalidation matrix, `hit_test` — and owns packet `US-0078` |
| R-19 | resolved | `GridText` stays, rebuilt adapter-side; `url.rs`, `url_policy.rs`, `color_classification.rs` added to the swap list |
| R-20 | resolved | `Interner` moved to `Terminal`; the borrow-check problem is named explicitly |
| R-21 | resolved | the cell stores only `GraphicId`; a `Placement` table holds geometry; the painter derives the offset; `one_extras_entry_per_image` |
| R-22 | resolved | liveness derived from `RowFlags::HAS_GRAPHIC` per row, swept at end of `feed`, with tests for `CSI 2 J`, row reset, scroll blank, trim and reflow |
| R-23 | resolved | one dependency table in the HLD: crate, pin, declaring crate, purpose |
| R-24 | resolved | one `Watermark`, owned by `RenderState`; `&mut self` justified |
| R-25 | resolved | "twelve" everywhere, including `pty.md` |
| R-26 | resolved | all four `terminal-view` files named; the narrow `frame.rs` claim kept |
| R-27 | resolved | absolute constants `GRAPHEME_SWEEP_ENTRIES` / `_CHARS` |
| R-28 | resolved | three-level integrity budget (O(1) per method, full walk per `feed`/`resize`/`render_update`, exhaustive behind `vt-paranoid`) plus a 60 s debug-suite budget measured at `US-0075` |
| R-29 | **partial (declared)** | accepted — see the judgement below |
| R-30 | resolved | ring length fixed for the session from `scrollback_limit + MAX_ROWS`; only a scrollback-limit change rehomes |
| R-31 | resolved | public `RowRemap` and the tracking-point slice deleted; the remap is an internal closure |
| R-32 | resolved | `events-and-api.md` "The `Terminal` struct and the field split" |
| R-33 | resolved | associated `Reader` / `Writer` types; `polling` recorded as a public dependency |
| R-34 | resolved | `has_extras` removed; five reserved bits; the stale comment gone |
| R-35 | resolved | `Passthrough`, the echo buffer and `Handled` cut; `FeedStats::unhandled_sequences` — one stale sentence remains, N-09 |
| R-36 | resolved | `? 9001` recognised and inert, `Reset` DECRQM, never counted as unhandled |
| R-37 | resolved | the pump order is the contract: replies written before any yield; `da1_is_answered_within_the_startup_budget` |
| R-38 | resolved | removed at the root by deferring mode 2027; `oneterm-pty` spawns `WcsWidth` unconditionally and the engine refuses `? 2027 h` on such a session |
| R-39 | resolved | each `measure_rows` fixture records its `OpenConsole.exe` version; IN-0030 gains a re-capture line |
| R-40 | **partial** | `Terminal::set_cell_pixels` is in the HLD API and `pty.md`, but `dispatch-and-modes.md:203` and `:406` still carry `Config::cell_pixels` — N-08 |
| R-41 | resolved | **independently verified**: `grep '^+++' vendor/patches/vte/*.patch` returns only `src/ansi.rs` for both patches, so `vte::Parser` + `Perform` are pristine under `[patch]`; the design adds a test that re-runs the check |
| R-42 | **partial** | the split is real (`LegacySnapshot` shim, per-consumer packets, a "runs behind the old engine?" table) but the `US-0081` / `US-0083` / `US-0084` boundary does not hold — N-04 |
| R-43 | resolved | `vt-diff` compares in the cell-exact `grid.expect` form |
| R-44 | resolved | the ten `model.rs` resize tests run against the old engine until `US-0082`; both suites green in the same commit |
| R-45 | resolved | `LegacySnapshot::display_row` / `row_id`, its lifetime and its consumers named |
| R-46 | resolved | every doc, allow-list and CI edit moved into the packet that adds the code |
| R-47 | resolved | fuzzing Linux-only, scheduled, never a packet gate; nightly exception recorded |
| R-48 | resolved | `bitflags` and `rustc-hash` pinned to 2.x in the dependency table |
| R-49 | resolved | flat engine modules, `parser/` the one folder with a stated reason — `graphics/` is a second folder but holds two files, N-14 |
| R-50 | resolved | template notes in both the HLD and the intake |
| R-51 | **partial (declared)** | accepted — see the judgement below |
| R-52 | resolved | the style sweep is deleted; ids never move; `DEC-0015` and the HLD memory table follow |
| R-53 | **partial (declared, understated)** | D13 is defensible; D11 is a second exception that is not declared and is not harmless — N-03 |
| R-54 | resolved | `ModeChanged`, `mode_watch`, `changed_rows` and the `damage` module deleted; the per-row `SeqNo` kept |
| R-55 | resolved | the spill list comes from `osc_claims`; `OSC_LARGE` documented as a memory ceiling only |
| R-56 | resolved | mode 2027 recognised and inert; the arena, the cap and `cluster_width()` still ship |
| R-57 | resolved | `grid.expect` is cell-exact, run-length, nothing trimmed, with the wrap flag, row count and viewport position; `state.expect` adds cursor, modes, palette, title, tab stops |
| R-58 | resolved | both files blessed by the **old** engine at `US-0072` and frozen; `bless` refuses without `--deviation <row-id>` |
| R-59 | resolved | six new recordings named, each mapped to the surface it covers |
| R-60 | resolved | fuzzing on Linux; `US-0072` owns creating the Windows bench job; tier 5 is a report, not a `#[test]` |
| R-61 | resolved | `--grid-json <dir>`, copied to a scratch directory with a recorded SHA-256 |
| R-62 | resolved | `terminal_from_rows(&[(text, wrapped)])` for reflow fixtures |
| R-63 | resolved | tier 4's separate geometry is deliberate and compared only against the other engine |
| R-64 | resolved | `Mode::AppKeypad` in the mode table and in `ModeSnapshot` |

**Totals: 59 resolved, 5 partial (R-29, R-40, R-42, R-51, R-53), 0 open, 0 regressed.**
Of the ten blockers, nine are fully resolved and one (R-42) is partial on a scoping boundary.

## Judgement on the three declared partial declines

**R-29 — no lazy history reflow. Accept.** The part that mattered is done: the microsecond target
is deleted from the phase plan, `reflow-and-resize.md` "Cost model, not a target" states
O(live rows x cols) explicitly, and the bench records three scrollback depths as a ratio against
the old engine. A second, lazy reflow path is a separate implementation with its own invalidation
rules, and the drag-resize cost it would buy has never been measured here — building it now would
be the same unfalsifiable performance work the intake forbids. The recorded ratio at 100 000 rows
is exactly the evidence that would justify the later packet.

**R-51 — dual-form rows deferred, not abandoned. Accept, and the shape is better than asked for.**
`US-0075` ships one `Vec<Cell>` representation behind `RowRef` / `RowMut`, recorded as deviation
G7, with the re-add gated on the tier-5 RSS numbers `US-0072` produces. That is a decision with a
named trigger rather than a preference, and the API boundary keeps the change internal. The
prior-art argument for dual-form storage is about engines at Ghostty's scale; deferring it removes
the largest correctness surface in the grid from the packet that must clear the parity gate.

**R-53 — D13 stays in `US-0076`. Accept for D13; the decline is understated.** D13 (the DA1
answer) is genuinely safe: DA answers are discarded by the ref harness, so it cannot move
`grid.expect`, and `crates/terminal`'s own DA1 test needs the final byte string before the swap.
But the resolution table claims "every free-correctness deviation moved to `US-0086`" while
`dispatch-and-modes.md:362` keeps **D11** (blink and overline stored) in `US-0076` with its own
recording risk marked "measure in `US-0072` — `sgr` and `underline` exercise SGR 5/6/53". That is a
second exception, it is not declared, and unlike D13 it can turn the gate red. See N-03.

## New findings

| N | Sev | Location | Problem and fix |
| --- | --- | --- | --- |
| N-01 | major | `grid-and-scrollback.md:55` and `DEC-0015` decision 1, versus `reflow-and-resize.md:39-41`, `:97`, `:175` | `AnchorKind` has five variants — `SavedCursor`, `SelectionStart`, `SelectionEnd`, `Graphic`, `Mark`. Reflow's resolution of R-06 and R-31 depends on `AnchorKind::ViewportTop` and on the **cursor** being an anchor ("the cursor, the saved cursor, ... and the viewport top"; "the cursor anchor clamps to the last row"). Neither variant exists. Three documents, two lists, on the mechanism R-02, R-06 and R-31 all rest on. **Fix:** add `Cursor` and `ViewportTop` to the enum and to `DEC-0015`'s sentence, and say whether the live cursor *is* the anchor or is re-derived from it. Effort S. Blocks `US-0075`. |
| N-02 | major | `high-level-design.md` phase plan, phase 8 (`US-0080`) versus phase 9 (`US-0081`) | `US-0080`'s exit criterion includes "IN-0028's evidence walk reproduced" — a Windows GUI walk — but the new engine is not behind the application until the `US-0081` shim. The criterion cannot be met in its own packet. In the pass-1 plan graphics came *after* the swap; the re-plan moved it before and kept the criterion. **Fix:** `US-0080` exits on `graphics::tests::*` plus `vt-diff` over the Sixel recordings; the evidence walk moves to `US-0081` or later. Effort S. |
| N-03 | major | `dispatch-and-modes.md:362` (D11) against `testing-and-bench.md` section 2 and `IN-0029.md` "Partially declined" | D11 stores blink and overline attributes and stays in `US-0076`. The old engine drops both, so the frozen `grid.expect` — now **cell-exact on `attrs`** after R-57 — will not contain them and the new engine will. The gate goes red inside the packet whose exit criterion is the gate: exactly the failure R-53 exists to prevent, on the one deviation whose recording risk the table itself flags as non-zero. **Fix:** move D11 to `US-0086` with D3/D5/D6/D12, or state that the attributes are stored but excluded from `grid.expect` and put that exclusion in the format. Effort S. Blocks `US-0076`. |
| N-04 | major | `migration.md` "What each slice needs" and the `US-0081` verification line, versus `US-0083` / `US-0084` | `US-0081`'s acceptance is "no file outside `crates/terminal`, `crates/local-shell`, `crates/ssh` changed" — i.e. it already changes `local-shell` and `ssh`, which it must, because each owns the `Arc<FairMutex<Term>>` and has to hold `oneterm_vt::Terminal` for the flip to compile. `US-0083` and `US-0084` are then packets for work `US-0081` has done; the table's justification for both is "needs the new `Terminal` type", which the flip supplied. **Fix:** state precisely what remains in `US-0083` / `US-0084` (`ResizePolicy` selection, `ChildEvent` plumbing, dropping the `alacritty_terminal` manifest line), or fold them into `US-0081` and keep the slice count honest. Effort S. |
| N-05 | major | `graphics.md:227-229` against `graphics.md:65-69` and `damage-and-render-state.md:240` | The edge-case list still reads "`take_graphics` moves them into the render state, which holds them until the next painted frame" — the pass-1 behaviour R-16 removed, in the same file that now says "`RenderState` never touches pixels". An implementer reading the edge-case list re-introduces the bug. **Fix:** rewrite the row as "images queue in the engine until the adapter's next `take_graphics`". Effort S. |
| N-06 | minor | `graphics.md:211-213` | Still repeats "the placement's `top` follows the row, because it is a `RowId`" — the exact claim line 63 of the same file says was wrong for every in-region scroll. Delete it or restate as "follows its anchor". |
| N-07 | minor | `high-level-design.md` consumer map | Two stale rows: `LineAccounting` becomes "`Terminal::rows_produced()`" (R-05 renamed it `lines_produced()`), and `resize_keeping_viewport_top` becomes "`Terminal::resize(size, ResizePolicy::KeepViewportTop, points)`" (R-31 removed `points`). |
| N-08 | minor | `dispatch-and-modes.md:203`, `:406` | `Config::cell_pixels` survives after R-40 replaced it with `Terminal::set_cell_pixels`; R-40 is only partly applied. |
| N-09 | minor | `events-and-api.md` `feed` contract, clause 3 | Still lists "the sequence echo" among the state that survives between calls, after R-35 deleted the echo buffer. |
| N-10 | minor | `high-level-design.md` memory table; `grid-and-scrollback.md:146` | "a 100 000-row scrollback costs 100 000 pointers" is wrong for `Box<[Option<Row>]>`: `Row` is `RowHeader` plus an inline `Vec<Cell>`, so a slot is about 48 B, not 8 B. R-30 made the ring length a session constant, so at `SCROLLBACK_MAX = 1_000_000` the primary screen allocates roughly 50 MB of empty slots eagerly in `Terminal::new`. **Fix:** either `Option<Box<Row>>`, or state the per-slot cost and the allocate-at-construction behaviour in the memory table. |
| N-11 | minor | `reflow-and-resize.md:215` | `MAX_COLS` is used in the clamp but defined nowhere; `MAX_ROWS = 1024` is defined in `grid-and-scrollback.md`. |
| N-12 | minor | `DEC-0015` decision 2 | Still says damage is read "through a **per-consumer watermark**" (R-54 deleted the multi-consumer API; there is one owner) and justifies resolved values with "the engine may renumber its tables" (R-52 made style ids immutable; only grapheme ids move, and clusters are copied under the lock). Both now over-state an otherwise correct decision. |
| N-13 | minor | `grid-and-scrollback.md:64` versus `:258-264` | `Anchors::shift_region(rows: Range<RowId>, ...)` but every call site passes a `ScrollRegion { top, bottom }` in viewport coordinates. Pick one coordinate space. |
| N-14 | minor | `high-level-design.md:73-77` versus `graphics.md:8`, `:188` | The flat-module list names `graphics.rs`; `graphics.md` specifies `crates/vt/src/graphics/{mod,sixel}.rs`. Two files is allowed by the style rule, so fix the HLD list, not the layout. |

New totals: **5 major, 9 minor, 0 blockers.** Every one is a text edit; none reopens a decision.

## Cross-checks the coordinator asked for

| Check | Result |
| --- | --- |
| Viewport-offset table versus the scroll operations | Consistent. Every `scroll_up` case names its `offset` effect and the offset table names the same cases; `ED 2` now moves the bottom while holding the offset, which is the reference's `display_offset` semantics. |
| Tracked-anchor list versus reflow anchors versus selection rotation | Consistent in mechanism (one list, `shift_region` plus `remap`, selection anchors registered in it, `Selection::rotate` deleted) but **not in membership** — N-01. |
| `StyleRun` copy versus `DEC-0015` wording | Consistent: `DEC-0015` decision 2 now says "run-length runs of resolved style values, never as interned ids". Its *rationale* is stale — N-12. |
| Placements table versus the painter | Consistent: `Placement` holds anchor, extent and pixel size; `render_update` copies the table; the painter derives the offset; `RenderState::placements()` exists. Two stale sentences elsewhere in the same file — N-05, N-06. |
| Parity snapshot versus the harness | Consistent: `grid.expect` (cell-exact, run-length, nothing trimmed) plus `state.expect`, blessed by the old engine, frozen, cross-checked once against upstream `grid.json`, and used by `vt-diff` as well. The one thing that can break it is D11 — N-03. |
| Shim packet versus consumer packets | The shim is real and its acceptance ("no consumer changed") is provable; the boundary with `US-0083` / `US-0084` is not — N-04. |

## Verdict

**The rework is genuine, not a table.** Nine of the ten blockers are resolved in the design text
with mechanisms rather than assertions — the viewport-offset table, the tracked-anchor list,
resolved style runs copied under the lock, `rows` plus `changed`, the placement table, the
cell-exact frozen expectations, and a `vte`-oracle argument I verified myself against the patch
files. `selection.md` is a real design, not a placeholder. Two of the three declared partial
declines are the right call; the third is right about D13 and silent about D11.

**Ready for packet creation, conditionally.** The five new majors are small text edits and each
blocks one packet, not the plan:

- **Open now, unchanged:** `US-0071` (`oneterm-pty`) and `US-0072` (benchmark, corpus, blessing,
  deviation grep, bench job). Ready after pass 1; the rework only sharpened their scope.
- **Open now:** `US-0073` (parser) and `US-0074` (cell, style, grapheme). Nothing outstanding
  touches either; the oracle precondition is verified and the dependency table is authoritative.
- **Before `US-0075`:** fix N-01 (`AnchorKind` is missing `Cursor` and `ViewportTop`).
- **Before `US-0076`:** fix N-03 (move D11 out of the parity packet, or exclude blink and overline
  from `grid.expect`).
- **Before `US-0080` / `US-0081`:** fix N-02 (the evidence walk cannot be `US-0080`'s exit) and
  N-04 (state what `US-0083` / `US-0084` still contain).
- **Housekeeping, before the packet that reads them:** N-05 through N-14. N-05 is the only one
  that would actively mislead an implementer.

No finding from either pass justifies another full design round. Make the N-01..N-05 edits inside
the packets they block, and proceed.
