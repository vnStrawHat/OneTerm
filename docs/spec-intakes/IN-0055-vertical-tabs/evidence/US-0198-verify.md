<<<<<<< HEAD
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
=======

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
>>>>>>> 70749f95 (docs(IN-0055): UI verification of US-0196 rework 3 and US-0198)
