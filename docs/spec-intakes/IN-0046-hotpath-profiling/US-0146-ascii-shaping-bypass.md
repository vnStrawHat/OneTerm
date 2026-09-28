# Work: plain ASCII runs skip text shaping

ID: US-0146
Intake: IN-0046
Created: 2026-09-28

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

- Change type: maintenance (performance; no pixel change)
- Risk lane: normal. Medium-high in the research table because it builds GPUI's
  `LineLayout` by hand; the design below limits that to fonts where the hand-built layout
  reproduces GPUI's own shaping bit for bit on a reference line, and falls through
  everywhere else.
- Spec Intake, when required: [`IN-0046`](IN-0046.md), candidate 4 of
  [`research/hotpath-evaluation.md`](research/hotpath-evaluation.md) § 5

## Outcome

A glyph-cache miss for a run that cannot need shaping builds its `LineLayout` from a
per-font table of glyph ids instead of calling GPUI's text system. Such a run is printable
ASCII only (0x20-0x7E), in a font whose features list turns `calt` off and turns nothing on
(ligatures off), and the font passed a one-time check that it lays out every printable
ASCII char as one glyph, in one run of the primary face, at a constant advance, with no
offsets. The terminal paints the same pixels as before.

## Scope

- [x] In scope:
  - Option (a) of the task: `LineLayout`, `ShapedRun` and `ShapedGlyph` have public
    fields in `gpui-pre` 0.3.7, so the layout is built by hand; no GPUI patch
    (`docs/PROJECT.md`: no `[patch]`, no forked source, which covers `gpui-pre` too).
  - A per-`FontKey` table filled once from one shaped reference line (the 95 printable
    ASCII chars plus common ligature probes), with a self-check that rebuilds the reference
    line by hand and compares it to GPUI's layout field by field; a failing check disables
    the fast path for that font.
  - `FontKey` gains the "ligatures off" flag, so the run key and the table key both carry
    it.
  - GPUI's private force-width pass reproduced for the hand-built layout.
  - A `FrameStats` counter for fast-path layouts.
  - Tests; before/after hotpath measurements; PrintWindow pixel comparison.
- [x] Out of scope: ligatures-on runs (the default setting, see Gaps); non-ASCII runs;
  GPUI's own cache; the other IN-0046 candidates.

## Acceptance

- [x] On the real platform text system, the fast path's layout equals
  `layout_line` / `layout_line_by_hash` output (glyph ids, byte indices, x/y positions,
  font id, width, forced and unforced) for the printable ASCII line and a few ASCII lines.
- [x] Each fall-through condition reaches the shaper (counter): a non-ASCII char, a
  combining mark, a control char, DEL, ligatures on (`calt` on or absent), another feature
  enabled, and a font whose check failed.
- [x] The table and run keys include everything the fast path depends on: font id comes
  from the table built for the exact `FontKey` (family, size, weight, slant), and the
  ligature flag is in `FontKey`.
- [x] PrintWindow capture of an ASCII-heavy screen, same font and size, before and after:
  0 differing pixels in the terminal area.
- [x] hotpath before/after table (flood with ligatures off, 2-tab TUI,
  `frame_time_under_output`) in this packet.
