# Work: a Sixel's cell footprint comes from the VT340 virtual cell, so images land at the wrong size

ID: BUG-0062
Intake: [`IN-0029`](IN-0029.md)
Created: 2026-09-16

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [x] In progress
- [x] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: **bug**
- Risk lane: normal
- Spec Intake, when required: [`IN-0029`](IN-0029.md)

Normal lane. No trust boundary, no persisted schema, no data loss, no external effect. The
engine's public API does not change: `VIRTUAL_CELL`, `Placement` and `SnapshotPlacement` keep
their spelling and their types. What changes is two numbers inside `Placement` (`cols`, `rows`)
and the size of one quad in the painter — behaviour, so a `CHANGELOG` entry under **Changed**,
not a signature, so no `vt-public-api` snapshot movement.

## Outcome

An image placed by a program that sized it from `CSI 14 t` covers the cells that program meant,
and is drawn at the pixel size it decoded to.

Today the engine divides an image's pixels by the VT340 virtual cell (`VIRTUAL_CELL = (10, 20)`)
to get its footprint in cells, and the painter then *stretches the quad to that footprint*. Since
[`BUG-0061`](BUG-0061-adapter-never-sets-cell-pixels.md) the embedder does tell the engine its
real cell (`Terminal::set_cell_pixels`, pushed from `TerminalElement::prepaint` before the first
resize), and the engine still ignores it for placement. The two errors compound into one visible
defect: the image is drawn at `device_cell / (10, 20)` of its intended size — a shrink at 100 %
display scale (a 9x18 device cell gives 0.9x), an enlargement at 200 % (18x36 gives 1.8x).

Owner's E2E, 2026-09-16, after `BUG-0061` merged: in OpenTUI examples v0.5.10 "Native Image Lab"
the dragon (`WEBP 384x576`) now renders as **pixels** rather than a quadrant-block mosaic — which
is what `BUG-0061` promised — but is visibly **smaller** than the same image in Windows Terminal.

After this packet: the footprint is `ceil(px_w / cell_w) x ceil(px_h / cell_h)` from the real
cell, and the painter draws the image at its native pixel size anchored at the placement's
top-left cell, clipped to that footprint. Both halves of the old error go at once; fixing only
one would move the image from "too small" to "the right size in the wrong number of cells", which
puts the prompt inside the picture.

## Scope

- [x] In scope:
  - `crates/vt/src/graphics/placement.rs` — `place` derives the cell footprint from
    `State::cell_pixels` when both axes are non-zero, and from `VIRTUAL_CELL` when they are not.
  - `crates/vt/src/graphics/sixel.rs` — `DecodedSixel` carries the **band pixel height**
    (`bands * 6`) instead of a row count already divided by 20, so `place` divides by the same
    cell the footprint uses. The conhost agreement is preserved exactly: at the fallback cell the
    quotient is the identical `bands * 6 / 20`.
  - `crates/vt/src/graphics/mod.rs` — the `VIRTUAL_CELL` doc comment, which currently asserts
    "the engine never learns the real font cell size".
  - `crates/terminal-view/src/render/{element,metrics}.rs` — the quad. `CellMetrics::image_quad`
    returns the clip rectangle (the footprint) and the image rectangle (native pixels, converted
    to logical by the existing `CellMetrics::logical`); `paint_graphics` calls it. The geometry
    moves out of `element.rs` deliberately: another worktree is editing that file and the
    arithmetic is what the new test needs to reach without a GPUI window.
  - `crates/terminal/src/{lib,content}.rs` — the `SIXEL_VIRTUAL_CELL` re-export goes (see below)
    and `TerminalContent::placements` gains the doc that says what the footprint now means. The
    painter reads the footprint through a `Frame::placement` pass-through over the existing
    `placements()` slice, so no new public adapter method is added for one caller.
  - Tests at three levels (see Verification Plan) and the four docs below.
