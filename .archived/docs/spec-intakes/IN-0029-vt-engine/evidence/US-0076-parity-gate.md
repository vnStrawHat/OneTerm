# Evidence: US-0076 parity gate

Intake: IN-0029
Packet: ../US-0076-dispatch-and-modes.md
Date: 2026-09-13

The 45-recording parity gate, run through the **new** engine against the
expectations frozen by the **old** one at `US-0072`, plus the old-versus-new
differential over the same recordings and over the `vt-bench` fixtures.

Commands:

```text
cargo run -p oneterm-tools --bin vt-corpus -- check --engine new
cargo run -p oneterm-tools --bin vt-corpus -- check --engine old
cargo run -p oneterm-tools --bin vt-diff -- --quiet
cargo run -p oneterm-tools --bin vt-diff -- --fixtures --quiet
```

The same two comparisons run inside `cargo test --workspace` as
`corpus_check::the_new_engine_matches_the_frozen_expectations` and
`corpus_check::the_alacritty_reference_corpus_matches_its_frozen_expectations`.

## Result

**45 of 45 green, with no `expected-diffs.json` file at all.** Every recording
matches the frozen `grid.expect` and `state.expect` cell-exactly. The
corrections the design tables predicted would need a declared window
(`C9`/`grid_reset` above all) are, measured, free.

## 1. Per recording, new engine against the frozen expectations

| Recording | Geometry | Scrollback | Bytes | Verdict |
| --- | --- | ---: | ---: | --- |
| `alt_reset` | 106x30 | 0 | 21636 | pass |
| `clear_underline` | 66x32 | 0 | 178 | pass |
| `colored_reset` | 116x63 | 0 | 201 | pass |
| `colored_underline` | 157x40 | 0 | 1747 | pass |
| `csi_rep` | 105x29 | 0 | 981 | pass |
| `decaln_reset` | 106x30 | 0 | 551 | pass |
| `deccolm_reset` | 106x30 | 0 | 1707 | pass |
| `delete_chars_reset` | 106x30 | 0 | 395 | pass |
| `delete_lines` | 116x31 | 0 | 2290 | pass |
| `erase_chars_reset` | 106x30 | 0 | 575 | pass |
| `erase_in_line` | 139x33 | 0 | 1366 | pass |
| `fish_cc` | 105x29 | 0 | 2465 | pass |
| `grid_reset` | 105x29 | 100 | 2046 | pass |
| `history` | 105x29 | 1000 | 1168 | pass |
| `hyperlinks` | 140x32 | 0 | 1175 | pass |
| `indexed_256_colors` | 105x29 | 0 | 9369 | pass |
| `insert_blank_reset` | 106x30 | 0 | 576 | pass |
| `issue_855` | 105x29 | 0 | 16625 | pass |
| `ll` | 105x29 | 0 | 2187 | pass |
| `newline_with_cursor_beyond_scroll_region` | 105x29 | 0 | 1030 | pass |
| `origin_goto` | 114x37 | 0 | 894 | pass |
| `region_scroll_down` | 116x31 | 10 | 6875 | pass |
| `row_reset` | 172x47 | 1200 | 54265 | pass |
| `saved_cursor` | 139x35 | 0 | 373 | pass |
| `saved_cursor_alt` | 139x35 | 0 | 238 | pass |
| `scroll_in_region_up_preserves_history` | 80x30 | 10 | 5753 | pass |
| `scroll_up_reset` | 102x29 | 0 | 6665 | pass |
| `selective_erasure` | 10x3 | 0 | 43 | pass |
| `sgr` | 139x35 | 0 | 1190 | pass |
| `tab_rendering` | 116x63 | 0 | 750 | pass |
| `tmux_git_log` | 105x29 | 0 | 12913 | pass |
| `tmux_htop` | 105x29 | 0 | 51126 | pass |
| `underline` | 134x64 | 0 | 1135 | pass |
| `vim_24bitcolors_bce` | 174x96 | 0 | 350752 | pass |
| `vim_large_window_scroll` | 172x47 | 0 | 303187 | pass |
| `vim_simple_edit` | 80x24 | 0 | 5061 | pass |
| `vttest_cursor_movement_1` | 105x29 | 0 | 6318 | pass |
| `vttest_insert` | 105x29 | 0 | 3854 | pass |
| `vttest_origin_mode_1` | 105x29 | 0 | 34278 | pass |
| `vttest_origin_mode_2` | 105x29 | 0 | 18521 | pass |
| `vttest_scroll` | 105x29 | 0 | 18215 | pass |
| `vttest_tab_clear_set` | 116x31 | 0 | 2100 | pass |
| `wrapline_alt_toggle` | 139x35 | 0 | 2037 | pass |
| `zerowidth` | 139x35 | 0 | 965 | pass |
| `zsh_tab_completion` | 105x29 | 0 | 433 | pass |

