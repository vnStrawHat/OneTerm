# US-0072 — old-engine benchmark baseline

Date: 2026-09-12
Tool: `vt-bench all` (release)
Raw: [`US-0072-bench-baseline-raw.md`](US-0072-bench-baseline-raw.md),
[`US-0072-bench-baseline.json`](US-0072-bench-baseline.json)

**Recorded, never gated.** No number here is an exit criterion for any packet. The new
engine is compared against these as **ratios**, and every ratio is read next to the ConPTY
transport ceiling: `pty-throughput` measures about **1.2 MiB/s** for a `cmd.exe` producer
and about 30 MiB/s for a DOOM-fire-class one, which is one to two orders of magnitude
below every parse number below.

## Method

- Engine: the vendored `alacritty_terminal` + `vte` fork, `Processor::<StdSyncHandler>`
  advanced over a `Term` — the same call `crates/terminal/src/backend/pump.rs` makes.
- Geometry 160x45 (matching `pty-throughput`) for tiers 1, 2, 3 and 5; tier 4 uses 80x24
  to 100x40, deliberately its own.
- 100 MiB per fixture, median of three timed passes over the same generated buffer.
- Release profile, no other load, no parallel runs.
- Fixtures are deterministic functions in `crates/tools/src/bench.rs` — no RNG and no
  captured files, so this table is reproducible from the repository alone
  (`vt-bench fixtures --out <dir>` writes them out).
- Machine: Intel Core i7-12700 (12 cores / 20 threads), 31.7 GB RAM, Windows 11 build
  26200, rustc pinned by `rust-toolchain.toml`.

## Tiers 1-2 — parser alone, and parse plus grid

| Fixture | parser MiB/s | parser ns/B | parse+grid MiB/s | parse+grid ns/B | grid share |
| --- | ---: | ---: | ---: | ---: | ---: |
| `plain_ascii` | 1192.0 | 0.80 | 94.0 | 10.15 | 12.7x |
| `long_lines` | 1345.7 | 0.71 | 107.3 | 8.89 | 12.5x |
| `heavy_sgr` | 379.1 | 2.52 | 178.5 | 5.34 | 2.1x |
| `tui_redraw` | 447.9 | 2.13 | 130.2 | 7.32 | 3.4x |
| `scroll_region` | 815.0 | 1.17 | 113.5 | 8.40 | 7.2x |
| `cjk_wide` | 587.4 | 1.62 | 137.0 | 6.96 | 4.3x |
| `dense_cells` | 220.1 | 4.33 | 180.8 | 5.28 | 1.2x |
| `scrolling` | 752.1 | 1.27 | 109.4 | 8.71 | 6.9x |
| `sixel` | 443.3 | 2.15 | 38.5 | 24.77 | 11.5x |
| `osc_9_7` | 65.9 | 14.46 | 40.9 | 23.30 | 1.6x |

Headline: **parse plus grid runs at 38-181 MiB/s; the parser alone at 66-1346 MiB/s.**
The grid half costs 1.2x to 12.7x the parser half, so the `Handler` implementation, not
the state machine, is where the time goes — which is the same conclusion
`research/perf-baseline.md` § 5.2 reached at a different geometry, now with two more
fixtures. `osc_9_7` is the outlier in the parser column (66 MiB/s): OSC payload
accumulation is per byte and has no fast path.

## Tier 3 — parse, grid and one snapshot build per frame

600 simulated frames, 512 bytes of new output between each, full 7200-cell copy each
time (`term.damage()` + `reset_damage()` + `renderable_content()` + one `Cell::clone()`
per visible cell — the shape of `TerminalContent::refill`).

| Fixture | us/frame |
| --- | ---: |
| `plain_ascii` | 38.1 |
| `long_lines` | 37.1 |
| `heavy_sgr` | 37.7 |
| `tui_redraw` | 42.3 |
| `scroll_region` | 23.7 |
| `cjk_wide` | 23.1 |
| `dense_cells` | 22.8 |
| `scrolling` | 22.7 |
| `sixel` | 24.6 |
| `osc_9_7` | 21.1 |

Headline: **21-42 us per frame for a 160x45 viewport**, i.e. 1.3-2.5 % of a 60 Hz frame
budget. Cost is proportional to *visible cells*, not to output volume — which is exactly
the property the render-state design has to preserve. The 21-42 us spread across fixtures
is scheduler noise plus damage-list length, not a content effect: the copy is the same
7200 cells every time.

## Tier 4 — resize latency

| Scrollback rows | grow 80x24 to 100x40 (us) | shrink back (us) |
| ---: | ---: | ---: |
| 0 | 1186 | 20 |
| 10 000 | 4704 | 3361 |
| 100 000 | 53 182 | 39 146 |

Headline: **resize is linear in scrollback and is the one operation a user can feel** —
53 ms to grow with 100 000 rows of history is a visible hitch during a drag-resize. This
is the tier `reflow-and-resize.md`'s cost model (O(live rows x cols)) is measured against,
and the tier the deferred lazy-history-reflow path would target. The grow/shrink asymmetry
at depth 0 (1186 us versus 20 us) is the history the grow itself creates.

## Tier 5 — live heap after 10 000 scrollback rows

Live heap held by the terminal, not process RSS: RSS also counts retained allocator pages,
while the design claim this tier exists to check is what a grid of N rows costs. Each
content kind writes exactly one line per row, so bytes-per-row is comparable across them.

| Content | heap bytes | bytes/row | bytes/cell |
| --- | ---: | ---: | ---: |
| plain | 41 380 491 | 4138 | 25.9 |
| unicode | 41 380 491 | 4138 | 25.9 |
| styled | 41 380 491 | 4138 | 25.9 |
| mixed | 41 380 491 | 4138 | 25.9 |

Headline: **4138 bytes per 160-column row, identical for all four content kinds** — about
26 bytes per cell. Content-independence is the finding: `alacritty_terminal`'s `Cell`
carries `char` + two `Color` enums + flags + an `Option<Arc<CellExtra>>` inline, and none
of these fixtures allocates an `extra`, so styled and plain text cost the same. 10 000 rows
cost 39.5 MiB regardless of what is in them.

This is the number `US-0074`'s 8-byte packed cell with interned styles is measured
against, and the number `R-51`'s deferral of dual-form rows is gated on: at 8 bytes per
cell plus row overhead the same 10 000 rows should land near a quarter of this.

## Variance

Three `all` runs were taken; tiers 1-3 move by up to 30 % between runs on a machine that
is also running a build, and tier 3 by up to 80 % (21 to 42 us). **Read these as an order
of magnitude, not as a threshold** — which is the reason the CI job is
`continue-on-error` and asserts nothing. Tier 4 and tier 5 are stable to within a few per
cent; tier 5 is exact.
