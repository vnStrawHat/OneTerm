# Work: Sixel decoding and grid placement in the terminal engine

ID: US-0066
Intake: IN-0028
Created: 2026-09-11

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [x] In progress
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

A Sixel DCS sequence fed to the `Term` becomes an RGBA image plus cell-anchored references in
the grid at the cursor, the cursor moves below the image, the snapshot carries the new image
out once, and DA1 advertises Sixel.

## Scope

- [x] In scope: `vendor/patches/vte/0002` (DCS hooks); `vendor/patches/alacritty_terminal/0003`
  (`graphics.rs`, `CellExtra.graphic`, `Term` hooks, `take_graphics`, `set_cell_size`, DA1);
  `TerminalContent.graphics`; `TerminalInput::set_cell_size` (required, per the "no silent
  defaults" rule in `docs/terminal-backend.md` § 9) + model + session macro + fake session;
  tests in `crates/terminal`; `vendor/README.md`, `docs/terminal-backend.md` § 5.2 / § 9.
- [x] Out of scope: rendering (US-0067); XTSMGRAPHICS; DECSDM (sixel display mode); Kitty /
  iTerm2 protocols; per-Term image memory limits (pending images leave the `Term` at the next
  snapshot).

## Acceptance

- [x] Decoder cases from the LLD pass (colours, repeat, HLS, raster crop, clamp, empty):
  `sixel_tests::{decodes_colors_columns_and_carriage_return, repeat_hls_and_partial_columns,
  raster_attributes_fix_the_size_and_a_new_band_moves_down, width_is_clamped,
  empty_and_non_sixel_dcs_place_nothing}`.
- [x] Placement cases from the LLD pass (cells, cursor end position, clip on the right,
  scroll into history, `CSI 2J`, overwrite, RIS, single `take_graphics`, cell size):
  `sixel_tests::{places_cells_and_moves_the_cursor_below_the_image,
  image_wider_than_the_grid_is_clipped_on_the_right, image_at_the_bottom_scrolls_into_history,
  erase_and_overwrite_drop_the_reference, cell_size_changes_the_row_count}`.
- [x] `CSI c` answers `\x1b[?62;4c` (`primary_device_attributes_advertise_sixel`).
- [x] `bash vendor/refresh.sh --check` passes with the new patches (2026-09-11).
- [x] `pwsh scripts/ci-local.ps1` green (2026-09-11, run once for the intake with US-0067 on
  the same branch).

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0028-sixel-graphics/high-level-design.md` and
  `low-level-design/vendor-graphics.md` — the design.
- `vendor/README.md` § 2 / § 4 — patch list and generation procedure; must list the new
  patches.
- `docs/terminal-backend.md` § 5.2 (snapshot contents) and § 9 (`TerminalSession` trait) —
  must mention `graphics` and `set_cell_size`.
- `docs/agents/dependencies.md` § 1 rule 6 — forks are never hand-edited; followed.
- `docs/PROJECT.md` — invariant "vendored trees == pristine + patches"; followed.

### Documentation Action

Update required: `vendor/README.md`, `docs/terminal-backend.md`.

Reason: both enumerate the fork deltas and the session contract.

### Reconciliation

Changed: `vendor/README.md` § 2 (patches `vte/0002`, `alacritty_terminal/0003`),
`docs/terminal-backend.md` § 5.2 (snapshot `graphics`) and § 9 (`set_cell_size`).

## Context

- The pump already runs `Processor::advance(&mut term, bytes)` under the lock; DCS bytes are
  delivered one `put` call each, so the decoder must be cheap per byte (no allocation per
  byte).
- `Term::linefeed` and `carriage_return` are `Handler` methods on `Term`; the placement calls
  them so scroll region and scrollback rules are reused.
- The `serde` feature is on by default for the vendored crate: new cell types derive
  `Serialize`/`Deserialize` under `cfg_attr`.

## Plan

- [x] Materialise pristine `vte` and `alacritty_terminal` in a scratch git repo, apply the
  series, implement, export patches, `refresh.sh --check`.
- [x] `oneterm-terminal`: snapshot field, trait method, model, macro, fake session, tests.
- [x] Docs.
- [x] `ci-local` (run once for the intake after US-0067).

## Decisions

- `DEC-0012` — cell-anchored graphics in the `Term`.

## Verification Plan

- `cargo test -p oneterm-terminal sixel`.
- `bash vendor/refresh.sh --check`.
- `pwsh scripts/ci-local.ps1`.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

- `cargo test -p oneterm-terminal`: 256 passed (11 `sixel_tests`), 2026-09-11. The first
  cut of `image_at_the_bottom_scrolls_into_history` expected the image rows in history; a
  linefeed at the bottom pushes the *top* lines out, so the rows end at lines 0-1 (test
  corrected, behaviour matches xterm).
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `bash vendor/refresh.sh --check`: both crates verified.
- Gaps: P2 (background select) is ignored, untouched pixels are always transparent; Pan/Pad
  aspect ratio ignored; XTSMGRAPHICS not answered; no test for a hostile stream without
  `ST` beyond the dimension clamp (bounded by construction).

## Handoff

None.
