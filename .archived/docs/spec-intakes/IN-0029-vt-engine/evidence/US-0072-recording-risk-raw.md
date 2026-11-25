| Id | Correction or deviation | Sequences searched | Recordings that carry them |
| --- | --- | --- | --- |
| C1 | `DCH` is a plain shift left by `n` | `CSI Ps P` | `decaln_reset` (1 DCH, max count 1); `deccolm_reset` (1 DCH, max count 15); `delete_chars_reset` (12 DCH, max count 1); `erase_chars_reset` (1 DCH, max count 21); `insert_blank_reset` (1 DCH, max count 1); `region_scroll_down` (1 DCH, max count 4); `scroll_up_reset` (1 DCH, max count 11); `underline` (4 DCH, max count 1) |
| C2 | `ED 1` clears row 0 | `CSI 1 J` | `vttest_cursor_movement_1` (1 ED 1); `vttest_origin_mode_1` (2 ED 1) |
| C3 | A short region scroll rotates, then blanks | `CSI r` with `CSI S` / `T` / `L` / `M` | `vim_24bitcolors_bce` (regions 1..56 x320, 1..57 x321, scrolls Lx18 (max 28), Mx11 (max 28)); `vttest_insert` (regions 0..0 x2, 2..23 x1, scrolls Lx24 (max 24), Mx24 (max 24)) |
| C4 | Insert mode repairs wide pairs | `CSI 4 h` plus a non-ASCII print | `vttest_insert` (IRM set 1x, non-ASCII printed 1x) |
| C5 | `CPR` is region-relative under `DECOM` | `CSI ? 6 h` plus `CSI 6 n` | `origin_goto` (DECOM set 1x, CPR 0x); `vim_24bitcolors_bce` (DECOM set 0x, CPR 1x); `vim_large_window_scroll` (DECOM set 0x, CPR 1x); `vim_simple_edit` (DECOM set 0x, CPR 1x); `vttest_insert` (DECOM set 1x, CPR 0x); `vttest_origin_mode_1` (DECOM set 3x, CPR 0x); `vttest_origin_mode_2` (DECOM set 1x, CPR 0x); `vttest_scroll` (DECOM set 1x, CPR 0x) |
| C6 | `RIS` resets the colour overrides | `ESC c` plus a palette OSC | `grid_reset` (RIS 1x, palette OSC {104: 1}) |
| C7 | `OSC 4` applies complete pairs | `OSC 4` with an even colour-argument count | none of the 45 |
| C8 | `? 47` / `? 1047` / `? 1048` implemented | `CSI ? 47 h/l`, `? 1047`, `? 1048` | none of the 45 |
| C9 | `DECSTR` implemented | `CSI ! p` | `grid_reset` (1 DECSTR) |
| C10 | `CSI ? 5 W` restores the default tab stops | `CSI ? 5 W` | none of the 45 |
| C11 | Blink and overline attributes stored | `SGR 5`, `6`, `53`, `55` | none of the 45 |
| G3 | Pending wrap is not armed while `DECAWM` is off | `CSI ? 7 l` | `vttest_origin_mode_1` (?7: 3 set, 1 reset); `vttest_origin_mode_2` (?7: 3 set, 1 reset); `vttest_scroll` (?7: 3 set, 1 reset); `vttest_tab_clear_set` (?7: 3 set, 1 reset) |
| D12 | Reverse wrap implemented | `CSI ? 45 h/l` | `vttest_cursor_movement_1` (?45: 0 set, 1 reset); `vttest_insert` (?45: 0 set, 1 reset); `vttest_origin_mode_1` (?45: 0 set, 1 reset); `vttest_origin_mode_2` (?45: 0 set, 1 reset); `vttest_scroll` (?45: 0 set, 1 reset); `vttest_tab_clear_set` (?45: 0 set, 1 reset) |