- [x] Gates green: fmt, clippy with and without `hotpath-profiling`,
  `cargo test -p oneterm-terminal-view`, `check-doc-paths`, `check-english`, full
  `ci-local`.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0018-rebuild-terminal-render-engine/low-level-design/render-pipeline.md`
  § Glyph cache — the cache's key and miss path.
- `docs/spec-intakes/IN-0027-font-fallbacks-ligatures/high-level-design.md` and
  `US-0065-ligatures.md` — `calt` follows the ligature switch; the painter anchors each
  glyph at its cell.
- `crates/terminal-view/src/render/glyphs.rs` module rustdoc.
- `docs/gui-layout.md` — not relevant (no layout change).

### Documentation Action

Update required: render-pipeline LLD § Glyph cache (the fast path, its conditions and its
self-check); `glyphs.rs` rustdoc; `IN-0046.md` candidate list (US-0146 created).

Reason: the cache's miss path changes.

### Reconciliation

Changed: render-pipeline LLD § Glyph cache (struct sketch, miss path, new "ASCII fast
path" paragraph); `glyphs.rs` module rustdoc and item docs; `FrameStats` field docs;
`IN-0046.md` (US-0146 linked); `research/hotpath-measure.ps1` (`-NoLigatures`). The
IN-0027 docs stay correct: the ligature setting and the cell-anchored painter are
unchanged, and ligatures-on runs take the old path.

## Context

- `GlyphCache::shape` (`crates/terminal-view/src/render/glyphs.rs`) calls
  `WindowTextSystem::layout_line_by_hash` on a miss. That probes GPUI's frame caches by a
  linear scan over their entries, then shapes with DirectWrite and applies
  `apply_force_width_to_layout` (private). Research: 434 ms of `PlanCache::update`'s 949 ms
  in the 300k-line flood, 6.6 us per call.
- DirectWrite's glyph run: `x = pen + advanceOffset`, `y = -ascenderOffset`,
  `pen += advance`, `index` = UTF-8 byte of the cluster.
- The grid painter re-anchors every glyph at its cell (`CellAnchor`), so for a one-glyph
  cell only the glyph id, the `y` and the font id reach the pixels; the gutter labels
  (`forced = false`, no cell map) and the cursor read `x` as shaped.
- Ligatures are on by default (`FontConfig::ligatures`), so the fast path only works for a
  user who turned them off.

## Plan

- [x] `FontKey.plain`; `AsciiGlyphs` table + self-check; fast path in `shape`.
- [x] Tests (real text system on Windows; fall-through counters; key coverage).
- [x] Measurements before/after; pixel comparison.
- [x] Docs; gates; commit.

## Decisions

None.

## Verification Plan

- Unit: `cargo test -p oneterm-terminal-view` (fast path vs real shaping on the Windows
  DirectWrite text system; fall-through counters; key tests).
- Measurement: release `--features hotpath-profiling` before (main) and after,
  `hotpath-measure.ps1 -Mode Flood` and `-Mode Tui` with `font.ligatures: false`;
  `frame_time_under_output` (fast-dev) before/after.
- Pixel: the before and after builds, same private profile, an ASCII-heavy screen,
  PrintWindow capture, pixel diff of the terminal area.
- Gates: `cargo fmt --all -- --check`, clippy with and without `hotpath-profiling`,
  `python scripts/check-doc-paths.py`, `python scripts/check-english.py`, full
  `pwsh scripts/ci-local.ps1`.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [x] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

### What changed

- `crates/terminal-view/src/render/glyphs.rs`: `FontKey.plain`; `GlyphCache.ascii`
  (`HashMap<FontKey, Option<AsciiGlyphs>>`, empty until the first ASCII miss of a plain
  font, cleared with the cache); `AsciiGlyphs::{read, layout}`; `apply_force_width` (GPUI's
  private pass, copied); `same_layout`. `shape` tries the fast path on a miss and falls
  back to `layout_line_by_hash`.
- `FrameStats.ascii_layouts` (a subset of `shape_calls`, which still counts every miss);
  printed in the diagnostics log line.
- Test-only: `gpui_platform` as a Windows dev-dependency of `oneterm-terminal-view`
  (GPUI's headless Windows platform carries the no-op text system, so the test opens the
  real one); `frame_time_under_output` now runs with ligatures off.

Why option (a): `LineLayout`, `ShapedRun`, `ShapedGlyph` and `GlyphId` have public fields
in `gpui-pre` 0.3.7, so no patch is needed (`docs/PROJECT.md` forbids `[patch]` and forked
source for every third-party crate, `gpui-pre` included). A per-character cache (b) would
still have to assemble a `LineLayout`, so it is (a) with a bigger key. The table costs one
shaped line per font variant (about 400 B each, at most four per view).

Why the self-check instead of trusting a cmap lookup: DirectWrite reports
`x = pen + advanceOffset`, `y = -ascenderOffset`, `pen += advance` per glyph; rebuilding
the reference line and comparing every field proves, per font, that those offsets are 0,
the advance is constant, there is one face and nothing ligates among the probes. Lilex
(the default font), Consolas and Courier New pass in every variant tried; Segoe UI
(proportional) fails and keeps shaping.

### Tests

`cargo test -p oneterm-terminal-view`: 393 passed, 3 ignored (the measurement tests). New:

- `ascii_fast_path_matches_directwrite` (Windows): real DirectWrite; Lilex regular and bold
  (loaded from `crates/app/fonts`), Consolas regular and bold italic, Courier New; 13 and
  15 px; unforced and forced at 8 and 9.5 px (both branches of the force-width pass); the
  printable line plus six ASCII lines; `same_layout` against `layout_line_by_hash`; every
  run counted as a fast-path layout; 94 distinct glyph ids (a real shaper, not the stub);
  Segoe UI's table is `None` and its run is shaped.
- `ascii_fast_path_falls_through_to_the_shaper`: counters for plain ASCII (fast), a hit,
  non-ASCII, a combining mark, a control char, DEL, no features list, `calt=1`,
  `calt=0 + ss01=1`, and a font whose check failed; one table per `FontKey`; `clear`
  drops the tables.
- `font_key_plain_needs_calt_off_and_nothing_on`, `reference_starts_with_printable_ascii_in_order`.

### Pixel comparison

`before` = main `42c44b0f`, `after` = this branch; both release `hotpath-profiling`
builds, same private profile (`font.ligatures: false`, cursor blink off, gutter off), 1280x800,
cmd.exe `type` of an ASCII-heavy sample (the 94 printable chars, code, paths, URLs,
hashes, bold, italic, bold italic, colours inside one run, underline, strikethrough,
inverse, dim, ligature pairs, plus non-ASCII and box-drawing lines that fall through).
Two `PrintWindow` captures per build:

| Pair | Terminal and docks (y 34..765) | Whole window |
| --- | ---: | ---: |
| before 1 vs after 1 | **0 px** | 336 px, all in y 773..781 |
| before 2 vs after 2 | **0 px** | 269 px, all in y 773..781 |
| before 1 vs before 2 (noise) | 0 px | 136 px, y 773..781 |
| after 1 vs after 2 (noise) | 0 px | 188 px, y 773..781 |

y 773..781 is the status bar (clock, CPU, MEM). Captures:
[`evidence/US-0146-pixel-before.png`](evidence/US-0146-pixel-before.png),
[`evidence/US-0146-pixel-after.png`](evidence/US-0146-pixel-after.png).

### Measurements

Machine: the IN-0046 machine, Windows 11, toolchain 1.96.0, release (fat LTO) with
`hotpath-profiling`; another agent was building in parallel. `hotpath-measure.ps1
-NoLigatures` (both sides, since the fast path needs ligatures off). Raw reports:
[`research/raw/us-0146/`](research/raw/us-0146/).

300k-line flood (two runs per side):

| Site | before (r1 / r2) | after (r1 / r2) | change |
| --- | ---: | ---: | ---: |
| `GlyphCache::shape` avg | 6.43 / 6.64 us | 0.45 / 0.33 us | -94 % |
| `GlyphCache::shape` total | 416 / 437 ms (65k calls) | 30 / 21 ms (67k / 64k calls) | -94 % |
| `build_row_plan` avg | 21.8 / 22.4 us | 10.6 / 9.9 us | -54 % |
| `PlanCache::update` avg | 961 / 987 us | 590 / 540 us | -42 % |
| `TerminalElement::prepaint` avg | 983 / 1010 us | 620 / 562 us | -41 % |
| `TerminalElement::prepaint` total | 943 / 981 ms | 611 / 529 ms | -41 % |
| `TerminalElement::paint` avg | 58 / 59 us | 69 / 61 us | noise |

Two-tab TUI, 180 s (one run per side):

| Site | before | after |
| --- | ---: | ---: |
| `GlyphCache::shape` avg / total | 359 ns / 416 ms (1.157 M calls) | 162 ns / 188 ms (1.157 M calls) |
| `build_row_plan` avg | 10.95 us | 9.53 us |
| `PlanCache::update` avg | 526 us | 485 us |
| `TerminalElement::prepaint` avg | 552 us | 518 us |

The TUI mostly hits the cache, so only the misses got cheaper; the `update` and `prepaint`
changes there are within the run-to-run noise US-0144 measured (about 15 %).

`frame_time_under_output` (fast-dev, 3 runs per side, run back to back): flood avg
1830 / 2039 / 1886 us before, 1960 / 1821 / 2067 us after; idle 286 / 456 / 275 us before,
298 / 282 / 278 us after. No measurable change: the test runs on GPUI's no-op text system,
whose "shaping" is a loop over chars, and its repeated line hits the cache. It is not the
right instrument for this change; the hotpath flood is.

### Gates

- `cargo fmt --all -- --check`: clean.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean; with
  `--features oneterm-app/hotpath-profiling`: clean.
- `python scripts/check-doc-paths.py`, `python scripts/check-english.py`,
  `python scripts/verify-dependency-graph.py`, `python scripts/third-party-notices.py --check`:
  pass.
- Full `pwsh scripts/ci-local.ps1` (2026-09-28, after deleting `target/release`):
  `ci-local: all checks passed.`

### Gaps

- **Ligatures are on by default** (`FontConfig::ligatures = true`), so the default setting
  never takes the fast path; the gain above is for users who turn ligatures off. A calt-on
  fast path would need to know which ASCII sequences a font's `calt` rewrites, which the
  public text-system API does not expose.
- The check probes a fixed list of ligature and kerning candidates; a font that ligates or
  kerns an ASCII pair outside the probes with `calt` off would get per-char glyphs from the
  fast path. For grid text only the glyph ids differ (the painter anchors glyphs at cells);
  no such monospace font was found. Gutter labels and the cursor read `x` directly.
- Linux and macOS: the real-shaper test is Windows-only (DirectWrite); elsewhere the
  self-check decides per font at run time, unmeasured.
- `RunKey.forced` keys the flag, not the forced width (BUG-0078 F3, unchanged).
- One TUI run per side; the flood has two.

## Handoff

None.
