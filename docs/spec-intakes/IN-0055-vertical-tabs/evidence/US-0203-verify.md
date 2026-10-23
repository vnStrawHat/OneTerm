# US-0203 verification (advisor)

- Packet: `US-0203-remove-zoom.md`; commits `e929e521` (packet) and `0dcc6536` (code), reviewed at `018eccd5` on `verify/in-0055-drag-zoom` (base `d2d15e03`), 2026-10-23, worktree `agent-ab6a00f0423e3f789`. The records commit precedes the code commit.
- Read: the four panels' `zoomable` / `zoom_control`, `crates/actions/src/elevated_policy.rs`, `crates/settings-ui/src/key_bindings/{key_bindings_actions,state}.rs`, `crates/state/src/dock_persistence.rs`, `dock_util.rs`, `crates/workspace/src/layout/workspace/{mod,persistence,dock_skin,layout}.rs` and tests, `crates/app/src/ssh_client_panel.rs`, `crates/sftp-ui/src/{panel,render}.rs`, `docs/agents/persistence.md`, `docs/gui-layout.md`, `docs/sftp-browser-design.md`, the HLD; kit `reference/gpui-kit/crates/base/src/dock/tab_group.rs:388-420` (`set_zoomed` refuses a zoom-in of a panel whose `zoomable()` is false), `dock_area.rs:727-760` (`set_zoomed_in` goes through the group), `component/src/dock/tab_panel.rs:140-146` (no control unless `zoomable()`), `:704-711` (`ToggleZoom` refused when the control is `None`), `component/src/dock/invalid_panel.rs:36-46` (kit defaults).
- No code changed, no pinned test added: the two defects are documentation lines.

## Advisor review (US-0198 rework 8, US-0202, US-0203)

### Verdict

**PASS with two doc fixes to make before merge** (D1, D2 below; one line each). No code defect. Nothing in OneTerm can zoom a panel: every OneTerm panel answers `zoomable() == false` and `zoom_control() == None` (`terminal_panel.rs:668-670, 717-719`; `session-ui/src/panel.rs:288-290, 298-300`; `sftp-ui/src/panel.rs:756-758, 766-768`; `ssh_client_panel.rs:200-202, 220-222`), no OneTerm code calls `set_zoomed_in` / `set_zoomed_out` / `toggle_zoom` outside tests (grep), the kit registers no key for `ToggleZoom` (grep of `reference/gpui-kit/crates/component/src`), and `BINDABLE_ACTIONS` has no `toggle_zoom` row. The terminal's font zoom is untouched (no file under `crates/terminal-view/src/input` is in the diff).

### Gate re-run

As recorded in `US-0198-verify.md` "Advisor review (US-0198 rework 8, US-0202, US-0203)": every listed crate green (`oneterm-state` 79, `oneterm-actions` 4 included), clippy clean, `check-english` and `check-doc-paths` passed.

### Coder's weakest points, attacked

