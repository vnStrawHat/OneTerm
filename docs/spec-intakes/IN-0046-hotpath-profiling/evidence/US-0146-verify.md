# US-0146 independent verification

- Date: 2026-09-28
- Commit under test: `f34f44dd` (branch `perf/ascii-shaping-bypass`, one commit on main
  `42c44b0f`)
- Packet: [`US-0146`](../US-0146-ascii-shaping-bypass.md); intake [`IN-0046`](../IN-0046.md);
  owning doc: the render-pipeline LLD, § Glyph cache
  (`docs/spec-intakes/IN-0018-rebuild-terminal-render-engine/low-level-design/render-pipeline.md`)
- Host: Windows 11 Enterprise 10.0.26200, locale en-US, toolchain 1.96.0, the worktree's own
  `target/`, `CARGO_BUILD_JOBS=2`, other agents building in parallel. GUI runs: only the pids
  this verification launched, each with a private `USERPROFILE`.

## Verdict: FAIL (F1 blocks)

The one-time check is not sufficient for every ASCII run. **Fira Code** (ligatures off)
passes the check. It then paints a different glyph than main for a backtick that follows an
uppercase letter or another backtick: main draws `grave.case`, the fast path draws `grave`.
An exhaustive DirectWrite probe finds this, and a PrintWindow pair of main and the fix
shows it: **176 differing pixels in the terminal area**, on the ```` ```md``` A`B` ```` line.
Every other monospace font installed here matches exactly after passing the check. That
covers every ordered printable pair, the punctuation triples, long random lines, fractional
sizes, all four variants and forced widths, with x/y exact. The speed claim reproduces:
flood `GlyphCache::shape` 6.85 us -> 0.31 us, and the fast path is inert with ligatures on.
The width-forcing copy is identical to GPUI's. A one-line strengthening of the check (every
ordered pair once, about 2-3 ms per font variant, measured) rejects Fira Code and keeps all
the others; see F1.

## Evidence

### 1. Is "the reference line matches" sufficient? (the kerning / `liga` question)

**What stays on with ligatures off.** OneTerm's "ligatures off" is the features list
`[("calt", 0)]` (`terminal_view/render.rs` `terminal_font`). gpui-pre-windows 0.3.7
`apply_font_features` (`direct_write.rs:1785-1822`) turns `liga` and `clig` **on** unless
they are listed as 0, and adds `calt` = 0. DirectWrite applies its defaults on top: `kern`,
`ccmp`, `locl`, `rlig`, `rclt`, `mark`, `mkmk`. So with `calt = 0`, `liga` and `kern` are
still applied, and so are the required features that cannot be turned off at all.

**What the installed fonts carry** (fontTools, GSUB/GPOS feature lists):

| Font | `liga` | `kern` (GPOS) | other ASCII-reachable rules |
| --- | --- | --- | --- |
| Lilex (bundled) | no | no | `ccmp`, `locl` contextual |
| Consolas | no | no | `dlig` fi/fl (off by default), `locl` grave |
| Courier New | yes (no pure-ASCII ligature) | yes (contextual `ChainContextPos`) | `rlig`, `rclt`, `ccmp` |
| Cascadia Code / Mono | no | no | `rclt`, `ccmp` |
| Fira Code | no | no | `ccmp` contextual (fmt 2); `locl` AFK ligature `'n` -> `napostrophe` |
| Lucida Console | no | no | `locl` only |

**Exhaustive probe on real DirectWrite.** The probe is a local test, not committed. It uses
the same `gpui_platform` text system as `ascii_fast_path_matches_directwrite`. For each font
and variant (regular, bold, italic, bold italic) and each size (12, 13, 13.5, 14, 15, 16.5,
20 px), it builds the font's `AsciiGlyphs` table with the committed `read`. When the table
exists, it compares `table.layout(text)` with `layout_line(text)`. The comparison covers
glyph ids, positions, byte indices, font id, width, ascent and descent, flattened over runs.
The texts compared:

- all 9,025 ordered pairs of printable ASCII;
- for the regular variant, all 35,937 triples of the 33 punctuation chars;
- 72 tricky strings (`Ta AV // :: .. -- <- |> 0x () [] {} "" '' *** ... www fi ffi ffl fj
  Th st -> => != === !== <=> <!-- /* */ #{ #[ && || ::= >>= ..= 'n` ..., leading and
  trailing spaces, the gutter label `[00:00:00] 0000`);
- 50 seeded random lines of 80 to 500 chars.

Each tricky string and random line was compared unforced, forced at the rounded cell width,
and forced at the rounded cell width + 0.5 px.

