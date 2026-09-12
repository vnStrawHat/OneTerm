# US-0072 — recording risk for every correction and deferred deviation

Date: 2026-09-12
Tool: `vt-corpus grep-deviations`
Raw output: [`US-0072-recording-risk-raw.md`](US-0072-recording-risk-raw.md)

This is the measured answer the "Affected recordings" / "Recording risk" columns of
`low-level-design/dispatch-and-modes.md` and `low-level-design/grid-and-scrollback.md`
ask for. Until those cells are updated by the design owner they still read
"measure in `US-0072`"; this file is the measurement.

## How it was measured

At the **parser** level — `vte::Parser` plus a recording `Perform` — not with a byte
search. A byte search for `\x1b[1J` misses a sequence split across a chunk boundary,
counts `1J` inside an OSC payload, and cannot tell `SGR 5` from the `5` in `SGR 38;5;n`.
The state machine gets all three right, and the SGR walk steps over extended-colour
arguments in both the semicolon and the colon form.

What it deliberately does **not** do is emulate the terminal. Several corrections only
bite under a runtime condition (`C1` needs a `DCH` count that reaches `cols - col`; `C2`
needs the cursor on row 1), and deciding those would mean reimplementing the engine this
packet exists to measure. The table names the recordings that **can** trigger each
correction; the packet that implements it writes the exact cells into that recording's
`expected-diffs.toml`, and the gate then fails on any difference outside that window.

## Results

| Id | Correction | Packet | Recording risk (measured) |
| --- | --- | --- | --- |
| **C1** | `DCH` is a plain shift left by `n` | `US-0075` | **8 recordings send `DCH`**: `decaln_reset` (1, max 1), `deccolm_reset` (1, max 15), `delete_chars_reset` (12, max 1), `erase_chars_reset` (1, max 21), `insert_blank_reset` (1, max 1), `region_scroll_down` (1, max 4), `scroll_up_reset` (1, max 11), `underline` (4, max 1). The design named only `delete_chars_reset`; seven more are at risk. The quirk needs `count >= cols - col`, so the large counts (`erase_chars_reset` 21, `deccolm_reset` 15, `scroll_up_reset` 11) are the likely ones. |
| **C2** | `ED 1` clears row 0 | `US-0075` | **2 recordings**: `vttest_cursor_movement_1` (1), `vttest_origin_mode_1` (2). The design said "any recording sending `CSI 1 J`"; these are the two. |
| **C3** | A short region scroll rotates, then blanks | `US-0075` | **2 recordings** combine `DECSTBM` with `IL`/`DL`: `vim_24bitcolors_bce` (regions 1..56 x320 and 1..57 x321; `IL` x18 max 28, `DL` x11 max 28) and `vttest_insert` (regions 0..0 x2, 2..23 x1; `IL` x24 max 24, `DL` x24 max 24). **Neither of the two recordings the design predicted** (`region_scroll_down`, `scroll_in_region_up_preserves_history`) pairs a region with a region-scroll primitive. `vttest_insert`'s region is 22 rows with counts up to 24, which is exactly the "count at or above the region height" case. |
| **C4** | Insert mode repairs wide pairs | `US-0075` | **1 recording**: `vttest_insert` (IRM set once, 1 non-ASCII character printed). The design's "if it inserts over a wide character" is confirmed possible but not certain — one non-ASCII print is a thin margin. |
| **C5** | `CPR` is region-relative under `DECOM` | `US-0076` | **8 recordings** touch one half or the other, but **no recording sends both**: `origin_goto`, `vttest_insert`, `vttest_origin_mode_1`, `vttest_origin_mode_2`, `vttest_scroll` set `DECOM` and never ask for a position; `vim_24bitcolors_bce`, `vim_large_window_scroll`, `vim_simple_edit` send `CSI 6 n` and never set `DECOM`. Risk to `grid.expect` is **none**: DSR answers are discarded by the harness anyway. |
| **C6** | `RIS` resets the colour overrides | `US-0076` | **1 recording**: `grid_reset` (1 `RIS`, 1 `OSC 104`). `OSC 104` with no argument already resets indices 0-255, so the palette is empty before the `RIS` either way. Risk to `state.expect`'s `palette.*` keys: none measured. The design's guesses `colored_reset` and `decaln_reset` set no palette entry. |
| **C7** | `OSC 4` applies complete pairs | `US-0076` | **None of the 45.** `indexed_256_colors` sends 240 `OSC 4` sequences, every one of them a well-formed `4;<index>;rgb:..` triple (odd total parameter count), which the engine being replaced already accepts. The design's "`indexed_256_colors` if it sends an even count" resolves to **no**. |
| **C8** | `? 47` / `? 1047` / `? 1048` implemented | `US-0076` | **None of the 45.** The three recordings the design named — `wrapline_alt_toggle`, `alt_reset`, `saved_cursor_alt` — all use `? 1049`, which the engine being replaced already implements. This correction is free. |
| **C9** | `DECSTR` implemented | `US-0076` | **1 recording**: `grid_reset` sends one `CSI ! p`. The engine being replaced ignores it, so implementing it **will** change that recording's grid. The design said "none expected"; that is wrong, and `grid_reset` needs an `expected-diffs.toml` at `US-0076`. |
| **C10** | `CSI ? 5 W` restores the default tab stops | `US-0076` | **None of the 45.** Confirms the design's "none expected". |
| **C11** | Blink and overline attributes stored | `US-0076` | **None of the 45.** No recording sends `SGR 5`, `6`, `53` or `55`. The design predicted `sgr` and `underline` would go red on `attrs`; both were inspected and neither carries those parameters (`sgr` exercises `9`, `4` and the `38`/`48` colour forms; `underline` exercises `4:0`-`4:3`, `21` and `24`). N-03's reason for deferring D11 to `US-0086` therefore does not hold — the correction can land in `US-0076` with no declared diff at all. |
| **G3** | Pending wrap not armed while `DECAWM` is off | `US-0075` | **4 recordings reset `? 7`**: `vttest_origin_mode_1`, `vttest_origin_mode_2`, `vttest_scroll`, `vttest_tab_clear_set` (each: `? 7` set 3x, reset 1x). Observable only through `EL 0` and `HT` while the flag would have been armed, so a declared diff may be needed in one or more of the four. |
| **D12** | Reverse wrap (`? 45`) implemented | `US-0086` | **6 recordings reset `? 45`** and none sets it: `vttest_cursor_movement_1`, `vttest_insert`, `vttest_origin_mode_1`, `vttest_origin_mode_2`, `vttest_scroll`, `vttest_tab_clear_set`. Because the mode is only ever turned **off**, implementing it changes nothing in the corpus. Risk: none. |

## Findings the design should absorb

1. **C11 is free.** No recording uses `SGR 5 / 6 / 53 / 55`. The N-03 argument for keeping
   blink and overline out of the parity packet is not supported by the corpus.
2. **C9 is not free.** `grid_reset` sends `CSI ! p`, which the old engine drops. It is the
   one correction in this list that is certain to move `grid.expect`.
3. **C8, C10, C7 are free.** All three resolve to "none of the 45".
4. **C1 and C3 are riskier than the design assumed.** C1 touches eight recordings, not
   one; C3's two real candidates are not the two the design named.
5. The remaining "measure in `US-0072`" cells (`D12`, `G3`) are answered above.