1. **`SshClientPanel::zoomable() == false` unpinned.** Confirmed unpinned and confirmed hard to pin: `SshClientPanel::new` (`ssh_client_panel.rs:63-70`) builds `SessionPanel::new_entity`, which needs `SshSessionStore` (its test installer `install_empty_for_test` is `#[cfg(test)] pub(crate)`, `session_state.rs:867-868`, and the real `init` needs the vault, `:844-862`), and `oneterm-app` has no gpui `test-support` dev-dependency (`crates/app/Cargo.toml` has no `[dev-dependencies]`). The code is a two-line literal, read. Recommendation (follow-up, not a blocker): expose `install_empty_for_test` under a `test-support` feature of `oneterm-session-ui`, as `oneterm-vault` does, and add the one `oneterm-app` test.
2. **A future panel inheriting the kit default.** Real, and one case exists today: the kit's `InvalidPanel`, which `DockArea::load` substitutes for an unregistered panel name in a saved `docks.json`, implements `Panel` with the defaults (`invalid_panel.rs:46`: `zoomable() == true`, `zoom_control() == Some(Menu)`), so its strip group would show a "Zoom In" row and could zoom; with the trap release gone, switching such a zoomed group to the rail would leave it stuck until the switch is turned off and the menu used again. It needs a hand-edited or foreign `docks.json` and a user who zooms a broken placeholder, so **Low / informational**, no action asked; if the owner wants a belt, the cheapest is a test that walks the default layout's panels and asserts `zoomable() == false` for each (`layout_tests.rs` has the helpers).
3. **Legacy serde field vs `docs/agents/persistence.md`.** Compliant: no incompatible change, so no version bump (the rules ask for one only for an incompatible change, `persistence.md:95-96`); the document owner (`oneterm-state`) handles the field, not a feature crate; the field is read into `_legacy_zoomed_panel: Option<Value>` (`dock_persistence.rs:43-44`), so a string, `null` or any other value loads, and `skip_serializing` means no write carries it. The pre-existing hazard the coder saw is real: without the typed field, `#[serde(flatten)] dock_fields` (`:36-38`) would carry `zoomed_panel` into every later write as layout. Tests: `a_stored_zoomed_panel_loads_and_is_not_written_back` (`:544-588`; a v2 hand-written document, the v0 and v1 fixtures), the 0.5.2 fixture reads (`:285-291`, `:430-431`) and the workspace round trip (`layout_tests.rs:341-400`). `ui_config.json` with a `toggle_zoom` override: `a_saved_toggle_zoom_override_is_ignored` (`state.rs:394-421`) shows the id drops out and the other overrides stand. No other data is lost on either path.
4. **Any remaining path that can zoom.** Kit actions: the `ToggleZoom` handler refuses when the control is `None` (`tab_panel.rs:704-711`); the control is `None` whenever `zoomable()` is false (`:140-146`). `...` menu row and toolbar button: drawn from the same `zoom_control`, so absent. `DockArea::set_zoomed_in`: goes through `TabGroup::set_zoomed`, which refuses a zoom-in of a non-zoomable active panel (`tab_group.rs:396-398`); no OneTerm caller remains. Double-click on a tab: the kit's dock has no double-click handler (grep of `reference/gpui-kit/crates/{base,component}/src/dock`). Keyboard: no binding (kit or OneTerm). The startup restore is gone (`mod.rs:203-224`). Pinned by `a_panel_that_will_not_zoom_has_no_zoom_control_and_cannot_be_zoomed` (`tab_rail_tests.rs:791-834`) with a positive control. **So deleting the trap release does not re-open the owner's bug** for any OneTerm panel; see 2 for the one placeholder case.
5. **SFTP Expand after the dock zoom is gone.** The expand toggle still switches the browser (`render.rs:110-130`), `SshClientPanel` follows `SftpExpandedChanged` (`ssh_client_panel.rs:70-76`) and renders only the browser while expanded (`:237-240`). A persisted `sftp_table_state.expanded = true` is applied asynchronously after the panel is built (`sftp-ui/src/panel.rs:268-292`, inside `cx.spawn`), so the subscription is already in place and the two stay in step at startup without the old `set_zoomed` mirror. The tooltip and `README.md` no longer say "across the workspace". The "Maximize / Minimize icons" wording in `docs/sftp-browser-design.md:1313` is still correct: those are the button's icon names (`render.rs:113-117`).
6. **Elevated policy.** `toggle_zoom` is out of `ALLOWED_WHEN_ELEVATED` (`elevated_policy.rs:80-85`) and `action_classified_for_elevated_window("toggle_zoom") == None` is asserted (`key_bindings_actions.rs:554-558`). The ignored elevation tests are unaffected (no `toggle_zoom` reference remains).
7. **Dead code.** `dock_util::{find_tab_node_by_panel_name, panel_name_for_tab_node}` are gone with their only callers; the `NodeId` import is dropped; clippy clean.

### Defects

- **D1 (Low, docs).** `docs/gui-layout.md:32` still says "It is zoomable: expanding the SFTP Browser (IN-0025) zooms this node so the browser fills the workspace like a zoomed terminal tab, with the Session section hidden until it collapses." It contradicts § Zoom (`:97-104`) and `ssh_client_panel.rs:200-202`. Fix shape: replace the sentence with "Expanding the SFTP Browser (IN-0025) hides the Session section and shows the browser's Local + Remote layout inside the dock; there is no panel zoom (`US-0203`)."
- **D2 (Low, docs).** The HLD diagram, `docs/spec-intakes/IN-0055-vertical-tabs/high-level-design.md:33`, still lists the strip header's suffix as `+` (title_suffix), zoom, `...`, right-dock toggle. Fix shape: drop the zoom entry.

### Notes (non-blocking)

