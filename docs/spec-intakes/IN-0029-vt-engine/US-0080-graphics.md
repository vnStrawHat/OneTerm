# Work: Graphics in the engine (Sixel)

ID: US-0080
Intake: IN-0029
Created: 2026-09-12

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [ ] Planned
- [ ] In progress
- [x] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: new capability
- Risk lane: high_risk
- Spec Intake, when required: IN-0029

## Outcome

`oneterm-vt` decodes Sixel itself. A `DCS q` payload is decoded by a first-party decoder
ported from `vendor/alacritty_terminal/src/term/graphics.rs`, the finished image is placed at
the cursor in the VT340 10x20 virtual cell under the conhost cursor rule
(`bands * 6 / 20` rows down, column kept), every covered cell carries **only** a `GraphicId`
in its interned extras, and the placement is a **tracked anchor** so the image moves with its
content through `IL`, `DL`, `SU`, `SD`, a region scroll and reflow. When no row inside the
placement's extent still carries `RowFlags::HAS_GRAPHIC` — or its anchor is dead — the
placement is released and `VtEvent::GraphicReleased(id)` fires exactly once. Pixels queue
until `Terminal::take_graphics()` drains them, which is the only drain (R-16), and the render
state carries the placement table so the painter derives each cell's `(col, row)` offset
without engine access.

## Scope

- [ ] In scope: `crates/vt/src/graphics/` (decoder, placement, release sweep, public types);
  the `pub mod graphics;` line and re-exports in `crates/vt/src/lib.rs`; DCS routing and the
  `RIS` drop in `crates/vt/src/terminal/dispatch.rs`; the graphics state field on
  `crates/vt/src/terminal/mod.rs`'s `State` plus `Terminal::take_graphics`; the placement
  table's exposure through `crates/vt/src/render/state.rs`; one Sixel recording added to the
  parity corpus with a `--dir` flag on `vt-corpus` so it can be blessed by the **old** engine.
- [ ] Out of scope: the `crates/terminal` shim and its `GraphicStore` / `paint_graphics`
  consumers (`US-0081`); the IN-0028 GUI evidence walk (N-02, `US-0081`); Kitty graphics,
  XTSMGRAPHICS, `DECSDM`, `? 8452` and OSC 1337 (later intakes); deleting
  `crates/terminal/src/sixel_tests.rs` (`US-0082`); `cargo fuzz` targets (Linux only).

## Acceptance

- [x] Every test named in [`low-level-design/graphics.md`](low-level-design/graphics.md)
      § Verification exists under that exact name in `graphics::tests`.
- [x] All eleven expectations in `crates/terminal/src/sixel_tests.rs:46-271` are reproduced at
      engine level with the same inputs and the same expected cells and cursor.
- [x] A placement moves with an in-region scroll, `IL` and `DL`, and is dropped when a **row
      reset** clears `HAS_GRAPHIC` — the alternate screen's `ED 2`, `RIS`, a scroll blank — or
      when its anchor dies to a history trim or a reflow, each firing `GraphicReleased`
      exactly once.
- [x] Overwriting a covered cell with text drops **that cell's** reference and no other. It
      does **not** release the placement, and neither does `EL 2`: neither path resets the
      row, so `RowFlags::HAS_GRAPHIC` survives as the false positive R-22 explicitly allows,
      and the renderer keeps painting the whole image from its placement (`DEC-0012`, the
      documented v1 limitation). Nor does `CSI 2 J` on the **primary** screen, where R-13
      scrolls the graphic-bearing row into scrollback alive.
- [x] An image emitted at the bottom scrolls its top bands into history with its cells.
- [x] An image wider or taller than the grid is clipped, never wrapped.
- [x] A 32 MiB unterminated Sixel stream stays bounded and stamps no cells.
- [x] `Terminal::take_graphics` hands each image out exactly once, and no number of
      `render_update` calls consumes one.