| Font | check | mismatches after passing the check |
| --- | --- | --- |
| Lilex, Consolas, Courier New, Cascadia Code, Cascadia Mono | passes, all 4 variants x 7 sizes | **0** (45,328 texts regular, 9,391 per other variant) |
| Lucida Console | passes regular and italic; **fails** bold and bold italic (simulated bold), which then shape | 0 |
| **Fira Code** | **passes**, all 4 variants x 7 sizes | **170** regular, **105** per other variant |
| Segoe UI, Arial, a missing family | fails, all variants and sizes | n/a (shaped) |

Every Fira Code mismatch is the same substitution: `grave -> grave.case` whenever the
backtick follows `A-Z` or another backtick. The pairs are `` A` `` ... `` Z` `` and
` `` `, plus every random line that contains one of them. The glyph count and x/y are
identical; only the glyph id differs. The reference line contains `` _` `` but no
`` X` `` or ` `` `, so the check cannot see the rule.

Answer to the kerning / `liga` question: yes, `liga` and `kern` stay on with `calt = 0`. No
installed monospace font showed an ASCII kern pair or `liga` ligature: Courier New carries
both features, and its x positions matched on all pairs. The failure comes from a
**contextual substitution in a feature that cannot be turned off** (Fira Code's `ccmp`
chain). A second one is locale-dependent: Fira Code's `locl` ligature `'n` -> `napostrophe` for the
Afrikaans language system. It is not exercised here, because DirectWrite takes the language
from the user locale (`components.locale`) and this host is en-US. It is shown from the
font's GSUB (`latn`/`AFK `/`locl`, lookup 28). Under an `af-*` locale, every `'n`, for
example `'node_modules'`, would differ in glyph **count**.

**Does anything read `position.x` / `.y` for grid text?**

- `paint_shaped_line` (`element.rs:496-527`) re-anchors x per cell (`CellAnchor`) and uses
  `glyph.position.y` as shaped.
- The cursor glyph (`cursor.rs:171-189`) and the gutter labels (`element.rs:457-488`, no
  cell map) use x and y as shaped. `gutter_width` uses `line.width` (`state.rs:346-347`).
- Selection, underline, strikethrough, hyperlink hover, inverse and wide cells are
  cell-based quads that never read the layout. IME and non-ASCII text never take the fast
  path.

x/y are nevertheless exact for every passing font at every size tried, including fractional
advances (Consolas 7.147461 px at 13 px, Fira Code 7.3846154 px at 12 px), 500-char lines and both force-width
branches. The fast path accumulates `pen += advance` in `f32` in glyph order, which is what
`DrawGlyphRun` does (`context.width += glyph_advances[i]`, `direct_write.rs:1597-1606`).

**F2 structure note.** DirectWrite reports trailing whitespace as a second `ShapedRun` with
the same font id. For example, `"! "` gives 1 run on the fast path and 2 shaped. The
flattened glyphs and the width are identical, and the painter walks every run, so the pixels
are the same. `same_layout` would still report such a pair as different, which is why the
probe compares flattened glyphs.

**A stronger check, measured.** A reference made of an Eulerian walk that contains every
ordered printable pair once (9,027 chars) was shaped once per variant at 13 and 15 px:

- shaping cost 1.5-3.2 ms per font variant (debug test binary; GPUI is opt-level 3);
- Fira Code: `pair-line-equal = false` (rejected: it would shape);
- Lilex, Consolas, Courier New, Cascadia Code, Lucida Console regular and italic:
  `true` (they keep the fast path).

This covers every two-char context: `` X` ``, ` `` `, `'n`, `Ta`, `AV` and every kerning
pair. It cannot prove longer contexts, so the packet's gap text still has to say so.

### 2. Width-forcing copy

`glyphs.rs` `apply_force_width` against gpui-pre 0.3.7 `text_system/line_layout.rs:899-922`
`apply_force_width_to_layout`: identical statement for statement. Only the comments were
dropped. The initial values are the same (`glyph_pos = 0`, `NEG_INFINITY`, `px(0.)`), and
so are the `> last + force * 0.5` base test, the `abs() > px(1.)` tolerance, and the
combining-mark branch. **No drift.** Maintenance risk: a gpui-pre bump that changes the
private pass would leave the copy behind silently. Only the Windows DirectWrite equality
test (forced 8 and 9.5 px) would catch it, and only for base glyphs. The packet's Gaps do
not mention this (F6).

### 3. Cache-key completeness

- Bold and italic are separate font ids. `FontSet` gives each variant its own `Font`
  (weight, style) and a `FontKey` with `weight_bits`/`italic`. `resolve_font` maps them to
  different faces. The probe built one table per variant, and Lucida Console's simulated
  bold fails on its own.
- Size and feature changes: `plain` and `size_bits` are in the key, and `ensure_fonts`
  (`state.rs:211-218`) calls `clear()` on any `Font` or size change, which drops the tables
  too. So `ascii` holds at most 4 entries.
