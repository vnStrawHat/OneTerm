# BUG-0078 adversarial verification

- Subject: `84ae4be0` `fix(terminal-view): glyph cache no longer preallocates a 24 MB table per
  view` and `2a59ffc9` (docs), on top of `main` @`de845969`.
- Packet: [`../BUG-0078-glyph-cache-preallocates-its-table.md`](../BUG-0078-glyph-cache-preallocates-its-table.md)
- Date: 2026-09-25. Host: Windows 11, MSVC, `CARGO_BUILD_JOBS=4`, worktree-local target dir.
- `main` binaries and tests were built from `de845969`'s `crates/terminal-view/src/render/`
  checked out over the fix (`git diff de845969 2a59ffc9 -- crates Cargo.toml Cargo.lock` lists
  only those five files), then restored.

## Verdict: PASS

The cache now holds the very `Arc<LineLayout>` that the old `ShapedLine` wrapped, and the
painter reads the same fields it read before. The terminal area renders pixel-identical on
`main` and on the fix, with underline, strikethrough, wavy underline, ligatures, wide CJK,
fallback and emoji glyphs, bold and italic, combining marks, and the cursor over a ligature.
The memory figures reproduce within 0.2 MB. The new test fails on `main`'s layout. The gate
is green on `2a59ffc9`. Findings F3, F4 and F8 are Low, and none blocks the merge.

## Findings

### F1 (Info): rendering parity by construction

- gpui-pre 0.3.3 `WindowTextSystem::shape_line_by_hash` (`text_system.rs:464`) builds a
  decoration `SmallVec` from the runs. It then calls `self.layout_line_by_hash(text_hash,
  text_len, font_size, runs, force_width, ..)` with the same arguments, and returns
  `ShapedLine { layout, text: "", decoration_runs }`. `ShapedLine` derefs to its
  `Arc<LineLayout>` (`text_system/line.rs:43`), and `len()` and `width()` return
  `layout.len` and `layout.width`. The fix calls `layout_line_by_hash` with the same six
  arguments, so a miss gets the same layout object, and a hit returns it.
- The painters did not change: the diff only changes the parameter type of
  `paint_shaped_line` (`element.rs:484`). The body of `CursorPaint::paint` (`cursor.rs:171`)
  is unchanged. Both already iterated `line.runs[].glyphs[]` through `Deref` and called
  `paint_glyph` and `paint_emoji` with `run.font_id`, `glyph.id` and `glyph.position`. They
  never called `ShapedLine::paint`, and they never read `decoration_runs` or `text`. The
  cache's `TextRun` has `underline: None` and `strikethrough: None`, so those runs held nothing
  anyway. Underline and strikethrough come from `RowPlan.decorations` through
  `GridPainter::paint_decorations` (`window.paint_underline` and
  `window.paint_strikethrough`). Backgrounds come from `BgSpan` quads. Neither path touches a
  shaped line. Font fallback glyph ids, emoji flags and the per-run `font_id` are fields of
  `ShapedRun` inside `LineLayout`, so they are carried over. Bidi does not apply: gpui's line
  layout has no bidi pass and the terminal grid is LTR.
- **Empirical check.** One launched instance per build, with a private `USERPROFILE` and a
  seeded `terminal.json`: Fira Code 15 px, ligatures on, gutter on, cursor blink off. The
  instance runs `prompt $G`, then `type`s a UTF-8 sample. Then it types `x=>y` and presses
  Left three times, which puts the block cursor on the `=>` ligature (`CursorPaint.glyph`). A
  `PrintWindow` capture of 1280x800 was taken twice per build, and the pixels were compared
  byte by byte:

  | Pair | Terminal (x 150..830, y 66..760) | Gutter (x 0..150) | Whole window |
  | --- | --- | --- | --- |
  | main r1 vs fix r1 | **0 px** | 2204 px, all in x 77..84 | 2486 px |
  | main r2 vs fix r2 | **0 px** | 2204 px, all in x 77..84 | 2440 px |
  | main r1 vs main r2 (noise) | 0 px | 5442 px | 5588 px |
  | fix r1 vs fix r2 (noise) | 0 px | 5442 px | 5586 px |

  The gutter difference is the seconds digit of the per-line `[hh:mm:ss]` clock, since the runs
  were taken at different times. The rest of the window difference is the status-bar clock and
  `MEM` figure. The gutter labels themselves are the `GutterLabel.line` holder, and they render
  identically apart from the digits that changed. The captures are
  [`BUG-0078-verify-render-main.png`](BUG-0078-verify-render-main.png) and
  [`BUG-0078-verify-render-fix.png`](BUG-0078-verify-render-fix.png).

### F2 (Info): lifetime and invalidation unchanged

- Before the fix, the cache also held an `Arc<LineLayout>`, the one inside `ShapedLine.layout`.
  So how long a layout outlives gpui's two-frame `LineLayoutCache` (`finish_frame` swaps and
  clears, `line_layout.rs:559`) is exactly as before. Only the 3 KB `SmallVec` wrapper is gone.
- There is nothing in a layout that can go stale. `FontId`s come from the append-only
  `font_ids_by_font` map and are never freed. The glyph atlas is keyed at paint time by
  `RenderGlyphParams { font_id, glyph_id, font_size, subpixel_variant, scale_factor, .. }`, so
  a DPI change re-rasterizes from the same layout. Layout positions are in logical pixels.
- Invalidation code is untouched. `RenderState::ensure_fonts` clears the cache when the `Font`
  value or the font size changes (`state.rs:211`), which covers family, features, fallbacks,
  weight and ligatures. A theme change touches only colours, which are applied at paint time
  from `ColorSpan`s. A DPI change rebuilds every row plan (`PlanCache.cell`), and the plans
  re-read the cache.