- [x] `vt-corpus check --engine new` is 45/45 on the vendored corpus and `vt-diff` reports
      45/45 identical, plus the new Sixel recording identical in both.
- [x] No `unsafe`, no new external dependency.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/graphics.md` — the contract for this
  packet: ownership (R-21, one extras entry per image), the row-derived release signal (R-22),
  the decoder grammar, the placement rule and the edge-case list.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/grid-and-scrollback.md` — tracked
  anchors are the one mechanism for row motion; `AnchorKind::Graphic(GraphicId)` and
  `RowFlags::HAS_GRAPHIC` were already shipped by `US-0075`.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/cell-and-style.md` — `Extras` already
  carries `graphic: Option<GraphicId>`; the interning ladder never fails and falls back to
  id 0.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/damage-and-render-state.md` — R-16,
  `take_graphics` is the only drain and `RenderState` never touches pixels; `RenderCell.graphic`
  carries the id only.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/dispatch-and-modes.md` and
  `crates/vt/src/terminal/` — the `dcs_hook` / `dcs_put` / `dcs_unhook` path, and DA1 already
  answering `?62;4;22c`.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/events-and-api.md` —
  `VtEvent::GraphicReleased(GraphicId)` already exists.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/testing-and-bench.md` — the corpus is
  frozen and blessed by the old engine only (R-58); the new engine never blesses.
- `docs/spec-intakes/IN-0028-sixel-graphics/` (HLD, `US-0066`, `US-0067`) — the acceptance
  rework that produced the 10x20 virtual cell and the `bands * 6 / 20` cursor rule.
- `docs/decisions/DEC-0012-*` (graphics are cell-anchored), `DEC-0015-*` (absolute row ids and
  incremental render state).
- `docs/spec-intakes/IN-0029-vt-engine/research/api-surface.md` § 3 — what the view consumes:
  `GraphicData { id, width, height, rgba }` and a per-cell `(id, col, row)`.

### Documentation Action

**Update required, in one place only:** `migration.md`'s doc table assigns
`docs/spec-intakes/IN-0028-sixel-graphics/{high-level-design,low-level-design/vendor-graphics}.md`
to this packet, "superseded in place, pointing at `graphics.md`".

**No contract change anywhere else.** The reviewed IN-0029 LLDs already describe the behaviour
this packet implements, including the pieces earlier packets pre-shipped for it
(`Extras.graphic`, `AnchorKind::Graphic`, `RowFlags::HAS_GRAPHIC`, `RowMut::mark_graphic`,
`VtEvent::GraphicReleased`, `RenderCell.graphic`, `Cell::is_erasable`'s R-13 rule). The
Deviations section below records where the implementation differs from the LLD's sketch; none
of those is a change to accepted behaviour, and this packet must not edit the intake, the HLD
or any IN-0029 LLD.

Reason: the design is accepted and detailed; the work is to make the engine match it.

### Reconciliation

Changed:

- `docs/spec-intakes/IN-0028-sixel-graphics/high-level-design.md` — a superseded-in-place note
  pointing at `graphics.md`, keeping IN-0028 as the record of **why** the 10x20 cell and the
  `bands * 6 / 20` rule are what they are.
- `docs/spec-intakes/IN-0028-sixel-graphics/low-level-design/vendor-graphics.md` — the same
  note, plus the two things that genuinely change (the per-cell `GraphicCell` becomes a bare
  `GraphicId` plus a placement anchor; the release signal is new).
- `crates/vt/tests/corpus/NOTICE` — a section for the new OneTerm-owned `oneterm/` corpus
  directory, so the vendored NOTICE keeps covering only the vendored files.

The no-change reason for every IN-0029 owning doc still holds. The deviations below are
recorded here for the design owner; the LLD rows they touch (`DcsSink`, the reserved Kitty
fields, the `render_update` release scan, the overlap invariant, the
`release_event_fires_on_clear_screen` screen choice) are the design owner's to reconcile, and
this packet does not edit them.

## Context

- `AnchorKind::Graphic(GraphicId)`, `RowFlags::HAS_GRAPHIC`, `RowMut::mark_graphic`,
  `Extras { hyperlink, graphic }`, `GraphicId`, `VtEvent::GraphicReleased` and
  `RenderCell.graphic` all already exist — `US-0074`, `US-0075` and `US-0079` pre-shipped the
  shape. This packet fills them in.
- The parser already enforces `DCS_MAX_BYTES = 16 MiB` and reports `dcs_unhook(aborted: true)`
  past it (`US-0073`), so the "DCS cap" half of the outcome is a wiring and a test, not new
  parser code.
- `Screen::occupied_rows` already treats a graphic-bearing cell as non-erasable (R-13), so
  `CSI 2 J` on the **primary** screen scrolls an image into scrollback rather than discarding
  it. The release-on-clear case is therefore the **alternate** screen (and a zero-scrollback
  primary), which is the TUI-repaint case the capability exists for.
- Another agent is building the `crates/terminal` shim (`US-0081`) concurrently; the public
  API below is frozen once committed.

## Plan

- [x] Port the decoder to `crates/vt/src/graphics/sixel.rs`.
- [x] Placement + row-derived release sweep in `crates/vt/src/graphics/placement.rs`.
- [x] Public types and `GraphicsState` in `crates/vt/src/graphics/mod.rs`; `Terminal::take_graphics`.
- [x] Route `dcs_hook` / `dcs_put` / `dcs_unhook`; drop pending images and the in-flight parser on `RIS`.
- [x] Copy the placement table into `RenderState`; add the painter's offset helper.
- [x] Tests in `crates/vt/src/graphics/graphics_tests.rs`.
- [x] Add the `sixel_basic` corpus recording, bless it with the **old** engine, gate it.

## Decisions

- `DEC-0012` — graphics are cell-anchored; the renderer paints a whole image from its
  placement.
- `DEC-0015` — absolute row ids and the incremental render state; several render states are
  allowed, which is why `take_graphics` and not `render_update` drains.

## Verification Plan

- `cargo test -p oneterm-vt graphics::` — the 25 named behaviours plus the ported `sixel_tests`
  expectations.
- `cargo test -p oneterm-vt` — the whole engine suite, for the `State`, dispatch and render
  edits.
- `cargo run -p oneterm-tools --bin vt-corpus -- check --engine new` and `--engine old`.
- `cargo run -p oneterm-tools --bin vt-diff` (corpus) and `vt-diff --fixtures` (the bench
  streams, which include a Sixel generator).
- `pwsh scripts/ci-local.ps1` green, raw totals recorded.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Public API

Frozen for `US-0081`. Everything below is `pub` from `oneterm-vt`.

```rust
// oneterm_vt::graphics  (re-exported at the crate root)
pub const VIRTUAL_CELL: (u16, u16) = (10, 20);   // the VT340 / conhost cell
pub const MAX_DIMENSION: u32 = 4096;             // per axis
pub const MAX_PIXEL_BYTES: usize;                // 4096 * 4096 * 4
pub const MAX_PLACEMENTS: usize = 256;           // live placements; oldest is released

