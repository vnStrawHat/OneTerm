# Work: Paint Sixel images in the terminal grid

ID: US-0067
Intake: IN-0028
Created: 2026-09-11

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [ ] In progress
- [ ] Implemented
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

- [ ] In scope: `image` workspace dependency (core types only); `Frame::graphics()`,
  `Cell.graphic`; `GraphicStore` in `RenderState`; `paint_graphics` pass; `set_cell_size`
  call from `prepaint`; IN-0018 HLD paint row, `docs/osc-sequences-checklist.md`,
  `docs/agents/dependencies.md`, `README.md`; GUI walk with a generated Sixel file.
- [ ] Out of scope: per-row image bands (text over image, partial erase); selection colour
  over images; a setting to disable graphics.

## Acceptance

- [ ] A frame whose snapshot carries a new image registers it once; the same id in later
  frames is not re-uploaded (store test).
- [ ] The 65th image (or the first over 64 MB) evicts the oldest and the evicted id is no
  longer painted (store test).
- [ ] A visible image contributes exactly one `paint_image` call per frame, at the origin
  derived from its first visible cell (paint stats test with a fake session whose grid holds
  graphic cells).
- [ ] GUI (Windows): a generated 96 x 48 Sixel test card prints in a local shell at the cursor,
  the prompt returns below it, scrolling moves it, `cls` removes it
  (`evidence/US-0067-*.png`).
- [ ] `pwsh scripts/ci-local.ps1` green.

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

Pending.

## Context

- `gpui::RenderImage::new` takes `image::Frame`s in BGRA order (`elements/img.rs` swaps
  channels after decoding); `Window::paint_image(bounds, corners, Arc<RenderImage>, 0, false)`
  and `Window::drop_image` release the atlas tile.
- Polychrome sprites paint after glyphs inside a layer (`scene.rs` primitive order), so the
  image pass can run anywhere in the grid layer; it runs after the quad passes for clarity.
- `GridPainter` already has the geometry (`cell_origin`) and `FrameStats`; a new `images`
  counter feeds the tests.

## Plan

- [ ] `Cargo.toml`: `image = { version = "0.25", default-features = false }`; terminal-view
  dependency.
- [ ] `frame.rs`: `GraphicRef`, `Cell.graphic`, `Frame::graphics()`.
- [ ] `state.rs`: `GraphicStore` (insert, get, evict + `drop_image`), `FrameStats.images`.
- [ ] `element.rs`: `set_cell_size` on metrics change; `paint_graphics` pass; tests.
- [ ] Docs, CI, GUI walk (Python Sixel encoder in the scratchpad writes the test card).

## Decisions

- `DEC-0012`.

## Verification Plan

- `cargo test -p oneterm-terminal-view` (store + paint tests).
- `pwsh scripts/ci-local.ps1`.
- GUI: fast-dev build, `type` the Sixel file through `chcp 65001`, screenshots.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Pending.

## Handoff

None.