- DPI/scale: the layout is in logical px. DirectWrite `CreateTextFormat` takes the logical
  `font_size` (`direct_write.rs:538-548`), and GPUI's own line cache has no scale in its key
  either. Scale enters only at `paint_glyph` rasterization, like subpixel positioning. No
  re-shape is needed on a scale change. The forced width is not keyed (BUG-0078 F3,
  unchanged).

### 4. Dependency policy

- `docs/agents/dependencies.md` § 1 table: `gpui_platform` "app crate only"; § 2: "Only
  `oneterm-app` directly uses `gpui_platform`". The commit adds
  `[target.'cfg(windows)'.dev-dependencies] gpui_platform.workspace = true` to
  `crates/terminal-view/Cargo.toml` and updates neither sentence (F3).
- `python scripts/verify-dependency-graph.py`: passed (20 packages).
  `python scripts/third-party-notices.py --check`: up to date. It skips dev-only edges
  (`third-party-notices.py:192-194`). `gpui-pre-platform 0.3.7` is already in `Cargo.lock`,
  so `deny.toml` needs no change, and the lock diff is one edge.
- CI Linux/macOS: the table is `cfg(windows)` and the test is `#[cfg(windows)]`, so neither
  job resolves or builds the dependency for this crate.

### 5. Measurements (release, `hotpath-profiling`, one flood per build and setting)

Built here: main `42c44b0f` and the fix, each `cargo build -p oneterm-app --release
--features hotpath-profiling`. `hotpath-measure.ps1 -Mode Flood`, own pid, reports kept in
the session scratchpad.

| Site (avg / total, calls) | main, ligatures off | fix, ligatures off | main, ligatures on | fix, ligatures on |
| --- | ---: | ---: | ---: | ---: |
| `GlyphCache::shape` | 6.85 us / 563 ms (82,113) | **307 ns** / 21.7 ms (70,512) | 6.19 us / 432 ms (69,829) | 6.15 us / 434 ms (70,616) |
| `build_row_plan` | 23.87 us | 9.59 us | 21.20 us | 21.16 us |
| `PlanCache::update` | 1.06 ms | 524 us | 925 us | 920 us |
| `TerminalElement::prepaint` | 1.08 ms | 540 us | 941 us | 936 us |
| `TerminalElement::paint` | 65 us | 60 us | 61 us | 57 us |

The direction reproduces: ligatures off, `shape` -96 %, `update` -51 %, `prepaint` -50 %.
The packet's -94 / -42 / -41 % are the same direction (this single run shows larger gains). Ligatures on: unchanged, so
the fast path does not trigger for the default setting, and there is no regression.

### 6. Pixel check

`pixel.ps1` (scratchpad): 1280x800, private profile, cursor blink off, gutter off. It runs
`prompt $G & cls & type sample.txt`. The sample has four lines of
`fi ffi fl => -> != == <= >= www fifi office`, the 95 printable chars, code with
```` ```md``` A`B` ````, paths, a URL, bold, italic, bold italic, colours within one run,
underline, strike, inverse, dim, six pangram lines with ligature pairs, and a non-ASCII line.
Then it types `rem a=>b` and moves Left three times (block cursor on `=` of `=>`), and
drag-selects row 0 across `fi ffi fl => ... fifi office`. Two PrintWindow captures per run.

| Pair | Terminal area (y 34..765) | Whole window |
| --- | ---: | ---: |
| Lilex, ligatures off: main 1 vs fix 1 / main 2 vs fix 2 | **0 / 0 px** | 175 / 212 px, all y 773..781 (status bar clock, CPU, MEM) |
| Lilex, ligatures on: main vs fix (x2) | **0 / 0 px** | 221 / 178 px, y 773..781 |
| **Fira Code, ligatures off: main vs fix (x2)** | **176 / 176 px** | 414 / 456 px |
| Noise pairs (main 1 vs main 2, fix 1 vs fix 2, each setting) | 0 px | 91..181 px, y 773..781 |

The Fira Code difference forms 8 glyph-wide clusters at y 246..251: the 2nd and 3rd
backticks of both fences and the two backticks of `` A`B` ``. Crop (main above, fix below,
3x): [`US-0146-verify-fira-grave.png`](US-0146-verify-fira-grave.png).

### 7. Tests

- `cargo test -p oneterm-terminal-view`, 3 runs: `393 passed; 0 failed; 3 ignored` each time.
- Mutation A, the check skipped (`read` always returns `Some(table)`): **killed**.
  `ascii_fast_path_matches_directwrite` fails at `glyphs.rs:699` (Segoe UI counters
  `(1, 1)` vs `(0, 1)`). Restored.