pub struct GraphicData { pub id: GraphicId, pub width: u32, pub height: u32, pub rgba: Vec<u8> }
pub struct Placement {
    pub id: GraphicId,
    pub anchor: AnchorId,          // resolve through `Terminal::grid().anchors()`
    pub cols: u16, pub rows: u16,  // extent in cells, already clipped right
    pub pixel_size: (u32, u32),
}

impl Terminal {
    /// Drain, oldest first, each image exactly once. THE ONLY DRAIN (R-16).
    pub fn take_graphics(&mut self) -> Vec<Arc<GraphicData>>;
    pub fn placements(&self) -> &[Placement];
}

// oneterm_vt::render
pub struct RenderPlacement {
    pub id: GraphicId,
    pub row: RowId, pub col: u16,   // the image's top-left cell, anchor resolved
    pub cols: u16, pub rows: u16,
    pub pixel_size: (u32, u32),
}

impl RenderState {
    pub fn placements(&self) -> &[RenderPlacement];
    pub fn placement(&self, id: GraphicId) -> Option<&RenderPlacement>;
    /// The `(col, row)` offset inside the image's cell grid — what the old
    /// `GraphicCell { col, row }` carried per cell.
    pub fn graphic_offset(&self, id: GraphicId, row: RowId, col: u16) -> Option<(u16, u16)>;
}

