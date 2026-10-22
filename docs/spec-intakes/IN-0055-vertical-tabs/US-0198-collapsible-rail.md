# Work: Collapse the vertical tab rail to a 44 px strip

ID: US-0198
Intake: IN-0055
Created: 2026-10-21

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [ ] In progress
- [x] Implemented
- [ ] Changed
- [x] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: new capability (UI) on an accepted contract
- Risk lane: normal (presentation inside OneTerm's own renderer seam; the one persisted field, `ui_config.json` `vertical_tabs.collapsed`, already exists, is parsed leniently since `US-0196`, and gains its writer here; no auth, secret, migration or external effect)
- Spec Intake, when required: IN-0055 (owner ruling Q4, 2026-10-20: collapsed rows show the tab's position number with the status icon as a badge and the label in a tooltip)

## Outcome

The vertical tab rail can be collapsed to a 44 px strip and expanded again. A button in the rail header (collapse while expanded, expand while collapsed) flips `vertical_tabs.collapsed` through `UiConfig::set_vertical_tabs_collapsed`: live on the next frame, stored once in `ui_config.json` (default `false` omitted; the elevated window applies it in memory and writes nothing), remembered across a restart and while the Vertical Tabs switch is off. While collapsed the rail is exactly 44 px wide whatever the stored width preference or the group width is, the resize handle is not drawn, and each row is a 44 px cell showing the tab's position number (1, 2, 3 ...) with the OSC 7501 status icon as a corner badge and the full title in a tooltip. Click selects, middle-click closes, right-click opens the tab menu, dragging the row's content drags the tab into an empty Space and the grip still reorders; only rename-by-double-click is gone (the menu's Rename... remains).

## Scope

- [x] In scope: `UiConfig::set_vertical_tabs_collapsed` (the one writer, writes nothing when unchanged); the header button and tooltips; the collapsed header (button, `+`, zoom stacked in 44 px); `VerticalTabs::COLLAPSED_WIDTH` and the pure width selection (`rail_width`); the handle off while collapsed; `TabRowLayout::Collapsed(position)` and the compact row in `render_tab_strip` (number, status badge, recording dot, tooltip, no title/cwd/chips/rename); the collapsed row cell in the rail (44 px, no scrollbar gutter, the grip as an overlay); `docs/gui-layout.md`, `docs/agents/persistence.md`, the HLD.
- [x] Out of scope: hover-to-expand (intake follow-up), a collapse shortcut or app-menu item, animating the width change, channel chips in the collapsed row (see Plan), keyboard tab navigation.

## Acceptance

- [x] `UiConfig::set_vertical_tabs_collapsed`: default is `false` and is omitted from the file, `true` round-trips, a wrong-typed value loads as `false` without quarantine (existing lenient parse, pinned again), a setter call with the stored value changes nothing, and the entity changes before the write so an elevated window still collapses in memory.
- [x] Width selection: collapsed is 44 px for any preference and any group width (unit test); expanded is `clamp_tab_rail_width` as before.
- [x] Collapse and expand are live: the header button flips the rail between its width and 44 px on the next frame, in both directions, rebuilding no panel and ending no session; the content frame's margin follows.
- [x] Collapsed, the resize handle is not drawn and a drag at the rail's edge resizes nothing; expanding restores the preference's width.
- [x] Collapsed rows are 44 px cells stacked in tab order, each showing its position number; a click selects, the number follows the position after a reorder or a close.
- [x] The status icon is a badge of the cell and the number is centred (tests); the recording dot and the tooltip are in the element but not pinned by a test (see Gaps).
- [x] Every row action works collapsed except rename-by-double-click: middle-click closes, the right-click menu opens (with "Close Tabs Below"), the row content starts `DragTerminalTab`, the grip starts `DragPanel`.
- [x] The Vertical Tabs switch off while collapsed keeps `collapsed = true`; turning it on again returns to the collapsed rail.
- [ ] Zoom, the `+` menu and the scrolling of 40 collapsed rows still work (header stacked, rows scroll, active row in view): the stacked header and 40 cells are for `ui-verifier`; the existing zoom and 40-row tests cover the expanded rail only.
- [x] Full `pwsh scripts/ci-local.ps1` passes.
- [ ] On screen, by `ui-verifier`: light and dark themes, expanded and collapsed, the badge against the overlay scrollbar with 40 tabs, the tooltip, restart persistence, the elevated window.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0055-vertical-tabs/high-level-design.md` § Rail anatomy, § State Machine, § Data Flow 6, § UI Wireframe (collapsed), § Data Ownership.
- `docs/spec-intakes/IN-0055-vertical-tabs/IN-0055.md` Q4 and the `US-0198` packet entry (owner accepted all questions as recommended).
- `docs/spec-intakes/IN-0055-vertical-tabs/US-0197-resizable-rail-width.md` — the template for the width path, the handle and the persistence rules; its handoff names this packet.
- `docs/gui-layout.md` § Vertical tabs; `docs/agents/persistence.md` `ui_config.json` row (the elevation rule, `DEC-0019` M4).
- `docs/agents/crate-dependency-rules.md` (R2, R4) — the shell still reaches the row only through `PanelHandle`; the layout travels in `oneterm-state`.

### Documentation Action

- Update required: `docs/gui-layout.md` (Vertical tabs: a **Collapsing** paragraph, the closing "later packet" sentence), `docs/agents/persistence.md` (`collapsed` has its writer and effect), the HLD (collapsed wireframe and rows, data ownership), this packet and the intake's tick.

Reason: collapsing becomes user-visible behaviour with a writer.

### Reconciliation

Changed: `docs/gui-layout.md` (header bullet, a **Collapsing** paragraph with wireframe, the closing sentence), `docs/agents/persistence.md` (`collapsed` has its writer), `high-level-design.md` (header row of the anatomy table, the collapsed wireframe), `IN-0055.md` (packet link), this packet. Unchanged and still valid: the HLD state machine and Data Flow 6, `docs/agents/crate-dependency-rules.md` (no new edge: the layout value travels in `oneterm-state`, below both crates).

## Context

- The collapsed row is the panel's own `title()` drawn under a third `TabRowLayout` value (HLD: "the row element renders its compact form when it reads `collapsed`"), so the drag, middle-click and context menu are the one implementation both layouts use; the rail wraps its `title()` call in the scope as it does for the expanded row and supplies the 1-based position.
- `US-0197`'s width path is `TabRail::begin_frame`; collapsing short-circuits it before the group clamp.

## Plan

- [x] Records commit: this packet.
- [x] `VerticalTabs::COLLAPSED_WIDTH` and `UiConfig::set_vertical_tabs_collapsed`, with the settings tests.
- [x] `TabRowLayout::Collapsed(position)`; the compact row and its tooltip in `render_tab_strip`; "Close Tabs Below" for it.
- [x] `tab_rail`: `rail_width`, the collapsed header and cell geometry, the header button, no handle, no gutter; workspace tests (live flip, handle, numbers, actions, switch-off memory).
- [x] Docs, gate, debug binary.

Choices left open by the HLD (decided here, for review):

- *Cell size.* 44 x 44 px (a square cell as wide as the strip). The expanded row is 64 px because it carries two lines; a number and a badge need no more than a square.
- *Badge placement.* The status icon sits in the cell's top-right corner (3 px inset) and the recording dot in the top-left (3 px inset); both are the same elements the expanded row draws. The number is centred in the whole cell, so the 12 px reorder grip (shown on hover, as expanded) is an overlay on the cell's left edge, painted after the content so it still wins the press there.
- *Channel chips.* Not drawn collapsed: 44 px cannot hold a chip beside a two-digit number and a badge, and the HLD's wireframe shows none. Broadcast membership stays visible in the expanded rail and the status bar. Recorded as a known loss for the owner.
- *Scrollbar.* No gutter is reserved while collapsed (it would take 16 of 43 px). The overlay scrollbar's thumb can cover part of the badge while the pointer is over it; `ui-verifier` to confirm with 40 tabs. (Superseded by D3: the strip draws no scrollbar at all, see Rework 3.)
- *Tooltip.* The full tab label only (not the directory); the status icon keeps its own tooltip, which wins when the pointer is on the badge.
- *Header.* Collapsed: a column of expand button, `+` and zoom, each centred, 1 px divider below. Expanded: the collapse button sits with the zoom button on the right. Icons `PanelLeftClose` / `PanelLeftOpen` (the kit's, as its sidebar uses), tooltips "Collapse Tabs" / "Expand Tabs".
- *Resizing mid-collapse.* The handle and its tracker are not built while collapsed; a drag cannot be in progress when a click on the header button lands, so no drag state needs resetting.

## Decisions

No decision record: the stored field, the state machine and the owner's Q4 ruling are the HLD's.

## Verification Plan

`cargo test -p oneterm-settings`, `-p oneterm-state`, `-p oneterm-terminal-view`, `-p oneterm-workspace`; clippy; fmt; `check-doc-paths`, `check-english`; the full `pwsh scripts/ci-local.ps1` (`CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=2`; the bash script stops on this Windows machine at its `oneterm-vt` six-dependency check because Windows' `sort.exe` shadows GNU `sort`, the PowerShell script runs the same checks); the debug app for `ui-verifier`.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

- **Tests.** `oneterm-settings` `the_collapsed_flag_is_lenient_and_written_alone` (off-and-collapsed writes `{"collapsed": true}` alone; a wrong-typed `collapsed` reads as expanded and keeps the other fields; the constant is 44) beside the existing default / round-trip / garbage tests; `oneterm-state` `a_collapsed_row_carries_its_position`; `oneterm-workspace` unit `the_collapsed_rail_is_44_px_whatever_the_preference_and_the_group` (`rail_width`), and `tab_rail_tests`: `the_header_button_collapses_the_rail_to_a_strip_and_expands_it` (click on the real header buttons, width 300 to 44 to 300, the terminal's origin follows the strip, the handle exists only expanded, 44 px cells stacked, no panel removed), `a_collapsed_cell_is_built_with_its_position_and_a_click_selects_it`, `a_collapsed_cell_keeps_the_content_drag_and_the_grip_reorder` (content drags the tab, the 12 px grip overlay reorders), `the_switch_off_keeps_the_rail_collapsed`, `the_collapse_setter_skips_an_unchanged_value` (0, 1, 1, 2 `UiConfig` updates); `oneterm-terminal-view` `a_collapsed_cell_shows_the_position_and_the_status_badge_only` (44 x 44, number centred, no title / directory / close button, the badge in the top-right corner clear of the number), `a_collapsed_cell_draws_the_position_it_is_given` (a two-digit position is wider than a one-digit one) and the "Close Tabs Below" label rule extended to `Collapsed`.
- **Gate:** `pwsh scripts/ci-local.ps1` on Windows (`CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=2`, worktree `target`): all checks passed (`cargo test --workspace`: 2509 passed, 0 failed, 19 ignored in that step; the three `--ignored --test-threads=1` lines, `check-ignored-tests.py`, the `oneterm-vt` steps, `verify-dependency-graph.py`, `check-doc-paths.py`, `check-english.py`, `check-theme-contrast.py` (1638 pairings, 624 hierarchy comparisons) and the notices check included). `bash scripts/ci-local.sh` was tried first and stops at the `oneterm-vt` six-dependency check on this machine only because Windows' `sort.exe` shadows GNU `sort` in that shell; the PowerShell script is the same set of checks.
- **Debug binary** `target\debug\oneterm.exe` built (`cargo build -p oneterm-app`). **Smoke launch** with a private profile (scratch cwd and `USERPROFILE`, `target/ui_config.json` `{"vertical_tabs":{"enabled":true,"collapsed":true}}`): ran 10 s, the config file untouched; the pid was stopped. No UI verdict here.
- **Elevation:** the setter has no write path of its own; it calls `UiConfig::persist`, which asks `write_refusal` (existing `an_elevated_window_refuses_to_write_a_perfectly_readable_config`).

### Rework after advisor review and the UI walk (2026-10-21)

- **A1** (advisor, low): collapsing or expanding kept the pixel scroll offset, so with 40 tabs the displayed tab could leave the viewport. `TabRail` now remembers the last drawn `collapsed` and re-reveals the displayed tab when it flips (`tab_rail.rs`, `render`). The advisor's pinned test `expanding_the_rail_keeps_the_active_row_in_view` lost its `should_panic` and passes.
- **A2** (advisor, low): the expanded row's budget is now exact, bottom padding 9 px plus the 1 px divider (`tab_title.rs`, `TabRowLayout::VERTICAL_ROW_DIVIDER`); recorded in the HLD, `gui-layout.md` and the US-0196 Rework 3 entry.
- **A3** (advisor, info): the collapsed header has `min_h` of three 20 px buttons (`COLLAPSED_HEADER_HEIGHT`, pinned by the collapse test).
- **A4** (advisor, info): a middle or right press on the 12 px grip overlay reaches the cell's own close and menu handlers, because the overlay handles only the left-press drag, so those presses fall through to the cell. **Accepted** as a larger target; documented in `gui-layout.md`.
- **A5** (advisor, info): the Verification Plan names `pwsh scripts/ci-local.ps1`, with the reason.
- **U1** (ui-verifier, low): the cell's hover tooltip stayed over the right-click menu. The tooltip is now a small view that draws nothing while `TerminalPanel::tab_tooltip_hidden` is set; a right press sets it and the pointer entering the cell clears it (test `a_right_click_hides_the_collapsed_cells_tooltip_until_the_pointer_returns` pins the flag; that the drawn tooltip really goes away is for `ui-verifier` to re-walk).
- **U2** (ui-verifier, low): the divider stopped at the scrollbar's gutter; the gutter moved from the list into each row's own padding, so the divider spans the rail (workspace 40-tab test: the row spans the list, its content clears the gutter).

- **Gate after the rework:** `pwsh scripts/ci-local.ps1` (`CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=2`): all checks passed, `cargo test --workspace` 2512 passed, 0 failed, 19 ignored; debug app rebuilt.

### Rework 2 (ui-verifier re-walk, D3 and nits)

- **D3 (high): in an overflowing collapsed rail a click on a cell did not select the tab and paged the list.** Root cause: the kit's overlay scrollbar (`gpui-base` `scrollbar.rs`) paints its track in the right `WIDTH` = 16 px of the scroll area (`:22`, bounds at `:1409-1411`), and while the bar is shown (always, under OneTerm's `ScrollbarMode::Always`) its `MouseDownEvent` listener (`:1716-1745`) stops propagation for any press inside that track and jumps the offset to the click (`:1736-1742`). The collapsed cell is 43 px wide, so the right 16 px (37 %) of every cell was the scrollbar's; the expanded rail pads its rows by the gutter and never overlapped it. Fix, in our code only: the collapsed strip draws the same kit scrollbar through `Scrollbar::vertical(..).styles(..)` with a 6 px track and 4 px thumb (`tab_rail.rs`, `narrow_scrollbar`) instead of `vertical_scrollbar`, whose track is the 16 px default and cannot be styled. Alternatives rejected: reserving a gutter only while overflowing (the cell would shrink from 43 to 27 px and the number would jump as tabs are added), and an axis or mode that takes no track presses (the thumb must stay draggable). Accepted cost: a press on the cell's right 6 px, where the badge sits, is the scrollbar's at all times. Test `a_click_on_a_cell_of_an_overflowing_collapsed_rail_selects_it` (40 collapsed tabs; pointer moved onto the cell first, which is what shows the bar; clicks at 10 %, 50 % and 80 % of the cell width select that tab and leave the list where it is) failed before the fix (observed at 90 % of the width, and by the same geometry at 80 %). A click at 90 % (inside the 6 px track) is consumed by design and is not asserted.
- **N1:** the row's divider is drawn with `border_b(px(TabRowLayout::VERTICAL_ROW_DIVIDER))`, so the state constant is the one source.
- **N2:** the comment at `tab_title.rs` now says what gpui does (clears the tooltip on a press and shows it again on the next move while the cell is hovered).

- **Gate after rework 2:** `pwsh scripts/ci-local.ps1` (`CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=2`): all checks passed, `cargo test --workspace` 2513 passed, 0 failed, 19 ignored; debug app rebuilt.

### Rework 3 (ui-verifier re-walk 2: D3 remains, D4)

- **Decision (coordinator): the collapsed strip draws no scrollbar.** `narrow_scrollbar` and `COLLAPSED_SCROLLBAR_WIDTH` are deleted; `tab_rail.rs` adds the kit scrollbar only when expanded. The list still scrolls by wheel or touchpad and the displayed tab is revealed by `scroll_to_item` (on a tab change and when the rail collapses or expands). **D4** (the 4 px thumb cannot be dragged) goes with it: a thumb thin enough to leave the cell clickable has no usable hit area, so the strip has none.
- **What was eating the right of the cell.** Only the scrollbar track: a press inside it pages the list and is not passed on (`gpui-base` `scrollbar.rs:1716-1745`, the track being the right `track_width` px of the scroll area, `:1409-1411`). The "dead zone of about 14 px, nothing moves at x=43..50" of re-walk 2 is a coordinate mix-up, not a second hit-eater: the driver's `click x y` posts **client** coordinates, while the screenshots are of the whole window, whose 8 px frame offsets them. The cells are client x = 0..42 (screenshot 8..50), so client 37..41 is the 6 px track (screenshot 45..49, which paged the list) and client 43..50 is the rail's border and the terminal area (a click there does nothing to the tabs). Checked on the real binary with the same driver and no scrollbar (25 collapsed tabs, a different tab active before each probe, selection read from the accent bar; the wheel path is covered by the test only): clicks at client x = 14, 22, 28, 33, 36, 38, 40 and 42 all select; 44..50 are outside the rail. No other element covers the cell: the resize handle is not built while collapsed, the grip overlay is the left 12 px, the badge and its tooltip sit inside the cell.
- **Test.** `a_click_across_a_cell_of_an_overflowing_collapsed_rail_selects_it` now scrolls the list by wheel first (which shows the bar), then clicks every pixel across a cell, 0 to 42, each on a tab that is not the displayed one, with the pointer moved there first; every click selects and the list does not move. Run against the previous commit (6 px track) it fails at 37 px across the cell, which is where the 6 px track starts (43 - 6). The harness uses cell-relative coordinates, which is why the earlier test could not show the verifier's apparent 14 px zone: that zone was the window frame.
- The row divider uses `TabRowLayout::VERTICAL_ROW_DIVIDER` and the tooltip comment is exact (nits N1, N2, done in the previous commit).

- **Gate after rework 3:** `pwsh scripts/ci-local.ps1` (`CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=2`): all checks passed, `cargo test --workspace` 2513 passed, 0 failed, 19 ignored; debug app rebuilt.

### Rework 4 (owner): the collapse button lives in the status bar

Acceptance rework of this packet (reopened, not a new BUG). Owner request, 2026-10-21 (translated from Vietnamese): "Move the Collapse/Expand Tabs button to the status bar, left-aligned, just before the clock." The other 2026-10-21 owner requests (no zoom button, `+` on the right, no scrollbar) are in the `US-0196` Rework 4 entry.

- **Outcome.** The rail header no longer holds the collapse / expand button. The status bar has it as the first item of its left group, immediately to the left of the clock: `PanelLeftClose` with the tooltip "Collapse Tabs" while the rail is expanded, `PanelLeftOpen` with "Expand Tabs" while it is collapsed (the same icons and words as the header button had). It calls the same writer, `UiConfig::set_vertical_tabs_collapsed(!collapsed, cx)`, so a value equal to the stored one still writes nothing and the elevated window still applies it in memory and writes nothing (`write_refusal`).
- **When it shows.** Only while Vertical Tabs is on (`UiConfig::read_vertical_tabs(cx).enabled`, read when the bar renders; the setters already `refresh_windows`, which re-renders the cached bar). It is not tied to whether the active tab group happens to draw the rail (the SSH Client group does not): the button is a switch for the stored preference, which every rail follows. The elevated window shows it (the rail can show there) and its click changes memory only.
- **Where the code is.** `crates/workspace/src/layout/statusbar.rs` (`build_status_bar`): `workspace` already depends on `settings`, so no crate edge is added (R2, R4 hold); `StatusBar::left(toggle)` before `.left(clock)`. The centre region's width budget (`divide_centre`) gets the button's 28 px (20 px plus the left region's 8 px gap) taken off the window width while the button is shown.
- **Collapsed header.** With the button gone the collapsed header holds only `+` (see `US-0196` Rework 4); the three-button stacking floor is deleted.
- **Tests.** `tab_rail_tests`: the collapse test flips the rail through the setter and asserts the header has neither button; `statusbar.rs` `caching::the_tabs_toggle_shows_only_with_vertical_tabs_and_flips_collapsed` (absent while the switch is off; present while on, left of the clock; a click collapses and the second expands).
- **Acceptance.**
  - [x] The rail header has no collapse / expand button, expanded or collapsed.
  - [x] The status bar shows the button just left of the clock only while Vertical Tabs is on, and its click flips `collapsed` live.
  - [ ] On screen, by `ui-verifier`: placement and spacing next to the clock in two themes, the tooltip, an elevated window, a narrow window (the centre breadcrumb still gives way first).
- **Gate.** `pwsh scripts/ci-local.ps1` (`CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=2`): all checks passed; `cargo build -p oneterm-app` debug binary built.
- **Documentation.** `docs/gui-layout.md` ("Collapsing", Status bar), `high-level-design.md` (wireframes, anatomy, collapse step).

### Rework 5 2026-10-21 (owner): a separator before the clock, and a full-information hover card on a collapsed cell

Acceptance rework of this packet (reopened, not a new BUG). Owner requests, 2026-10-21 (translated from Vietnamese): (1) "Add a separator between Collapse/Expand Tabs and the clock." (3) "While collapsed, hovering a tab shows the tab's full information like when expanded." Request 4 (hover background of every rail row) is recorded in the `US-0196` Rework 5 entry because that packet owns the row look. (The owner's request 2 is `US-0199`, `IN-0022`.)

**Outcome.**

- *Separator.* The status bar draws a vertical separator between the Collapse / Expand Tabs button and the clock, only while the button is shown. It is the bar's own `separator()` (1 x 12 px, `cx.theme().border`) that already divides the centre region from the right group and the right group's items, so the left group now reads `[button] | [clock]`. The kit `Divider` was not used: the bar's separator is the one shape every other divider in the bar has, and it is a plain `div` with a theme token. The centre budget's chrome constant `TABS_TOGGLE_CHROME` grows by the separator's 1 px and one more 8 px gap of the left region: 28 -> 37 px.
- *Hover card.* The collapsed cell's title-only tooltip is replaced by a hover card with the same content as the expanded vertical row: line 1 the channel chips, the recording dot and the title; line 2 the directory (the status bar's `breadcrumb_label`, cut at its start) with the OSC 7501 status icon at the end. The card is the kit's `Tooltip::element` (popover fill, `popover.foreground`, `border`, shadow), 220 px wide in all (`VerticalTabs::DEFAULT_WIDTH`, the default rail width; the kit's tooltip chrome is 18 px, so the content is 202 px). Its content is built by the same function the expanded row uses (`vertical_row_lines` in `tab_title.rs`), rebuilt from the live panel on each frame the card is shown, so a working icon or a changing directory is current. A right-press still hides the card until the pointer comes back to the cell (`CellTooltip` / `tab_tooltip_hidden` unchanged).

**Choices left open by the requests.**

- *Surface of the card.* The kit tooltip's popover surface, not the rail's, so the muted directory on `popover.background` is the pair `check-theme-contrast.py` already measures (`muted.foreground` and the warning bell are listed on that surface); no new surface.
- *Width.* 220 px fixed; a long title or directory is cut by the same ellipsis rules as the expanded row (the title at its end, the directory at its start).
- *Status icon's own tooltip inside the card.* Kept (the same element), it is never reached because the pointer is on the cell.

**Acceptance.**

- [x] With Vertical Tabs on, a separator sits between the button and the clock (test: `toggle.right() + 8 + 1 + 8 == clock.left()`); with it off there is neither.
- [x] The centre budget accounts for the separator (the constant equals button + separator + two gaps).
- [x] The hover card is built from the same function as the expanded row (test draws the card: title, directory below it, status icon on line 2, width. The channel chips and the recording dot come from the same `tab_chips` / `recording_dot` helpers the expanded row uses, but the card test does not drive a channel or a log, so they are not pinned there).
- [x] Status bar separators sit only between drawn items (ui-verifier D2, 2026-10-22). Cause: `build_status_bar` drew the two centre separators and the two right-group separators unconditionally, so an empty cwd and branch (a tab with no shell, a non-repository) left two dangling after the clock; it was already so on main. Now the centre's leading separator needs the cwd or the branch, the middle one needs both, and the right group's follow the network and resource figures that are drawn (test `separators_only_sit_between_drawn_items`; the width budget still counts the full set, which only under-uses the room).
- [ ] On screen, by `ui-verifier`: separator in two themes, the card on a collapsed cell with chips / a long directory / the status icon, hidden while the context menu is open.

### Rework 6 2026-10-22 (owner): the shell icon replaces the position number, the status is the cell's background

Acceptance rework of this packet (reopened, not a new BUG). Owner request, 2026-10-22 (translated from Vietnamese): "When the vertical tab rail is collapsed, instead of showing 1 2 3 4 5 6, show the shell icon: cmd shows a cmd icon, PowerShell shows a PowerShell icon; by default show one generic shell icon if there is no icon for that shell. The OSC 7501 status icon is no longer shown on the tab; instead the tab's background colour is the status colour." The sentence is about the collapsed rail: expanded rows and the horizontal strip keep their status icon, and the collapsed hover card keeps the full row with its status icon. This supersedes the 2026-10-20 owner ruling Q4 (position number plus status badge) for the cell only.

**Outcome.**

- *Shell icon.* A collapsed cell shows one monochrome glyph, centred, in place of the position number: `AppIcon::Cmd` (new `cmd.svg`) for `cmd`, `AppIcon::Powershell` (new `powershell.svg`) for Windows PowerShell and PowerShell 7, `AppIcon::Terminal` (the existing `terminal.svg`) for everything else (Bash and Git Bash, Zsh, Sh, WSL, SSH, a custom shell, a tab whose shell is unknown, an empty tab). The glyph is `Icon::small()` (16 px; the old number was `text_sm`, 14 px).
- *Source of the shell.* The tab's active terminal view carries `duplicate_config: Option<SessionDuplicateConfig>` (set for every spawned local shell and every SSH connect; `view.rs`), which is the launch data the Duplicate command already uses. `Local(LocalShellConfig { kind, program, .. })` is classified by `tab_title::shell_icon`: an explicit `program` wins and is named by its last path component with `.exe` removed (`program_display_name`, the function the tab label uses), compared case-insensitively (`cmd` -> cmd; `powershell`, `pwsh` -> PowerShell; any other name -> generic, so `kind: cmd` with `program: nu.exe` is the generic glyph exactly as its label says "nu"); with no program, `kind` decides (`ShellKind::Cmd` -> cmd, `PowerShell` and `Pwsh` -> PowerShell, the rest generic). `Ssh(_)` and `None` -> generic. The window title is never read. There is no Git Bash or WSL kind: Git Bash is `Bash` with a program and WSL is a program run inside another shell, so both are generic.
- *Original artwork.* `cmd.svg` (a window frame with a title rule and a prompt chevron with an underscore) and `powershell.svg` (a slanted panel with a `>_` chevron) are drawn from scratch for this repository on the 24 x 24 grid, `fill="none"`, `stroke="currentColor"`, `stroke-width="2"`, round caps and joins, like `terminal.svg` and `clock.svg`. They are not Microsoft's logos or marks and are not copied or traced from any third-party icon; they stand for the shell category, not for the vendor. No licence entry is needed (`third-party-notices.py` lists crates, and an own SVG is the project's).
- *Status as background.* No badge and no animation in the cell. The cell's fill follows the tab's OSC 7501 summary state (`tab_status`, which is `None` for a split tab, so a split tab has no fill, as it had no icon): Working `primary`, Blocked `warning`, Done `success`, Error `danger` (the tokens `tab_status_icon` uses; one pure function, `collapsed_fill`). Idle and none draw no fill, so the cell keeps its normal look (the rail's hover fill, or `tab.active.background` when selected). **The glyph colour is computed (owner ruling, 2026-10-22):** on a filled cell the shell glyph, and the recording dot (which is `danger`, so it would vanish on an Error fill), take whichever of the theme's `background` and `foreground` has the higher WCAG contrast against the fill (`oneterm_theme::legible_on`, reusing the contrast helper `apply_warning_legibility` already had; a tie goes to `background`). The themes' own `*_foreground` tokens are not used: measured, they were below 3:1 on 48 of 156 pairs in 23 of 39 variants (worst 1.42:1). On an unfilled cell the glyph inherits the tab's text colour, as the number did. The recording dot stays in the top-left corner.
- *Reduced motion* is moot in the cell (nothing animates); the hover card's icon is unchanged and keeps its own animation.

**Choices left open by the request.**

- *The accent bar on a filled cell.* The rail paints the 2 px selected-row bar in `selection_accent` (the theme's `table.active.border`, or `primary` where a theme has none), which can be the very colour of a Working fill. So the bar must not sit on the fill: the panel's fill is an absolute child of the cell that starts `TabRowLayout::ACCENT_BAR_WIDTH` (2 px, now a constant of `oneterm-state` shared with the rail) to the right of the cell's left edge when the tab is the selected one, leaving the bar on the ordinary `tab.active.background` it has always been read against. An unselected cell's fill reaches the left edge.
- *Hover on a filled cell.* The rail's hover fill is a child drawn behind the cell's content, and the status fill is drawn over it, so a filled cell takes **no hover fill**: the status colour is never replaced or tinted by `list.hover.background`; hover feedback on a filled cell is the hover card (and the cursor). An unfilled (idle) cell keeps the hover fill exactly as before. A selected filled cell shows its fill and the bar. The Tab's hover text colour does not reach the glyph on a filled cell because the glyph's colour is explicit.
- *Position removed.* `TabRowLayout::Collapsed` becomes a unit variant: nothing reads the position any more (the grip, the `aria` position and the divider use the rail's own index), so the field, `TAB_NUMBER_SELECTOR`, the badge inset constant and the number tests are deleted.
- *Contrast.* `scripts/check-theme-contrast.py` holds the picked glyph to `GLYPH_FLOOR` (3:1), **gating**: for each variant and each status fill (`primary.background`, `warning.background`, `success.background`, `danger.background`) the better of `background` and `foreground` against it (row `background|foreground (glyph)`). The kit gives a theme that omits `success.background` / `danger.background` its built-in palette (mode-dependent); the script models it (`KIT_DEFAULTS`). Result: 151 of 156 pairs clear 3:1; five pairs in three light variants do not, because neither of the theme's two tokens can reach it on that fill: Catppuccin Latte `primary` 2.61:1, Everforest Light `primary` 2.81:1 / `success` 2.84:1 / `danger` 2.66:1, Solarized Light `success` 2.99:1. They are listed in `KNOWN_BELOW`, printed as `KNOWN` on every run and exempt from failing, so every other pair and variant gates and a sixth failure breaks the check. Themes are not retuned: the owner ruled on 2026-10-22 (below) to keep them as they are.

**Acceptance.**

- [x] `shell_icon`: cmd, Windows PowerShell and PowerShell 7 (by kind and by program name), Git Bash (`Bash` with a `bash.exe` program), Zsh, Sh, SSH, a custom program, a tab with no recorded launch data -> the right glyph, and `kind: cmd` with `program: nu.exe` -> generic (unit tests).
- [x] A collapsed cell draws the shell glyph and no number, no title, no status badge; with Working, Blocked, Done and Error it draws the status fill, with Idle or none it draws none; the expanded row still draws its status icon (tests on a drawn frame).
- [x] `collapsed_fill` maps each state to its `primary` / `warning` / `success` / `danger` fill and picks `background` or `foreground` for the glyph (unit test against a theme: tokens only, a dark fill gets the light token, a light fill the dark one); `oneterm_theme::legible_on` is unit-tested on its own.
- [x] The accent bar of a selected filled cell is not under the fill (test: the fill starts 2 px in); an unselected one starts at the edge.
- [x] `check-theme-contrast.py` gates the picked glyph at 3:1 in every variant; five (variant, fill) pairs in three light themes still read below it and are recorded in `KNOWN_BELOW` (ratios in the Contrast choice above), not tuned.
- [x] Full `pwsh scripts/ci-local.ps1` passes; `cargo build -p oneterm-app` builds.
- [ ] On screen, by `ui-verifier`: the glyphs at 16 px in light and dark, each status fill with the glyph readable, the bar on a selected filled cell, hover on a filled and an idle cell, the hover card still showing the status icon, the recording dot on a filled cell.

**Evidence (coder).** `pwsh scripts/ci-local.ps1` (`CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=2`): all checks passed. New tests in `crates/terminal-view/src/panel/tests/collapsed_cell.rs` (`the_shell_decides_the_icon`, `the_panel_reads_the_shell_from_its_terminal`, `each_state_has_its_own_fill_and_none_for_idle`, `a_collapsed_cell_is_the_shell_icon_on_the_status_fill`, `a_selected_fill_leaves_the_accent_bar_its_own_background`, `the_expanded_row_keeps_its_status_icon`); the old position / badge tests are deleted and `the_working_icon_animates_in_every_row_layout` now covers the strip and the expanded row only. The colour of the drawn fill is not read back from a frame (gpui test windows expose bounds, not pixels): `collapsed_fill` is the unit-tested mapping and the drawn test pins that the fill exists, is full-size and starts after the bar. Debug binary: `target\debug\oneterm.exe` in this worktree.

**Owner ruling 2026-10-22 on the contrast (same rework).** The glyph colour is computed at run time (`legible_on(fill, [background, foreground])`, see Outcome and Contrast above) instead of the themes' `*_foreground`, and the contrast tier gates again. Code: `crates/theme/src/theme.rs` (`legible_on`, exported by `oneterm-theme`), `collapsed_fill` and the recording dot in `tab_title.rs`, `scripts/check-theme-contrast.py` (`PICKED_GLYPH_FILLS`, `KNOWN_BELOW`; the advisory tier is gone).

**Owner rulings 2026-10-22 on the open questions of rework 6.** (1) The PowerShell collapsed-cell icon (`crates/theme/assets/icons/powershell.svg`, a slanted panel with a `>_` chevron) is kept as drawn; the owner accepts it knowingly despite its resemblance to the PowerShell mark. (2) The five `KNOWN_BELOW` glyph / fill pairs (Catppuccin Latte `primary`; Everforest Light `primary`, `success`, `danger`; Solarized Light `success`) are kept and no theme is retuned; `scripts/check-theme-contrast.py` now says "accepted by the owner 2026-10-22" instead of "awaiting the owner" (the stale-entry check is unchanged: an entry that stops matching a below-floor row still fails the run).

**Rework after the advisor review and the UI walk of rework 6 (2026-10-22).** Advisor PASS with two Low findings and one nit (evidence `evidence/US-0198-verify.md`, "Advisor code review (rework 6)"); ui-verifier PASS for everything walkable.

- *F1 (stale exemption).* `check-theme-contrast.py` now compares `KNOWN_BELOW` with the rows that are actually below the glyph floor (`split_known`): an entry no such row matches (the pair now reads, or the variant or fill is gone) is printed as `STALE KNOWN_BELOW entry` and fails the run.
- *F2 (summary).* The closing line now reads `glyphs >= 3.0:1 except N known glyph rows below the floor` while any are exempt (`summary`), and the plain wording only when none are.
- *N4 (self-test).* The script's `--self-test` (which `ci-local` already runs through every normal run) now asserts the picked-glyph tier on a fixture (the better token wins, 21:1 on black, the floor carried, a muddy fill fails and only `KNOWN_BELOW` exempts it), the stale entry (a passing pair and a never-matched pair), the summary wording and the kit-default injection (`with_kit_defaults`: fills in what a theme omits, never overrides). The advisor's two pins (`scripts/test_check_theme_contrast.py`) passed against the fix and were folded into this self-test; the file is deleted, so there is no new gate line.
- *N2 (grip glyph on a filled cell).* The reorder grip's glyph is drawn by the rail over the cell, in `muted.foreground`, which the status fill can swallow. The cell now reports the colour it filled itself with (`TabRowLayout::report_cell_fill`, a one-shot global in `oneterm-state`, the same hand-over pattern as the layout scope, because the rail cannot read the tab's status across R4), the rail reads it once per collapsed cell (`take_cell_fill`) and `grip_glyph_color` draws the glyph in the same `legible_on(fill, [background, foreground])` pick the cell's own glyph uses; an unfilled cell keeps `muted.foreground`. Tests: `a_cells_fill_is_handed_over_once` (state), `the_grip_glyph_reads_on_a_filled_cell` (rail), and the drawn cell test now asserts the fill the rail is told.

### Rework 7 2026-10-22 (owner): collapsed strip polish, 16 px icon, the cell reorders, a placed hover card

Acceptance rework of this packet (reopened), with `US-0196` Rework 7 (the expanded rows, header and selected / hover fills, which the cells share). Measured by the design walkthrough: a 12 x 10 px icon in a 43 px cell, the 12 px grip overlay on the cell's left edge, a hover card flush to the strip with square corners and a flat background and a reserved blank second line.

**Owner choices (implemented as given).**

- **The collapsed icon is 16 px** (`size_4`, `Icon::medium()`; it was `small()`, 14 px), centred. The header is the `+` alone, centred, no label.
- **No grip in a collapsed cell**: the cell itself is what is dragged. **How the two drags are told apart** (the request asked to record it). Existing behaviour: the row's content starts `DragTerminalTab`, which only an empty Space accepts, and the 12 px grip starts the rail's `DragPanel`, which only the rail's rows and the space below them accept. The shell cannot accept `DragTerminalTab` (`pub(crate)` in the terminal crate, R4) and the terminal crate cannot accept `DragPanel` without dock internals, so one payload cannot serve both targets. In a collapsed cell **the shell icon is the content** and keeps starting `DragTerminalTab` (drag it onto an empty Space), and **the rest of the cell** (the 43 x 44 px cell outside the 16 px icon, about 87 % of it) starts the reorder `DragPanel`, dropped on another cell or below the last one. It is the same split the expanded row has (content drags to a Space, the leading edge reorders), with the cell's margin in the place of the grip. The cell's cursor is the open hand. Open for the owner: a press on the icon itself never reorders; if reorder should win there, the Space drag moves to the cell's margin instead.
- **The hover card**: 6 px from the strip's outer edge, 6 px radius, the popover background, `border` and `shadow_md` of the kit's tooltip, **one line when the tab has no directory** (no reserved blank second line), and as wide as its content up to 280 px. The card is still gpui's native tooltip (so the right-click hide rule and the show delay are unchanged), but gpui places a native tooltip at the pointer (+1 px), which is why it was only "flush" by accident, at one pointer position. The card now reads the pointer and the cell's measured bounds when it is built (gpui reads the pointer for the placement in the same call) and offsets itself with a relative inset, so its left edge is the strip's right edge (the cell's right edge plus the rail's 1 px border) plus 6 px and it is vertically centred on the cell, whatever the pointer's position in the cell (test: two pointer positions give one card position).

**Acceptance.**

- [ ] A collapsed cell's shell icon is 16 x 16 and centred in the cell (terminal-view test).
- [ ] A collapsed cell has no grip element; dragging the cell outside its icon reorders (the tab lands in the dropped cell's slot), dragging the icon starts the tab drag and reorders nothing (workspace test, probe cell modelled on the real one).
- [ ] The hover card: single line without a directory and two lines with one, content-sized up to 280 px, 6 px radius, popover background and border tokens, its left edge 6 px right of the strip whatever the pointer position (terminal-view tests).
- [ ] Full `pwsh scripts/ci-local.ps1` passes; `cargo build -p oneterm-app` builds.
- [ ] On screen, by `ui-verifier`: the 16 px icon in both themes, a drag from the cell's margin reorders, a drag from the icon lands in an empty Space, the card's gap and radius in both themes.

### Gaps

- Not pinned by a test: the tooltip text on a cell (gpui exposes no way to read a shown tooltip in a test) and the recording dot's place; both are plain code in `render_tab_strip` / `collapsed_row`.
- Not walked on screen (the coder does not give the UI verdict): the glyphs and spacing of the stacked header, the badge against the overlay scrollbar with 40 tabs, the grip overlay on hover, tooltip, restart persistence, the elevated window, light and dark themes.
- Known loss, by design here: channel chips are not shown in the strip (see Plan); the status bar and the expanded rail still show membership.
- Linux and macOS compile only in CI.

## Handoff

Implemented and gate-green; `ui-verifier` next (debug binary `target\debug\oneterm.exe` in this worktree). After acceptance tick `US-0198` in the intake. This is the last packet of `IN-0055`.
