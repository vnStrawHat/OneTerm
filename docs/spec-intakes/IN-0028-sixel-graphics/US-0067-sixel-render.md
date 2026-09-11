# Work: Paint Sixel images in the terminal grid

ID: US-0067
Intake: IN-0028
Created: 2026-09-11

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

- Change type: new capability
- Risk lane: normal
- Spec Intake, when required: IN-0028

## Outcome

Images placed by US-0066 are drawn in the grid at their cells, scroll with the text, are
clipped at the grid edge, vanish when their cells are erased, and the view keeps at most 64
images / 64 MB of decoded pixels.

## Scope

- [x] In scope: `image` workspace dependency (core types only); `Frame::graphics()`,
  `Cell.graphic`; `GraphicStore` (`render/graphics.rs`) in `RenderState`; `paint_graphics`
  pass; `set_cell_size` call from `prepaint`; IN-0018 HLD paint row, `docs/osc-sequences-checklist.md`,
  `docs/agents/dependencies.md`, `README.md`; GUI walk with a generated Sixel file.
- [x] Out of scope: per-row image bands (text over image, partial erase); selection colour
  over images; a setting to disable graphics.

## Acceptance

- [x] A frame whose snapshot carries a new image registers it once; the same id in later
  frames is not re-uploaded (`graphics::tests::store_uploads_once_and_evicts_oldest`,
  `element_tests::sixel_image_paints_once_per_frame` via `images_uploaded`).
- [x] Past the image or byte cap the oldest entry is evicted and its id is no longer served
  (`store_uploads_once_and_evicts_oldest` with test limits; production caps 64 / 64 MB).
- [x] A visible image contributes exactly one `paint_image` call per frame and none once its
  cells lose their references (`sixel_image_paints_once_per_frame`, `stats.images`); the
  origin rule (`cell origin - fragment offset`) is exercised by the GUI walk.
- [x] GUI (Windows): the 96 x 48 test card prints at the cursor, `after` follows below it,
  the image scrolls into history and comes back clipped at the grid edge, `cls` removes it
  (`evidence/gui-walk.md`, `evidence/US-0067-*.png`).
- [x] `pwsh scripts/ci-local.ps1` green (2026-09-11).

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0028-sixel-graphics/high-level-design.md` — paint pass and store.
- `docs/spec-intakes/IN-0018-rebuild-terminal-render-engine/high-level-design.md` § Frame
  Pipeline paint row and § Cross-frame State — must list the image pass and the store.
- `docs/osc-sequences-checklist.md` § Group J — must say Sixel is supported.
- `docs/agents/dependencies.md` § 3 — must list `image`.
- `README.md` § Terminal emulator — feature bullet.

### Documentation Action

Update required: the four docs above.

Reason: each enumerates the thing this packet adds.

### Reconciliation

Changed: IN-0018 HLD (prepaint/paint rows, primitive-order note, `graphics: GraphicStore`
row), `docs/osc-sequences-checklist.md` § Group J and the status summary,
`docs/agents/dependencies.md` § 3 (`image`), `README.md` § Terminal emulator. The published
`gpui-pre 0.3.3` `paint_image` takes `(bounds, image_bounds, ..)` and draws their
intersection, so the grid bounds clip the image without a content mask; IN-0028 HLD step 9
describes it as clipping by the grid bounds, which holds.

## Context

- `gpui::RenderImage::new` takes `image::Frame`s in BGRA order (`elements/img.rs` swaps
  channels after decoding); `Window::paint_image(bounds, corners, Arc<RenderImage>, 0, false)`
  and `Window::drop_image` release the atlas tile.
- Polychrome sprites paint after glyphs inside a layer (`scene.rs` primitive order), so the
  image pass can run anywhere in the grid layer; it runs after the quad passes for clarity.
- `GridPainter` already has the geometry (`cell_origin`) and `FrameStats`; a new `images`
  counter feeds the tests.

## Plan

- [x] `Cargo.toml`: `image = { version = "0.25", default-features = false }`; terminal-view
  dependency.
- [x] `frame.rs`: `GraphicRef`, `Cell.graphic`, `Frame::graphics()`.
- [x] `graphics.rs`: `GraphicStore` (ingest, get, evict + `drop_image`); `FrameStats.images`
  and `images_uploaded`.
- [x] `element.rs`: `set_cell_size` on metrics change; `paint_graphics` pass; tests.
- [x] Docs, GUI walk (`evidence/make_sixel.py` writes the test card).
- [x] CI.

## Decisions

- `DEC-0012`.

## Verification Plan

- `cargo test -p oneterm-terminal-view` (store + paint tests).
- `pwsh scripts/ci-local.ps1`.
- GUI: fast-dev build, `type` the Sixel file through `chcp 65001`, screenshots.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [ ] Integration proof
- [x] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

- `cargo test -p oneterm-terminal-view`: 284 passed (2026-09-11), including
  `store_uploads_once_and_evicts_oldest` and `sixel_image_paints_once_per_frame`.
- GUI: `evidence/gui-walk.md` with three crops; the local cmd shell's `type` carried the DCS
  through ConPTY unchanged.
- Gaps: text typed over an image is hidden by it and a partially erased image is painted
  whole (`ponytail:` note in `paint_graphics`, DEC-0012 upgrade path); selection quads sit
  under images; HiDPI maps image pixels to logical pixels (upscaled on a 2x display); no
  real Sixel producer or SSH host available for the walk; XTSMGRAPHICS queries unanswered.

## Handoff

None.