// already shipped, now populated
RenderCell.graphic: Option<GraphicId>          // the id only, never an offset
VtEvent::GraphicReleased(GraphicId)            // evict the texture; fires exactly once
```

**What the shim does.** After each `feed`, handle `VtEvent::GraphicReleased(id)` by dropping
the view's texture for `id`, then call `Terminal::take_graphics()` and upload each
`GraphicData`. At paint time, for a `RenderCell` with `graphic == Some(id)`, ask the
`RenderState` for `graphic_offset(id, row.id, col)`; `None` means the image is gone — paint
nothing. `pixel_size` and `VIRTUAL_CELL` give the scale (`cell_width / 10`,
`line_height / 20`). `RenderState` never holds pixels.

**Release delivery.** Releases queue in the engine and are pushed into the batch by `feed`,
including `feed(&[], ..)`, because `resize` can release placements and has no batch of its own.
A shim that resizes should feed once (an empty feed is enough) before assuming nothing died.

## Deviations from the LLD

Recorded, not applied to the LLD; each is the design owner's to reconcile.

1. **No `DcsSink` trait.** The LLD publishes a three-method `pub(crate)` trait with one
   implementation. `GraphicsState` holds `Option<SixelParser>` directly instead. A trait with
   one implementor is dead flexibility, and the Kitty protocol it was meant to leave room for
   arrives on the **APC** path, not the DCS one, so it would not use this trait anyway.
2. **No reserved Kitty fields on `Placement`** (`placement_id`, z-index, draw-under-text).
   Dead configuration until something reads it; the record already exists per placement, which
   is the part that is hard to add later.
3. **`render_update` does not run the release scan.** `damage-and-render-state.md` says it
   does. It cannot deliver the event — there is no batch — and R-16 keeps `RenderState` out of
   graphics ownership entirely, so scanning there would produce a release nobody can observe.
   The scan runs at the end of `feed` and after `resize`, and `feed` delivers.
4. **The overlap debug invariant is not implemented.** The LLD wants "no two placements with
   overlapping column ranges on the same bottom row". It is **false** under this design: print
   an image, `CUP` back to the same cell, print another, and the first placement stays live on
   a `HAS_GRAPHIC` row-flag false positive, which the LLD itself declares allowed. The
   dangling-reference invariant is implemented (`graphics::assert_integrity`, debug only, run
   from `feed`), scoped to cells **inside** a live placement's extent — outside one a stale id
   is inert, because the painter resolves it through the placement table and finds nothing.
5. **`release_event_fires_on_clear_screen` is written on the alternate screen.** On the
   primary screen `Cell::is_erasable`'s R-13 rule (shipped by `US-0074`) makes `CSI 2 J`
   scroll a graphic-bearing row into scrollback instead of discarding it, so the image is
   **still alive** and must not be released — which the test now asserts explicitly. The
   alternate screen's `ED 2` resets rows, which is the TUI-repaint case R-22 exists for.
6. **`MAX_PLACEMENTS = 256`, oldest-released.** The LLD bounds nothing. A stream emitting an
   image per line of a million-row scrollback would otherwise grow the placement table and the
   linear release sweep without limit; the bound frees the view's texture rather than leaking.
7. **An unknown colour mode selects the register — a parity break.** For
   `#Pr;Pu;Px;Py;Pz` with `Pu` outside `{1, 2}`, the old engine returns early and selects
   **nothing**, leaving the previously selected register in force; this engine selects `Pr`
   without defining it. The LLD's grammar table ("other modes select only") is followed over
   the port. It is a deliberate, undeclared behaviour difference from the engine being
   replaced, not an accident: harmless because no Sixel producer emits a mode outside
   `{1, 2}`, unreachable from the corpus and the fixtures (both are identical), and covered by
   no test in either suite. Flagged here so the design owner can confirm the LLD text or ask
   for the port's behaviour back.
