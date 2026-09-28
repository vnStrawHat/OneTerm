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
- [x] Reopened (acceptance rework)
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
(ligatures off), and the font passed a one-time check that it lays out every ordered pair
of printable ASCII chars as one glyph per char, the same glyph in every context, in one run
of the primary face, at a constant advance, with no offsets. The terminal paints the same
pixels as before.

## Scope

- [x] In scope:
  - Option (a) of the task: `LineLayout`, `ShapedRun` and `ShapedGlyph` have public
    fields in `gpui-pre` 0.3.7, so the layout is built by hand; no GPUI patch
    (`docs/PROJECT.md`: no `[patch]`, no forked source, which covers `gpui-pre` too).
  - A per-`FontKey` table filled once from one shaped reference line (every ordered pair
    of the 95 printable ASCII chars plus longer ligature probes), with a self-check that rebuilds the reference
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
- [x] Rework (verify [`evidence/US-0146-verify.md`](evidence/US-0146-verify.md), FAIL on
  F1): a font with a contextual rule on any two-char ASCII context is rejected (Fira Code
  on real DirectWrite; a synthetic pair swap and synthetic probe ligatures in a unit
  test); the Fira Code PrintWindow pair against main shows 0 px in the terminal area.

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

Rework: the LLD paragraph now names the features that stay on with `calt = 0`, the pair
walk, what fails the check only when the reference shows it, the unproven longer contexts
and locales, the run-split note (verify F2) and the force-width re-diff;
`docs/agents/dependencies.md` § 1 and § 2 carry the test-only `gpui_platform` exception
(F3) and § 4 the re-diff step for the copied pass (F6).

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

None. The `gpui_platform` test-only exception is recorded in the owning policy
(`docs/agents/dependencies.md` § 2) rather than as a decision record.

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
- [ ] Platform proof
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
  real one). `frame_time_under_output` still measures the default path (ligatures on); the
  ligatures-off figures below came from a temporary local edit.

Why option (a): `LineLayout`, `ShapedRun`, `ShapedGlyph` and `GlyphId` have public fields
in `gpui-pre` 0.3.7, so no patch is needed (`docs/PROJECT.md` forbids `[patch]` and forked
source for every third-party crate, `gpui-pre` included). A per-character cache (b) would
still have to assemble a `LineLayout`, so it is (a) with a bigger key. The table costs one
shaped line per font variant (about 400 B each, at most four per view).

Why the self-check instead of trusting a cmap lookup: DirectWrite reports
`x = pen + advanceOffset`, `y = -ascenderOffset`, `pen += advance` per glyph; rebuilding
the reference line and comparing every field proves, per font, that those offsets are 0,
the advance is constant, there is one face and nothing in the reference is substituted.
Lilex (the default font), Consolas and Courier New pass in every variant tried; Segoe UI
(proportional) and Fira Code fail and keep shaping.

### Rework after the verify FAIL (2026-09-28)