45 recordings, 45 pass, 0 pass-with-declared-diffs, 0 fail

## 2. The corrections, measured rather than assumed

`vt-corpus grep-deviations`' sequence scan says which recordings *can* reach
each correction; the gate says whether any of them actually does. Both columns
are below, because "no recording sends the sequence" and "the sequence is sent
and the outcome is identical" are different findings.

| Correction | Sequence reaches | Grid or state difference |
| --- | --- | --- |
| C1 `DCH` is a plain shift left | 8 recordings send `CSI P` (`decaln_reset`, `deccolm_reset`, `delete_chars_reset` x12, `erase_chars_reset`, `insert_blank_reset`, `region_scroll_down`, `scroll_up_reset`, `underline` x4) | **none** — the quirk needs `count >= cols - col` and no recording reaches it |
| C2 `ED 1` clears row 0 | 2 recordings (`vttest_cursor_movement_1`, `vttest_origin_mode_1`) | **none** — row 0 is already blank where they send it |
| C3 short region scroll rotates then blanks | 15 recordings pair `DECSTBM` with a region scroll | **none** |
| C4 insert mode repairs wide pairs | `vttest_insert` | **none** |
| C5 `CPR` region-relative under `DECOM` | 3 recordings send `CSI 6 n`, none with `DECOM` set | **none**; answers are discarded by the harness anyway |
| C6 `RIS` resets the colour overrides | `grid_reset` (one `RIS`, one `OSC 104`) | **none** — `OSC 104` had already emptied 0-255 |
| C7 `OSC 4` applies complete pairs | `indexed_256_colors` (240 well-formed triples) | **none** |
| C8 `? 47` / `? 1047` / `? 1048` | **none of the 45** — the three alt recordings all use `? 1049` | none |
| C9 `DECSTR` implemented | `grid_reset` **does** send `CSI ! p` | **none.** The design called this "the one certain diff in this table"; measured, the soft reset's effects are all overwritten by what follows it in that recording. `terminal::tests::decstr_soft_reset_scope` proves the sequence is implemented and not silently dropped |
| C10 `CSI ? 5 W` | **none of the 45** | none |
| C11 blink and overline stored | **none of the 45** sends SGR 5 / 6 / 53 / 55 as a final parameter | none |
| C12 wide pairs repaired after every in-row mutation | no width-2 glyph in any recording | none |
| C13 / C14 reflow corrections | **unreachable**: `corpus_replay` runs at one fixed geometry and never resizes, and `DECCOLM` clears without resizing (trap 40) | none |
| C15 selection kill over clamp | **unreachable**: the corpus carries no selection state in either expectation file | none |

`C12`-`C15` were added to `corpus::KNOWN_DEVIATIONS` in this packet so a later
window can name them without reopening the array.

## 3. The gate is live

Three real differences were found and fixed while getting to green, which is
the evidence that the comparator is not comparing the new engine with itself:

| Found on | Difference | Cause |
| --- | --- | --- |
| `sgr`, 810 cells over 6 rows | trailing blanks carried `bg=i1` where the reference has `nBackground` | `Screen::row_mut` materialised an unwritten slot with the cursor's **erase** cell, so one glyph landing on a fresh row repainted every untouched column with the live background-erase colour |
| `tui_redraw` fixture, 38 cells | `wrap` 1 where the reference has 0 | the `WRAPPED` row flag (deviation G1) outlived the cell that carried it; the reference wipes `WRAPLINE` on any write to the last column |
| `terminal::tests::osc_parameters_past_the_sixteenth_are_re_split` | palette indices past the eighth unset | the OSC accumulator ate the separator closing the sixteenth parameter, so P9's re-split was not reversible |