8. **`SixelParser::new()` takes no parameters.** The LLD's `start(&Params, u8)` passed `P1;P2;P3`
   to a decoder that has never read them (parity: `P2` is ignored deliberately). The dispatch
   layer does the `q` test instead.
9. **`Vec<ScrollReport>` out of `graphics::place`.** The placement's line feeds must be
   reported as `RowsScrolled` / `RowsTrimmed`, and `Handler::report` — the engine's single
   reporting path — lives in a private module. Returning the reports keeps that path single.

## Evidence and Gaps

### Commands and results

- `pwsh scripts/ci-local.ps1` — **all checks passed** (fmt, clippy `-D warnings`,
  `cargo test --workspace`, and all five Python policy checks).
- `cargo test --workspace` raw totals over **57 test-result sections: 1549 passed / 0 failed /
  8 ignored**. The `US-0076` baseline on this branch was 57 / 1521 / 8; the delta is this
  packet's 26 `graphics::tests` plus the 2 new corpus-gate tests.
- `cargo test -p oneterm-vt`: 358 + 5 + 5 passed, 0 failed, 2 + 1 ignored.
- `cargo test -p oneterm-vt --lib graphics::`: **26 passed / 0 failed** in 0.57 s.
- `vt-corpus check --engine new`: **45 recordings, 45 passed, 0 failed**; `--engine old` the
  same, so the frozen expectations did not move.
- `vt-diff`: **45 recordings, 45 identical, 0 differing**.
- `vt-diff --fixtures`: **10 identical, 0 differing** at 160x45 / 256 KiB — including
  `sixel`, which repeats a real two-band `DCS q` image about seven thousand times. At
  `US-0076` that fixture was identical only because *both* engines dropped the DCS; it is now
  identical because both engines decode, place and scroll it the same way.
- New corpus recording `crates/vt/tests/corpus/oneterm/sixel_basic` (160 bytes, 20x8, 100
  lines of history): a placed 25x45 image at line 3 column 4, a second image clipped on the
  right, and a third at the bottom that scrolls into history. Blessed by the **old** engine
  (`vt-corpus bless --engine old --dir crates/vt/tests/corpus/oneterm`, 108 rows, 6 state
  entries) and then frozen; `vt-corpus check --engine new --dir ...` and `vt-diff --dir ...`
  both report it identical. Its `state.expect` pins `cursor row=7 col=0`, which is the conhost
  rule the whole intake turns on.

### Test map

| LLD verification row | Test |
| --- | --- |
| the ten `sixel_tests.rs` behaviours | `decodes_a_minimal_sixel`, `rgb_and_hls_colour_registers`, `repeat_and_band_control_characters`, `untouched_pixels_stay_transparent`, `raster_attributes_declare_the_size`, `dimensions_clamp_at_4096`, `placed_at_the_cursor_column_and_clipped_right`, `cursor_descends_bands_times_six_over_twenty`, `scrolls_into_history_with_its_cells`, `overwriting_or_erasing_a_cell_drops_the_reference`, `ris_drops_pending_images_but_not_the_id_counter`, `clear_screen_does_not_drop_pending_images`, `da1_advertises_sixel`, plus `empty_and_non_sixel_dcs_place_nothing` |
| R-21, one extras entry | `one_extras_entry_per_image` (a 400 x 200-cell image: **+1** entry) |
| R-02, in-region scroll | `placement_moves_with_an_in_region_scroll` (`SU` then `IL`) |
| R-22, the release signal | `release_event_fires_on_clear_screen`, `release_event_fires_on_row_reset_and_scroll_blank`, `release_event_fires_when_the_anchor_row_is_trimmed`, `release_event_fires_when_reflow_drops_the_anchor` |
| the painter's offset | `painter_offset_is_derived_from_the_placement` |
| the byte cap | `payload_byte_cap_aborts_an_endless_sixel` (a 32 MiB stream: one abort, no cells, no image) |
| aborted DCS | `aborted_dcs_stamps_no_cells` |
| mode 2026 | `images_survive_a_frame_skipped_by_mode_2026` (also covers `render_state_never_drains_graphics`) |
| the debug invariant | `integrity_rejects_a_dangling_graphic_ref` |
| ConPTY byte loss | `corrupt_band_does_not_panic` |

