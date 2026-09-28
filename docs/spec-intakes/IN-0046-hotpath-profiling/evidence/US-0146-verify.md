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