- **N1 (Info).** The packet's Plan still shows `- [ ] Docs, gate, debug binary.` while Acceptance and Evidence record them as done.
- **N2 (Info).** `docs/review-refresh-2026-08/01-architecture.md:117, 304` name `zoomed_panel`; a dated review, left as history (correct).

### Still by hand (ui-verifier / owner)

No Maximize / Minimize button in the strip toolbar and no "Zoom" row in the `...` menu, in both layouts; `Shift+Esc` does nothing; the SFTP Expand toggle shows the dual pane inside the right dock and the Session section comes back on Collapse; a profile whose `docks.json` holds `zoomed_panel` starts unzoomed and the next save drops it; `Ctrl+=` / `Ctrl+-` / `Ctrl+0` still change the terminal font.

## Rework D1 review

- Scope: `git diff 5aeb4dac b0944bff` (one commit, `b0944bff`) on `verify/in-0055-drag-zoom`, worktree `agent-ab6a00f0423e3f789`, 2026-10-23. Read only; no cargo (disk), no GUI. Versions verified against the locked crates (`Cargo.lock`: `gpui-component` / `gpui-base` 0.7.3, `gpui-pre` 0.3.9); the `render_toolbar` in `reference/gpui-kit` is byte-identical to the 0.7.3 copy.
- Read: `crates/terminal-view/src/panel/tab_title.rs:948-1058`, `terminal_panel.rs:718-756`, `lib.rs:26-30`, `panel/tests/tab_menu_and_card.rs:429-523`, `crates/workspace/src/layout/workspace/dock_skin.rs:157-206`; kit `component/src/dock/tab_panel.rs:275-357` (`render_toolbar`), `:665-683` (the trailing `h_flex`), `component/src/menu/dropdown_menu.rs:102-172` (`render_popover`), `component/src/menu/popup_menu.rs:334-363, 755-765` (`new`, `build`, `rebuild`), `component/src/button/button.rs:674`, `base/src/dock/tab_group.rs:218-229, 902-957` (`is_closable`, `TabGroupRenderer`), `base/src/test_support.rs:65-173`; gpui `app.rs:2151-2175` (`apply_entity_created_effect`), `:2496-2513` (`observe_new`), `:3230-3240` (`new` pushes `EntityCreated`).

### Verdict

**PASS with defects** (one Medium, test-only; the rest Low / informational). The production path is sound: the one-shot is armed inside the kit's `PopupMenu::build` closure and taken by the `EntityCreated` effect of that same `cx.new`, before any other `PopupMenu` can be created, and the rebuild keeps the menu's focus, trigger and action plumbing. The weak spot is the thread-local when nothing takes it, which the coder's own control test provokes.

### Attack list

1. **Can the armed flag leak and rewrite an unrelated menu?** In production, no. The only caller of `tab_list_menu` is `TerminalPanel::dropdown_menu` (`terminal_panel.rs:730`; grep of `crates/`), and the only caller of `Panel::dropdown_menu` is the kit's `...` button closure (`tab_panel.rs:337-339`), which `DropdownMenuPopover::render_popover` runs inside `PopupMenu::build` (`dropdown_menu.rs:131-133`), itself `cx.new` (`popup_menu.rs:363`). `App::new` pushes `Effect::EntityCreated` straight after the build closure returns (`app.rs:3235-3239`), effects are FIFO, and the closure creates no other `PopupMenu` (no submenu in the kit's rows or in `tab_list_rows`), so the next `PopupMenu` observer call is for that menu. The popover builds once per open (`menu_state` caches it, `dropdown_menu.rs:118-128`, and clears on dismiss), so the builder does not run on every render and the arm is not repeated. A second window or the SFTP / context menus are untouched because they never arm. The `observe_new` hook runs with `window: None` only for a `PopupMenu` created outside a window update (`app.rs:2164-2173`); the take still clears the slot on that path. Fragility (Low, see N1): the arm-then-take pairing depends on the kit's builder creating no `PopupMenu` entity between the panel's rows and the end of its closure; a future kit submenu in that menu would take the flag first.
   **Leak when no observer is installed (D1, Medium, tests only).** With `override_strip_menu` not installed nothing takes the slot. `strip_menu_rows(cx, false)` (the control test, `tab_menu_and_card.rs:508-515`) arms it and returns; the closure, with a strong `Entity<TerminalPanel>` of a dropped `App`, stays in the test thread's local. libtest reuses threads, so a later test on that thread that calls `crate::init` and opens any `PopupMenu` (the tab context-menu tests in the same file, `:19, :129, :365`, do both) has that menu rebuilt as a dead panel's tab list: `panel.read(app)` (`tab_title.rs:1021`) on an entity of another `App` panics or downcasts a reused slot. Order-dependent, so a flake rather than a sure failure.