- Mutation B, the ligature probes deleted from `REFERENCE`: **survives**, all 10 glyph tests
  pass (F4). Restored.

### 8. Records

- Packet: follows `docs/templates/work.md` (all sections, proof block, Evidence and Gaps).
  Created 2026-09-28. Owning docs are named and reconciled.
- LLD § Glyph cache matches the code, except one sentence: "A fallback face, a ligature,
  kerning or a proportional font fails the check". That holds only when the reference line
  exhibits it (F5).
- The proposed harness `story` row is in the report to the coordinator.

### 9. Gate

`CARGO_BUILD_JOBS=2 pwsh scripts/ci-local.ps1` (after deleting `target/release`), final line:
`ci-local: all checks passed.` (exit 0; run on `f34f44dd` with this file present)

## Findings

- **F1 (high, blocking; correctness of the check).** Fira Code passes the one-time check
  and then renders `grave` where main renders `grave.case` after `A-Z` or a backtick. This
  was measured: 170 mismatching probe texts on regular, 105 on each other variant, at every
  size, and 176 px in a before/after PrintWindow pair. By the task's own criterion ("a pair
  differs after passing the check"), this fails. A second case is locale-dependent (from the
  font tables, not executed): under an Afrikaans locale, Fira Code's `locl` ligates `'n`, so
  the glyph count differs. Root cause: the check samples one fixed context per char, while
  required features (`ccmp`, `locl`, `rlig`, `rclt`) and the still-on `liga`/`kern` can
  rewrite any context. Fix, measured above: make the reference cover every ordered printable
  pair (an Eulerian walk, 9,027 chars, 1.5-3.2 ms once per variant). Keep the ligature
  probes (three-char contexts). Update the Gaps to say that longer contexts are still
  unproven. Add a test for a pair outside the old reference, for example with Fira Code or
  a font that ships a contextual rule, if one can be bundled or found on the CI image.
- **F2 (info).** Trailing whitespace is a separate `ShapedRun` in DirectWrite's layout, and
  the fast path's is one run. The pixels are identical (flattened glyphs and width are
  equal). Any future consumer that indexes `runs` must not assume run parity.
- **F3 (medium, policy / records).** The `gpui_platform` Windows dev-dependency of
  `oneterm-terminal-view` contradicts `docs/agents/dependencies.md` ("app crate only", § 1
  and § 2), and the doc was not updated. The mechanics are fine: graph check passes, notices
  exclude dev edges, `cfg(windows)` keeps it out of Linux/macOS, no `deny.toml` change.
  The cost is that the crate's test binary now builds and links the platform crate. Fix:
  amend both sentences ("plus a Windows-only dev-dependency of `terminal-view` for the real
  text system in tests, US-0146") or record the exception in the packet's Decisions.
- **F4 (low, test).** No test shows that the ligature probes do anything. Deleting them from
  `REFERENCE` passes every test, because no tested font ligates them with `calt` off. With
  F1's pair line, add an assertion that a table is rejected when the reference is not
  reproduced for a reason other than proportional width.
- **F5 (low, records).** The LLD says a kerning, ligature or fallback font "fails the
  check". That holds only for effects visible in the reference line. The packet's gap says
  "no such monospace font was found", and Fira Code is one. The packet ticks Platform proof
  while Linux and macOS are unrun (US-0144 left it unticked pending CI). The change of
  `frame_time_under_output` to ligatures off means the in-repo frame-time test no longer
  measures the default path. That is fine for this packet, but it should be noted where the
  IN-0046 frame-time baselines are compared.
- **F6 (low, maintenance).** `apply_force_width` is a verbatim copy of a private gpui-pre
  0.3.7 function (no drift today). Name it in the packet's Gaps and in the GPUI-bump
  procedure (`dependencies.md` § 4): re-diff it on every `gpui-pre` bump.

## Gaps

- The Afrikaans `'n` case is derived from Fira Code's GSUB and was not executed (en-US
  host). Other `locl` language systems of other fonts were not enumerated.
- Fonts not installed here (JetBrains Mono, Iosevka, Hack, Source Code Pro, Monaspace,
  Victor Mono, Nerd Font patches) were not probed. The pair-line fix makes that less
  important, but does not remove it.
- Contexts longer than two chars were probed only for punctuation triples and random lines.
- Linux/macOS text systems: not run (Windows host), as in the packet.
- One flood per build and setting; the TUI load was not re-run.
- The probe test, the pixel driver and the raw hotpath reports stay in the session
  scratchpad, not in the repository.

---

# Second pass: verification of the rework (2026-09-28)

- Commit under test: `1f52e390` (branch `perf/ascii-shaping-bypass`: `f34f44dd` implementation,
  `726fd2ef` first verification FAIL, `1f52e390` rework; on main `42c44b0f`)
