# BUG-0062 independent verification

Verifier: hostile-review agent in the isolated worktree
`.claude/worktrees/agent-a2f21587a84ab4144`, on a private branch reset to
`fix/sixel-real-cell` (`9b9d96c1`); base `main` is `47e7e93c`.
Date: 2026-09-16. Nothing committed, nothing pushed, `harness.db` opened read only,
no application process started.

## Verdict

**PASS.** Every claim the packet makes was reproduced from its own code with tests the
implementer did not write. The footprint rule, the `band_pixels` move, the three-row
footprint table, the `1 x 1` case, the half-set fallback, the native-pixel quad and the
removal of `SIXEL_VIRTUAL_CELL` all hold exactly as stated. No blocking defect. Eight
observations below, all either pre-existing behaviour the packet inherits, documentation
that promises very slightly more than the code delivers, or nits.

## Findings

### 1. Footprint rule — CONFIRMED (informational)

`crates/vt/src/graphics/placement.rs:113-118` returns `state.cell_pixels` only when both
axes are non-zero and `VIRTUAL_CELL` otherwise; `:50-51` divides with
`cells_for` = `div_ceil` saturating to `u16::MAX` (`:122-124`). `cell_h` cannot be zero at
that point, so the `div_ceil` cannot panic and the `unwrap_or(u16::MAX)` on
`:45` is unreachable rather than load-bearing.

Reproduced for eight `(pixels, cell)` pairs including non-divisible ones
(`v_footprint_is_ceil_over_the_real_cell`): `383x575 @ 9x18 -> 43x32`,
`385x577 @ 9x18 -> 43x33`, `1x1 @ 9x18 -> 1x1`, `1x600 -> 1x34`, `600x1 -> 67x1`,
`100x100 @ 7x15 -> 15x7`, `384x576 @ 18x36 -> 22x16`.

### 2. `band_pixels` and the cursor — CONFIRMED (informational)

`crates/vt/src/graphics/sixel.rs:259` carries `band * 6`; `placement.rs:45` floor-divides
it by the same `cell_h` the footprint used. The cursor keeps its column
(`v_cursor_keeps_its_column_and_lands_on_the_last_band_row`, started at column 5).

### 3. Footprint / cursor table, independently measured