2. **Keyboard navigation, action dispatch, focus.** Identical. `rebuild` (`popup_menu.rs:755-765`) swaps `self` out, clears `menu_items` and `selected_index`, runs the builder on the *same* struct and puts it back, so `focus_handle`, `action_context`, `trigger_focus_handle` and `previous_focus_handle` survive; the throwaway `Self::new(cx)` placeholder is dropped. The kit sets the trigger focus and focuses the menu right after `build` (`dropdown_menu.rs:134-139`), before the effect runs, and the rebuild keeps that handle. Rows are made with the same `menu.item` / `menu.menu` API as the kit's, so arrow keys, Enter and the `ClosePanel` dispatch (handled on the tab panel element, `tab_panel.rs:718`) behave as before. `selected_index == None` after the rebuild is the state of a fresh menu.
3. **A cheaper override point.** None exists. `TabGroupRenderer` (`tab_group.rs:902-957`) exposes `frame`, `content_frame`, `render_tab_bar`, `render_active_panel`, `render_drop_indicator`, `render_empty`; `render_tabs` and `render_toolbar` are private methods of the kit's skin (`tab_panel.rs:275, 433`), and `OneTermTabGroupSkin::render_tab_bar` delegates the horizontal strip whole to `inner.render_tab_bar` (`dock_skin.rs:194-206`). `Panel::zoom_control() == None` only sets `menu_zoom == false`, which greys the row (`tab_panel.rs:342-348`); an `AnyElement` cannot have a child removed. So the choice is this hack, a 250-line copy of `render_tabs`, or a kit change; the hack is the smallest. The one-frame flash is real: the menu is built inside the `Popover` content closure during a draw, and `EntityCreated` effects flush after that draw, so the kit's rows paint once before `rebuild` notifies. At 60 Hz it is one frame of a greyed row; not worth more code.
4. **Test quality.** The control test is genuine: without `crate::init` the same click, draw loop and snapshot read must yield a row containing "Zoom In", so the main test cannot pass by reading an empty menu or a closed one. Menu rows carry `Role::MenuItem` (`menu_item.rs:97`) and `snapshots` / `label` exist (`test_support.rs:65-173`). The 18 px: the trailing `h_flex` has `px_2()` (8 px, `tab_panel.rs:674`) and the `...` button is `xsmall` = `size_5` (20 px, `button.rs:674`), so its centre is 8 + 10 = 18 px from the bar's end when nothing follows it; `right_button` (`:683`) would follow only with a right dock, which the test's layout has none of. Brittle to a kit padding or size change, but self-diagnosing: both tests fail on a missed click and the comment says what is assumed. `rows.len() >= 2` is loose (it does not pin the two tab labels or the absence of Close); acceptable. Not verified by running (no cargo here); `target/debug/deps` holds two `oneterm_terminal_view-*.exe` test builds (14:09, 14:11) older than the final lib build (14:16) and the commit (14:17), and neither the packet nor the commit records a gate run for D1.
5. **Docs.** `docs/gui-layout.md:101` and the packet (`US-0203-remove-zoom.md:32, 55, 80, 125-131`) describe the mechanism, the one-frame cost and the removal condition accurately; `check-english.py` (1159 files) and `check-doc-paths.py` (219 paths) pass. Wording gap (D2): the packet's "Known cost" says the arm "is taken by the very next `PopupMenu` created, which is that menu" without stating the precondition (the observer must be installed) that its own control test violates.

### Defects