- Host: as in the first pass (Windows 11 Enterprise 10.0.26200, user locale en-US, toolchain
  1.96.0). The worktree's own `target/`, `CARGO_BUILD_JOBS=3`, one other agent building. GUI
  runs: only the pids this verification launched, each with a private `USERPROFILE`.

## Verdict: PASS

The rework closes F1. The pair walk rejects Fira Code in every variant and size, under every
locale tried. It also rejects JetBrains Mono (the same `ccmp` backtick rule), a font the first
pass could not probe. For every font that passes the check, the fast path equals DirectWrite's
shaping exactly: glyph ids, x, y, byte index, font id, width, ascent and descent. That holds for
all ordered pairs, all punctuation triples, 72 tricky strings and 50 long random lines, at five
sizes, in four variants, forced and unforced, and under 18 locales. The Fira Code and Lilex
ligatures-off PrintWindow pairs against main show 0 px in the terminal area. The mutations the
rework claims are killed are killed. One probe deletion (`...`) survives the F4 test (N2,
low). The speed claim reproduces. The gate is green.

## Evidence

### 1. Exhaustive DirectWrite probe (fonts x variants x sizes)

A local `#[test]`, not committed, was compiled as a child module of `glyphs.rs`, so it
reaches the crate-private `AsciiGlyphs::{check, read, layout}` through the same
`cfg(windows)` `gpui_platform` dev-dependency as `ascii_fast_path_matches_directwrite`. For
each family, variant (`FontSet::get`, as the app builds them: regular, bold, italic, bold
italic), size (12, 13, 13.5, 15, 20 px) and `calt = 0`, the probe does the following:

- It shapes `REFERENCE` with the platform text system and runs the committed `check`. It also
  calls the committed `read` through the window and asserts that both agree.
- When the table exists, it compares `table.layout(text, size, None)` with the platform's
  `layout_line(text)` on 45,084 texts: 9,025 ordered pairs, 35,937 triples of the 33
  non-alphanumeric printable chars (space included), 72 tricky strings (URLs, Windows and
  Unix paths, hashes, ```` ``` ```` fences, `` A`B` ``, `'n`, `'node_modules'`, quotes,
  ellipses, arrows, `IJ ij L.L`, `i I istanbul`, leading and trailing spaces, whitespace-only
  runs, the gutter label, digits) and 50 seeded random lines of 80 to 500 chars.