### Files

New: `crates/vt/src/graphics/{mod,sixel,placement,graphics_tests}.rs`;
`crates/vt/tests/corpus/oneterm/sixel_basic/{recording,size.json,config.json,grid.expect,state.expect}`;
this packet.
Edited: `crates/vt/src/lib.rs` (one `pub mod` line, four re-exports);
`crates/vt/src/terminal/mod.rs` (the `graphics` field, the sweep and drain in `feed`, the sweep
in `resize`, `take_graphics`, `placements`, one `EngineView` field);
`crates/vt/src/terminal/dispatch.rs` (the three DCS methods and the `RIS` drop);
`crates/vt/src/render/{state,mod}.rs` (`RenderPlacement`, one `EngineView` field, the copy and
three accessors); `crates/vt/src/render/render_tests.rs` (one field in the test harness);
`crates/tools/src/corpus.rs` (`oneterm_dir`); `crates/tools/src/bin/vt-corpus.rs` (`--dir`);
`crates/tools/tests/corpus_check.rs` (two gate tests); `crates/vt/tests/corpus/NOTICE`; the two
IN-0028 documents. `crates/vt/Cargo.toml` and `Cargo.lock` untouched.

`crates/vt/src/grid/` is **not** touched: `AnchorKind::Graphic`, `RowFlags::HAS_GRAPHIC` and
`RowMut::mark_graphic` were already there. `crates/terminal/` is **not** touched, so `US-0081`
is unblocked.

### Gaps

1. **No E2E and no platform proof.** IN-0028's GUI walk is `US-0081`'s by N-02: the new engine
   is not behind the application until the shim lands, and a Sixel walk needs the bundled
   `conpty.dll` + `OpenConsole.exe` pair (the inbox `conhost.exe` on Windows 11 24H2 swallows
   Sixel DCS payloads).
2. **No fuzz target.** `cargo fuzz` is Linux-only and this session is Windows;
   `crates/vt/fuzz/fuzz_targets/sixel.rs` is not written. `corrupt_band_does_not_panic` carries
   one seed of the shape by hand.
3. **A split image can strand cell references.** An `SD` or `IL` that moves part of an image
   below its own anchor's extent leaves cells carrying an id whose placement may later be
   released. Those cells are inert (the painter resolves through the placement table), and the
   debug invariant is scoped so it does not flag them — but the row still reads as
   `HAS_GRAPHIC` until it is reset. It is a false positive in the direction the design allows.
4. **`Terminal::placements()` is a linear scan** and so is `RenderState::placement()`. Bounded
   by `MAX_PLACEMENTS`; index them if a workload with hundreds of live images appears.
5. **The release scan is O(live placements x rows)** per feed, skipped entirely when nothing is
   placed. `graphics::assert_integrity` is O(placements x rows x cols) in **debug** builds only
   (marked `ponytail:` in the source).
6. **`docs/vendor/README.md` § 2's stale `set_cell_size` claim** is called out by the LLD as
   something the replacement docs must not repeat; nothing here repeats it, and the vendored
   README is not this packet's to edit.

## Handoff

Next owner: `US-0081` (the `crates/terminal` shim). The public API above is frozen; nothing in
`crates/terminal/` was touched.
