# US-0075 verification — grid, scrollback and tracked anchors

Verifier: independent review agent, 2026-09-12.
Under review: branch `worktree-agent-a8c57df6ea2ef0000` @ `864c81c`, off `feat/vt-engine` @ `3b60094`.
Reference: `vendor/alacritty_terminal/src/{term/mod.rs,grid/mod.rs,term/cell.rs}`.

**Verdict: merge after fixes.** Two blockers (a reachable grid corruption the design's own
integrity assertion catches, and silent destruction of every primary-screen anchor on any
alternate-screen scroll), three majors, nine minors. Nothing was fixed and nothing was committed;
this file is untracked.

---

## 1. Pass / fail table

| # | Check | Result | Raw evidence |
| --- | --- | --- | --- |
| 1 | Scope: only `crates/vt/src/grid/**`, `src/lib.rs`, packet | **PASS** | `git diff feat/vt-engine...HEAD --name-status` (merge-base `3b60094`): 8 `A` under `crates/vt/src/grid/`, `M crates/vt/src/lib.rs`, `A docs/spec-intakes/IN-0029-vt-engine/US-0075-grid-and-scrollback.md`. 4254 insertions, 0 deletions. No LLD/HLD/`IN-0029.md` edit, no `crates/vt/src/parser/`, no `Cargo.toml`, no `Cargo.lock`. |
| 2a | `pwsh scripts/ci-local.ps1` green | **PASS** | `EXIT=0`, `ci-local: all checks passed.` All nine steps ran (`cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, the five Python policy checks, `third-party-notices.py --check` → "THIRD-PARTY-NOTICES.md is up to date."). |
| 2b | Raw `test result:` totals | **PASS** | 54 sections summed: **1259 passed / 0 failed / 5 ignored**. Matches the packet exactly. |
| 2c | `cargo test -p oneterm-vt` within R-28's 60 s | **PASS** | `test result: ok. 81 passed; 0 failed; 0 ignored; ... finished in 0.24s`; wall clock incl. cargo **0.84 s**. Budget 60 s. |
| 3 | Behaviour vs the reference, trap by trap | **FAIL (2 blockers)** | § 2 below. 24 traps checked; 22 match the reference or a declared correction, 2 do not. |
| 4 | The 7 recorded deviations | **6 justified, 1 needs the design owner**; 3 further undeclared deviations found | § 3 below. |
| 5 | Memory | **PASS** (one wrong figure in the packet) | Re-measured 45x160 / 100 000 LF: `heap 6291616 B, 62.9 B/row, allocated_rows 0, ring 131072` → 48 B/slot exactly as claimed. Fully written 100 000 rows: `134291616 B` = **1342.9 B/row**, not the packet's 1378.5. `set_scrollback_limit(1000)` → `heap 1434784 B, allocated 1044, ring 2048` — trimming really frees rows. |
| 6 | Performance sanity (release) | **PASS** | 200x50, 1e6 iterations each: `scroll_up` full screen **16.2 ns/op** (ring rotation — a cell memmove would be ~5 µs), `linefeed` at the region bottom **16.9 ns/op**, `insert_lines` in a 40-row region **76.7 ns/op** (O(region rows) slot moves, not O(cells)), `print` **11.8 ns/op**. |
| 7 | Code quality | **PASS** | No `unsafe`, no `unwrap`/`expect`/`panic!`/`todo!`/`#[allow(dead_code)]` anywhere in `crates/vt/src/grid/**` outside tests (scan of all five source files). No new dependency (`Cargo.toml` untouched). Every public item carries a doc comment except seven `new()` constructors, which sit directly under their type's docs. Impossible states are `debug_assert!`; untrusted counts clamp (`n.min(region.height())`, `col.min(cols-1)`, `Size::clamped`) and an unplaceable wide glyph is dropped and counted (`dropped_wide`) — matches `error-policy.md` and the crate's own "nothing panics on input" promise. |
| 8a | Packet completeness | **PASS** | Outcome / Scope / Acceptance / Documentation / Reconciliation / Plan / Decisions / Verification Plan / Evidence and Gaps / Handoff all present; status `Implemented`; proof boxes Unit + Verify command. Seven design-owner readings and eight gaps recorded. Two content errors: § 4 minor M7 and M8. |
| 8b | `harness.db` row `US-0075` | **PASS** | Read-only sqlite3: `status=implemented`, `risk_lane=high_risk`, `contract_doc=.../grid-and-scrollback.md`, `packet_doc=.../US-0075-grid-and-scrollback.md`, `unit_proof=1`, `verify_command=pwsh scripts/ci-local.ps1`, `last_verified_result=pass`, `intake_id=34`. Evidence and notes mirror the packet (incl. the same 1378.5 B error). |
| 8c | Commit trailer | **PASS** | `Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>` and `Claude-Session: https://claude.ai/code/session_018tAaHVGPnSzrEHLVcg3sx1`, both exact, both last in the body. |

Every test the LLD's Verification section names exists under its name: 41 `grid::tests::`,
6 `grid::anchor::tests::`, 1 `grid::props::scroll_and_erase_preserve_integrity` (256 cases, full
two-screen walk after every step) = the 48 claimed.

---

## 2. Trap-by-trap comparison

`ref` = `vendor/alacritty_terminal`. "probe" = a scratch integration test written for this review
under `crates/vt/tests/`, run in a debug build, and deleted afterwards.

| Trap | Pinning test | Implementation path | Verdict |
| --- | --- | --- | --- |
| 1 — pending wrap then `BS`; `BS` at col 0 | `pending_wrap_then_backspace`, `backspace_at_column_zero_is_a_noop` | `screen.rs:552-559` returns at col 0 **without** clearing the wrap, decrements and clears otherwise | **match** (`ref term/mod.rs:1415-1425`) |
| 2 — pending wrap then `EL 0` | `pending_wrap_then_el0_erases_nothing`, `decawm_off_then_el0_erases` | `screen.rs:1005-1007` early return for `Right` only | **match**; the `DECAWM`-off half is declared deviation G3 |
| 3 — pending wrap then `HT` | `pending_wrap_then_tab_wraps_and_returns` | `screen.rs:849-852` `wrapline()` then `return` | **match** (`ref :1385-1388`) |
| 5 — wide char at the last column | `wide_char_at_last_column_wrap_on_and_off` | `screen.rs:905-925` | **FAIL — blocker B1.** Wrap-off drop-and-count is G3 and fine; wrap-on writes the `LeadingWideSpacer` with `RowMut::set`, not the repairing write. Probe: cols 8, wide glyph at 6-7, `CUP` to col 7, print a wide glyph → `[N,N,N,N,N,N,Wide,LeadingWideSpacer]` and `assert_integrity` panics `wide cell without its spacer at RowId(0):6`. The reference routes the spacer through `write_at_cursor` (`ref :1120-1126` → `:1007-1024`), which clears the left half first. |
| 6 — overwriting half a wide pair (same row + cross row) | `wide_pair_repair_across_rows`, `cell::tests::wide_pair_repair_on_overwrite` | `screen.rs:944-965` + `cell.rs:repair_wide_pair_in_row` | **match**, gate included (`ref :1007-1013`: repair only when the *overwritten* cell is `Wide`/`WideSpacer`, leading-spacer release only at `col <= 1`) |
| 7 — insert mode over a wide char | `insert_mode_over_wide_char_repairs_the_pair` | `screen.rs:899-904`, `row.rs:343-353` | **match as a correction (C4)**. The reference's `column + width < columns` gate is absent, but the `n.min(cols-col)` clamp makes the shift a no-op at exactly the columns the reference skips — verified case by case. Side effects only (minor M9). |
| 8 — zero-width char at column 0 | `zero_width_at_column_zero_attaches_to_column_zero` | `screen.rs:977-995` | **match** (`ref :1082-1096`), with a redundant-but-harmless `col > 0` guard |
| 9 — `ED 2` keeps content, scrolls into history | `ed2_scrolls_the_viewport_into_history`, `ed2_keeps_the_scrolled_back_viewport_position`, `ed2_over_an_image_keeps_the_image_rows` | `screen.rs:1095-1122` | **match**. Occupancy scan uses `Cell::is_erasable` incl. R-13 (graphic cells not erasable). Probe with 8 rows of history, offset 3: `offset 3 -> 6, visible_top RowId(2) -> RowId(2), history 5 -> 8`, cursor screen index unchanged, `lines_produced` unchanged at 8. See deviation (ii). |
| 10 — `ED 3` snaps to the bottom | `ed3_resets_the_viewport_to_the_bottom` | `screen.rs:1125-1144` sets `offset = 0` **before** the "is there history" check | **match** (`ref :1810-1837`); returns `Option<RowId>` so `US-0079` can still emit `ScreenCleared` unconditionally (trap 12) |
| 11 — `ED 1` and row 0 | `ed1_clears_row_zero` | `screen.rs:1070-1077`, no `cursor_row > 1` guard | **correction C2**, as declared |
| 12 — `ScreenCleared` before the history check | (US-0076/0079) | `clear_history` returns `None` rather than short-circuiting the caller | **supported** |
| 13 — `? 47` / `? 1047` / `? 1048` | — | out of scope (C8, `US-0076`) | n/a |
| 14 — `? 1049 h` clobbers the primary `DECSC` | `entering_alt_screen_overwrites_the_saved_cursor`, `leaving_alt_screen_restores_the_entry_cursor` | `terminal_grid.rs:225-244` | **match on the cursor**; but `swap_alt` does more than the reference — see major F5 |
| 15 — invalid `DECSTBM` | `decstbm_invalid_range_is_a_noop_and_valid_homes_the_cursor` | `screen.rs:584-594` returns `false` on `top >= bottom`, every valid call `goto_origin(0,0,origin)` | **match** (`ref :2185-2207`) |
| 16 — cursor outside the region | `linefeed_below_the_region_does_not_scroll`, `insert_and_delete_lines_outside_the_region_are_noops` | `screen.rs:765-804` | **match**. Probe with region 2..5: `IL`/`DL` at index 0 → `None`, `IL` at index 5 → `None`, `IL` at index 3 → `Some(...)`; `LF` below the region walks to the last row and stops |
| 17 — scrollback fills only from a region at row 0; short region blanks | `scroll_up_fills_history_only_from_row_zero`, `scroll_up_with_a_bottom_bounded_region_keeps_the_rows_below`, `small_region_scroll_rotates_then_blanks` | `screen.rs:614-724` | **match**. Probe: region 2..5, cursor at index 4, 20 line feeds → `history 0 -> 0, newest RowId(5) -> RowId(5)`. `DL` with the cursor on row 0 of a row-0 region *does* fill history, as the reference does (`ref :1524-1534` → `scroll_up_relative` → `Grid::scroll_up` `region.start == 0`). C3's "rotate then blank" is observationally identical to the reference's short-circuit, since every row of the region is reset either way — the correction is real but produces no cell difference |
| 18 — `SD` never pulls from history | `scroll_down_never_pulls_from_history` | `screen.rs:728-761` | **match** (`ref grid/mod.rs:190-247`) |
| 19 — `DCH` end-clamp | `delete_chars_shifts_left_by_n` | `row.rs:311-323` plain `copy_within` | **correction C1**, as declared |
| 27 — tab stops after a resize | `tab_stops_survive_and_regrow_across_a_resize` | `screen.rs:189-199` | **match**. Probe 24 → 12 → 24 with stops 8 and 16 cleared: result `[0, 16]` — the cleared stop in the retained prefix stays cleared, the regrown half resurrects the absolute multiple of 8 |
| 28 — resize resets the scroll region | (covered by `ring_mask_is_constant_across_resizes`) | `screen.rs:1166-1215`, unconditional after the same-size early return | **match** (`ref :670-716`) |
| 36 — `Row::reset` bounded by `occ` | `row_reset_respects_occ_and_the_background_template` | `row.rs:109-118` | **match in spirit**; the discriminant is the interned style id rather than the background alone — declared deviation 7, safe direction |
| 37 — `is_empty` looseness | (`US-0074`) | `cell.rs:is_erasable` | **match**, minus `WRAPLINE`, which is a row flag here (declared G1) |
| 46 — first visible cell | `first_visible_cell_is_viewport_top_column_zero` | `screen.rs:319-334` | **dissolved** by the offset model, as the LLD says |
| — anchors under `IL`/`DL`/`SU`/`SD` | `region_scroll_moves_anchors_with_their_content`, `anchor_in_a_blanked_region_dies` | `anchor.rs:137-150` | **match**. Probe, region 1..5, `SU 2`: marks on rows 1-2 die, rows 3-4 shift −2, rows 0 and 5 untouched |
| — anchors dropped when their row leaves history | `trim_kills_anchors_below_oldest` | `anchor.rs:167-173` | **FAIL — blocker B2** (lane-blind; see § 4) |
| — `RowId` stability across pushes | `row_ids_are_monotonic_across_scroll_clear_and_ris`, `trimmed_slot_is_cleared_before_reuse` | `screen.rs:449-473` | **match**. Probe, 3 rows / 3 scrollback / 10 prints: ids 0..9 strictly monotonic, content follows its id, trimmed ids read as blanks and are never reused |
| — viewport offset, every row of the LLD table | `offset_zero_follows_output`, `scrolled_back_view_holds_still_while_rows_are_pushed`, `offset_is_capped_by_history` | `screen.rs:478-487`, `:668-670`, `:683`, `:1126`, `:1212` | **match**. Probe walked the table: sticky 0; `scroll_viewport(-3)` → 3; `LF` while scrolled back → 4; `SU` inside a non-zero region → unchanged; `ED 3` → 0; `scroll_viewport(-99)` clamps; `scroll_to_bottom` → 0. (`SU` inside a non-zero region: the *reference* bumps `display_offset` here too, because the bump sits above the `region.start == 0` branch in `Grid::scroll_up:265-268`; the implementation follows the LLD instead. No corpus exposure — recordings replay at offset 0.) |
| — `RowsScrolled` emission | `rows_scrolled_event_matches_the_anchor_shift` | `screen.rs:596-607, 686-695, 712-720, 747-757` | **FAIL — major F3** |
| — `assert_integrity` catches a corrupt state | (property test) | `screen.rs:1374-1400`, `terminal_grid.rs:258-287` | **PASS, demonstrated**: the blocker-B1 probe reaches a corrupt state through the public API alone and `assert_integrity` panics on it with a precise message. The walk does check row-id/slot agreement, row width, wide pairs, content-hint false negatives, both screens' runs disjoint, and every live anchor in range. It cannot see a *dead* anchor, which is why B2 hides from it. |

Not probed because the packet does not own them: `EL` with `DECSCA`-protected cells
(`Cell::protected()` is stored and documented as "not yet honoured by any erase"; `DECSED`/`DECSEL`
are `US-0076`), reverse wrap `? 45` (`US-0086`), reflow (`US-0077`).

---

## 3. Judgment on the seven recorded deviations

| # | Deviation | Judgment |
| --- | --- | --- |
| (i) | Row-id lanes per screen (`PRIMARY_ORIGIN = 0`, `ALT_ORIGIN = 1 << 63`) | **Justified — the LLD and DEC-0015 should change.** The reading is correct: one counter cannot keep two contiguous runs once both screens allocate, and a gap breaks `history_len = newest - oldest + 1 - rows`. `slot = id & mask` still holds (`ALT_ORIGIN & mask == 0`, and the screens own separate rings). No consumer breaks on the identity itself: `TerminalGrid::screen_of(id)` routes unambiguously; selection, marks, graphics placements and reflow all work inside one lane; DEC-0015's "stable identity" clause is about content across pushes, which the lanes preserve; the agent panel and the gutter read `lines_produced`, not ids. **But the reading was adopted without auditing the code that compares a `RowId` against a screen-wide bound, and that is exactly what blocker B2 is** (`Anchors::trim` is lane-blind). Minor M6 (two entries per `AnchorKind::Cursor`) is the second unaudited consequence. Adopt the lanes *and* fix both. |
| (ii) | `ED 2` bumps the scroll offset rather than leaving it | **Justified — the LLD cell and research § 8 trap 9 are both wrong.** `Grid::clear_viewport` itself does not touch `display_offset`, but the `scroll_up(&(0..lines), positions)` it calls does (`grid/mod.rs:265-268`), and `Term::clear_screen(All)` adds nothing beyond a vi-cursor correction (`term/mod.rs:1808-1820`). So the reference bumps it. Measured: offset 3 → 6 with `visible_top` unchanged. Correct the table cell to match its own prose and fix trap 9's wording. |
| (iii) | `shift_region` skips the cursor / saved cursor / viewport top; the field is the authority | **Justified.** The cursor *is* updated elsewhere, correctly and everywhere: `scroll_up_into_history` adds `+n` to both cursors (`screen.rs:662-664`) so the screen index is preserved; the region paths deliberately leave them; `IL`/`DL` read `cursor_row_index()` as the origin, which would be destroyed by a content shift. Measured: cursor index 3 preserved across full `SU 2` and `SD 1`; `DECSC` at index 4, `SU 2`, `DECRC` → index 4 col 3, which is what the reference does (its `saved_cursor.point.line` is screen-relative and no scroll touches it). `Anchors::remap` does **not** skip them, which is exactly what `reflow-and-resize.md:37-43` needs. The LLD sentence "the saved cursor's row is a tracked anchor, so `DECRC` after a region scroll lands where the reference lands" should be reworded — under `shift_region` the anchor would land somewhere the reference does not. Note the fragility: `Screen::sync_anchors` overwrites the entries from the fields, so `US-0077` must read the remapped entries back *before* syncing. |
| (iv) | Wide-pair repair is compulsory on erase and shift; `US-0076` must expect an undeclared difference | **Justified as behaviour; the risk statement is both overstated and pointing at the wrong thing.** The design's own integrity assertion forbids orphaned spacers, so the repair is forced. Corpus exposure, measured: I UTF-8-decoded all 45 recordings and **none contains a width-2 glyph** (CJK/Hangul/fullwidth ranges), so the repair produces no parity diff today. Upper bound if the scan missed something — recordings carrying both non-ASCII bytes and one of `EL`/`ECH`/`DCH`/`ICH` — is 22 of 45, the material ones being `vim_large_window_scroll` (8 hi-bytes, 836 EL), `vim_24bitcolors_bce` (10, 399 EL), `tmux_git_log` (9, 233 EL), `tmux_htop` (6, 125 EL + 1 ICH), `region_scroll_down` (72 hi-bytes, 1 DCH, 12 EL), `zerowidth` (30, 2 EL), `wrapline_alt_toggle` (24, 5 EL), `issue_855` (36, 21 EL), `fish_cc` (33, 15 EL), `colored_underline` (32, 3 EL). What *does* need declaring across the corpus is major F4, which this deviation does not mention. |
| (v) | `scroll_up` case 2: rows below keep content and screen place, not ids | **Justified.** The LLD's "keep their ids **and** their content" is impossible once `newest` advances, and the implementation reproduces `Grid::scroll_up`'s lift-rotate-put-back exactly (`grid/mod.rs:285-292` + the `region.end - positions .. region.end` reset). Measured: region 0..3 of a 5-row screen scrolled by 1 → rows below stay put, region bottom blanks, one row enters history. The LLD's case-2 *anchor* column is wrong too and the packet does not say so — see minor M8. |
| (vi) | `Cursor::template` is a `Cell`, not a `Style` | **Needs the design owner, and the recorded justification is incomplete.** The packet records only the `Row::reset` discriminant consequence (true, and safe). The consequence it does not record is that **every erase now fills with the whole style**, not the background — major F4. |
| (vii) | Both wide repairs gated on the cell being overwritten | **Justified**, byte-for-byte the reference's own gate (`term/mod.rs:1007-1013`), and the stated failure mode (a wrapping wide glyph releasing its own leading spacer) is real. |

### Three further deviations the packet does not record

Listed as findings F4, F5 and M8 below.

---

## 4. Findings

### Blockers

**B1 — a wide glyph wrapping over an existing wide pair corrupts the grid.**
`crates/vt/src/grid/screen.rs:917-924`. The `LeadingWideSpacer` is written with
`self.row_mut(id).set(col, spacer)`. `set` does not repair, so when the last column already holds
the `WideSpacer` of a pair, the `Wide` at `cols - 2` is left without its spacer — the exact state
`assert_integrity` forbids. Reachable through the public API in three operations.

```
probe: cols 8; print U+FF21 at col 6; CUP col 7; print U+FF22
row0 widths = [Narrow, Narrow, Narrow, Narrow, Narrow, Narrow, Wide, LeadingWideSpacer]
panicked at crates/vt/src/grid/screen.rs:1457: wide cell without its spacer at RowId(0):6
```

The reference writes this spacer through `write_at_cursor` (`ref term/mod.rs:1120-1126`), which
runs the same-row repair first (`ref :1007-1024`).
*Fix:* replace the `set` with `self.write_at_cursor(CellContent::Scalar(' '),
CellWidth::LeadingWideSpacer)` — it already applies the template and performs both repairs — and
extend `wide_char_at_last_column_wrap_on_and_off` with the over-a-pair case. Consider adding
`Print` at a chosen column to the property test's op set so this class is reachable there.

**B2 — any alternate-screen scroll kills every primary-screen anchor.**
`crates/vt/src/grid/anchor.rs:167-173`. `Anchors::trim(oldest)` marks dead every anchor whose row
is below `oldest`, with no lane bound. The alternate screen has `scrollback_limit = 0`, so
`push_rows` trims on *every* scroll (`screen.rs:453-458`) and `scroll_up_into_history` then calls
`anchors.trim(self.oldest)` with an alt `oldest >= 1 << 63` — above every primary row id.

```
probe: 8 rows of primary history, a Mark and a SelectionStart on RowId(5)
before swap:            mark Some(Pos { row: RowId(5), col: 0 }) sel Some(...)
alt oldest RowId(9223372036854775808) newest RowId(9223372036854775811)
after one alt linefeed: mark None sel None
back on primary:        mark None sel None
```

One line feed inside vim, tmux or htop therefore destroys every OSC 133 prompt mark, the selection
and every graphics placement on the primary screen. The screen-owned entries survive only because
`sync_anchors` revives them through `Anchors::set`. `assert_integrity` cannot catch it (it skips
dead anchors) and the property test cannot either (it only asserts live anchors are in range),
which is why 1259 green tests missed it. This violates DEC-0015's "anchors are engine-owned, not
consumer-owned" contract directly.
*Fix:* make `trim` lane-scoped — `trim(&mut self, live: Range<RowId>)` killing only
`anchor.pos.row < live.start && anchor.pos.row >= screen_origin`, or simply
`trim(origin: RowId, oldest: RowId)` with `origin <= row < oldest`. Add
`anchor::tests::an_alt_screen_scroll_leaves_primary_anchors_alone`, and strengthen the property
test to assert that a registered mark on a row still inside its screen's live range is still alive.

### Majors

**F3 — `ScrollReport.scrolled` contradicts both the anchor motion and `damage-and-render-state.md`.**
`screen.rs:686-695` (whole viewport / bounded) and `:596-607` (empty region).
`damage-and-render-state.md:188` defines `RowsScrolled` as reporting **in-region** motion, "because
that moves content without moving the viewport". The implementation emits it for every case:

```
whole-viewport SU 2: report RowsScrolled { top: RowId(2), bottom: RowId(7), delta: -2 }
  anchor Pos { row: RowId(3) } -> Some(Pos { row: RowId(3) })     // did not move
bounded SU 0..4 by 2: report RowsScrolled { top: RowId(2), bottom: RowId(5), delta: -2 }
  tail anchor RowId(5) -> RowId(7)                                 // +2, not in any report
invalid region {top:5,bottom:2}: RowsScrolled { top: RowId(6), bottom: RowId(5), delta: 0 }
```

So a `RowId`-keyed row cache that follows the report corrupts itself on an ordinary full-screen
scroll, and misses the tail shift on a bottom-bounded one. Acceptance criterion R-02 ("anchors …
match the `RowsScrolled` report") does not hold, and no test covers either case —
`anchor::tests::rows_scrolled_event_matches_the_anchor_shift` asserts the anchor shift only for the
non-zero-region case and just `delta`/`history_rows` for the full one.
*Fix:* emit `delta = 0` (or no `RowsScrolled`) when ids kept their content, and a second report for
the tail's `+n`; clamp the empty-region report to a well-formed range. If `RowsScrolled` is instead
meant to be screen-index motion, say so in the LLD and add the test that proves it.

**F4 — every erase fills with the whole style, where the reference fills with the background only.**
`screen.rs:1004-1046` (`EL`, `ECH`, `DCH`, `ICH`), `:1064-1077` (`ED 0`/`ED 1`), `row.rs:300-338`.
The fill value is `self.cursor.template`, a `Cell` carrying one interned `StyleId` — fg, bg and all
attributes — plus the extras id and the OSC 133 semantic. The reference fills with `bg.into()`:
`ref term/mod.rs:1669-1671` (`clear_line`), `:1548-1553` (`erase_chars`), `:1583-1588`
(`delete_chars`), `:1216-1219` (`insert_blank`), `:1789-1800` (`ED 0`/`ED 1`). The grid LLD's own
table says "filled with the template **background**".

```
probe: template style = fg Red, attrs UNDERLINE; ECH 4
ECH cell style = fg Named(Red) bg Named(Background) attrs Attrs(UNDERLINE)
```

Erasing while an underline, strikeout, inverse or an open `OSC 8` hyperlink is active therefore
leaves visibly underlined / hyperlinked blanks. Corpus exposure is broad — `underline`,
`clear_underline`, `colored_underline`, `selective_erasure`, `erase_in_line`, `alt_reset`
(1743 SGR / 26 EL / 8 ECH), `row_reset` (3470 SGR), `vim_24bitcolors_bce` (19064 SGR / 399 EL),
`tmux_htop`, `tmux_git_log`. This is not in the C-table and not in the packet's deviation list, so
`US-0076`'s first parity run will fail on it across many recordings with no declared diff.
The implementation's motive is sound (a background-only fill would need `&mut Interner` on every
erase path), so this is a design decision, not a slip.
*Fix / owner call:* either (a) add a `C` row "erased cells keep the whole SGR template, not only
the background" with the recordings above, or (b) keep a second interned "erase cell" on the
cursor, recomputed whenever the SGR template changes, so erases stay off the interner *and* match
the reference.

**F5 — entering the alternate screen resets its scroll region, its tab stops and clears it with the
default background.** `terminal_grid.rs:229` calls `Screen::reset`, which is `RIS`-level
(`screen.rs:1147-1157`: clear history, cursor, saved cursor, **region**, **tabs**, offset, reset all
rows with the *just-reset* default template). The LLD's own pseudocode says `alt.reset_all_rows()`.
The reference has one `scroll_region` and one `tabs` table on `Term`, shared by both screens, and
resets the alt rows with `inactive_grid.cursor.template` — which it has just cloned from the primary
cursor, so the clear is a BCE clear (`ref term/mod.rs:729-740`, `grid/mod.rs:reset_region`).

```
primary region before swap = ScrollRegion { top: 1, bottom: 4 }
alt region after entry     = ScrollRegion { top: 0, bottom: 6 }      // ref: 1..4
primary region after exit  = ScrollRegion { top: 1, bottom: 4 }      // ref: whatever alt set
primary stop at 8 before swap = False ; alt stop at 8 after entry = True   // ref: False
alt row0 style after entry = StyleId(0)  (template style StyleId(1))  // ref: the red template
```

Affects `alt_reset`, `wrapline_alt_toggle`, `saved_cursor_alt`, `tmux_htop`, `tmux_git_log`,
`vim_*`. Undeclared.
*Fix:* in `swap_alt`, reset only the alt's rows, using the entering cursor's template; leave region
and tabs alone (or, if per-screen region/tabs is the intended model, record it as a deviation with
the recordings above and state what `? 1049 l` restores).

### Minors

- **M6** — both screens register their own `Cursor` / `SavedCursor` / `ViewportTop`
  (`screen.rs:265-273`), so the list holds two entries per kind and a consumer reading
  `AnchorKind::Cursor` cannot tell which screen's it is. The LLD and DEC-0015 define `Cursor` as
  "the active screen's cursor", singular. Add a screen discriminant or document the pairing.
- **M7** — the packet and the `harness.db` evidence both say a written 160-column row costs
  "1378.5 B/row (48 B slot + 160 x 8 B of cells)". The stated arithmetic gives 1328 and the measured
  figure is **1342.9** (134 291 616 B / 100 000, i.e. 62.9 B of ring share + 1280 B of cells).
  Correct both. The headline 62.9 B/row and `allocated_rows() == 0` reproduce exactly.
- **M8** — an eighth design-owner reading the packet does not record: the LLD's `scroll_up` case 2
  anchor column says `shift_region(region, -n, kill = top n)`, which would kill anchors whose
  content is safely in scrollback and shift anchors whose content never moved. The implementation
  correctly does neither. The LLD should change.
- **M9** — `screen.rs:899-904` runs the insert-mode shift even at columns where the reference skips
  it. The clamp makes the shift itself a no-op, but the row is still opened, stamped `DIRTY` with a
  fresh `seq`, given `occ = cols`, and swept by `repair_wide_pairs`. One spurious dirty row per
  glyph in insert mode.
- **M10** — `row.rs:300-307` and `:311-353` run `repair_wide_pairs` over the **whole** row on every
  `EL` / `ECH` / `DCH` / `ICH` / insert shift, where the reference repairs only the boundary.
  Correctness-neutral, O(cols) per in-row op.
- **M11** — `blank_slot` / `reset_row` (`screen.rs:422-473`) allocate a full row whenever the
  template is not `Cell::EMPTY`. That is correct BCE behaviour, but it means a program holding a
  non-default background defeats the lazy-row memory claim entirely. The packet's memory section
  should say so.
- **M12** — the `vt-paranoid` feature `testing-and-bench.md:50` mandates is not wired, because
  `Cargo.toml` is outside the packet's file scope. Self-declared; needs a one-line follow-up so the
  R-28 tiering is actually feature-gated rather than unconditional in the property test.
- **M13** — `Screen::row(id)` on a trimmed id returns blanks, indistinguishable from a never-written
  row; only `row_range()` tells a consumer the difference. Worth a sentence in the doc comment.
- **M14** — `TerminalGrid::wrapline()` is public and unconditional, while the reference's `wrapline`
  returns immediately with `DECAWM` off (`ref :973-976`). `US-0076` must never call it in that
  state; a doc note on the method would make the contract explicit.

### Confirmed correct, worth recording

- `linefeed` at the bottom of a region that does not start at row 0 grows neither history nor
  `newest` (20 line feeds in region 2..5: `history 0 -> 0, newest RowId(5) -> RowId(5)`).
- The pending-wrap flag survives `ECH`, `DCH`, `ICH` and `EL 2` and is cleared only by the
  positioning operations, matching the reference exactly.
- `lines_produced` counts line feeds and history-filling region scrolls, and is unchanged by `ED 2`,
  an alternate-screen swap, a resize and `RIS`.
- Tab-stop resize resurrects absolute multiples of eight in the regrown region (`[0, 16]` after
  24 → 12 → 24 with 8 and 16 cleared) — trap 27 reproduced, quirk included.
- `set_scrollback_limit` really rehomes and frees: 134 MB / 100 000 allocated rows → 1.43 MB /
  1044 rows, ring 131072 → 2048.

---

## 5. Verdict

**Merge after fixes.** The packet is unusually thorough — every LLD-named test exists, the property
test carries the full two-screen walk, the memory claim reproduces to the byte, the perf shape is
right, there is no `unsafe`, no `unwrap` and no new dependency, and six of the seven recorded
deviations are correct readings that the LLD should absorb. But two defects must not ship:

1. **B1** — fix the leading-spacer write and add the regression test.
2. **B2** — make `Anchors::trim` lane-scoped and add the test.

Then resolve the three majors before `US-0076` runs the parity gate: **F3** (`RowsScrolled`
contract), **F4** (erase template — a design-owner call), **F5** (`swap_alt` doing `RIS` on the
alternate). F4 and F5, together with M8, are three further deviations that belong in the packet's
"for the design owner" list and in the LLD's deviation tables. The minors can follow.