- Forced: each tricky string and random line goes through GPUI's own `layout_line(..,
  Some(w))`, that is, the private `apply_force_width_to_layout`, at four widths: the advance
  snapped at scale 1, 1.25 and 1.5, and the advance + 0.5 px. The fast path's
  `table.layout(.., Some(w))` is compared with it (488 texts per combination). The pairs and
  triples are not forced. Their unforced layouts are equal bit for bit, and the force pass is
  a deterministic function of the unforced layout, whose copy is proven equal to GPUI's on
  these 488 forced texts per combination.
- The comparison flattens the runs. It checks id, x, y, index, emoji flag and font id per glyph,
  then width, ascent, descent and len.

| Font (source) | Check result, 4 variants x 5 sizes | Mismatches after a passing check | Why the check fails |
| --- | --- | --- | --- |
| Lilex (bundled `crates/app/fonts`, all four files) | pass 20/20 | **0** (20 x 45,084 unforced + 488 forced) | n/a |
| Consolas | pass 20/20 | **0** | n/a |
| Courier New | pass 20/20 | **0** | n/a |
| Cascadia Code | pass 20/20 | **0** | n/a |
| Cascadia Mono | pass 20/20 | **0** | n/a |
| Lucida Console | pass R and I (10/10); **fail** B and BI (0/10) | **0** | simulated bold: every glyph off the constant-advance grid |
| Fira Code | **fail 0/20** | n/a (shapes) | `` ` `` has 2 ids in the walk (67 x `grave`, 28 x `grave.case`) |
| JetBrains Mono (`reference/gpui-kit/.../JetBrainsMono-Regular.source.ttf`, loaded with `add_fonts`; B/I/BI synthesized) | **fail 0/20** | n/a (shapes) | `` ` `` has 2 ids (67 / 28): the same `ccmp` rule as Fira Code |
| Hack, Iosevka, Source Code Pro, Ubuntu Mono, DejaVu Sans Mono | not installed, no file in the repository or its references | not probed | - |

**Total mismatches after a passing check: 0.** Every passing combination also shows the
F2 run split on 1,184 texts (trailing whitespace is a second `ShapedRun` in DirectWrite's
layout, not in the fast path's). The flattened glyphs and width are identical, so the pixels
are too.

Check cost (debug test binary, GPUI at opt-level 3, `layout_line` of the 9,026 + 70 char
reference plus `check`): 1.8-4.1 ms per variant. There are four outliers of 9.5-16.7 ms, on the
first bold or italic face at 12 px (Consolas, Courier New), where the face is loaded for the first
time. The shaper pays that load on the first run anyway. The packet's 2.1-4.5 ms describes the
steady case.

### 2. Locale

GPUI does not take a locale parameter. `DirectWriteTextSystem::new` reads
`GetUserDefaultLocaleName` once, and `layout_line` passes that value to `CreateTextFormat`
(`gpui-pre-windows` 0.3.7, `direct_write.rs:174-176, 540-548`). Changing the user locale would
modify the host outside the worktree, so the probe instead rebuilt GPUI's `layout_line` against
DirectWrite directly with an explicit locale. It uses `CreateTextFormat(family, collection,
weight 400, normal, locale)`, `CreateTextLayout`, the typography GPUI sets for `calt = 0`
(`liga` 1, `clig` 1, `calt` 0), and one `Draw` into an `IDWriteTextRenderer` that records ids,
`pen + advanceOffset`, `-ascenderOffset` and cluster indices exactly as GPUI's
`DrawGlyphRun` does. For this it took local, uncommitted `windows` and `windows-core` 0.62
dev-dependencies, both already in the lock through `gpui-pre-windows`.

- **Control:** under en-US the replica equals GPUI's `layout_line` on the reference line,
  all pairs and the tricky strings: 9,098 texts x 5 fonts (Lilex, Fira Code, Consolas,
  Cascadia Code, JetBrains Mono), **0 mismatches**.
- **Check + probe per locale:** 18 locales (en-US, af-ZA, tr-TR, az-Latn-AZ, kk-KZ, tt-RU,
  nl-NL, ca-ES, ro-RO, pl-PL, el-GR, ru-RU, de-DE, vi-VN, ja-JP, zh-CN, ar-SA, he-IL) x 6
  fonts x 13 and 15 px, regular. Fira Code and JetBrains Mono are **rejected under every
  locale**. Lilex, Consolas, Cascadia Code and Courier New pass under every locale, and the fast
  path equals the replica on all 45,084 texts each time: **0 mismatches**.
- **The locale rules are real and executed:** under af-ZA, Fira Code ligates `'n` (2 glyphs
  become 1 in `'n`, `'node_modules'`). Under tr-TR, Fira Code and JetBrains Mono swap `i` for
  `i.loclTRK` in 204 of the pair and tricky texts. A GSUB scan (fontTools) of every installed
  monospace font found no other ASCII-only `locl` rule. Lilex's `NLD` rule needs `Iacute`, its
  `CAT` rule and Fira Code's need `periodcentered`, and Consolas's `grek` rule never applies to
  Latin text. The tr-TR rule is context-free, so a font that otherwise passed would read the
  substituted id into its table under that locale.

Records note (N3, low). The packet's gap sentence "A locale change while OneTerm runs is not
picked up (the tables live until the font changes)" is true, but it has no consequence for
correctness. GPUI's shaper does not pick the change up either, because the locale is fixed
when the text system is created. The check and the shaper always share one locale per
process.

### 3. Pixel pairs (main `42c44b0f` vs `1f52e390`)