- **D1 (Medium, test stability).** `tab_title.rs:966-968` arms a thread-local with a strong `Entity<TerminalPanel>`; with no observer installed nothing clears it (`:992-1004` is the only take). The control test `without_the_rebuild_the_kits_strip_menu_still_has_its_zoom_row` (`tab_menu_and_card.rs:508-515`) leaves it armed for the next test on the thread. Reproduction: run `strip_menu_rows(cx, false)` then any test that calls `crate::init` and opens a `PopupMenu` on the same thread. Fix shape (two lines each): capture `entity.downgrade()` and `return menu` when `upgrade()` fails, so a stale closure is a no-op rather than a read of a dead entity; and have `strip_menu_rows` clear the slot after the walk (a `#[cfg(test)] pub(super) fn disarm_strip_menu()` beside `strip_menu_is_armed`). Pinned, not run: `the_control_walk_leaves_no_strip_menu_rebuild_armed` (`tab_menu_and_card.rs`, end of file) with the accessor `tab_title::strip_menu_is_armed` (`#[cfg(test)]`); it fails on this head and passes once the slot is cleared.
- **D2 (Low, docs).** `US-0203-remove-zoom.md:129` ("Known cost"): add that the pairing holds only while `override_strip_menu` is installed, which `oneterm_terminal_view::init` does once; a consumer that builds a `TerminalPanel` without `init` keeps one panel alive in the slot per menu open.
- **D3 (Low, i18n).** `tab_title.rs:1013` writes the Close row as the literal `"Close"` where the kit uses `t!("Dock.Close")` (`tab_panel.rs:352`). OneTerm ships English only, so no visible change; note it in the comment or use `gpui_component::t!` if the crate exports it. Dead today in any case (`TerminalPanel::closable` is `false`, `terminal_panel.rs:662-664`).

### Notes (non-blocking)

- **N1 (Info).** The pairing is order-based, not identity-based. A cheap belt if the kit ever grows a submenu in that menu: the observer could check `cx.entity_id()` against an id recorded by the arm; not possible today because the panel never sees the menu's entity, which is the whole reason for the hack. Left as is.
- **N2 (Info).** No gate record for D1 in the packet or the commit message; the Evidence section's gate line describes the tree before D1.

### Still by hand (owner / ui-verifier)

`cargo test -p oneterm-terminal-view` and clippy on this head (the three strip-menu tests, including the pinned one, which is expected to fail until D1 is fixed), then the full gate before merge. On screen: with Vertical Tabs off, the `...` menu shows the tab rows only, the active tab ticked, a row click switches tabs, Esc closes the menu and focus returns to the strip; the terminal's right-click context menu and the SFTP pane menus are unchanged after opening the `...` menu once; the same in a second window if one is open.

## Rework D1 re-review

- Scope: `git diff dff31acc 23c8bb87` (one commit, `23c8bb87`, the fixes for D1-D3 above) on `verify/in-0055-drag-zoom`, 2026-10-23. Read only; no cargo, no GUI. The coder's gate (`ci-local.ps1`, 2570 passed, 0 failed) is recorded in the packet and not reproduced here.
- Read: `crates/terminal-view/src/panel/tab_title.rs:962-1049`, `panel/tests/tab_menu_and_card.rs:486-500`, `US-0203-remove-zoom.md:131-137`; kit `gpui-component-0.7.3/src/lib.rs:127, 155-163`; `rust-i18n-macro` `src/lib.rs:515`; Rust 1.94.1 `library/test/src/lib.rs:625-640`.

### Verdict

**PASS.** All three fixes are correct and minimal. One correction to my own D1: its cross-test reach was over-stated (see 1).

### Each fix, attacked