### F3 (Low, pre-existing, not a regression): `RunKey` keys `forced`, not the forced width

`RunKey.forced` is a `bool`. gpui's own key holds the `force_width` value. A cell-width change
that keeps the `Font` value and size (a monitor with a different scale that snaps the cell
differently, or a `cell_width` override) does not clear the cache. The runs that were shaped
with the old `force_width` are then reused. The text painter re-anchors every glyph to its cell
(`CellAnchor`), so only offsets inside a cell carry over. This is identical on `main`. It is
recorded as a gap, not a finding against this packet.

### F4 (Low, doc nit, pre-existing): LLD eviction pseudo-code

The eviction bound is unchanged: the `retain` block, `CAPACITY = 4096` and `GRACE = 2` are
byte-identical to `main`. The fix changes only the entry type. The comment in
`render-pipeline.md` says `insert ; if len > capacity { retain .. }`, but the code runs
`retain` *before* the insert when `len >= capacity`. The new paragraph under it states this
correctly ("when an insert finds the map at the cap"). The O(n) `retain` on every miss while
the map is full of recent entries is on `main` too. The packet already lists it as a gap.

### F5 (Info): memory reproduced

The runs used `measure.ps1`, release builds, and a private `USERPROFILE`. `-Mode S2` ran 3
times per build, interleaved main/fix, and `-Mode Full` ran once per build. Values are
privws / commit in MB. Raw rows were written to a scratch CSV and are not appended to
`research/measurements.csv`.

| | S1 | S2 (3 tabs) | Commit per added tab | S3 (Full) |
| --- | --- | --- | --- | --- |
| main r1 / r2 / r3 | 49.9/182.7, 49.7/182.7, 49.6/182.4 | 54.2/236.4, 53.8/236.1, 53.5/235.8 | 26.85, 26.70, 26.70 | 86.8 / 245.2 |
| fix r1 / r2 / r3 | 49.6/158.8, 49.5/158.7, 49.4/158.5 | 53.6/164.9, 53.7/165.0, 53.4/164.6 | 3.05, 3.15, 3.05 | 62.5 / 173.6 |
| Δ (mean) | −0.2 / **−24.0** | −0.3 / **−71.3** | **26.75 → 3.08** | **−24.3** / −71.6 |

These agree with the packet: S1 commit 157-158 MB, +3.1 MB per tab, and S3 privws about
−22 MB. No first-launch outlier appeared here, because both binaries had already run for the
render captures.

### F6 (Info): frame time

`cargo test -p oneterm-terminal-view --lib frame_time_under_output -- --ignored --nocapture`
(dev profile, 2 runs each):

| | flood avg / p50 / p95 (ms) | idle avg / p95 (ms) | rows planned |
| --- | --- | --- | --- |
| main | 34.8 / 34.4 / 44.5; 35.3 / 34.3 / 49.7 | 5.3 / 6.2; 6.0 / 7.2 | 13,868 of 17,733 |
| fix | 31.9 / 30.0 / 42.2; 32.1 / 30.7 / 42.6 | 5.5 / 6.4; 5.9 / 7.6 | 13,868 of 17,733 |

There is no regression. Flood is about 8% faster, and idle is within noise.

### F7 (Info): tests

- `cargo test -p oneterm-terminal-view` ×3: `389 passed; 0 failed; 3 ignored`, each time.
- The new test prints `48 B/bucket, 8192 buckets, 401408 B table + 262144 B layouts` on the
  fix.
- **Discrimination.** The test was transplanted into `main`'s `glyphs.rs`. The only
  adaptation is the pointer identity, which goes through `&**ShapedLine`. There it reports
  `bucket = 3024 B` and `new() capacity = 7168`, full at `24780800 B table`, and it fails:
  `assertion left == right failed: nothing is allocated up front`. The `bucket <= 64` and
  `< 1 MiB` assertions would fail as well. The hit-identity assertion passes on both builds,
  as expected, since hits were already `Arc`-shared.

### F8 (Low): records

- The packet follows the template up to *Evidence and Gaps*, but it has no `## Handoff`
  section. Add it at close (merge state, next owner).
- The proof block ticks **Platform proof**, but only local Windows ran; CI has not run on this
  branch. `BUG-0077` kept `platform_proof = 0` until CI. The proposed row does the same.
- `Created: 2026-09-25`, Owning Docs Reviewed, the Documentation Action and the Reconciliation
  are present and accurate. The IN-0018 HLD rows and the LLD edits (row plan, shape step,
  glyph cache block, the new paragraph on value and bound, cursor struct, allocation plan) match
  the code. The "48 bytes" and "64 bytes of `LineLayout` header" figures match `size_of` on
  this toolchain. The IN-0045 intake checkbox and link were updated.

## Gate

The gate was `pwsh scripts/ci-local.ps1` on `2a59ffc9`. The first attempt, with
`CARGO_BUILD_JOBS=4`, stopped in `cargo test --workspace` with
`rustc-LLVM ERROR: out of memory` while compiling `oneterm-ssh`. That was host memory
pressure from parallel agent builds, not a code failure. The rerun with
`CARGO_BUILD_JOBS=2` passed every step (terminal-view lib: 389 passed, 0 failed,
3 ignored). The final line was:

```
ci-local: all checks passed.
```

## Gaps

- The pixel check covers one font (Fira Code), one scale factor, cmd.exe and Windows
  DirectWrite. macOS and Linux shaping were not run. By construction they take the same
  code path.
- A font-size, DPI or theme change was checked by code reading (F2), not by a live switch.
- S6 (agent load) was not rerun; the packet's S6 rows stand. S3 is a single run per build.
- F3 (the cache is not keyed on the forced width) and the O(n) `retain` at the cap predate
  this packet and remain open.