Both builds were `--profile fast-dev`, with source the only difference; pixels do not depend
on the opt-level. The driver is `pixel.ps1` (session scratchpad), run from the pid it
launches. Each run uses a private `USERPROFILE` and a working directory whose `target/terminal.json`
sets the family, `size: 15`, the ligature switch, `cursor.blink: false` and
`layout.show_gutter: false`. The window is 1280x800. It runs
`chcp 65001 >nul & prompt $G & cls & type sample.txt`. The sample has 19 lines: the ligature
row, the 95 printable chars, ```` ```md``` A`B` `ls -la` Q`R Z`` 'node_modules' 'n ````, code,
paths, URL, hash, bold, italic and bold italic with backticks, colours within one run,
underline, strike, inverse, dim, pangrams, `i I istanbul IJ ij L.L`, a shell line, a log line,
non-ASCII, box drawing, a coloured trailing-space run, and indented code. Then it types
`rem a=>b` and presses Left three times, which puts the block cursor on `=` of `=>`. It
drag-selects row 0 from `fi` across `ffi fl ff ffl => -> != == <=`. It takes two PrintWindow
captures per run. The terminal area is y 34..765.

| Pair | Terminal area, capture 1 / 2 | Whole window |
| --- | ---: | ---: |
| Fira Code, ligatures off (now shaped): main vs rework | **0 / 0 px** | 683 / 718 px, all y 772..783 (status bar) |
| Lilex, ligatures off (fast path), cursor on `=>`, selection over `fi`: main vs rework | **0 / 0 px** | 762 / 697 px, all y 772..783 |
| Lilex, ligatures on: main vs rework (first pair) | 6 / 6 px | 648 / 680 px |
| Lilex, ligatures on: 3 more fresh processes (main, rework, main), cross pairs | main vs main **10, 12, 2 px**; rework vs rework 16 px; main vs rework **0**, 12, 8 px | - |
| Same process, capture 1 vs 2 (every setting and build) | 0 px | 30..166 px, y 773..781 |

The ligatures-on differences are ±1 in one colour channel at the same few anti-aliased pixels
(x 413-414 at y 75/78 inside the selected `<=`, x 278 at y 92/95). Two **main** processes
differ from each other by as much as main differs from the rework, and one main-vs-rework pair
is 0 px. So this is cross-process rasterization noise, not the change. The flood below also
shows the ligatures-on path never takes the fast path: `GlyphCache::shape` averages 7.85 us,
the same as main. The first pass saw 0 px on this setting; that is the same noise, just
absent in that run.

### 4. F3: the test-only `cfg(windows)` dev-dependency

Judgement: **acceptable; keep it, do not move the test.**

- R1-R12 (`crate-dependency-rules.md`) govern edges between OneTerm crates in the
  `-e normal` graph. `gpui_platform` is a third-party GPUI layer taken as a
  `[target.'cfg(windows)'.dev-dependencies]` entry. It adds no OneTerm edge and no normal edge,
  so no rule is touched. `verify-dependency-graph.py` and `third-party-notices.py --check` pass,
  and dev edges are skipped by the notices script.
- `dependencies.md` § 1 and § 2 now name the exception, its scope (a test that must compare
  against the real platform text system, which GPUI's test and headless platforms replace)
  and its only user. The policy exists so that window and event-loop integration stays in the
  app. A test that only calls `PlatformTextSystem` does not break that.
- The alternative is to move the test to the app crate and reach the cache through a
  `#[doc(hidden)] pub` hook. That would publish `GlyphCache`, `AsciiGlyphs`, `same_layout` and
  `FontSet` from `oneterm-terminal-view` for a test's sake: a larger surface, and one a future
  refactor would have to keep. The dev-dependency costs only build time for
  `cargo test -p oneterm-terminal-view` alone; `cargo test --workspace` builds the platform
  crate for the app anyway.
- Residual risk, for Platform proof: on the `windows-latest` CI job the test calls
  `current_platform(false)`, which creates a real D3D11 device (`DirectXDevices::new`
  enumerates DXGI adapters; a runner without a GPU relies on the basic render adapter). This
  is unproven until CI runs it. If it fails there, the fix is a skip when no adapter exists,
  not a policy change.

### 5. Mutations (each applied, the listed tests run, file restored)

| Mutation | Test | Result |
| --- | --- | --- |
| M1 delete probe `www` from `PROBES` | `ascii_check_rejects_a_contextual_rule` | killed (`find(probe).unwrap()`) |
| M3 delete probe `<=>` | same | killed |
| **M2 delete probe `...`** | same | **survives** (N2) |
| M4 force-width factor `0.5` -> `0.4` | `force_width_copy_matches_gpui` | killed |
| M5 force-width tolerance `px(1.)` -> `px(0.5)` | same | killed |
| M6 `check` always returns `Some(table)` | `ascii_*` | killed: `ascii_check_rejects_a_contextual_rule` and `ascii_fast_path_matches_directwrite` (Segoe UI counters `(1, 1)` vs `(0, 1)`, `glyphs.rs:830`) |
| M7 `same_layout` ignores glyph ids (Segoe UI still fails on x) | `ascii_*` | killed: the **Fira Code** assertion fails (`(1, 1)` vs `(0, 1)`), so that assertion is live too |

The two-char probes (`fi fl ff -> => != == <= >=`) are redundant with the pair walk, so
deleting them changes nothing, correctly. `...` is the only three-plus-char probe that the F4
test does not name.

### 6. Tests, lints, measurements

- `cargo test -p oneterm-terminal-view`, 3 runs: `395 passed; 0 failed; 3 ignored` each time.
  The DirectWrite test's own timings: Lilex, Consolas, Courier New 2.1-3.5 ms; Fira Code
  2.9-3.7 ms, rejected.