- [x] Out of scope:
  - **DECGRA, the Sixel aspect ratio and the raster attributes (`" Pan;Pad;Ph;Pv`).** Unchanged.
    They decide the image's *pixel* dimensions, which this packet takes as given; it only changes
    how those pixels are divided into cells.
  - **XTSMGRAPHICS (`CSI ? 2;1;0 S`, maximum image geometry).** Still unanswered, still
    `US-0106`'s (`IN-0039`). It is *not* trivially derivable here: the honest answer is
    `cols * cell_w` x `rows * cell_h` clamped by `MAX_DIMENSION`, and picking between the grid
    bound and the decoder bound, choosing the `Pa`/`Pv` sub-parameter shapes and pinning the
    reply bytes is the query-reply surface's job, not a placement packet's. Recorded under
    Follow-ups, unchanged from `BUG-0061`.
  - **Re-placing an image when the font size or the DPI scale changes after it was placed.** The
    footprint is fixed at placement time and stays in cells; the painter keeps drawing at native
    pixels, clipped to that footprint, so a font that grew leaves margin inside the box and a font
    that shrank crops the image. Documented, not fixed: re-deriving a placement would have to
    re-run the scroll that produced it, which is not a thing the grid can replay.
  - **Reporting `(10, 20)` from `CSI 14 t`.** Rejected in `BUG-0061` and still rejected: it lies
    to a DPI-aware program about the window size.

## The fallback is a contract, not a safety net

`Terminal::new` starts `cell_pixels` at `(0, 0)` and `set_cell_pixels` is the only way to move
it. An embedder that never calls it — `crates/vt`'s own tests, the `headless` example, the
`crates/tools` corpus runner, and any consumer of the git dependency that has not read guide
chapter 8 — keeps **exactly** today's behaviour: the VT340 10x20 cell, the `bands * 6 / 20`
cursor walk, byte-identical corpus output. That is what makes this change safe to land without
touching a single corpus snapshot, and it is stated in the guide rather than left to be
discovered: *an embedder that skips `set_cell_pixels` gets VT340 sizing.*

The guard is "both axes non-zero", not "either": a half-set `(9, 0)` would divide by zero. One
axis at zero is an embedder bug, and falling back whole is the behaviour that cannot panic.

## `SIXEL_VIRTUAL_CELL` is removed

`crates/terminal/src/lib.rs` re-exports `oneterm_vt::VIRTUAL_CELL` under that name for exactly
one consumer: `element.rs:373`, the rescale this packet deletes. After the change the painter
never divides by a virtual cell at all — it draws native pixels and clips to a footprint already
expressed in cells — so the re-export has no caller and goes. `oneterm_vt::VIRTUAL_CELL` stays
public and stays documented, because it is still the fallback the engine places by and an
embedder may want to name it. Removing the alias is not a public-surface change: `oneterm-terminal`
is a workspace-internal crate, not the published one.

## Documentation

### Owning Docs Reviewed

- [`low-level-design/graphics.md`](low-level-design/graphics.md) § "Placement" — carries the
  `VIRTUAL_CELL` placement rule, the "the engine never learns the real font cell size **for
  placement**" sentence that `BUG-0061` qualified, and the `device_cell / (10, 20)` consequence
  it named as `BUG-0062`'s. **Changed**: it is the owning contract for exactly this behaviour.