1. **D1, is the slot cleared on every test path?** `strip_menu_rows` now disarms unconditionally after reading the rows (`tab_menu_and_card.rs:496-498`), on both the observer and the control path, so the two existing walks and the pinned test leave nothing. A panic before that line (`expect("the strip is drawn")`, `:479`) would skip it; that does not matter, and neither did the original hazard as much as I said: libtest runs every test on a freshly spawned thread (`library/test/src/lib.rs:630-634`, `thread::Builder::new().name(...).spawn`), so a `thread_local!` never crosses from one test to the next. The real residue on `b0944bff` was a strong `Entity<TerminalPanel>` (with its fake session) held until the test thread exited, plus the dormant hazard of a stale closure reading a dead entity if anything ever did take it. Both are gone: the closure holds a `WeakEntity` (`tab_title.rs:968`), and `strip_menu` returns the menu as the kit built it when `upgrade()` fails (`:1028-1030`). `#[gpui::test]` iterations within one test do share the thread, and the disarm covers that too. The D1 severity should be read as Low in hindsight; the fix stands as the right shape regardless.
2. **D1, could a weak upgrade succeed for a stale panel in another window?** Only while the panel entity still lives in the same `App`, and only if something takes the slot after the arm's own menu. In production the arm and the take are the same `cx.new` (`dropdown_menu.rs:131-133` builds, `app.rs:3235-3239` queues `EntityCreated`, flushed at the end of the same update), so the slot is never stale. Without the observer the slot is simply overwritten on each open (`*slot.borrow_mut() = Some(...)`, `tab_title.rs:969-971`), so at most one closure exists, naming the panel of the last `...` menu opened; nothing can take it because `init` is the only installer and runs once at startup. Windows do not enter: `strip_menu` reads the panel and its group (`:1031-1041`), not window state, and a `WeakEntity` of a dropped `App` upgrades to `None` (its ref-count table is gone). No path rebuilds a menu as another panel's tab list.
3. **D3, is "the kit's `t!` table is not reachable from our crate" true?** Essentially yes. `t!` is `rust_i18n`'s macro and expands to `crate::_rust_i18n_translate(...)` of the *calling* crate; OneTerm has no `rust-i18n` dependency and no `i18n!` table (grep of `Cargo.toml` and `crates/`), so `t!("Dock.Close")` cannot be written here. Precisely: the kit's generated `pub fn _rust_i18n_translate` (`rust-i18n-macro/src/lib.rs:515`, emitted into `gpui_component` by `lib.rs:127`) and `gpui_component::locale()` / `set_locale()` (`lib.rs:155-163`) are technically callable, so the table is reachable through an underscore-prefixed generated function that is not a supported API. Not worth taking: OneTerm never calls `set_locale`, so the kit's own rows are English too, and the literal matches every other Close row in the file. The comment at `tab_title.rs:1042-1044` is accurate enough; no change asked.

### Defects

None.

### Still by hand (owner / ui-verifier)

Unchanged from the D1 review: the on-screen walk of the `...` menu with Vertical Tabs off (tab rows only, active ticked, row click switches, Esc closes and focus returns), the terminal and SFTP context menus unchanged after opening it, and the same in a second window.

## UI verification, real input (ui-verifier, sonnet/medium), build 018eccd5 and D1 build b0944bff

Covers US-0198 rework 8, US-0202 and US-0203. Screenshots are kept in the coordinator's session scratchpad (r10w-shots), not in the repo.

## UI verification (real input)

Binary r10bin/oneterm.exe (built 018eccd5), SendInput driver, private profiles r10w-*, dark theme (Zed One Dark), 1600x918 client. Screenshots: scratchpad\r10w-shots\ (prefix by item: e1/e2 rail expanded, c1-c3 collapsed, h3-h6 strip, f font, z/k zoom+settings, s SFTP, q zoomed_panel). Pids spawned and stopped: 44704, 64752, 28360, 40892 (the last quit by its own close button).

| # | Item | Result | Evidence |
|---|------|--------|----------|
| 1 | Rail collapsed: reorder from icon, top margin, left edge, right edge, TL/BR corners, bottom edge | PASS | c1-sheet3.png (">_" cell moved top/bottom on each of 7 drags) |
| 1 | Collapsed drop on empty Space moves terminal in, emptied tab closes | PASS | c2-space2-mid.png (blue Space fill), c2-space2-after.png |
| 1 | Collapsed drop on live terminal (centre, bottom, left edge zone, right edge zone) | PASS: no split, no move | c3-l3-mid.png, c3-l4-after.png |
| 2 | Rail expanded: reorder from title, subtitle, top margin, bottom edge, TL corner | PASS | e1b-after, e1-sheet.png, e1-sheet2.png, e1-sheet3.png |
| 2 | Expanded drop on empty Space moves terminal in | PASS | e2-space-mid.png, e2-space-after.png |
| 2 | Expanded drop on live terminal (4 positions incl. top bar of the area) | PASS | e2-live-sheet.png |
| 3 | Strip: reorder by title, left padding, right padding, top edge, bottom edge, with drop line | PASS | h3-sheet.png, h3-sheet2.png |
| 3 | Strip drop on empty Space moves terminal in | PASS | h4-space-mid.png, h4-space-after.png |
| 3 | Strip drop on live terminal (4 positions) | PASS | h5-sheet.png |
| 3 | x closes; double-click on label opens "Rename Tab" dialog | PASS | h6-sheet.png |
| 4 | No zoom button in tab bar / dock headers; no "Zoom" row in tab menu or terminal context menu; Shift+Esc inert; Settings search "zoom" finds nothing | PARTLY | m1.png, m2.png, z01.png, k2.png; see D1 |
| 5 | docks.json copy with zoomed_panel starts unzoomed, drag works, saved docks.json has no zoomed_panel | PASS | q-sheet.png; docks.json after quit has keys schema_version, center, right_dock, version |
| 6 | SFTP Expand shows dual pane inside the dock width; Collapse restores Session | PASS (no connection, "No SFTP connection" state only) | s1.png, s2c.png |
| 7 | Ctrl+= grows, Ctrl+- shrinks, Ctrl+0 resets font | PASS | f-sheet.png |