- `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, the
  same with `--features oneterm-app/hotpath-profiling`, `check-doc-paths`, `check-english`,
  `verify-dependency-graph`, `third-party-notices --check`: pass (run on this commit's tree).
- One flood per setting, the rework's release `--features hotpath-profiling` build,
  `hotpath-measure.ps1 -Mode Flood` (own pid). Main's figures are the rework's committed raw
  reports (`research/raw/us-0146/rework/main-flood-*.json`); main was not rebuilt here.

| Site (avg) | main, lig off | **this run, lig off** | main, lig on | **this run, lig on** |
| --- | ---: | ---: | ---: | ---: |
| `GlyphCache::shape` | 7.29 us | **423 ns** (-94 %) | 7.87 us | **7.85 us** |
| `build_row_plan` | 24.65 us | 11.27 us | 26.02 us | 25.59 us |
| `PlanCache::update` | 1.10 ms | 623 us (-43 %) | 1.15 ms | 1.12 ms |
| `TerminalElement::prepaint` | 1.12 ms | 646 us (-42 %) | 1.18 ms | 1.14 ms |
| `Parser::advance` (host load) | 830 ns | 814 ns | 845 ns | 839 ns |

This run was on a quieter host (`Parser::advance` is at main's level), and it reproduces the
packet's -94 % `shape`. It shows `update` at -43 %, where the packet's rework run, on a busier
host, showed -37 %. With ligatures on, the figures are within noise of main, so there is no
regression on the default setting.

### 7. Records

- Packet: Reopened is ticked; the rework section maps F1-F6; Acceptance has a rework item;
  Platform proof is unticked (Linux/macOS and the Windows CI job are pending). The Gaps cover
  contexts longer than two chars, locale, copied GPUI code and bump drift, fonts not probed,
  ligatures on by default (no gain for default users), `RunKey.forced` keying the flag and not
  the width, and the run counts. Two gap lines are now partly stale: JetBrains Mono has been
  probed (rejected), and the locale line is benign (N3).
- LLD § Glyph cache: matches the code (features that stay on, pair walk, "fails when the
  reference exhibits it", unproven contexts and locales, run split, force-width re-diff).
- `dependencies.md`: § 1 table row, § 2 exception paragraph, § 4 step 4 re-diff: present and
  accurate.
- Proposed harness `story` row: in the report to the coordinator (`harness.db` was read, not
  written).

### 8. Gate

`CARGO_BUILD_JOBS=3 pwsh scripts/ci-local.ps1` (after deleting `target/release`, with this
file present), final line: `ci-local: all checks passed.` (exit 0, second run). The first
run failed in `cargo test --workspace` on
`oneterm-core` `config::shell::tests::powershell_enter_guard_decides_the_four_bindings`
(`pwsh.exe with default`: empty verdict). That test launches the real `pwsh.exe`, and
`crates/core` is unchanged between main and `1f52e390`. Run alone it passed 3 of 3, so the
failure was a host flake under load, not this change.

## Findings

- **N1 (info).** The rework's check is sufficient on every installed monospace font and on
  JetBrains Mono, across sizes, variants, forced widths and 18 locales: 0 mismatches after a
  passing check. The check rejects both real fonts with a contextual ASCII rule (Fira Code,
  JetBrains Mono). F1 is closed.
- **N2 (low, test).** Deleting the `...` probe from `PROBES` survives
  `ascii_check_rejects_a_contextual_rule`, because that test lists 10 of the 11 probes of three
  or more chars and omits `...`. The packet's "deleting the probes from `PROBES` fails it" holds
  for the others. Fix: add `"..."` to the test's list (one token). Not blocking.
- **N3 (low, records).** The locale gap is benign: GPUI captures the user locale once per
  process, so the check and the shaper cannot disagree about it. JetBrains Mono can move from
  "not probed" to "probed, rejected (`ccmp` backtick rule)".
- **N4 (info).** The ligatures-on PrintWindow pairs differ by 2-16 px between any two processes
  of the same build (±1 channel anti-aliasing), so "0 px" on that setting is not a stable
  criterion. The ligatures-off pairs, where the fast path runs, are 0 px in every capture.
- **N5 (info, Platform proof).** The committed DirectWrite test creates a real D3D11 device on
  the Windows CI runner. This is unproven until CI runs it.

## Gaps

- Hack, Iosevka, Source Code Pro, Ubuntu Mono, DejaVu Sans Mono, Monaspace, Victor Mono and Nerd
  Font patches are not on this host and were not probed. JetBrains Mono was probed from the
  reference copy, regular only; bold and italic are synthesized from it.
- The locale probe ran through a DirectWrite replica of GPUI's `layout_line`. It was verified
  equal to GPUI under en-US, but it is not GPUI itself under the other locales. Regular weight
  only, 13 and 15 px.
- Contexts longer than three chars were covered only by the tricky strings and random lines.
- Linux and macOS text systems: not run.
- The ligatures-on pixel criterion is limited by cross-process noise (N4).
- Main was not re-measured; its flood figures are the committed rework raw reports.
- The probe tests, the replica, the pixel driver, the captures and the hotpath reports stay in
  the session scratchpad.
