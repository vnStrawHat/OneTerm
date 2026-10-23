# US-0198 verification (advisor)

## Advisor code review

- Packets: `US-0196-vertical-tab-rail.md` Rework 3 (commits `bfc41610`, `78458872`) and `US-0198-collapsible-rail.md` (commits `ae1bdf12`, `832242aa`, `a866cc93`); `git diff 734096a4..a866cc93`, branch `verify/in-0055-rework3-us-0198` in worktree `agent-ad487657edc3792e1`.
- Read: `tab_rail.rs` (`rail_width`, `RailFrame`, `begin_frame`, `render`, `header`, `row`), `tab_title.rs` (`tab_cwd`, `collapsed_row`, `render_tab_strip`), `terminal_panel.rs:443-448` (`breadcrumb_label`), `status.rs:30-33`, `ui_config.rs:117-209` (lenient reader), `:483-500` (setter), `tab_row_layout.rs`; the new tests in `tab_rail_tests.rs` and `panel/tests.rs`; the two packets, the HLD (Rail anatomy, wireframes, State Machine, Data Flow 6, Data Ownership), `docs/gui-layout.md` "Vertical tabs", `docs/agents/persistence.md`; gpui 0.3.9 (`gpui-pre-0.3.9`) `elements/div.rs:2965-2990` (drag start: `pending_mouse_down` set when the hitbox is hovered, the drag starts on the first move past the threshold only while `!cx.has_active_drag()`), `window.rs:5958` (mouse listeners dispatched in reverse paint order), `div.rs:4322-4328` (`scroll_to_item` records an index resolved in prepaint against the live child bounds), `div.rs:2481-2485` (the offset is clamped to the content, in pixels).
- Gate re-run here (`CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2`, worktree `target`): `cargo test -p oneterm-settings -p oneterm-state -p oneterm-terminal-view -p oneterm-workspace` = 56 + 77 + 414 + 106 passed, 0 failed (3 + 3 ignored as recorded); `cargo clippy` on the same four crates `--all-targets -- -D warnings` clean; `cargo fmt --check`, `check-english.py`, `check-doc-paths.py` green. The full gate was not re-run (the coder's `pwsh scripts/ci-local.ps1` record stands; disk on `D:` is at 6.6 GB free).
- Tests added (commit on this branch, `tab_rail_tests.rs`): `forty_collapsed_cells_scroll_without_a_gutter_and_reveal_the_active_one` (coverage of the coder's untested point 4: 40 collapsed cells overflow, 43 px wide, the newly active one is revealed; passes) and the pin `expanding_the_rail_keeps_the_active_row_in_view` (`#[should_panic]`; finding 1; flip by removing the attribute).

## Verdict

**PASS with defects.** One low-severity behaviour defect (finding 1, pinned), the rest documentation or informational. The grip overlay, the row divider and drop lines, the header, the breadcrumb sharing, the lenient `collapsed` parse, the elevation path and the records all hold.

## The coder's own points

1. **Grip overlay hit-testing (collapsed).** Holds. The grip is `absolute().left_0().top_0()` with `w(12) h_full()` and is pushed as `trail` after the title (`tab_rail.rs:659-665`, `:733-735`), so it paints last. gpui dispatches mouse listeners in reverse paint order (`window.rs:5958`), and a drag starts in the first listener whose `pending_mouse_down` is set and that sees no active drag (`div.rs:2979-2984`); both the grip's and the title row's hitboxes are hovered over the 12 px overlap (`Hitbox::is_hovered` is membership in the hit-test stack, `window.rs:862`), so the grip's listener runs first there and starts `DragPanel`, the title's sees `has_active_drag()` and stays idle. Outside the overlap only the title row's `on_drag` is hovered, so content drag wins on the right side. The press itself: `pending_mouse_down` is set by the mouse-down of every hovered drag element (`div.rs:2965`), so the press on the grip lands. Pinned by the coder's `a_collapsed_cell_keeps_the_content_drag_and_the_grip_reorder` (grip at `GRIP_SELECTOR-2`, 12 px wide; content at title centre + 8 px). One side effect, not a defect (finding 4): the title row's middle-click and context-menu listeners also fire for a press on the overlay, because the title row is hovered under the grip; expanded, the grip is beside the title and takes neither.
2. **Row border vs drop lines, the 1 px content-box shrink.** Holds. `border_b_1` in `theme.border` on the `Tab` (`tab_rail.rs:692-694`), sizes are border-box so the row stays 64 / 44 px (coder's test: grip column 63 px); `DropEdge::Above` sets `border_t_2().border_b_0()` and `Below` `border_b_2()`, both with `border_color(line)` (`:706-712`), so a row never shows two colours. The selected accent bar is `absolute().top_0().bottom_0()` against the padding box, so it stops above the divider. The one consequence worth a line in the records (finding 2): the expanded row's inner `v_flex` budget is still 10 + 24 + 20 + 10 = 64 px (`tab_title.rs:359-375`, `TabRowLayout::VERTICAL_ROW_PADDING` and the two line heights) inside a 63 px content box; taffy shrinks the two fixed-height lines by a fraction of a pixel. Nothing visible, and `vertical_first_line_centre()` (used for the grip glyph) is off by under 0.3 px; `a_long_directory_is_cut_at_its_start_and_follows_the_width` and the geometry tests still pass.
3. **Header fit in 44 px.** Holds by construction: the collapsed header is a `v_flex().items_center().gap_1().py_1()` of three `xsmall` ghost icon buttons (`tab_rail.rs:531-541`; the `+` is `Button::new("add-shell-tab").xsmall()`, `terminal_panel.rs:725-731`), each narrower than the 43 px content width. Unlike the expanded header it has no fixed height (`HEADER_HEIGHT` is applied only to the `h_flex` branch, `:543`); its height would change if the zoom button came and went, but `TerminalPanel::zoom_control` is `PanelControl::Both`, so the zoom button is always there (finding 3, informational). The on-screen spacing is the `ui-verifier`'s.
4. **Untested: tooltip, recording dot, 40 collapsed rows + overlay scrollbar vs badge.** The tooltip is `Tooltip::new(drag_title)` on the title row only `when(collapsed)` (`tab_title.rs:441-445`); the recording dot is an absolute `top/left 3 px` child of `collapsed_row` (`:271-277`). 40 collapsed rows: now covered by the advisor test (overflow, no gutter, reveal of the newly active cell). The overlay scrollbar over the badge is a visual item for `ui-verifier`. What the 40-row case did expose is finding 1.

## Everything else checked

- **`breadcrumb_label` for SSH and before the first OSC 7.** `tab_cwd` is `self.breadcrumb_label(cx)` (`tab_title.rs:55-57`), the same function `status.rs:30-33` hands the status bar through `oneterm_state::active_terminal::breadcrumb`. It is `active_terminal()?` then `session.cwd()?` then `cwd.display().to_string()` (`terminal_panel.rs:445-448`): `None` for an empty Space or before any OSC 7 (line 2 shows no path; the status bar's widget hides, `widgets/breadcrumb.rs:4`), and for SSH the `PathBuf` built from the remote string is displayed unchanged (`PathBuf::display` rewrites nothing; `the_rows_directory_is_the_status_bars` pins `/srv/app/releases`). Equality with the status bar is by construction, which is what the owner asked. `cwd_display` / `is_drive_path` and their test are gone; no other caller existed. Collapsed cells never call it (`cwd = vertical.then(..)`, `:307`), so the strip costs no cwd read.
- **Lenient `collapsed`.** `VerticalTabs::from_value`'s `flag` reads `Bool` only; `null`, a missing key, a string, a number or an array give `false` with a warning and leave `enabled` and `width` intact (`ui_config.rs:181-190`); the whole-object fallback covers a non-object. `the_collapsed_flag_is_lenient_and_written_alone` pins `"yes"`, `1`, `[]`, `null`, and that off-and-collapsed serialises as `{"collapsed": true}` (`is_default` on the struct, `Not::not` on the field).
- **Setter and elevation.** `set_vertical_tabs_collapsed` returns before touching the entity when unchanged, else updates the entity, `persist` (which asks `write_refusal(is_restricted())` first) and `refresh_windows` (`ui_config.rs:490-500`), the same shape as the width setter. The header button captures the rendered `collapsed` and writes `!collapsed`, right under `refresh_windows` since every rail re-renders after a flip. `set_vertical_tabs_enabled` never touches `collapsed` (switch-off memory; `the_switch_off_keeps_the_rail_collapsed`).
- **Per-frame cost.** `begin_frame` reads `UiConfig::read_vertical_tabs` once per group per frame as before and now also keeps `collapsed` in the `RailFrame` cell; `header` builds one more `Button`; a collapsed row clones one `SharedString` for the tooltip closure. No new entity reads, no new observers, nothing per row beyond the element tree. The handle and tracker are not built while collapsed (`:468-472`).
- **Scrolling.** `scroll_to_item(position)` is resolved against the live child bounds in prepaint, so it is correct for 44 px cells (advisor test). `overflowing` is the previous frame's `max_offset().y > 0`, unchanged.
- **Crate rules.** No new edge: `oneterm-workspace` already depended on `oneterm-settings` and `oneterm-state`; `oneterm-terminal-view` reads only `TabRowLayout` from `oneterm-state` and hardcodes no width (the cell is `flex_1`; the test host's `44.` is test-only). The shell reaches the cell through `PanelHandle::title` inside the `Collapsed(position + 1)` scope, as the HLD's rendering seam says.
- **Records.** `docs/gui-layout.md` Collapsing paragraph, wireframe and bullets match the code (44 px, no handle, no gutter, grip overlay painted after the content, tooltip, "Close Tabs Below", chips not drawn); `docs/agents/persistence.md` names the writer and the switch-off memory; the HLD header row, Data Ownership row and the collapsed wireframe match; `IN-0055.md` links the packet and leaves its tick for acceptance; the US-0196 Rework 3 section names the deleted helper, the divider token and the drop-line rule. `check-doc-paths` and `check-english` green.

## Findings

1. **Low (behaviour) — the active row can leave the viewport on collapse or expand with many tabs.** `tab_rail.rs:368-372`: the rail calls `scroll_to_item` only when `active_changed(&self.last_active_ix, active_ix)`; a layout flip keeps the `ScrollHandle`'s pixel offset (gpui clamps it to the new content, `div.rs:2481-2485`) while every row changes height 64 <-> 44 px, so the row that was in view is no longer the one at that offset. Reproduction (pinned, `should_panic`): 40 tabs, collapsed, select tab 39 (revealed at the bottom), click Expand: row 39 is drawn below the list's bottom edge; the user has to scroll to find the selected tab. Fix shape: remember the last drawn `collapsed` in `TabRail` (a `Cell<bool>` next to `overflowing`) and, when it differs in `render`, call `self.scroll_handle.scroll_to_item(position of active_ix)` (or simply `self.last_active_ix.set(None)` from the header button's click handler so the existing `active_changed` path re-reveals it on the next frame). Then remove `#[should_panic]` from `expanding_the_rail_keeps_the_active_row_in_view`.
2. **Low (records) — the expanded row's inner budget is 64 px in a 63 px content box.** `tab_title.rs:359-375` and `TabRowLayout::VERTICAL_ROW_PADDING` / `VERTICAL_FIRST_LINE_HEIGHT` / `VERTICAL_SECOND_LINE_HEIGHT` still sum to 64 while the `Tab`'s `border_b_1` leaves 63 px; taffy absorbs the 1 px by shrinking the two lines fractionally. The HLD "10 px above line 1 and below line 2" and the Rework 3 note "inside the row's 64 px" should say the divider takes 1 px of the bottom padding; or make it exact by lowering the bottom padding to 9 px (one constant). No visible effect either way.
3. **Info — the collapsed header has no fixed height** (`tab_rail.rs:531-541` vs `HEADER_HEIGHT` on `:543`). Stable today because the zoom button is always present for terminal panels; if a panel without `PanelControl::Both` ever shares the rail, the cells would shift by one button height when zoom toggles. Optional: `min_h(HEADER_HEIGHT)` or a fixed `h` of three buttons.
4. **Info — middle-click and right-click on the 12 px grip overlay reach the title row collapsed, not expanded** (`tab_rail.rs:659-665`; the row's `on_mouse_down(Middle)` and context menu at `tab_title.rs:455-470` fire for any hovered press). Harmless (a bigger close / menu target) but undocumented; a half sentence under "The reorder grip" in `docs/gui-layout.md` if the owner cares.
5. **Info — packet Verification Plan names `bash scripts/ci-local.sh`** (`US-0198-collapsible-rail.md:98`) while the gate that ran is `pwsh scripts/ci-local.ps1` (explained in Evidence). Cosmetic.

## For the owner / ui-verifier to check by hand

- The stacked header's glyphs and spacing in both themes; the collapse / expand button position beside zoom when expanded.
- The badge against the overlay scrollbar thumb with 40 collapsed cells; the tooltip text; the recording dot's corner.
- Finding 1 on screen: with 40 tabs select the last, expand, see whether the selected row is in view.
- Restart persistence of `collapsed`; the elevated window collapsing in memory with no write.
- The divider and the drop line on a hovered row in a light and a dark theme.


## UI verification (US-0196 rework 3 and US-0198, ui-verifier, 2026-10-21)

Build: copy of the debug binary for head a866cc93, private profile, 1280x800. Screenshots (prefix `uiv198-`) in the session scratchpad `uiv198-shots\`. Child shells die in this sandbox, so no status badge, no OSC 7 cwd.

### US-0196 rework 3
| Item | Verdict | Evidence |
|---|---|---|
| 1 px bottom divider on each vertical row | PASS | Dark `02`: pixels 62,68,81 at y=129, 193, 257 (one row each); light `21`, `24` faint but visible. |
| Rows still 64 px | PASS | Dividers 64 px apart (`02`). |
| Horizontal strip unchanged | PASS | `16` (switch off): no divider, same strip as before. |
| Drop line distinct from the divider | NOT WALKED | Posted-message drags (`03`..`03g`) showed only the drag ghost; no drop line and no reorder happened, so this is a driver limit, not a verdict. Owner-only. |
| Line 2 cwd equals status bar cwd | NOT WALKED (owner-only) | No cwd reported in the sandbox. |

### US-0198
| Item | Verdict | Evidence |
|---|---|---|
| Collapse button in header, tooltip "Collapse Tabs" | PASS | `04` |
| Rail becomes 44 px; terminal follows | PASS | `05` (strip x=8..51, border at 51) |
| Position numbers 1,2,3 centred | PASS | `05`, crop shows centred digits; 2-digit numbers fine (`12`, `13`) |
| Status badge top-right | NOT WALKED | needs a live shell status |
| Tooltip with title on cell hover | PASS | `06`, `23` |
| Expand button tooltip "Expand Tabs" | PASS | `08` |
| `+` and zoom stacked, fit; `+` menu opens | PASS | `05`, `11` |
| No resize handle while collapsed | PASS | `14`: drag from the edge changes nothing, no hover line |
| Click selects; accent bar follows | PASS | `09` |
| Context menu works (Rename, Duplicate, Close, Close Others, Close Tabs Below) | PASS (see D1) | `10` |
| `vertical_tabs.collapsed` persists across restart | PASS | config held `collapsed: true`; relaunch `22` still collapsed (light theme kept too) |
| Switch off then on returns to collapsed rail | PASS | `16` strip, `17` collapsed rail; file kept `{"collapsed": true}` while off |
| 39 collapsed tabs: scroll, active in view | PASS | `12` (active 39 in view), `13` (wheel back to top) |
| Scrollbar versus badge | NOT WALKED (no badge). The thumb (x=41..47) is clear of the centred numbers, including 2 digits (`12`). The top-right badge sits where the thumb is drawn, so an owner check is needed with a real status icon. |
| Light and dark, expanded and collapsed | PASS | dark `02`, `05`; light `19`, `20`, `21`, `23`, `24` |

### Defects
- D1 (low): in the collapsed rail, the cell tooltip of the hovered cell stays on screen when the right-click menu opens and covers the menu's first item (Rename...). Screen: collapsed rail. Steps: hover cell 2 (tooltip), right-click it (`10`).
- D2 (low): in the expanded rail with a scrollbar (many tabs), the row divider stops at the scrollbar gutter (light `21`: ends x=212, rail edge 227), so the divider looks cut short. Steps: 40 tabs, expanded.
- Note (info): idle numbers in collapsed cells are fairly dim in dark (`05`); legible.

### Owner-only checks
Drop line versus divider while dragging a row (and grip reorder); line 2 cwd equals the status bar (`cmd` tab); status badge in the cell corner and its overlap with the overlay scrollbar thumb; recording dot; elevated window; middle-click close in collapsed cells.

Verdict: the UI meets the requirements for everything walkable (partly: the items above are not walked).

## Advisor re-review (rework after A1-A5 and U1-U2, 2026-10-21)

- Reviewed: `git diff e5147f23..11717240` (commits `b2ed9ebf`, `d2d09538` evidence merge; `1e592798` fix; `11717240` docs) on `verify/in-0055-rework3-us-0198`. No conflict markers anywhere under `crates/`, `docs/`, `scripts/` (scanned by script, not grep).
- Gate re-run here (`CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2`): `cargo test -p oneterm-settings -p oneterm-state -p oneterm-terminal-view -p oneterm-workspace` = 56 + 77 + 415 + 108 passed, 0 failed; clippy on the four crates `--all-targets -- -D warnings` clean; `check-english.py` green. The advisor pin `expanding_the_rail_keeps_the_active_row_in_view` is unpinned (no `should_panic`) and passes, so finding 1 is closed by a test that failed before the fix.
- gpui 0.3.9 read for U1: `elements/div.rs:3746-3752` (any mouse-down clears a visible non-hoverable tooltip), `:3836-3842` (a mouse move while hovered and with no active tooltip schedules a new show), `:698-708` (`.tooltip(..)` is `hoverable: false`); kit `button.rs:674,682` (`XSmall` = `size_5` / `h_5` = 20 px).

### Verdict

**PASS.** All five fixes do what the records say; the remaining notes are informational.

### Each fix

- **A1 `last_collapsed` re-reveal** (`tab_rail.rs:220`, `:235`, `:377-385`). `tab_changed` and `layout_changed` are both evaluated as `let`s before the `||`, so neither `Cell` misses a frame. Steady state: `replace(collapsed)` returns the same value, `layout_changed` is false, nothing fires per frame. First frame: `last_collapsed` starts `false`, so a rail that starts collapsed (persisted) reports a change on its first frame; harmless, because `tab_changed` is true on that frame anyway (`last_active_ix` starts `None`) and the result is the same single `scroll_to_item`. A click on a row still sets `last_active_ix` first (no snap); a later flip re-reveals on purpose. Rail off then on: `render` is not called while off, so the cell is stale only across a state that cannot change `collapsed` (the button lives in the rail). Split centre: one `TabRail` per group, each with its own cell.
- **A2 9 px bottom padding** (`tab_title.rs:382-386`, `tab_row_layout.rs:35-37`). `pt 10 + 24 + 20 + pb 9 = 63` = the content box under `border_b_1`; `vertical_first_line_centre()` stays 22 (test). One coupling note (N1 below): the rail draws the divider as the literal `border_b_1`, not `px(TabRowLayout::VERTICAL_ROW_DIVIDER)`, so the constant describes the rail rather than driving it.
- **A3 `COLLAPSED_HEADER_HEIGHT`** (`tab_rail.rs:84`, `:544`). `3 * 20 + 4 * 4 = 76` matches the content exactly (`py_1` 4 + 4, two `gap_1`s, three `h_5` buttons); as `min_h` with border-box sizing the header is 77 px when all three buttons are present (76 content + 1 px divider) and never shrinks below 76 when one is absent. Test asserts `rows[0].top() - rail.top() >= 76`.
- **U1 `CellTooltip` + `tab_tooltip_hidden`** (`tab_title.rs:245-258`, `:464-488`; `terminal_panel.rs:180`, `:302`). Mechanism check against gpui: the right-press itself already clears the visible tooltip (`div.rs:3746-3752`, the kit tooltip is non-hoverable); what the walk saw is the re-show, because the cell stays hovered beside the menu and the next mouse move schedules a fresh show (`:3836-3842`). The flag targets exactly that: the new tooltip view draws an empty `div` while `hidden` is set, and `window.refresh()` on the press makes the current frame honour it. Stale-flag cases: (a) menu dismissed with Escape, or by a click on an item, while the pointer never left the cell: no hover-enter fires, so that cell's tooltip stays empty until the pointer leaves and returns, which is what `docs/gui-layout.md` says ("until the pointer returns"); (b) right-press then leave without a menu: cleared on return; (c) the flag is per `TerminalPanel`, and a panel is one cell in one group, so no cross-cell sharing; the drag preview and the strip never read it (`when(collapsed)`). No leak: the `CellTooltip` entity is created by the tooltip builder per show and dropped with gpui's `ActiveTooltip`; `on_hover` writes a `Cell` without a refresh, the press refreshes once; no notify loop. The test pins the flag only; the drawn tooltip is the ui-verifier's.
- **U2 gutter inside each row** (`tab_rail.rs:182-184`, `:425-429`, `:708`). The `Tab` is `w_full` so the row, its divider and the drop lines span the rail; `pr(Scrollbar::width())` shrinks only the content box, so line 2's status icon still clears the thumb (40-tab test: `first.right() == list.right()`, content right <= list right - 16). Toggling `overflowing` moves the content by 16 px as it did when the padding sat on the list; no new layout shift. Collapsed rows get no gutter (`gutter: overflowing && !collapsed`), as before.

### Notes (informational, no action required)

- **N1** — `tab_rail.rs:710` `border_b_1` and `tab_row_layout.rs:37` `VERTICAL_ROW_DIVIDER = 1.0` are the same 1 px twice; `.border_b(px(TabRowLayout::VERTICAL_ROW_DIVIDER))` would make the state constant the one source.
- **N2** — The code comment at `tab_title.rs:477-479` says gpui "keeps a tooltip while its own element is hovered"; it is re-shown after the press clears it (gpui `div.rs:3746-3752`, `:3836-3842`). Same fix, more exact wording.
- **N3** — U1 case (a) above: after an Escape with the pointer still on the cell, the tooltip is an empty view until the pointer leaves. Acceptable; a dismiss hook on the kit menu would be the only cleaner path.

### Still by hand (ui-verifier / owner)

- U1 on screen: right-click a collapsed cell, wait past the tooltip delay, confirm no tooltip over the menu; dismiss with Escape without moving, move out and back, confirm the tooltip returns.
- U2 on screen: 40 expanded tabs, dividers reach the rail's right edge, status icons clear the thumb.
- A1 on screen: 40 tabs, select the last, collapse and expand, the selected row stays in view.

## UI re-walk (rework head 11717240, ui-verifier, 2026-10-21)

Screenshots `uiv198b-*` in the scratchpad `uiv198-shots\`. Dark theme, 1280x800.

| Item | Verdict | Evidence |
|---|---|---|
| U1 tooltip gone while the right-click menu is open, "Rename..." not covered | PASS | `01`, `02` (menu fully visible, Rename... readable, no tooltip) |
| U1 Escape dismiss, move off and back: tooltip works again | PASS | `03`, `04` |
| U1 click-away dismiss, tooltip works again | PASS | `05`, `06` |
| U2 expanded, 40 tabs: divider spans full rail width | PASS | `12`, `33`: divider pixels (62,68,81) at x=150..227 on every row; rows 64 px apart |
| U2 row content not under the scrollbar thumb | PASS | thumb x~217..222, row text ends x~134 |
| A1 select a tab, Expand: selected row visible; collapse again: still visible | PASS (with the D3 caveat) | selected via the expanded rail (`34`), collapse shows it (`35`, tab 12 with accent); expand/collapse with the last tab selected keeps it visible (`09`, `10`, `12`, `13`, `33`) |
| Expanded spacing (9 px + 1 px divider), collapsed header buttons | PASS | `33`; header column unchanged (`07`, `28`) |
| Light theme | NOT RE-WALKED (no light change in this rework; `19`..`24` of the first walk stand) |

### New defect
- **D3 (high, probably a regression or a latent bug of the collapsed rail): in a collapsed rail that overflows (18 and 40 tabs), clicking a cell does not select the tab.** Expected: the clicked tab becomes active (accent bar, terminal switches). Observed: the active tab stays the same (`22`, `26`, `27`: 34 stayed active after clicks on 35, 33 and 28; `29`..`32`: 18 stayed active after clicking 3; `37`: 12 stayed active after clicking 14), and in several cases the list scrolled by about a page after the click (`21`, `22`, `24`, `27`, `29`). The same click selects normally with 5 tabs in the collapsed rail (`38`) and in the expanded rail with 40 tabs (`34`). Likely cause to check (not a code review): with no gutter reserved, the overlay scrollbar's track may take presses over the whole cell width; a click that pages the list instead of selecting is consistent with the jumps. Steps: `vertical_tabs.enabled` and `collapsed` true, add 18 or more tabs, click any cell other than the active one.
- Because of D3 A1 was exercised by selecting in the expanded rail; selecting tab 39 by clicking in the collapsed rail (the owner's flow) is blocked by D3.

Verdict: partly. U1 and U2 pass; D3 needs a fix and a re-walk.

## Advisor re-review 2 (D3 narrow scrollbar, N1, N2; 2026-10-21)

- Reviewed: `git diff 705db540..8da2c685` (`fd6fd8c1` UI re-walk evidence, `8da2c685` D3 fix + N1/N2) on `verify/in-0055-rework3-us-0198`. Worktree clean before and after the experiment below.
- Gate re-run here (`CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2`): four-crate tests 56 + 77 + 415 + 109 passed, 0 failed; clippy on the four crates `--all-targets -- -D warnings` clean.
- **Does the test fail without the fix?** Yes. With `tab_rail.rs:501-504` reverted locally to the plain `.vertical_scrollbar(&self.scroll_handle)` (not committed; file restored with `git checkout`, worktree clean), `a_click_on_a_cell_of_an_overflowing_collapsed_rail_selects_it` fails at `tab_rail_tests.rs:1299` with `click at 0.8: left 3, right 5`: the click at x = 34 of 43 px on cell 5 was swallowed by the 16 px track and the selection stayed on cell 3 (the 0.5 click had passed). With the fix the test passes.

### Verdict

**PASS.** The root cause is the kit's track hit area, `.styles(..)` narrows that hit area and not only the paint, the mount is the kit's own, the thumb still drags, and the one remaining overlap (the badge's right edge) is the accepted cost the records state.

### Attack points

- **Root cause (kit `reference/gpui-kit/crates/base/src/scrollbar.rs`).** `WIDTH = 16` (`:22`); per axis `track_width = styles.track.width.or(theme track width).unwrap_or(WIDTH)` (`:1376-1381`); the bar `bounds` for a vertical axis is the right `track_width` of the hitbox (`:1409-1413`); while `is_visible` the `MouseDownEvent` listener does `cx.stop_propagation()` for any press inside `bounds`, then either starts a thumb drag or jumps the offset to the click (`:1716-1745`). `is_visible = state.visibility_requested || visibility_opacity > 0.0` (`:1599`), and OneTerm sets `ScrollbarMode::Always` (`crates/theme/src/theme.rs:299`), so in OneTerm the track takes presses at all times, not only after a wheel scroll or while hovered as the packet's D3 text says (doc nit D3-N1 below; the test's preparatory mouse move is harmless). No theme override of the track width exists in `crates/theme`, so the strip's 6 px is the one in effect.
- **Does `.styles(..)` shrink the hit track?** Yes: the same `track_width` value feeds the hit `bounds` (`:1409-1413`) that the mouse-down and mouse-move listeners test (`:1717`, `:1764`) and the painted bar; there is no second, wider hit rectangle. The thumb: `self.styles.thumb.width` wins over the theme's for the normal, hovered and active states (`:987-991`, state styles unset here), so the thumb is 4 px with a 1 px inset in every state and `thumb_bounds` is inside the 6 px track.
- **Thumb drag.** Unchanged path: a press inside `thumb_bounds` calls `scroll_handle.start_drag()` and records the grab point (`:1719-1731`), the move listener drags while `dragged_axis == Some(axis)` (`:1790-1800`); nothing in the override touches it. On-screen feel of a 4 px thumb is the ui-verifier's.
- **`viewport_from_layout` vs the expanded rail.** The kit's own `vertical_scrollbar` is `render_scrollbar` (`component/src/scroll/scrollable.rs:280-288`): `div().absolute().inset_0().child(Scrollbar::new(handle).id(..).axis(..).viewport_from_layout())`. `narrow_scrollbar` (`tab_rail.rs:291-306`) is the same mount with `Scrollbar::vertical` and the style override, so the collapsed and expanded rails resolve the same viewport (the `tab-rail-scroll` wrapper's layout bounds, `:1290`). The only differences are the kit's inspector-picking guard and its `scrollbar-overlay` debug selector, neither of which matters here.
- **Other overlay hit areas.** The status badge sits at `right 3 px` of the 43 px cell with an `xsmall` icon, so roughly x = 26..40; the 6 px track covers x = 37..43: a press on the badge's rightmost 3-4 px pages the list instead of selecting (the recorded accepted cost). Hover is unaffected (the kit stops only presses and thumb-drag moves), so the badge's own tooltip still shows. The grip overlay is on the left edge and never meets the track. Expanded rows keep the 16 px gutter inside the row, so the track and content do not overlap there.
- **N1, N2.** `border_b(px(TabRowLayout::VERTICAL_ROW_DIVIDER))` at `tab_rail.rs:736`; the tooltip comment at `tab_title.rs:477-479` now describes the clear-then-re-show. Both as recommended.

### Notes (informational)

- **D3-N1** — `US-0198-collapsible-rail.md` Rework 2 says the track takes presses "while the bar is shown (after a wheel scroll, or while hovered)"; with OneTerm's `ScrollbarMode::Always` (`theme.rs:299`) it is shown, and takes presses, always. One clause.

### Still by hand (ui-verifier)

- 40 collapsed tabs: click cells at their centre and right half, each selects; drag the 4 px thumb; a press on the badge's right edge pages the list (accepted).

## UI re-walk 2 (D3, head 8da2c685, ui-verifier, 2026-10-21)

Screenshots `uiv198c-*` in the scratchpad `uiv198-shots\`. Cells span x=8..50 in the window; 40 tabs collapsed, list settled by a wheel scroll before every click, a different tab active before each probe. "Selects" is read from the 2 px accent bar position.

| Probe | Result |
|---|---|
| Click at the left (x=22) and centre (x=28) | selects, no page jump (`02`, `03`, `04`) |
| Click at x=33, 36 | selects (`04`) |
| Click at x=37..41 (about 75% of the width) | FAIL: the tab does not select and the list pages (the active tab leaves the view) (`04-x38`, `05-x37/39/40/41`) |
| Click at x=43..50 | FAIL: the tab does not select, the list does not move (the previous tab stays active) (`05-x43..x50`) |
| Click at 5 tabs (no overflow) | selects (earlier walk `38`) |
| Select tab 39 by click, Expand, collapse again | PASS: accent visible expanded (`07`) and collapsed (`06`, `08`); the click was at x=28 |
| Thin scrollbar look | visible thin thumb at x~46..48, dark `10` and light `12`; it does not overlap the numbers |
| Drag the thumb | FAIL / NOT WORKING: dragging from (47, 150) to (47, 420) at the top of the list left the list unmoved (`10`, `11`); the wheel scrolls |
| Exact earlier repros (34 to 35/33/28, 18 to 3, 12 to 14) | re-done as the x probes above at several rows: left and centre clicks select, right-hand clicks do not |

### Defect
- **D3 remains, smaller (high-medium): in an overflowing collapsed rail, a press in the right part of a cell (x >= 37, i.e. about the right third, 14 px, not 6 px) does not select the tab.** x=37..41 pages the list; x=43..50 does nothing. Expected (per the coordinator): only the right-most 6 px may be taken by the scrollbar. Steps: 18+ collapsed tabs, wheel scroll once, press at x=40 on a non-active cell. The 75% position (x=40) from the brief is inside the dead zone.
- **D4 (low): the 4 px scrollbar thumb cannot be dragged** (press at x=47 on the thumb does not move the list), so the thin bar is display-only; the wheel works.

Verdict: partly. Selecting at the left and centre works and expand/collapse keeps the row visible; the right third of each cell still swallows presses.

## Advisor re-review 3 (no scrollbar in the collapsed strip; 2026-10-21)

- Reviewed: `git diff fe4d01b6..d044d982` (`d044d982`: `narrow_scrollbar` and `COLLAPSED_SCROLLBAR_WIDTH` deleted, the kit scrollbar mounted only `when(!collapsed)`, the widened test, the records, and the folded-in `a9c40611` UI re-walk 2 evidence). No conflict markers under `crates/`, `docs/`, `scripts/` (script scan). Worktree clean before and after the experiment.
- Gate re-run here (`CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2`): four-crate tests 56 + 77 + 415 + 109 passed, 0 failed; clippy on the four crates `--all-targets -- -D warnings` clean (so no dead code: `Scrollbar` is still imported for `Scrollbar::width()` at `tab_rail.rs:714`, the expanded gutter).
- **Does the test fail pre-fix?** Yes. With `tab_rail.rs` checked out from `8da2c685` (the 6 px track) and the new test from `d044d982`, `a_click_across_a_cell_of_an_overflowing_collapsed_rail_selects_it` fails at `tab_rail_tests.rs:1317`: `a click 37 px across the cell: left 7, right 9`, i.e. the first pixel of the 6 px track (43 - 6 = 37; the packet says 39, see nit below). File restored to `d044d982`, nothing committed. With the fix every pixel 0..42 selects.

### Verdict

**PASS.** The strip scrolls without the bar (gpui's own wheel path), the expanded rail is untouched, nothing is left dead, and the records describe the shipped behaviour; two wording nits.

### Checks

- **Wheel scrolling needs no scrollbar element.** The list is a base `Tabs` with `.overflow_y_scroll().track_scroll(&self.scroll_handle)` (`tab_rail.rs`), which is gpui's `Interactivity` (`div.rs:1535-1537`, `:1549-1552`). gpui registers the `ScrollWheelEvent` listener itself in `paint_scroll_listener` for any element with a scroll offset (`div.rs:3331-3339`): it moves `scroll_offset` by `event.delta.pixel_delta(line_height)` when the element's hitbox should handle the scroll, and the offset is clamped in prepaint (`:2481-2485`). The kit's `Scrollbar` only reads and writes the same `ScrollHandle`; `vertical_scrollbar` is an added overlay child (`scrollable.rs:24`, `:280-288`), not the scroll container, so removing it removes nothing from the wheel path. The coder's test proves it in the same frame set: a `ScrollWheelEvent` of -300 px at the list's centre moves the cells before any click.
- **Auto-reveal still covers the keyboard-less case.** `scroll_to_item` on a tab change and on a collapse / expand (`tab_rail.rs`, `tab_changed || layout_changed`, from the A1 fix) is unchanged; `forty_collapsed_cells_scroll_without_a_gutter_and_reveal_the_active_one` still passes.
- **Expanded rail unchanged.** The only edit in `render` is the removal of the `when(collapsed, ..)` arm; the `when(!collapsed, vertical_scrollbar)` arm, the gutter in each row, the handle and the tracker are as at `8da2c685`. `forty_tabs_scroll_and_the_active_one_is_brought_into_view` and the resize tests pass.
- **The verifier's "x = 43..50 dead" reading.** Consistent with the coder's explanation: the walk's table says the cells span window x = 8..50 and the driver posts client coordinates; client 37..42 is the 6 px track (window 45..50, which paged the list in the walk), client 43 is the rail's 1 px border and 44+ the terminal, where a click selects nothing and moves nothing. So the walk saw one hit-eater, the track, plus the frame offset, not a second one. The new test's pixel sweep (0..42) rules out any other eater inside the cell.
- **Records.** `docs/gui-layout.md:154` and the packet's Rework 3 say the strip draws no scrollbar, wheel / touchpad and auto-reveal scroll it, D4 is moot with it, and the expanded rail keeps the bar. The packet's "Scrollbar" plan line is marked superseded.

### Nits (wording only)

- **R3-N1** — `US-0198-collapsible-rail.md` Rework 3 "Test" bullet says the test fails at 39 px against the 6 px track; it fails at 37 px (the track is x = 37..42 of 43). One number.
- **R3-N2** — `docs/gui-layout.md:154` and the Rework 2 D3 bullet (`:129`) still say the kit bar "is shown after a wheel scroll or while hovered"; with OneTerm's `ScrollbarMode::Always` (`crates/theme/src/theme.rs:299`) it is always shown. Now moot for the strip (no bar) and harmless for the expanded rail, so optional; the earlier D3-N1 note is closed by the removal.

### Still by hand (ui-verifier)

- 40 collapsed tabs: wheel scrolls the strip; every cell selects at its left, centre and right edge; collapse / expand keeps the displayed tab in view; no thumb drawn in the strip; the expanded rail's scrollbar still drags.

## UI re-walk 3 (head d044d982, ui-verifier, 2026-10-21)

Screenshots `uiv198d-*` in the scratchpad `uiv198-shots\`.

**Coordinate space.** The driver's `click x y` takes client coordinates; screenshots are of the whole window with an 8 px frame, so a cell is client x=0..42 (screenshot x=8..50). The x values below are client x. This corrects the earlier re-walk 2: its "x=37..50" are screenshot-space-shifted client x, i.e. screenshot x=45..58, which is where the 16 px/6 px scrollbar track sat; the "x=43..50 dead zone" there was partly outside the cell (client x >= 43 is the terminal side), so only the x=37..42 part was a real defect, and that was the scrollbar track. This build has no strip scrollbar.

| Item | Verdict | Evidence |
|---|---|---|
| 40 collapsed tabs: click at client x = 2, 14, 22, 30, 36, 38, 40, 42 on a non-active cell (a different tab active before each) | PASS | `02-x2` .. `02-x42`: the clicked cell becomes active every time (accent at the clicked row), no list jump |
| Clicks at client x = 44..50 (outside the strip) | n/a, expected no selection | `02-x44` .. `x50`: previous tab stays active, nothing moves |
| Repros 34 to 35/33/28 and 12 to 14 equivalents: after a bottom-scrolled list, click tabs 35, 33, 28, 39 and (after scrolling up) 12 | PASS | `03-click-35/33/28/39/12`: accent on the clicked cell each time |
| Tab 39 selected, list scrolled to the top, Expand | PASS | `05`: selected row visible at the bottom |
| Collapse again | PASS | `06`: tab 39 visible with accent |
| Strip scrolls by wheel | PASS | wheel up/down moved the list (`04`, `01`) |
| No scrollbar drawn when collapsed | PASS | `01`, `02-x42`, `09` (light): no thumb |
| Expanded rail scrollbar unchanged, thumb draggable | PASS | `07` (thumb at top), `08` (dragging thumb from y=150 to 450 scrolled the list, thumb moved down) |
| Light and dark glance | PASS | dark `01`, `05`, `07`; light `09` (click at x=40 selects tab 5), `10` |

### Defects
None found in this walk. (The 4 px thumb drag defect D4 of re-walk 2 is gone with the scrollbar.) Not walked: the hover-to-reveal of an active cell for tab counts beyond 40, status badges (no live shell).

Verdict: yes for everything walkable.

## Advisor code review (rework 6)

Reviewed: coder head `996d5a11` (three commits on main `46ad8207`: packet, terminal-view + state + workspace, theme + script), on branch `verify/in-0055-rework6` in the coder's worktree, 2026-10-22. Scope: the collapsed cell shows the shell icon instead of the position number and the OSC 7501 state as its background instead of a badge.

### Verdict

**PASS** (with two Low findings in the contrast script's exemption mechanism, pinned by failing tests, and four notes). No product-code defect found; nothing blocks merge.

### Gate re-run (one crate per command, `CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2`)

| Check | Result |
|---|---|
| `cargo clippy -p oneterm-theme -p oneterm-terminal-view -p oneterm-workspace -p oneterm-state --all-targets -- -D warnings` | clean |
| `cargo test -p oneterm-theme` | 7 passed |
| `cargo test -p oneterm-state` | 77 passed |
| `cargo test -p oneterm-terminal-view` | 423 passed, 3 ignored (census) |
| `cargo test -p oneterm-workspace` | 112 passed, 3 ignored (census) |
| `python scripts/check-theme-contrast.py` | exit 0, prints the five `KNOWN` rows |
| `python scripts/check-theme-contrast.py --self-test` | passed |
| `python scripts/check-english.py`, `python scripts/check-doc-paths.py` | passed |

### Attack points (the brief's list)

1. **`duplicate_config` on every tab.** Every `TerminalView` the panel builds goes through `TerminalPanel::new_view` (`crates/terminal-view/src/panel/terminal_panel.rs:380-392`), the one place the field is written after `TerminalView::new` sets it `None` (`crates/terminal-view/src/terminal_view/view.rs:285`). Writers: `spawn_local_view` passes `Some(Local(shell))` (`terminal_panel.rs:346-352`) and serves `PanelSpec::DefaultShell` (startup, `--elevated-shell`), `PanelSpec::Shell` (the `+` menu, `crates/terminal-view/src/status.rs:66`) and the split paths (`crates/terminal-view/src/panel/spaces.rs:113`); `PanelSpec::Session` is built in two places only, SSH connect (`crates/session-ui/src/common.rs:491-494`, `Some(Ssh(..))`, which Connect All in Group also uses) and Duplicate (`crates/terminal-view/src/panel/duplicate.rs:220-223`, `Some`). Session restore and startup records (IN-0051) reach the same two. So the only `None` is a spawn failure (empty Space, generic glyph by design) and the test-support view. No silent-generic path found.
2. **Program-overrides-kind rule.** `shell_icon` (`crates/terminal-view/src/panel/tab_title.rs:181-201`) reuses `program_display_name` (`tab_title.rs:121-133`), the label's own function: last component after `/` or `\`, one trailing `.exe` removed case-insensitively, then `to_ascii_lowercase`. Walked: `C:\Windows\System32\cmd.exe` -> cmd; `C:/Program Files/PowerShell/7/pwsh` and `PWSH.EXE` -> PowerShell; `powershell_ise.exe`, `pwsh-preview.exe`, `nu.exe`, `wsl.exe` -> generic; a program that leaves no name (trailing separator) falls back to the kind, exactly as the label does. `program` is a `PathBuf` and `args` is separate (`crates/core/src/config/shell.rs:70-73`), so `powershell.exe -NoLogo` cannot be a program unless a user types it into the path field, in which case the label shows that same text and `resolve_shell` fails to spawn. Icon and label agree by construction (`shell_tab_title`, `tab_title.rs:144-157`).
3. **Which Space decides.** `tab_shell_icon` reads `tree.active_terminal()` (`tab_title.rs:78-84`), the focused Space; `set_active_space` notifies (`spaces.rs:148-162`) so a focus change between a cmd Space and a pwsh Space redraws the icon. The fill is `tab_status`, which is `None` for any split tab (`tab_title.rs:66-73`, the `IN-0049` ruling), so a split tab shows its active Space's glyph on no fill. Consistent with the packet text.
4. **`legible_on`.** `crates/theme/src/theme.rs:134-145`: reuses the existing WCAG `luminance`/`contrast` helpers (`theme.rs:113-132`), no new maths; strict `>` so the first candidate wins a tie, as documented and tested. Runtime does not composite alpha where the script does; every `background`/`foreground`/status token in the 39 variants is opaque, so no divergence today.
5. **Script model vs runtime rule.** Same tokens: `theme.primary/warning/success/danger` are `primary.background` etc. in the kit (`reference/gpui-kit/crates/component/src/theme/schema.rs:682-700`, `apply_color!`), and OneTerm mutates none of them at run time except `warning` (`apply_warning_legibility`, `theme.rs:206-212`), which is a no-op for every shipped theme because the script already gates `warning.background` as text at 4.5:1 on the very three surfaces `legible_warning` checks (`title_bar`, `background`, `popover`; script lines 269-273). Kit defaults: a theme that omits `success.background`/`danger.background` gets the mode's default palette (`apply_color!` else branch) = `green-500 #22c55e` / `red-500 #ef4444` light, `green-400 #4ade80` / `red-400 #f87171` dark (`default-theme.json`, `default-colors.json`), which is exactly `KIT_DEFAULTS`. `max(measure(background), measure(foreground))` equals the runtime pick when the fill has no gradient (none does). Model matches.
6. **`KNOWN_BELOW` cannot hide new failures.** The filter is `row[3] == PICKED_GLYPH and (variant, fill) in KNOWN_BELOW` (script lines 677-679), so a sixth pair, a text row or a hierarchy inversion still fails. Two weaknesses found, F1 and F2 below.
7. **SVGs.** Both are 24 x 24, `fill="none"`, `stroke="currentColor"`, width 2, round caps and joins, like `terminal.svg` and `grip-vertical.svg` (which are Lucide files). `cmd.svg` is a window frame + title rule + prompt; `powershell.svg` is a slanted panel + `>_`. Neither is byte- or path-identical to any kit SVG. `third-party-notices.py` reads `Cargo.lock` only, so no notice check is affected. See N1 on the PowerShell shape.
8. **Dead code after removing the position.** `TAB_NUMBER_SELECTOR`, `COLLAPSED_BADGE_INSET` and the two number tests are gone; `RowPlace.position` is still used for `set_position` (`crates/workspace/src/layout/workspace/tab_rail.rs:600`); no `Collapsed(` or `TAB_NUMBER` left in `crates/` or `docs/` outside historical IN-0055 text. Clean.
9. **Hover / accent / drop line / grip.** The rail's `Tab` is `gpui_base::Tab` (`tab_rail.rs:23`), which puts children directly in the row with no padding (`reference/gpui-kit/crates/base/src/tabs.rs:150-181`), so the panel's `flex_1 h_full` title spans the 44 px cell and the fill reaches the edges (43 px tall above the 1 px divider border). Paint order in the row: accent bar, hover-fill child (unselected only), then the title, so the status fill covers the hover fill and the bar's 2 px stays clear through the `left(2px)` inset; the drop line is the Tab's own `border_t_2`/`border_b_2` (`tab_rail.rs:638-645`), outside the title, so it stays visible over a filled cell. See N2 on the grip glyph.
10. **Records.** The `US-0198` Rework 6 section, the HLD sketch, the `IN-0055.md` Q4 amendment and `docs/gui-layout.md` agree with the code (tokens, inset, no hover tint, hover card keeps its icon, five `KNOWN_BELOW` pairs with the printed ratios).

### Findings

| # | Severity | Where | Finding | Fix shape |
|---|---|---|---|---|
| F1 | Low | `scripts/check-theme-contrast.py:676-679` | A `KNOWN_BELOW` entry is only consulted for rows that are below the floor, so an entry whose pair has since been retuned (or was mistyped and never matched) is kept for ever and reported nowhere. The comment says "Delete an entry when the theme is fixed" but nothing enforces it, unlike the `#[ignore]` census. | After computing `known`, `stale = KNOWN_BELOW - {(row[1], row[4]) for row in known}`; print each and return 1 when non-empty. Pinned by `scripts/test_check_theme_contrast.py::test_a_known_pair_that_now_reads_is_a_stale_entry_and_fails_the_run` (fails today). |
| F2 | Low | `scripts/check-theme-contrast.py:736-741` | The summary line still reads `all >= 4.5:1 (glyphs >= 3.0:1)` while five glyph rows are below 3:1 and exempt. | Append `; N known glyph rows below the floor` to the summary when `known` is non-empty. Pinned by `test_the_summary_does_not_claim_every_glyph_clears_the_floor_while_known_rows_are_below` (fails today). |

The pinning tests live in `scripts/test_check_theme_contrast.py` (new; not wired into `ci-local`; run `python -m unittest scripts/test_check_theme_contrast.py`). Flip or delete them when F1/F2 are fixed.

### Notes (owner decision or hand check, no code defect)

- **N1 (owner).** `powershell.svg` is a slanted parallelogram with a `>_` prompt. The packet states it is "not Microsoft's logos or marks"; the shape is however the same concept as the PowerShell mark (a skewed window with a prompt). Copyright-wise it is an original stroke drawing and no notice is needed; trademark-wise it is nominative use inside a terminal client. Low risk, but the owner should accept the claim knowingly or ask for a shape that does not echo the mark (for example a plain window with `PS`).
- **N2 (ui-verifier).** On hover, the rail's reorder grip glyph fades in over the cell's left edge in `muted.foreground` (`tab_rail.rs:574`), which is now painted over a `primary`/`warning`/`success`/`danger` fill and is held to no floor there. It may be invisible on a filled cell. Either accept (the grip still takes the press) or colour it with the same `legible_on` pick.
- **N3 (hand check).** Default Light / Default Dark (the kit's own themes in the menu) are not gated by the script, as before; their status fills are the kit palette, so legibility there follows from `KIT_DEFAULTS` being correct, which was verified above.
- **N4 (informational).** The script's `self_test` does not cover the new tier (no fixture asserts a `PICKED_GLYPH` row or the `KIT_DEFAULTS` injection); the ratio maths it shares is covered. A one-line fixture assertion would close that.

### Still by hand (ui-verifier)

The packet's open acceptance line: glyphs at 16 px light and dark, each status fill with the glyph readable, the bar on a selected filled cell, hover on a filled and an idle cell (and N2's grip glyph), the hover card still showing the status icon, the recording dot on a filled cell.

## UI verification (rework 6) (head 996d5a11, ui-verifier, 2026-10-22)

Screenshots `uivrw6-*` in the session scratchpad `uivrw6-shots\` (full-window PrintWindow shots with an ~8 px frame; the `z` files are nearest-neighbour crops of the rail; driver clicks are client coordinates, screenshot x = client x + 8). Binary: the coordinator's copy of the debug build. Private profile (scratch USERPROFILE + cwd).

**Environment limit.** In this session every child shell dies at start (`conpty: conout read ended`, `committed text delivery failed: terminal session is closed`), so no OSC 7501 can be fed and no status fill can be shown.

| Item | Verdict | Evidence |
|---|---|---|
| 1a Command Prompt tab shows the cmd icon (window frame, title rule, `>_`) | PASS | `03z`, `10-Zed-One-Lightz` |
| 1b Windows PowerShell and PowerShell 7 tabs show the PowerShell icon (slanted panel with `>_`), the two identical | PASS | `03z`, `05z` |
| 1c Other shell (Git Bash with an explicit `program`) shows the generic `>_` terminal icon | PASS | `08-b-bash`, `09z` |
| 1d Icons centred in the 44 px cell, consistent stroke style, legible at 16 px; dim when idle, bright when selected/hovered | PASS | `03z`, `05z` |
| 1e Same in light themes: Zed One Light, Catppuccin Latte, Solarized Light, Everforest Light | PASS | `10`..`13` (`z` crops) |
| 2 No position numbers, no status badge in any collapsed cell | PASS | `03`, `10`..`13` (idle only) |
| 3 Status fills (working / blocked / done / error), glyph colour, 2 px bar on a selected filled cell, no hover tint on a filled cell, hover card status icon | NOT WALKED | shells do not run here (`07-working`, `09-b-working`: command typed, no fill, session closed); owner-only |
| 4a Idle cell hover fill, drag-handle dots appear on hover | PASS | `04z`, `05z` |
| 4b Hover card shows the row title ("PowerShell") | PASS | `04z` (idle: no status icon, as expected; card body has blank space below the title, unchanged by this rework as far as seen) |
| 4c Click selects (accent bar moves) | PASS | `05z` |
| 4d Right-click menu with Input Channel row, Escape closes | PASS | `06-rmenu`, `06b-esc` |
| 4e Expanded rail unchanged (full-title rows) | PASS (idle only; status icon slot not seen without a status) | `20-expanded-1` |
| 4f Horizontal strip unchanged | PASS | `20-strip-1` |
| 5 Themes: dark (Zed One Dark), light (Zed One Light) icons; Catppuccin Latte / Everforest Light / Solarized Light icons | PASS; fills NOT WALKED | `03z`, `10`..`13` |

### Defects
None found in the walkable parts.

Verdict: partly (icons, no numbers, regressions all PASS; status fills and glyph legibility on Working / Blocked / Done / Error cannot be seen without a running shell).

### Owner-only checks
- Feed `printf '\033]7501;state=working\007'` (and blocked / done / error) in a live Git Bash, PowerShell, or Windows cmd tab: fill colour per state, glyph colour readable on every fill, 2 px accent bar visible on a selected filled cell, no hover tint on a filled cell, hover card still shows the status icon.
- Repeat in Catppuccin Latte, Everforest Light and Solarized Light (low-contrast fills), light and dark.
- A split tab shows no fill.

## Coder follow-up (rework 6 findings, 2026-10-22)

F1, F2, N4 and N2 are fixed in the coder's commit after this evidence (see the packet's "Rework after the advisor review and the UI walk of rework 6"). The two pinned tests passed against the fixed script and now live in its `--self-test`; `scripts/test_check_theme_contrast.py` is deleted, so the references to it above describe the pins as they were when written.

## Advisor re-review (rework 6)

Reviewed: `ebc10543` on `verify/in-0055-rework6` (after `da11e0c5`: `1989df8c` UI evidence, `ebc10543` F1, F2, N4, N2), 2026-10-22. No conflict markers in `crates/`, `docs/` or `scripts/`.

### Verdict

**PASS.** F1, F2 and N4 are fixed as asked. The N2 hand-over (`report_cell_fill` / `take_cell_fill`) is **safe as used today**: the ordering is synchronous and deterministic, every collapsed cell overwrites the slot (also with `None`) and the read clears it, so no fill can reach another cell, group or window. It is still a hidden side channel, and a cleaner shape exists inside the same crate (note R1); recommended as a follow-up, not a blocker.

### Gate re-run (one crate per command, `CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2`)

| Check | Result |
|---|---|
| `cargo clippy -p oneterm-theme -p oneterm-terminal-view -p oneterm-workspace -p oneterm-state --all-targets -- -D warnings` | clean |
| `cargo test -p oneterm-state` | 78 passed (+1: `a_cells_fill_is_handed_over_once`) |
| `cargo test -p oneterm-terminal-view` | 423 passed, 3 ignored (census) |
| `cargo test -p oneterm-workspace` | 113 passed, 3 ignored (+1: `the_grip_glyph_reads_on_a_filled_cell`) |
| `python scripts/check-theme-contrast.py` | exit 0; summary now reads `glyphs >= 3.0:1 except 5 known glyph rows below the floor` |
| `python scripts/check-theme-contrast.py --self-test` | passed |
| `check-english`, `check-doc-paths` | passed |

### N2 attacked

1. **Render-order dependence.** None. `TabRail::row` builds the cell's title with `handle.title(window, &mut scope)` (`crates/workspace/src/layout/workspace/tab_rail.rs:520-535`); the kit's `PanelHandle::title` is `self.update(cx, |this, cx| this.title(window, cx).into_any_element())` (`reference/gpui-kit/crates/component/src/dock/panel.rs:170`), and `TerminalPanel::title` is `render_tab_strip` (`crates/terminal-view/src/panel/terminal_panel.rs:700-701`), a plain function that calls `report_cell_fill` while it runs (`tab_title.rs:510-513`). So the report is complete before `take_cell_fill` at `tab_rail.rs:543`, within one straight-line function, independent of gpui's layout/paint order, of caching and of whether any other view re-renders. A frame in which the rail is not re-rendered rebuilds neither the cell nor the grip, so the two cannot disagree.
2. **No re-render of the cell.** The title is rebuilt on every `row()` call; there is no cached title element. The only way to get a grip without a fresh report would be a `handle == None` panel in a collapsed rail (`tab_rail.rs:536`), which cannot report; the slot it would read was cleared by the previous cell's take (or holds `None`), so it gets `muted.foreground`, correct for a cell that drew no fill.
3. **Leakage across cells / groups / windows.** `report_cell_fill` is called for every collapsed cell, with `None` when idle (`tab_title.rs:510-513`; pinned by the drawn test's "an idle cell reports no fill to the rail"), and `take_cell_fill` clears a `Some` (`crates/state/src/tab_row_layout.rs:85-91`). Within the rail every report is paired with a take in the same `row()`. Outside the rail, `TabRowLayout::Collapsed` is in force only inside the rail's scope (`TabRowScope` restores on drop, `tab_row_layout.rs:114-118`); the drag preview calls `handle.title` with no scope, so `collapsed` is false and nothing is reported (`tab_rail.rs:716-719`); the hover card is `tab_hover_card`, not `render_tab_strip`. gpui is single-threaded per `App`, so two windows' renders do not interleave inside `row()`. No path found where a stale `Some` survives to another cell.
4. **`set_global` during render.** `App::set_global` pushes `Effect::NotifyGlobalObservers` and inserts (`gpui/src/app.rs:2055-2059`); nothing observes `CellFill`, and the existing `TabRowScope` already sets a global on the same path, so this adds one no-op effect per collapsed cell per frame. Harmless.
5. **Simpler path within crate rules.** The rail cannot read the tab's status (R4: `workspace` must not depend on `terminal-view`), and the kit's `Panel` trait is not ours to extend, so some hand-over is needed. Alternatives weighed: (a) draw the grip inside the cell: the grip's drag is the rail's (`DragPanel`, `RailDragPreview`) and the cell's own `on_drag` is the Space-drop payload, so moving it would entangle two drag protocols; rejected. (b) `title_style` (`Panel::title_style -> Option<TitleStyle>`, kit `panel.rs:87`) is read by the rail without any global, but `TitleStyle` carries `background`/`foreground` for the whole tab and would restyle the horizontal strip too; rejected unless gated on layout, which reintroduces the scope. (c) **Carry the slot in `TabRowScope`** (see R1). The coder's choice is acceptable; (c) is the tidier version of it.

### F1, F2, N4 re-checked

- F1: `split_known` (`scripts/check-theme-contrast.py:706-720`) computes `stale = exempt - {(variant, fill) of rows actually below}`; `main` prints each `STALE KNOWN_BELOW entry` and returns 1 (`:773-777`, `:797-800`). Verified by the self-test (a passing pair and a never-matched pair both reported) and by reading `main`.
- F2: `summary()` (`:723-734`) says `except N known glyph rows below the floor` when any are exempt and the plain wording otherwise; live run confirms.
- N4: the self-test now covers the picked tier (better token wins, 21:1 on black, floor carried, a muddy fill fails and only `KNOWN_BELOW` exempts it), stale entries, the summary wording and `with_kit_defaults` (fills in, never overrides, per mode). My two pins are subsumed and `scripts/test_check_theme_contrast.py` is gone; nothing else references it.

### Notes (non-blocking)

- **R1 (Low, follow-up).** Tie the hand-over to the scope that already exists for the opposite direction: give `TabRowScope` the slot (`report_cell_fill(cx, fill)` writes a `CellFill` global exactly as now; `TabRowScope::cell_fill(&self) -> Option<Hsla>` reads it; `Drop` clears it along with restoring the layout). Then there is no `take`, a report can never outlive the scope that asked for the cell, and a future second caller cannot read another cell's value. Same crate, about ten lines, tests move unchanged. Not required for merge.
- **R2 (informational).** `grip_glyph_color` is only correct while the cell's own glyph pick is `legible_on(fill, [background, foreground])`; the two are now computed in two crates from the same inputs. If the cell's rule ever changes, the grip's must follow. A one-line comment at each site cross-referencing the other would do.

### Still by hand (ui-verifier)

The grip glyph on a hovered filled cell in a light and a dark theme (the UI walk at `1989df8c` predates N2).

## Advisor review (US-0198 rework 8, US-0202, US-0203)

Reviewed: `018eccd5` on `verify/in-0055-drag-zoom` (base `d2d15e03`), 2026-10-23, in worktree `agent-ab6a00f0423e3f789`. Rework 8 is commits `d4133dbc` (packet) and `98269539` (code); the records commit precedes the code commit. The sibling packets are in `US-0202-verify.md` and `US-0203-verify.md`.

### Verdict

**PASS.** The collapsed cell has one drag zone. `collapsed_row` takes no `icon_id` / `drag` (`crates/terminal-view/src/panel/tab_title.rs:491-497`) and the icon div is plain content (`:530-535`); `render_tab_strip` builds no payload for any layout (`:618-620`); the rail's `on_drag(drag, reorder)` on the cell (`crates/workspace/src/layout/workspace/tab_rail.rs:716-722`) is the only drag. The owner's "sometimes reorders, sometimes not" has the cause the packet names (the 16 x 16 icon in the middle of a 44 px cell started `DragTerminalTab`, which no reorder target accepts); the five other candidates the packet checked read the same in the code.

### Gate re-run (one crate per command, `CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2`, the worktree's `target`)

| Check | Result |
|---|---|
| `cargo test -p oneterm-workspace --lib` | 124 passed, 3 ignored |
| `cargo test -p oneterm-terminal-view --lib` | 444 passed, 3 ignored |
| `cargo test -p oneterm-settings --lib` / `-p oneterm-settings-ui --lib` | 56 / 60 passed (1 ignored) |
| `cargo test -p oneterm-app` | 26 passed |
| `cargo test -p oneterm-state --lib` / `-p oneterm-session-ui --lib` / `-p oneterm-sftp-ui --lib` / `-p oneterm-actions --lib` | 79 / 102 (1 ignored) / 78 / 4 passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean |
| `check-english.py` (1157 files), `check-doc-paths.py` (219 paths) | passed |

### Attacked

1. **Any pixel of the cell that is not the drag.** `the_whole_collapsed_cell_is_the_reorder_drag` (`tab_rail_tests.rs:1467-1522`) drags from the icon centre and eight points at 4 % / 96 % of the cell's width and height, each through the real skin, and asserts the reorder. `no_part_of_a_collapsed_cell_starts_a_tab_drag_of_its_own` (`crates/terminal-view/src/panel/tests.rs:1478-1500`) is on the real cell. Nothing else in the cell carries `on_drag` (`tab_title.rs:491-547`).
2. **Drop targets.** An empty Space takes the cell's `DragPanel` through `TerminalPanel::dragged_by` / `handle_panel_drop` (`crates/terminal-view/src/panel/spaces.rs:197-220`); an occupied Space and an unknown panel id change nothing and close no session (`tests.rs:697-718`, `source_probe.close_calls() == 0` both before and after the real move, `:737-740`).
3. **Records.** Packet section "Rework 8" lists cause, ruling, acceptance and findings; `docs/gui-layout.md:154` and the HLD rows say the same. The ui-verifier item (real input, many grab points) is open, as the packet says.

No defect. Still by hand: the `SendInput` walk on a collapsed rail, which the coder could not run (console session locked).
