# US-0072 — cross-check of `grid.expect` against upstream `grid.json`

Date: 2026-09-12
Tool: `vt-corpus cross-check --grid-json <dir>`
Raw output: [`US-0072-cross-check-raw.md`](US-0072-cross-check-raw.md)

## Result

**45 of 45 recordings match. Zero mismatches, zero unexplained differences.**

The `grid.expect` format therefore loses nothing that upstream's own reference test
compares. It is run in this packet once and never again (R-61): the 44 MiB of `grid.json`
is not committed.

## What was compared

For each recording, two independently produced grids:

| Side | How it was produced |
| --- | --- |
| Ours | Replay the recording through the vendored `alacritty_terminal` fork exactly as `crates/terminal/src/backend/pump.rs` does (`Processor::<StdSyncHandler>` over a `Term`), then `grid().clone()`, `initialize_all()`, `truncate()` — upstream's own `tests/ref.rs` procedure — and encode every cell. |
| Theirs | Deserialize upstream's committed `grid.json` and encode it through the **same** cell encoder. |

Comparison is the packet's own cell-exact diff: per grid `columns`, `lines`,
`display_offset` and the row count; per row the derived wrap flag; per column the content
(primary codepoint plus every zero-width codepoint), the flag set, foreground, background,
underline colour and hyperlink. Nothing is trimmed and no trailing blank is dropped.

## Caveats that had to be handled, and how

| Caveat | Handling |
| --- | --- |
| `Storage::PartialEq` asserts `zero == 0` and **panics** otherwise (trap 45) | Our side calls `truncate()`, which rezeroes the ring; the converter refuses any `grid.json` whose `raw.zero` is not 0. Neither side ever compares a rotated ring. |
| `Storage::eq` ignores `visible_lines`; `Row::eq` ignores `occ`; `Grid::eq` ignores `max_scroll_limit` (trap 44 / § 7.3) | The encoder reads none of the three. `occ` in particular is an over-approximating hint, not state. |
| Cursors are `#[serde(skip)]`, so `grid.json` carries no cursor, modes, palette, title, tab stops or scroll region | Exactly why `state.expect` exists. It has no upstream oracle and is not part of this cross-check; it is blessed by the old engine and frozen the same way. |
| Row order | Once `zero == 0`, `Storage` maps `inner[i]` to `Line(screen_lines - i - 1)`, so both sides emit rows newest first. `history` (1029 rows) and `row_reset` (1247 rows) confirm the mapping holds far into scrollback. |
| Auto-generated hyperlink ids are a process-global counter (`<n>_alacritty`) | Both sides renumber them per grid in first-appearance order, which preserves *which cells share a link* while making the file reproducible. Explicit ids (`hello`, `42` in `hyperlinks`) are kept verbatim. Without this, upstream's own ref test is order-dependent: its committed `grid.json` holds `0_alacritty` and `1_alacritty`, which only reproduce when that recording is the first to create a link in the process. |
| The vendored fork is not pristine upstream (three alacritty patches: standalone manifest, `Event::Osc`/`Event::ClearScreen`, Sixel) | The match is itself the measurement: **none of the three patches changes anything the 45 recordings pin.** The first two add events only; the Sixel patch adds a DCS path no recording exercises. `vendor/alacritty_terminal/src/grid/{mod,storage}.rs` were also verified byte-identical to the pinned upstream revision. |
| `bitflags` serializes flags as `"NAME \| NAME"`, including composite aliases | Parsed with `Flags::from_name`, so `BOLD_ITALIC` / `DIM_BOLD` / `ALL_UNDERLINES` resolve to their full bit patterns; emitted from the 15 primitive bits so the encoding cannot depend on which alias matched first. |

## Oracle provenance

Upstream `grid.json` was copied out of the cargo checkout into a scratch directory and
hashed, so the run is reproducible without a machine-specific path:

- Source: `<CARGO_HOME>/git/checkouts/alacritty-20195d12a03fa0c5/fcf32fe/alacritty_terminal/tests/ref/<name>/grid.json`
- 45 files, 44.0 MiB
- SHA-256 over the sorted `name\0<file sha256>\n` list:
  `9d9abcb0c5f2a83a70beca0dc7b98928b5a1875826804e11ae3f9fd343aeeded`
- Per-file SHA-256 sums were written next to the copies as `SHA256SUMS`.