### Defects

1. **D1 (Low-Medium) Disabled "Zoom In" row in the tab-list menu.** Horizontal strip, click the "..." button right of "+" in the tab bar (z5c.png): the menu shows Terminal Tabs list, a separator and a greyed "Zoom In". Expected: no zoom row anywhere (US-0203). Likely the kit's own overflow menu entry, not removed. Repro: Vertical Tabs off, open the "..." menu. (Not seen with the dock's right-hand toggle.)
2. **D2 (Low, not reproduced) First rail drag after launch did nothing.** Expanded rail, very first drag of the session (title of row 1 to row 4, e1-title.png) changed nothing; the same drag later (e1b) and 40+ others reordered. May be first-focus/threshold timing of my driver; one occurrence, could not repeat.

### Observations (not defects)

- The rail's right 4 px edge is the width resizer (US-0197): a drag from x=262 resizes the rail, it is not a reorder zone. Right-margin points (x 252-255) were not testable on the 140 px rail I accidentally made; left, top, bottom and corners inside the rail reorder.
- SFTP expanded in the dock width is cramped, as ruled (Local pane toolbar "Local C" and the Date column clip); left as the owner ruled.
- Strip drag preview chip truncates "PowerShell 7" to "PowerSh..." (cosmetic).
- Empty-Space hover fill is bright accent blue with dimmed hint text (h4-space-mid.png); legible as a drop cue.

### Owner-only / not walked

- Light theme and other dark variants of the drop cue (walked dark only).
- Real SFTP host content in the expanded dual pane (no connection made).
- Right-margin reorder zone of the expanded rail at full 266 px width (resizer overlaps the last 4 px).
- Touch/pen, IME, sleep/resume, UAC elevation.

## Re-walk D1 (build r11bin, head b0944bff)

Fresh profiles r10w-cwd-h2 (Vertical Tabs off) and r10w-cwd-v2 (on); pids 14024 and 91520, both stopped.

| Item | Result | Evidence |
|------|--------|----------|
| (a) "..." menu lists the tab list only, no Zoom row | PASS | w4sheet.png (3 shots after the click, header "Terminal Tabs", cmd.exe, PowerShell, PowerShell 7 with a check on the active one) |
| (a) No visible flash of a Zoom row | PASS with a caveat: the earliest shot is about 0.3 s after the click, so a single-frame flash cannot be ruled out | w4-1..3.png |
| (a) Mouse pick activates the tab | PASS | w5-mouse.png (cmd.exe active) |
| (a) Keyboard pick (Down, Enter) activates the tab | PASS: three Down presses highlight PowerShell, Enter activates it | w6sheet.png |
| (a) Esc closes the menu | PASS | w5-esc.png |
| (b) Strip tab right-click: Rename, Duplicate, Input Channel, Close, Close Others, Close to the Right | PASS | x123.png |
| (b) Terminal right-click: New Terminal, Duplicate Session, Split x4, Copy/Paste/Select All/Clear, Log | PASS | x123.png |
| (b) Session list right-click: New Session | PASS | x123.png |
| (b) SFTP panel menu (the vertical-dots button): New Folder, Upload, Rename, Delete, Refresh; SFTP row right-click: Open, Upload, Rename, Delete, New Folder, Refresh | PASS | x56.png |
| (b) Rail context menu: Rename, Duplicate, Input Channel, Close, Close Others, Close Tabs Below | PASS | v1-railmenu.png |
| (c) Strip reorder drag (PowerShell 7 to first) | PASS | y1-sheet.png |
| (c) Rail reorder drag (PowerShell 7 to first) | PASS | y2-sheet.png |

Defects: none. D1 is fixed.

Observation (Low, kit behaviour, not new): in the "..." menu the first Down press highlights nothing; the second highlights the first tab.