- `crates/vt/docs/guide/08-graphics.md` § "Placing an image" — the embedder-facing statement of
  the same rule ("Pixels are measured in a virtual cell of 10x20 ... the cursor walks down
  `bands * 6 / 20` rows"), plus the `set_cell_pixels` sentence that now has a second consequence.
  **Changed**.
- `docs/terminal-backend.md` § the adapter's duties to the engine — ends on "the engine still
  *places* Sixels in the VT340 `VIRTUAL_CELL` (10x20) and the view rescales by it, so the
  reported size is a report, not a placement input". That sentence becomes false. **Changed**.
- `crates/vt/CHANGELOG.md` § Unreleased — an embedder-visible behaviour change under clause 6 of
  the promise (the reply bytes and the placement geometry a program can observe). **Changed**,
  under **Changed**.
- [`DEC-0012`](../../decisions/DEC-0012-terminal-graphics-are-cell-anchored.md) — "graphics are
  cell-anchored". **Not changed**: this packet does not move an image off its cell anchor, it
  changes how many cells the anchor spans. The decision is what makes the fix possible.
- `DEC-0015` (absolute row ids and incremental render state) — reviewed for the placement's
  `RowId` anchor and the per-frame copy into `SnapshotState`. **Not changed**: the footprint is
  copied through unchanged, in cells, exactly as today.
- `crates/vt/src/terminal/mod.rs` `set_cell_pixels` / `dispatch.rs` `window_ops` — the source of
  the number this packet starts using. **Not changed**; `CSI 14 t` and `CSI 18 t` reply the same
  bytes as before.

### Documentation Action

Update required: `low-level-design/graphics.md`, `crates/vt/docs/guide/08-graphics.md`,
`docs/terminal-backend.md`, `crates/vt/CHANGELOG.md`.

Reason: all four state the *old* rule as a positive fact, and two of them state it as a rule that
will not change ("not tunable", "there is no `set_cell_size`"). A behaviour change that leaves
those sentences standing is how the next reader reintroduces the defect.

### Reconciliation

Changed, all four as planned, plus one not planned:

- [`low-level-design/graphics.md`](low-level-design/graphics.md) — the placement paragraph states
  the `ceil(pixels / cell)` rule, the fallback and what it guarantees, that the renderer draws
  native pixels and clips, the metrics-change behaviour, and why `DecodedSixel` carries band
  pixels. The `BUG-0061` sentence it replaces ("placement ... still use `VIRTUAL_CELL`, which is
  why an image ... lands at `device_cell / (10, 20)`") is gone, and so is the rescale formula.
- `crates/vt/docs/guide/08-graphics.md` — the embedder-facing version of the same, with the
  fallback stated as a consequence an embedder chooses rather than a default it inherits.
- `docs/terminal-backend.md` — the closing sentence of the cell-size paragraph said the reported
  size "is a report, not a placement input". It is now both, and the paragraph says what the
  painter does instead of rescaling.
- `crates/vt/CHANGELOG.md` § Unreleased § Changed — a behaviour entry, no signature, with the
  no-op-for-a-silent-embedder guarantee spelled out.
- `crates/terminal/src/content.rs` (**not planned**) — `TerminalContent::placements` is the one
  adapter doc a painter author reads for this geometry and said nothing about what `cols`/`rows`
  mean. Added while the re-export was being removed from the same crate.

`DEC-0012`, `DEC-0015` and the `set_cell_pixels` / `window_ops` sources keep their recorded
no-change reasons: no anchor moved, no row id changed, and `CSI 14 t` / `CSI 18 t` reply the same
bytes (`cell_pixels_reach_the_engine_and_csi_14_t` still passes unedited).

## Context

The engine side is four lines in one function. `place` already has `&mut State`, and
`State::cell_pixels` is the field `dispatch.rs`'s `CSI 14 t` arm reads; the only reason the
footprint did not use it is that the field was always `(0, 0)` when the design was written.

The `cursor_rows` move is what makes the change correct rather than half-correct.
`SixelParser::finish` currently returns `cursor_rows = bands * 6 / 20` — a row count in virtual
cells — and `place` walks the cursor down exactly that many rows while stamping `rows` of cells.
Leave it alone and a 384x576 image at a 9x18 cell stamps 32 rows while the cursor descends 28, so
the shell prompt lands **inside** the picture: the very thing the conhost agreement exists to
prevent. Carrying `bands * 6` instead and dividing in `place`, by the same cell the footprint
uses, keeps the two in step at every cell size and reproduces the old quotient bit for bit at the
fallback cell.

The painter side: `paint_graphics` resolves a cell's `GraphicId` to `(across, down)` through
`SnapshotState::graphic_offset` and walks back from the cell origin to the image's top-left. That
stays. What changes is the size of the quad — `m.logical(width)` x `m.logical(height)` rather
than `cell_width * width/10` x `line_height * height/20` — and the clip, which becomes the
footprint intersected with the grid bounds rather than the grid bounds alone. The footprint clip
is load-bearing on the fallback path and after a font change: `39 x 29` cells at a 9x18 cell is
`351 x 522` device pixels for a `384 x 576` image, and without the clip the overflow paints over
the text below it.

`CellMetrics::logical(i32) -> Pixels` already exists (`metrics.rs:66`) and is the same
device-to-logical conversion the shape rasteriser uses, so the quad lands on the same device grid
as the cell edges.

## Plan

- [x] The packet (this file), before any implementation edit.
- [x] `sixel.rs`: `DecodedSixel::cursor_rows` -> `band_pixels: u32`.
- [x] `placement.rs`: `cell_size(state)` helper, footprint and cursor walk from it; unit tests.
- [x] `graphics/mod.rs`: the `VIRTUAL_CELL` doc comment.
- [x] `crates/terminal`: `TerminalContent::placement`, drop the `SIXEL_VIRTUAL_CELL` re-export,
      adapter-level test that the footprint follows `set_cell_pixels`.
- [x] `crates/terminal-view`: `CellMetrics::image_quad` + its test, `paint_graphics` uses it,
      `frame.rs` placement pass-through.
- [x] Docs (five files) and the gate.

## Decisions

No new decision record. The rule this establishes — *the real cell when the embedder set one,
the VT340 cell when it did not* — is a correction to an existing design statement, and it lands
in [`low-level-design/graphics.md`](low-level-design/graphics.md), which already owns it.
[`DEC-0012`](../../decisions/DEC-0012-terminal-graphics-are-cell-anchored.md) is unchanged.

## Acceptance

Measurable by a hostile verifier from a clean checkout of this branch:

- [x] `cargo test -p oneterm-vt footprint` passes, and its assertions are exact cell counts for a
      `384 x 576` image:

      | cell pixels | source | expected `cols x rows` |
      | --- | --- | --- |
      | `(9, 18)` | `set_cell_pixels(9, 18)` — 100 % scale | `43 x 32` |
      | `(18, 36)` | `set_cell_pixels(18, 36)` — 200 % scale | `22 x 16` |
      | unset `(0, 0)` | never called | `39 x 29` (`VIRTUAL_CELL`) |

      and a `1 x 1` image is `1 x 1` at all three.
- [x] The same test proves the cursor walk stays inside the footprint: after the image, the
      cursor row is not above the last stamped row, at each of the three cell sizes.
- [x] `cargo test -p oneterm-terminal placement` passes: through `TerminalSession` /
      `TerminalContent`, a Sixel placed after `set_cell_pixels(9, 18)` reports a different
      `SnapshotPlacement::cols`/`rows` than the same Sixel placed with the cell unset, and both
      match the table above. This is the adapter-level proof that the real cell reaches placement,
      not only the engine's own field.
- [x] `cargo test -p oneterm-terminal-view image_quad` passes: for a `384 x 576` image at a 9x18
      device cell and scale factor 2.0, the image rectangle's **device** size is exactly
      `384 x 576` — the image's own pixels — and **not** `43 * 9` x `32 * 18` (`387 x 576`), the
      footprint's. The clip rectangle is the footprint.
- [x] `grep -rn "SIXEL_VIRTUAL_CELL" crates/` returns nothing.
- [x] `cargo test -p oneterm-tools --test corpus_check` passes with **no snapshot edited** in the
      diff: the corpus never calls `set_cell_pixels`, so every byte is unchanged.
- [x] `python scripts/vt-public-api.py --check` passes with **no snapshot edited**: nothing public
      changed spelling.
- [x] `pwsh scripts/ci-local.ps1 -Full` is green.

## Verification Plan

- Unit (`crates/vt/src/graphics/graphics_tests.rs`): a real `Terminal`, `set_cell_pixels`, a Sixel
  whose raster attributes declare `384 x 576`, and assertions on `Terminal::placements()[0]`'s
  `cols`/`rows` plus the cursor row, at `(9, 18)`, `(18, 36)` and unset. A `1 x 1` image at each.
- Unit (`crates/terminal-view/src/render/metrics.rs` tests): `CellMetrics::image_quad` — the image
  rectangle is the native pixel size, the clip rectangle is the footprint, and the origin walks
  back by the cell offset.
- Integration (`crates/terminal`): the adapter path — `TerminalSession::set_cell_pixels`, feed a
  Sixel, `TerminalContent::placements()` carries the real-cell footprint.
- Regression: `corpus_check` byte-identical; the existing `graphics_tests.rs` block (which never
  sets a cell) unchanged; `sixel_image_paints_once_per_frame` in `element_tests.rs` still paints.
- E2E: **not run here** (`e2e_proof 0`). The owner runs it; steps below.
- Platform: not applicable; no platform-specific code path.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

### E2E criterion (owner, after merge)

1. Build from `main` with this packet merged (`cargo build -p oneterm-app --profile fast-dev`)
   and start a fresh instance; do not reuse a running one.
2. In a local shell tab, run `opentui-examples.exe` (OpenTUI examples v0.5.10) and open **Native
   Image Lab**.
3. Expected: the dragon fills **the same cell box Windows Terminal gives it**. Judge it by
   comparing screenshots of the two at the same window size and the same display scale — the
   image's width in columns and height in rows must match, not merely "look bigger than before".
4. Repeat at 200 % display scale. Before this packet the image was ~1.8x too large there; it must
   now match Windows Terminal at that scale too. The two scales failing in opposite directions is
   the signature of the old bug and is what distinguishes a real fix from a tuned constant.
5. Cross-check: `img2sixel` output is unaffected in form, and will also now cover the cells its
   own pixel size implies.

## Follow-ups

- **`DECGRA` and the cursor walk.** Make the footprint and the walk agree when the raster
  attributes disagree with the data (verifier finding 7). Needs its own packet: it changes what
  `"Pan;Pad;Ph;Pv` means, and the reasoning is under Evidence and Gaps.
- **XTSMGRAPHICS.** `CSI ? 2;1;0 S` -> maximum image geometry, `CSI ? 1;1;0 S` -> colour
  registers, for `chafa` and `notcurses`. Owner: `US-0106` (`IN-0039`). Unchanged by this packet
  except that the maximum geometry now has a real cell size to be derived from.
- **Re-placing on a font or DPI change.** Out of scope above. If it is ever wanted, the shape is
  a placement-table pass on `set_cell_pixels` that re-derives `cols`/`rows` from `pixel_size`
  without moving the anchor — cheap, and it changes what cells an old image covers, which is a
  decision rather than a bug fix.

## Verification notes closed

Independent verification, 2026-09-16: **PASS, no blocker**
([`evidence/BUG-0062-verify.md`](evidence/BUG-0062-verify.md)). Findings 1-6 and 13 confirmed the
packet's own claims — the `ceil` rule, the `band_pixels` cursor, the footprint table, the native
quad, `SIXEL_VIRTUAL_CELL`'s single caller, the untouched corpus and public-api snapshots, and the
records. The remaining five are closed here:

| Finding | Closed by |
| --- | --- |
| 7 (`DECGRA` can put the cursor outside the image; pre-existing, docs overclaim) | **Docs, not code** — see the gap below for why. The "can never land inside" / "keeps the prompt below" wording is qualified at `sixel.rs`, `placement.rs`, `low-level-design/graphics.md` and guide 08; the verifier's two measurements (`v_raster_larger_than_the_data_...`, `v_raster_smaller_than_the_data_...`) are adopted, so the behaviour is now pinned rather than merely described. |
| 8 (the cursor lands *on* the last row, not below) | Corrected in all four places. The guide now states where the cursor stops, why (`bands * 6` is the height of the bands *above* the last), and that xterm's rule differs — flagged as informational, since this repository holds no xterm capture to pin it against. |
| 9 (a real encoder's trailing graphics newline shifts the cursor) | `v_a_trailing_graphics_newline_moves_the_cursor_below_the_image` is adopted, pinning the cursor row at `(9, 18)`, `(18, 36)` and the fallback for the payload shape `libsixel` and `img2sixel` actually emit — the shape the packet's own `dragon()` fixture omits. The guide names both shapes. |
| 10 (`Frame::placement` duplicates `SnapshotState::placement`) | Fixed as the verifier suggested: `TerminalContent::placement` is the one-line forward onto the existing `pub` helper, and `Frame::placement` forwards to it. One scan per image per frame instead of two, and the duplicate is gone. |
| 11 (stale "one virtual 10 x 20 cell" comment) | Corrected in `element_tests.rs`: the element pushes the real device cell in `prepaint` before the payload is fed, so that placement has not used the virtual cell since `BUG-0061`. |

Finding 12 (an unbounded band count costs a line feed per row) is recorded as pre-existing and not
acted on: it is the same shape as `main`'s, the `u16` clamp bounds it, and
`v_a_one_pixel_cell_does_not_panic_or_hang` — adopted — proves the worst cell size neither panics
nor hangs.

All 18 verifier tests are adopted verbatim and pass unmodified: 12 `v_*` in
`crates/vt/src/graphics/graphics_tests.rs` and the six of `verifier_bug_0062` in
`crates/terminal-view/src/render/metrics.rs`.

## Evidence and Gaps

Six commits on `fix/sixel-real-cell` from `main` (`47e7e93c`): the packet, the engine, the adapter
and view, the docs, a fixup for the rustdoc self-containment gate, and the verification close-out
(the adopted tests, the four doc qualifications, finding 10's de-duplication, and the report).

Test names, since none of them is selected by the obvious filter:

| Level | Command | Test |
| --- | --- | --- |
| Unit, engine | `cargo test -p oneterm-vt --lib graphics::` | `footprint_follows_the_embedder_cell_size`, `a_one_pixel_image_covers_one_cell_at_every_cell_size`, `a_zero_axis_falls_back_to_the_virtual_cell`, and the 12 adopted `v_*` |
| Unit, view | `cargo test -p oneterm-terminal-view --lib render::metrics` | `an_image_is_drawn_at_its_own_pixels_and_clipped_to_the_footprint`, and the six adopted `verifier_bug_0062::v_*` |
| Integration | `cargo test -p oneterm-terminal --lib session::` | `the_pushed_cell_size_decides_a_sixel_placement_footprint` |

The footprint table, from the engine and adapter tests (image `384 x 576`, the OpenTUI dragon):

| cell pixels | source | `cols x rows` | cursor lands |
| --- | --- | --- | --- |
| `(9, 18)` | `set_cell_pixels(9, 18)`, 100 % scale | `43 x 32` | row 31, the image's last |
| `(18, 36)` | `set_cell_pixels(18, 36)`, 200 % scale | `22 x 16` | row 15, the image's last |
| `(0, 0)` | never set — `VIRTUAL_CELL` | `39 x 29` | row 28, the image's last |
| `(9, 0)` / `(0, 18)` | half-set, an embedder bug | `39 x 29` | — |

A `1 x 1` image is `1 x 1` at all three cell sizes.

The view quad, from `an_image_is_drawn_at_its_own_pixels_and_clipped_to_the_footprint`: at a 9x18
device cell and scale factor 2.0 the image rectangle is `384 x 576` **device** pixels — the
image's own — while the clip is `387 x 576`, the `43 x 32` footprint. The test asserts the two
numbers differ, so it fails if the quad is ever sized from the footprint again.

- `cargo test -p oneterm-vt` (default) — 548 + 12 suites, all passed.
- `cargo test -p oneterm-vt --no-default-features`, `--all-features`, `--features vt-paranoid` —
  all passed, including the whole-history integrity walk.
- `cargo test --workspace` — passed, `corpus_check` included and **no corpus snapshot edited**:
  the corpus never calls `set_cell_pixels`, so every byte took the fallback path.
- `python scripts/vt-public-api.py --check --no-doc` — "public API surface unchanged
  (public-api.windows.txt)"; `--check-nameable` and `--diff-platforms` unchanged (the delta is the
  same six `oneterm_vt::pty` lines). No snapshot edited, so nothing to mirror to unix.
- `python scripts/check-english.py` (908 files), `scripts/check-doc-paths.py` (199 paths) — passed.
- `pwsh scripts/ci-local.ps1 -Full` — `ci-local: all checks passed`, `cargo deny` included
  (advisories ok, bans ok, licenses ok). Log kept out of the repository.

Gaps:

- **E2E not run here** (`e2e_proof 0`), and it is the criterion this packet exists for: nothing in
  this branch has been compared against Windows Terminal. The owner's steps are above, and they
  must be run at **both** 100 % and 200 % display scale — one scale alone cannot tell a real fix
  from a tuned constant, because the old error ran in opposite directions at the two.
- **The `crates/vt` rustdoc may not cite a work packet** (`ci-local`'s self-containment step): the
  first pass of the engine and test doc comments named `BUG-0062` and the gate caught it. The
  reasoning is in the doc comments, only the id is gone. Worth knowing before the next `crates/vt`
  edit, which is why it is recorded rather than quietly fixed.
- **No test covers a metrics change after an image is placed.** The behaviour is documented
  (the footprint stays, the picture is cropped or gains margin) and follows from the clip, but the
  GPUI test window pins `scale_factor` at 2.0 — the same limitation `BUG-0061` recorded.
- **`DECGRA` versus the cursor walk is documented, not fixed** (verifier finding 7, pre-existing
  on `main`). Making the footprint and the walk both follow `max(declared, measured)` was
  considered on the coordinator's prompt and is **not** a few lines:
  - it inverts `SixelParser::finish`'s documented rule that the declaration wins, which
    `raster_attributes_declare_the_size` pins (declared `1 x 3` against 12 measured px — `max`
    answers 12 and the test fails);
  - the image's pixel buffer is allocated at the declared size, so a footprint from `max` would
    cover cells the image has no pixels for, and the cursor would move over a picture that is not
    there;
  - tying the walk to the footprint rather than to the bands changes the fallback quotient away
    from `bands * 6 / 20`, which is the conhost agreement `cursor_descends_bands_times_six_over_twenty`
    pins and which the corpus runs through.

  It is a `DECGRA` semantics change needing its own packet and a fresh capture, and the original
  scope for this one put the raster attributes explicitly out of bounds. The behaviour is now
  pinned by two adopted tests and stated in four docs instead.
- The XTSMGRAPHICS and re-placing follow-ups above are open by design.

## Handoff

Touches `crates/vt/src/graphics/{mod,placement,sixel}.rs`, `crates/terminal/src/{lib,content}.rs`
and `crates/terminal-view/src/render/{element,metrics,frame}.rs`. A concurrent `terminal-view`
worktree is editing `element.rs`; the edit here is deliberately three lines inside
`paint_graphics` with the arithmetic moved to `metrics.rs`, so a rebase conflict is mechanical.

### `harness.db` row

Not inserted from this branch (`harness.db` is edited only by the coordinator). Run this once
against the database at the repository root.

```python
#!/usr/bin/env python3
"""Insert the BUG-0062 story row. Run once, from the repository root."""
import sqlite3

ROW = dict(
    id="BUG-0062",
    title="A Sixel's cell footprint comes from the VT340 virtual cell, so images land at the wrong size",
    created_at="2026-09-16",
    risk_lane="normal",
    contract_doc="docs/spec-intakes/IN-0029-vt-engine/low-level-design/graphics.md",
    packet_doc=(
        "docs/spec-intakes/IN-0029-vt-engine/"
        "BUG-0062-sixel-placement-uses-real-cell.md"
    ),
    status="implemented",
    unit_proof=1,
    integration_proof=1,
    e2e_proof=0,
    platform_proof=1,
    evidence=(
        "docs/spec-intakes/IN-0029-vt-engine/evidence/BUG-0062-verify.md"
    ),
    verify_command="pwsh scripts/ci-local.ps1 -Full",
    last_verified_at="2026-09-16",
    last_verified_result="pass",
    notes=(
        "Follow-up to BUG-0061. The engine divided image pixels by VIRTUAL_CELL (10x20) "
        "for the cell footprint and the view stretched the quad to it, so an image sized "
        "from CSI 14 t landed at device_cell/(10,20) of its size: smaller at 100% scale, "
        "1.8x larger at 200%. Footprint is now ceil(px/cell) from Terminal::cell_pixels, "
        "VIRTUAL_CELL only when the embedder never called set_cell_pixels (corpus stays "
        "byte-identical). The view draws native pixels clipped to the footprint; "
        "SIXEL_VIRTUAL_CELL is gone. E2E is the owner's opentui-examples run after merge."
    ),
    intake_id=34,
)

with sqlite3.connect("harness.db") as db:
    columns = ", ".join(ROW)
    placeholders = ", ".join("?" for _ in ROW)
    db.execute(f"INSERT INTO story ({columns}) VALUES ({placeholders})", tuple(ROW.values()))
print("inserted BUG-0062")
```