Image `384 x 576` declared by `DECGRA`, 96 bands, no trailing graphics newline
(the implementer's `dragon()` shape).

| cell pixels | cols x rows | cursor row | cursor col | inside the image |
| --- | --- | --- | --- | --- |
| `(9, 18)` | `43 x 32` | 31 | kept | yes, last row |
| `(18, 36)` | `22 x 16` | 15 | kept | yes, last row |
| `(9, 0)` / `(0, 18)` / `(0, 0)` | `39 x 29` | 28 | kept | yes, last row |
| unset | `39 x 29` | 28 | kept | yes, last row |
| `(1, 1)`, image `24x24`, 4 bands | `24 x 24` | 18 | kept | yes |

Screen-edge cases, same image at `(9, 18)`:

| case | result |
| --- | --- |
| screen `80 x 10` (image taller than the screen) | footprint stays `43 x 32`, 31 line feeds, cursor ends on the bottom visible row (9), visible row 0 is the image's row 22, visible row 9 is its row 31 — the placement stays anchored across the scroll |
| screen `20` cols, image started at column 5 | `cols` clipped to 15, never wrapped; nothing stamped left of column 5 |

### 4. View quad — CONFIRMED (informational)

`crates/terminal-view/src/render/metrics.rs:88-116` returns
`(clip = footprint cells, image = CellMetrics::logical(pixels))`;
`crates/terminal-view/src/render/element.rs:369-385` paints with
`self.bounds.intersect(&clip)` as the clip and `image_bounds` as the quad.
`gpui-pre-0.3.2/src/window.rs:4664-4712` treats the first argument as a clip and cuts a
**sub-tile** out of the sprite rather than scaling it, so the `387 - 384 = 3` device-pixel
`ceil` strip is never painted, not stretched. Reproduced without a GPUI window
(`verifier_bug_0062`): native `384 x 576` at scale 1.0 and 2.0; the fallback footprint
`39 x 29` clipping the image to `351 x 522`; a `18x36` cell after placement leaving margin
(`774 x 1152` clip, `384 x 576` image); scrolled 10 rows off the top leaving `384 x 396`
anchored at the grid top; right-edge clip to `180 x 576`; interior-cell walk-back
identical at scales 1.0 / 1.5 / 2.0.

### 5. `SIXEL_VIRTUAL_CELL` had exactly one caller — CONFIRMED (informational)

On `main`: `crates/terminal-view/src/render/element.rs:373` (the rescale) and the
re-export at `crates/terminal/src/lib.rs:63`. Both gone; `grep -rn SIXEL_VIRTUAL_CELL
crates/` returns nothing on this branch. Three doc hits remain
(`BUG-0061-...md:106`, `research/api-surface.md:49` and `:225`) — historical records, not
current claims, but `BUG-0061:106` now names a symbol that no longer exists.
**Severity: nit.**

### 6. Corpus / public-api untouched — CONFIRMED (informational)

No corpus snapshot and no `public-api.*.txt` in `git diff main...HEAD --stat` (16 files).
Structurally guaranteed as well: `grep -rn "set_cell_pixels" crates/tools crates/vt/examples`
returns nothing, so every fallback consumer keeps VT340 sizing. `corpus_check` green inside
`cargo test --workspace`.

### 7. `DECGRA` can still put the cursor outside the image — PRE-EXISTING, docs overclaim

**Severity: low. Not introduced by this packet.**

`crates/vt/src/graphics/sixel.rs:254` lets the declared raster size win over the measured
extents, while the cursor follows the band count. The two disagree in both directions:

- declared `384 x 576`, only 2 bands of data, cell `(9, 18)`: footprint `43 x 32`, cursor
  row **0** — the image's own *first* row (`v_raster_larger_than_the_data_...`);
- declared `384 x 60`, 96 bands, cell `(9, 18)`: footprint `43 x 4`, cursor row **31**,
  28 rows *below* the image (`v_raster_smaller_than_the_data_...`).

`main` behaves the same in shape (`29` rows / row 0, and `3` rows / row 28), so nothing
regressed. What is new is the wording it contradicts:
`crates/vt/src/graphics/sixel.rs:26-28` ("so the cursor can never land inside the image"),
`placement.rs:27-29` and `low-level-design/graphics.md` ("the footprint and the cursor walk
divide by the same number, which is what keeps the prompt below the image"). Both hold only
when the declared height matches the bands. Suggested fix: one qualifying clause, no code
change.

### 8. The cursor lands *on* the image's last row, not below it — docs imprecision

**Severity: nit.** With the real cell and no trailing graphics newline the cursor row is
`rows - 1` — inside the picture — which is what the implementer's own test asserts
(`footprint_follows_the_embedder_cell_size`, `assert_eq!(session.cursor(), (rows - 1, 0))`).
The guide and `placement.rs` say the rule "keeps the prompt below the image". It keeps the
prompt off the *body* of the image and on its last row, which the shell's own newline then
clears; "below" is the conhost outcome, not the engine's cursor position.

### 9. A real encoder's trailing graphics newline shifts the cursor — observation

**Severity: low, informational.** `libsixel` / `img2sixel` end the payload with `-`, which
makes `bands * 6` equal the full height. Measured
(`v_a_trailing_graphics_newline_moves_the_cursor_below_the_image`), same `384 x 576`:

| cell | rows | cursor row | verdict |
| --- | --- | --- | --- |
| `(9, 18)` | 32 | 32 | below the image |
| `(18, 36)` | 16 | 16 | below the image |
| unset (VT340) | 29 | 28 | still inside (`576 / 20 = 28.8`) |

So the real-cell path is strictly better than the fallback for the payload shape real
encoders emit. Worth knowing that the shipped `dragon()` fixture omits that trailing `-`,
i.e. the tests pin the less common of the two shapes. No change required.

### 10. `Frame::placement` duplicates an existing public helper — nit

`crates/terminal-view/src/render/frame.rs:499-506` re-implements
`crates/vt/src/snapshot/state.rs:330-332` (`SnapshotState::placement`, already `pub`, and
already what `graphic_offset` calls internally) verbatim. The result is two linear scans per
image per frame instead of one, and a new method where a one-line forward on
`TerminalContent` would have been the same size. `MAX_PLACEMENTS` is small, so this costs
nothing measurable. **Severity: nit.**

### 11. Stale comment in the view's sixel test — nit

`crates/terminal-view/src/render/element_tests.rs:592` still reads "2 x 12 pixels, one
virtual 10 x 20 cell". The element now pushes the real device cell in `prepaint` before the
payload is fed, so the placement no longer uses the virtual cell. The test asserts upload /
paint counts only, so it passes either way. **Severity: nit.**

### 12. Cost of a hostile band count — pre-existing, recorded only

`crates/vt/src/graphics/sixel.rs:140` never caps `band`, and `placement.rs:75` loops
`0..rows.max(cursor_rows + 1)` issuing a line feed per row, with `cursor_rows` saturating at
`u16::MAX`. A payload of N `-` bytes therefore costs `N * 6 / cell_h` line feeds — the same
shape as `main`'s `N * 6 / 20`, about 11 % more work at an 18 px cell and far more at an
absurd 1 px cell. No embedder sets a 1 px cell and the u16 clamp bounds the total, so this
is not a new exposure. **Severity: low, pre-existing.** Verified not to panic or hang at
`(1, 1)` (`v_a_one_pixel_cell_does_not_panic_or_hang`).

### 13. Records — CONFIRMED (informational)

- Packet status ticks (`Planned` / `In progress` / `Implemented`) and the `HARNESS:PROOF`
  block (`E2E` unticked, the other four ticked) agree with the snippet's
  `e2e_proof=0, unit=integration=platform=1`.
- The snippet's `ROW` keys are exactly the 17 columns of `story` in `harness.db`
  (`id … intake_id`), and `intake` row `34` is
  `docs/spec-intakes/IN-0029-vt-engine/IN-0029.md`, so `intake_id=34` is right.
  `BUG-0062` is absent from `story`, as the packet says (coordinator inserts).
- `crates/vt/CHANGELOG.md` entry sits under `## [Unreleased]` (line 41) in `### Changed`
  (line 222).
- `low-level-design/graphics.md` placement paragraph, guide chapter 8 § "Geometry, and the
  conhost agreement", `docs/terminal-backend.md` and `crates/terminal/src/content.rs` all
  now state the new rule and the fallback; the old `device_cell / (10, 20)` sentences are
  gone.
- The E2E criterion lists both 100 % and 200 % display scale (steps 3 and 4) and says why.

## What could not be verified

1. **The E2E itself.** No application was started (owner's standing instruction), so nothing
   here compares the dragon against Windows Terminal at either scale. The packet already
   records this as `e2e_proof 0`; it remains the criterion the packet exists for.
2. **Sixel scrolling mode off (`DECSDM`, `CSI ? 80 h/l`).** Not implemented — `mode.rs`'s
   private-mode table (`:178-202`) has no `80`, and `grep -rni decsdm crates/vt` finds
   nothing. The engine is always in DEC scrolling mode, so the "mode off" half of that check
   has no code to exercise. Out of scope for this packet and not claimed by it.
3. **`P2` background fill.** Parsed and deliberately ignored (`sixel.rs:46-48`). A
   `DCS 0;1;0 q` image places identically (`v_p2_and_background_fill_are_ignored_by_design`),
   so it cannot affect a footprint; the pixel-level behaviour it would change is untested
   because it is unimplemented.
4. **A metrics change after placement, end to end.** Confirmed as a real limitation:
   `gpui-pre-0.3.2/src/platform/test/window.rs:244` hard-codes `scale_factor` `2.0`. The
   arithmetic is covered at 1.0 / 1.5 / 2.0 through `CellMetrics::image_quad` directly
   (finding 4), but no test repaints an existing placement after the cell changes.
5. **Anything about `oneterm-vt`'s published crate consumers** beyond this workspace.

## Commands run

All from the verifier worktree, `CARGO_BUILD_JOBS=2`.

```
git reset --hard fix/sixel-real-cell          # 9b9d96c1, merge-base main 47e7e93c
cargo test -p oneterm-vt                       # pass
cargo test -p oneterm-vt --no-default-features # pass
cargo test -p oneterm-vt --all-features        # pass
cargo test -p oneterm-vt --features vt-paranoid# pass
cargo test --workspace                         # pass (corpus_check included)
cargo test -p oneterm-vt --lib v_              # 12 verifier tests, pass
cargo test -p oneterm-terminal-view --lib verifier_bug_0062   # 6 verifier tests, pass
python scripts/check-doc-paths.py              # 199 paths, pass
python scripts/check-english.py                # 908 files, pass
pwsh scripts/ci-local.ps1 -Full                # see below; log kept in the scratchpad
```

`python scripts/vt-public-api.py --check --no-doc` fails in a fresh worktree with
"no rustdoc output" until `target/doc/oneterm_vt` exists; the `--check` run inside
`ci-local.ps1` builds it first and is the one that counts.

`pwsh scripts/ci-local.ps1 -Full` was run on the **pristine** branch (the verifier tests
were removed first and re-applied afterwards), and printed `ci-local: all checks passed.`
with exit code 0. Every step green, including:

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --features oneterm-app/terminal-diagnostics -- -D warnings
cargo test --workspace
cargo test -p oneterm-vt --features vt-paranoid
cargo test -p oneterm-vt --features regex
cargo build/test -p oneterm-vt --no-default-features / --all-features
cargo run -p oneterm-vt --example headless
cargo doc -p oneterm-vt --no-deps [--all-features]
python scripts/vt-public-api.py --check --no-doc       -> "public API surface unchanged (public-api.windows.txt)"
python scripts/vt-public-api.py --check-nameable --no-doc -> "every type in a public signature is nameable"
python scripts/vt-public-api.py --diff-platforms       -> ok
rustdoc self-containment (crates/vt/src, crates/vt/docs/guide)
python scripts/verify-dependency-graph.py              -> 20 packages
python scripts/check-doc-paths.py                      -> 199 paths
python -m unittest scripts/test_check_english.py       -> OK
python scripts/check-english.py                        -> 909 files
python scripts/completion-catalog.py validate
python scripts/third-party-notices.py --check
cargo deny check licenses bans advisories              -> advisories ok, bans ok, licenses ok
```

No snapshot file (corpus or `public-api.*.txt`) is edited in `git diff main...HEAD`.

## Verifier tests (this worktree only, uncommitted)

- `crates/vt/src/graphics/graphics_tests.rs` — appended block
  "VERIFIER: BUG-0062 independent checks", 12 tests prefixed `v_`.
- `crates/terminal-view/src/render/metrics.rs` — appended module `verifier_bug_0062`,
  6 tests prefixed `v_`.

Both blocks are also kept at
`<scratchpad>/verifier_block.rs`, `verifier_block2.rs` and `metrics_block.rs`.