Verify: [`evidence/US-0146-verify.md`](evidence/US-0146-verify.md). With `calt = 0`,
DirectWrite still applies `liga` / `clig` (GPUI turns them on), `kern` and the required
`ccmp`, `locl`, `rlig`, `rclt`. Fira Code's `ccmp` turns a backtick after `A-Z` or a
backtick into `grave.case`; the first reference held only `` _` ``, so Fira Code passed and
painted 176 px differently from main.

- F1: the reference is now an order-2 de Bruijn walk through every ordered pair of the 95
  printable chars (9,026 chars), then the longer probes
  (`fi fl ff ffi ffl -> => != == <= >= === !== <=> ==> <!-- --> ::= ... www`). The check
  (`AsciiGlyphs::check`, split out of `read` so it can be fed a synthetic layout) fills the
  ids from every occurrence of each char and still compares the whole layout, so a char
  whose glyph differs in any pair fails. Cost, measured on real DirectWrite in the debug
  test binary (GPUI at opt-level 3), once per font variant: **2.1-3.3 ms** (Lilex, Consolas,
  Courier New at 13 and 15 px), **2.4-4.5 ms** for Fira Code, which is rejected.
- F3: the smaller change is the policy amendment (`dependencies.md` § 1 and § 2): the
  equality test reads crate-private items (`GlyphCache`, `AsciiGlyphs`, `same_layout`), so
  moving it to the app crate or a `crates/tools` bin would first mean making the glyph
  cache public API of `oneterm-terminal-view`.
- F4: `ascii_check_rejects_a_contextual_rule` swaps the glyph of `` ` `` in the `` A` `` pair
  of a stub layout, and merges each three-plus-char probe into one glyph; each must fail the
  check. The probe list is named in the test, so deleting the probes from `PROBES` fails it
  (mutation checked: killed). The DirectWrite test asserts Fira Code is rejected in three
  variants when it is installed (it is on this host).
- F5: LLD and gap wording; Platform proof unticked (Linux/macOS pending CI);
  `frame_time_under_output` back to the default font (ligatures on).
- F6: `force_width_copy_matches_gpui` runs the copy and GPUI's own pass (through
  `layout_line(.., Some(width))` on the stub shaper) at widths that exercise both the base
  and the non-base branch; mutations of the `0.5` factor and the `px(1.)` tolerance each
  fail it. `dependencies.md` § 4 asks for a re-diff on every `gpui-pre` bump.

### Tests

`cargo test -p oneterm-terminal-view`: 395 passed, 3 ignored (the measurement tests),
three runs after the rework. New (first cut plus rework):

- `ascii_fast_path_matches_directwrite` (Windows): real DirectWrite; Lilex regular and bold
  (loaded from `crates/app/fonts`), Consolas regular and bold italic, Courier New; 13 and
  15 px; unforced and forced at 8 and 9.5 px (both branches of the force-width pass); the
  printable line plus eight ASCII lines (including ```` ```md``` A`B` ```` and
  `'node_modules'`); `same_layout` against `layout_line_by_hash`; every run counted as a
  fast-path layout; 94 distinct glyph ids (a real shaper, not the stub); Segoe UI's table
  is `None` and its run is shaped; Fira Code rejected in three variants where installed.
- `ascii_check_rejects_a_contextual_rule`, `force_width_copy_matches_gpui`,
  `reference_walks_every_ordered_printable_pair` (all 9,025 pairs; no trailing space).
- `ascii_fast_path_falls_through_to_the_shaper`: counters for plain ASCII (fast), a hit,
  non-ASCII, a combining mark, a control char, DEL, no features list, `calt=1`,
  `calt=0 + ss01=1`, and a font whose check failed; one table per `FontKey`; `clear`
  drops the tables.
- `font_key_plain_needs_calt_off_and_nothing_on`.

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

Rework pair: main `42c44b0f` against the reworked build, the verifier's driver (sample with
```` ```md``` A`B` ````, the ligature rows, the block cursor on `=>`, a drag selection),
ligatures off, two captures each:

| Font | Terminal area (y 34..765) | Whole window |
| --- | ---: | ---: |
| Fira Code (now shaped) | **0 / 0 px** (was 176 / 176 px on `f34f44dd`) | 172 / 239 px, all y 773..781 |
| Lilex (fast path) | **0 / 0 px** | 131 / 199 px, all y 773..781 |

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

Rework re-measure (one flood per build and setting, the two builds run back to back; the
host was busier than for the first table, `Parser::advance`, which this change does not
touch, ran 0.79-1.08 us against 0.69-0.70 us before). Raw:
[`research/raw/us-0146/rework/`](research/raw/us-0146/rework/).

| Site (avg) | main, lig off | rework, lig off | main, lig on | rework, lig on (r1 / r2) |
| --- | ---: | ---: | ---: | ---: |
| `GlyphCache::shape` | 7.29 us | **417 ns** | 7.87 us | 10.17 / 7.17 us |
| `build_row_plan` | 24.65 us | 12.36 us | 26.02 us | 32.52 / 23.82 us |
| `PlanCache::update` | 1.10 ms | 689 us | 1.15 ms | 1.45 / 1.04 ms |
| `TerminalElement::prepaint` | 1.12 ms | 713 us | 1.18 ms | 1.47 / 1.06 ms |
| `Parser::advance` (host load) | 830 ns | 981 ns | 845 ns | 1.08 us / 787 ns |

Ligatures off: `shape` -94 %, `update` -37 %, `prepaint` -36 % (a busier run than the first
table). Ligatures on: the first rework run landed on a loaded host (`Parser::advance`
+28 %); the second is within noise of main. With ligatures on the only added work is the
`key.plain` test, so the fast path costs the default setting nothing. The pair walk moves
no per-run cost: it is paid once per font variant.

### Gates

- `cargo fmt --all -- --check`: clean.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean; with
  `--features oneterm-app/hotpath-profiling`: clean.
- `python scripts/check-doc-paths.py`, `python scripts/check-english.py`,
  `python scripts/verify-dependency-graph.py`, `python scripts/third-party-notices.py --check`:
  pass.
- Full `pwsh scripts/ci-local.ps1` (2026-09-28, after deleting `target/release`):
  `ci-local: all checks passed.` (first cut).
- Rework: `cargo test -p oneterm-terminal-view` x3 (395 passed, 3 ignored each), fmt,
  clippy with and without `hotpath-profiling`, `check-doc-paths`, `check-english`,
  `verify-dependency-graph`: pass. Full `CARGO_BUILD_JOBS=3 pwsh scripts/ci-local.ps1`
  after deleting `target/release`: `ci-local: all checks passed.`

### Gaps

- **Ligatures are on by default** (`FontConfig::ligatures = true`), so the default setting
  never takes the fast path; the gain above is for users who turn ligatures off. A calt-on
  fast path would need to know which ASCII sequences a font's `calt` rewrites, which the
  public text-system API does not expose.
- **Contexts longer than two chars are unproven.** The check covers every ordered pair plus
  the listed probes. A rule that fires only on a longer context not among the probes would
  pass the check and paint differently. The verifier's 35,937 punctuation triples and long
  random lines found none on the installed fonts; that is evidence, not proof. Such fonts
  exist in principle: Fira Code is a real monospace font whose `ccmp` rule the first
  (one-context-per-char) check missed.
- **Locale.** DirectWrite shapes with the user locale, and the check runs under the locale
  of the running process; `locl` rules of that locale are covered for pairs only (Fira
  Code's Afrikaans `'n` ligature is a pair, so it would be caught under an `af-*` locale), and
  rules of other locales are not exercised. A locale change while OneTerm runs is not
  picked up (the tables live until the font changes).
- **Copied GPUI code.** `apply_force_width` is a copy of `gpui-pre` 0.3.7's private
  `apply_force_width_to_layout`; a `gpui-pre` bump can move GPUI's pass. Guarded by
  `force_width_copy_matches_gpui` and the re-diff step in `dependencies.md` § 4.
- Fonts not installed here (JetBrains Mono, Iosevka, Hack, Source Code Pro, Monaspace,
  Victor Mono, Nerd Font patches) were not probed.
- Linux and macOS: the real-shaper test is Windows-only (DirectWrite); elsewhere the
  self-check decides per font at run time, unmeasured (Platform proof left open for CI).
- `RunKey.forced` keys the flag, not the forced width (BUG-0078 F3, unchanged).
- One TUI run per side; the flood has two before the rework and one per setting after.

## Handoff

None.