## 4. Old engine against the frozen expectations (the comparator's self-test)

```text
45 recordings, 45 passed, 0 failed (Old engine)
```

Unchanged from `US-0072`, which is the point: the expectations did not move.

## 5. `vt-diff`, old engine against new — the 45 recordings

```text
| Recording | Bytes | Rows | Result |
| --- | ---: | ---: | --- |
| `alt_reset` | 21636 | 30 | identical |
| `clear_underline` | 178 | 32 | identical |
| `colored_reset` | 201 | 63 | identical |
| `colored_underline` | 1747 | 40 | identical |
| `csi_rep` | 981 | 29 | identical |
| `decaln_reset` | 551 | 30 | identical |
| `deccolm_reset` | 1707 | 30 | identical |
| `delete_chars_reset` | 395 | 30 | identical |
| `delete_lines` | 2290 | 31 | identical |
| `erase_chars_reset` | 575 | 30 | identical |
| `erase_in_line` | 1366 | 33 | identical |
| `fish_cc` | 2465 | 29 | identical |
| `grid_reset` | 2046 | 129 | identical |
| `history` | 1168 | 1029 | identical |
| `hyperlinks` | 1175 | 32 | identical |
| `indexed_256_colors` | 9369 | 29 | identical |
| `insert_blank_reset` | 576 | 30 | identical |
| `issue_855` | 16625 | 29 | identical |
| `ll` | 2187 | 29 | identical |
| `newline_with_cursor_beyond_scroll_region` | 1030 | 29 | identical |
| `origin_goto` | 894 | 37 | identical |
| `region_scroll_down` | 6875 | 41 | identical |
| `row_reset` | 54265 | 1247 | identical |
| `saved_cursor` | 373 | 35 | identical |
| `saved_cursor_alt` | 238 | 35 | identical |
| `scroll_in_region_up_preserves_history` | 5753 | 40 | identical |
| `scroll_up_reset` | 6665 | 29 | identical |
| `selective_erasure` | 43 | 3 | identical |
| `sgr` | 1190 | 35 | identical |
| `tab_rendering` | 750 | 63 | identical |
| `tmux_git_log` | 12913 | 29 | identical |
| `tmux_htop` | 51126 | 29 | identical |
| `underline` | 1135 | 64 | identical |
| `vim_24bitcolors_bce` | 350752 | 96 | identical |
| `vim_large_window_scroll` | 303187 | 47 | identical |
| `vim_simple_edit` | 5061 | 24 | identical |
| `vttest_cursor_movement_1` | 6318 | 29 | identical |
| `vttest_insert` | 3854 | 29 | identical |
| `vttest_origin_mode_1` | 34278 | 29 | identical |
| `vttest_origin_mode_2` | 18521 | 29 | identical |
| `vttest_scroll` | 18215 | 29 | identical |
| `vttest_tab_clear_set` | 2100 | 31 | identical |
| `wrapline_alt_toggle` | 2037 | 35 | identical |
| `zerowidth` | 965 | 35 | identical |
| `zsh_tab_completion` | 433 | 29 | identical |

45 recordings, 45 identical, 0 differing
```

## 6. `vt-diff`, old engine against new — the `vt-bench` fixtures

The corpus is real captured sessions and has no 24-bit SGR churn, no
CJK-filled screen, no Sixel and no OSC 9;7. The bench generators do, at the
160x45 geometry `vt-bench` measures, 256 KiB each.

```text
| Recording | Bytes | Rows | Result |
| --- | ---: | ---: | --- |
| `plain_ascii` | 262208 | 10045 | identical |
| `long_lines` | 262254 | 10045 | identical |
| `heavy_sgr` | 262160 | 10045 | identical |
| `tui_redraw` | 262150 | 10045 | identical |
| `scroll_region` | 262157 | 10045 | identical |
| `cjk_wide` | 262328 | 10045 | identical |
| `dense_cells` | 333124 | 10045 | identical |
| `scrolling` | 262278 | 10045 | identical |
| `sixel` | 262178 | 10045 | identical |
| `osc_9_7` | 262192 | 10045 | identical |

10 recordings, 10 identical, 0 differing
```
