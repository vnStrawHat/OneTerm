# Work: Vertical tab rail behind a Settings switch

ID: US-0196
Intake: IN-0055
Created: 2026-10-20

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

- Change type: new capability (UI)
- Risk lane: normal (presentation inside OneTerm's own renderer seam plus one additive optional `ui_config.json` field; no auth, secret, migration or external effect)
- Spec Intake, when required: IN-0055 (owner ruling 2026-10-20: Q1-Q9 accepted as recommended)

## Outcome

With **Vertical Tabs** on (Settings > General > Interface, default off, applies on the next frame, survives a restart) every all-terminal centre tab group draws its tabs as a fixed 220 px **rail** down its left side instead of the kit strip above its terminal: a header with `+` and zoom, one scrolling row per tab. Every per-tab behaviour of the strip works in the rail, no panel is rebuilt and no session ends when the switch flips. Turning it off brings the strip back. The `vertical_tabs: {enabled, width, collapsed}` shape is stored in `ui_config.json` (omitted while default, lenient parse, no write from an elevated window); only `enabled` has a writer and a control in this packet.

## Scope

- [x] In scope: the Q8 layout spike (absolute rail in the group frame, left **margin** on the content frame, each visible terminal resized not overlapped); `UiConfig.vertical_tabs` (all three fields stored, lenient, one writer `set_vertical_tabs_enabled`); the Settings switch; the rail in `OneTermTabGroupSkin` (rail rule: centre + non-empty + every panel a `terminal` panel); rows reusing `TerminalPanel::title` in a vertical variant; selected row fill plus 2 px accent bar; 12 px grip with `AppIcon::GripVertical` and the open-hand cursor; vertical scrolling with the active row kept in view; reorder drop slots on rows and below the last row; base `TabList` / `Tab` roles with position in set; header `+` (the panel's `title_suffix`) and the rail's own Zoom In / Zoom Out button; "Close Tabs Below" while vertical; the drag-priority gpui test; `tab.active.foreground` on `tab_bar.background` in `scripts/check-theme-contrast.py` (floor and `HIERARCHY`); `docs/gui-layout.md`, `docs/agents/persistence.md`.
- [x] Out of scope: resizable width (`US-0197`), collapse (`US-0198`), a right-side rail, an app-menu item or action for the switch, keyboard tab navigation, label-drag reorder, the `AnyDrag` host-drag handlers (no OneTerm user, HLD § Rail anatomy).

## Acceptance

- [x] Q8 spike: in a gpui test, with the switch on the group's content region starts at the rail's width and the active terminal panel is laid out at (group width - 220 px) wide; flipping the switch changes its width in both directions (resized, not overlapped); the rail is absolutely placed and takes no space in base's column. If this fails: stop and report before any fallback.
- [x] The switch flips the layout on the next frame in both directions without rebuilding a panel or ending a session (test: same panel entity ids, panel count unchanged).
- [x] Rail rule: a centre group of terminal panels draws one rail row per tab; the SSH Client group draws no rail; a group holding any non-terminal panel, and any group outside the centre, keeps what it draws today (tests).
- [x] Row behaviour in the rail: click selects (and scrolls only when selection came from elsewhere); middle-click, `x`, double-click rename, the right-click menu, status icon, channel chips and recording dot all come from the shared row element (the same `render_tab_strip`). Proven here: click selects (gpui test), the real row fills the rail width and drags only by its content (terminal-view test); the rest is the unchanged element, walked on screen by `ui-verifier`.
- [x] Drag priority pinned by a gpui test: dragging the title row starts `DragTerminalTab`, the grip starts `DragPanel`, the `x` starts neither.
- [x] Reorder: dropping a `DragPanel` on a row inserts at that slot, below the last row appends (last slot for a panel of this group); drop slot logic unit tested.
- [x] 40 tabs: the rows scroll under the header; a tab that becomes active from elsewhere scrolls into view (test on the scroll offset).
- [x] Zoom keeps the rail; its button reads Zoom Out while zoomed, Zoom In otherwise; no `...` menu, no dock toggle.
- [x] "Close Tabs Below" while vertical, "Close to the Right" in the strip (test of the label rule).
- [x] `ui_config.json`: absent -> off; `{"vertical_tabs":{"enabled":true}}` -> on; garbage (`"x"`, `5`, wrong-typed fields) loads as default with no quarantine and the rest of the document intact; default omitted from the written file; width clamped to 140..=400 on read; an older document heals; an elevated window writes nothing (the existing `write_refusal` test; the setter has no write path of its own, it only calls `persist`, which asks `write_refusal`).
- [x] `scripts/check-theme-contrast.py` passes with `tab_bar.background` added to `tab.active.foreground`'s surfaces, both the floor and the `HIERARCHY` comparison.
- [x] `docs/gui-layout.md` documents the vertical layout and qualifies the "dock buttons in the tab bar" sentence as the horizontal layout's; `docs/agents/persistence.md` row amended.
- [x] Full `pwsh scripts/ci-local.ps1` passes.
- [ ] Screenshots of both layouts in a light and a dark theme, and the on-screen walk of every row behaviour, by `ui-verifier` (not the coder's verdict).

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0055-vertical-tabs/IN-0055.md` and `high-level-design.md` — the design and the owner ruling.
- `docs/spec-intakes/IN-0055-vertical-tabs/evidence/IN-0055-spec-review.md` — advisor findings, all folded into the HLD.
- `docs/gui-layout.md` — dock composition, tab strip, zoom, lock screen.
- `docs/agents/persistence.md` — the `ui_config.json` row and the elevation rule.
- `docs/agents/crate-dependency-rules.md` (R2, R4) — the shell reaches the row only through `PanelHandle`; no new crate edge.
- `docs/spec-intakes/IN-0049-osc-7501/US-0193-tab-status-icon.md` — the status icon rides the same row; its contrast carry-forward (lightness-only theme corrections).
- `docs/spec-intakes/IN-0054-right-dock-follows-the-tab-kind/high-level-design.md` — the right dock follows the tab kind through `AppState`, not the strip.

### Documentation Action

- Update required: `docs/gui-layout.md` (new "Vertical tabs" section; qualify the dock-buttons sentence), `docs/agents/persistence.md` (`ui_config.json` row gains `vertical_tabs`), `scripts/check-theme-contrast.py` (SURFACES), this packet and the intake (owner ruling, ticks).

Reason: a new user-visible layout, one additive persisted field, one new measured surface.

### Reconciliation

Changed: `docs/gui-layout.md` (new "Vertical tabs" section; the dock-button sentence qualified; source map; the contrast paragraph names the hover pair), `docs/agents/persistence.md` (`ui_config.json` row), `docs/agents/structure.md` (tree entries), `scripts/check-theme-contrast.py` (SURFACES), the intake (owner ruling) and this packet. Unchanged and still valid: `docs/terminal-split.md`, `docs/ssh-client-connect.md`, `IN-0054`'s HLD (the right dock follows the tab kind through `AppState`; the strip's dock button is simply absent in the rail).

## Context

- Base composes every group as a column and applies `.flex().flex_col()` after the skin's frame and content frame (`tab_group.rs:691-728`), so the rail is positioned absolutely and the content frame gets a left margin (not padding, which would put the rail inside the drop-handled bounds).
- Panel code runs `title()` with no knowledge of its group, so the shell cannot tell a panel "you are in a rail" through the panel. Choice left open by the HLD (it said the row reads `UiConfig`, which would give a terminal tab dropped into the right dock a rail-shaped row inside the kit strip): a small `oneterm_state::TabRowLayout` global, set by the rail only around its own `title()` calls (`TabRowLayout::scope` returns a `Drop` guard that restores the previous layout on every exit) and read by `render_tab_strip`. Everything that is not a rail row, including the kit's drag preview, keeps the horizontal row. The menu wording is captured at that time into the context-menu closure.
- The rail needs the dock area to know whether its group is in the centre: `OneTermDockSkin` captures the `DockArea`'s weak entity at construction and hands it to every group skin.
- `DragPanelPreview` has private fields (the kit constructs it itself), so the rail draws a small preview of its own for the grip drag.

## Plan

- [x] Records commit: owner ruling in the intake, this packet.
- [x] Settings: `VerticalTabs` struct (`enabled`, `width`, `collapsed`), lenient parse, skip-while-default, `set_vertical_tabs_enabled`, `read_vertical_tabs(cx)`, tests.
- [x] Q8 spike: skeleton skin with absolute rail + margin; gpui test of widths. Stop and report on failure.
- [x] `oneterm_state::TabRowLayout`; vertical row variant in `render_tab_strip`; `Close Tabs Below`.
- [x] Rail: header, rows (base `Tabs`/`Tab`), grip, scroll, drop slots, accessibility, zoom button.
- [x] Settings > General switch; icon `grip-vertical.svg`.
- [x] Tests (rail rule, live switch keeps entities, drag priority, drop slots, scroll, label rule, settings).
- [x] Contrast script surface; docs; gate; debug binary for ui-verifier.

## Decisions

No decision record: the choices (left rail, one switch, rows drawn by the panel) live in `docs/gui-layout.md`. Plan-level choices left open by the HLD are listed under Context above.

## Verification Plan

`cargo test -p oneterm-settings`, `-p oneterm-state`, `-p oneterm-workspace`, `-p oneterm-terminal-view`, `-p oneterm-settings-ui`; `python scripts/check-theme-contrast.py`; full `pwsh scripts/ci-local.ps1`; a smoke launch of the debug build with a private profile for `ui-verifier`.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

- **Q8 spike: passed.** Test `the_rail_is_beside_the_content_and_the_terminal_is_resized_not_overlapped` (`crates/workspace/src/layout/workspace/tab_rail_tests.rs`) drives the real `OneTermTabGroupSkin` in a window: switch off, the terminal spans the group; switch on, the rail is 220 px wide at x = 0, the terminal starts at x = 220 and is exactly 220 px narrower (resized, not overlapped), the rail and the terminal have the same height; switch off again, the terminal's box is byte-for-byte the original. Option B1 (absolute rail + left margin on the content frame) is the built design; B2 was not needed.
- **Gate:** `pwsh scripts/ci-local.ps1` on Windows (`CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=2`, worktree `target`): all checks passed, 141 suites, 5063 tests passed, 0 failed, 33 ignored (the ignored-test lines and `check-ignored-tests.py` included). `check-theme-contrast.py`: 1638 pairings all >= 4.5:1 and 624 hierarchy comparisons pass with the added surface (the numbers the advisor measured on a scratch copy).
- **New tests:** `oneterm-settings` 4 (default off and not written, garbage value loads as default without quarantine, width clamp 140..=400, survives a load and a save); `oneterm-state` 1 (`TabRowLayout` scope restores); `oneterm-workspace` 15 in `tab_rail` / `tab_rail_tests` (rail rule, scroll-change, below-last slot, zoom-button rule; layout spike, setter flips live, live switch keeps every panel and removes none, stacked rows and click selects, other groups keep the strip, rows use the vertical layout only inside the rail, 40 tabs scroll and the active one comes into view, zoom keeps the rail, drag priority (title row / `x` / grip), grip drop below the last row, lone tab has no reorder); `oneterm-terminal-view` 3 (menu wording per layout, the real row fills the rail width and drags only by its content, the strip's row is unchanged).
- **Smoke launch** of `target\debug\oneterm.exe` with a private profile (scratch cwd and `USERPROFILE`, `vertical_tabs.enabled` on): ran 10 s without a panic, the config file untouched; the pid was stopped. No UI verdict here.
- **Elevation:** the existing `an_elevated_window_refuses_to_write_a_perfectly_readable_config` covers `write_refusal`, the one function `UiConfig::persist` asks; the setter has no write path of its own.

### Deviations from the HLD (all recorded here, none changes behaviour the HLD promises)

1. The vertical row is selected by a small scoped global, `oneterm_state::TabRowLayout`, set by the rail around its own `title()` calls, not by `render_tab_strip` reading `UiConfig`: the HLD's reading would have drawn a rail-shaped row inside the kit strip for a terminal tab dropped into the right dock. `UiConfig` is read only by the shell (`UiConfig::read_vertical_tabs`, which tolerates a missing global so tab-group tests need no config).
2. `vertical_tabs.width` / `collapsed` are parsed, clamped and stored now, as ruled, but nothing applies them yet: the rail is a fixed 220 px (`tab_rail::RAIL_WIDTH`). A hand-edited width is therefore inert until `US-0197`.
3. The drag-priority test is split across the two crates that own the halves (R4: the shell cannot name `DragTerminalTab`): the workspace test uses a probe panel whose title row has its own drag and an `x` that stops left mouse-down, and proves the grip starts the reorder, the title row's own drag starts and reorders nothing, and the `x` starts neither; the terminal-view test proves the real row starts `DragTerminalTab` (accepted by a drop target for that type) and its `x` starts nothing.
4. The reorder `DragPanel` is attached to the grip only (not to the whole `Tab` as the kit does), so the title row and the reorder drag never share a hitbox; the visible effect is the HLD's.
5. The rail's drag preview is its own small chip (the kit's `DragPanelPreview` has private fields).
6. No accessibility label is set on rows beyond the base `Tab`'s role, `aria_selected` and position in set: a row's name comes from its text content (the terminal panel offers no `tab_name`). Whether the platform reads it is for the `ui-verifier` walk.

### Gaps

- Not walked on screen (the coder does not give the UI verdict): live switch in the real window, light and dark screenshots, 40-tab scroll, drag into a Space, reorder by grip, lock/unlock, elevated window, restart persistence.
- Linux and macOS compile only in CI.

## Rework (ui-verifier D1-D4, advisor F1-F4)

Done on branch `verify/US-0196` on top of the advisor evidence; the UI walk is appended to `evidence/US-0196-verify.md`.

- **D1 (medium) root cause: shared code, fixed in OneTerm code.** The accent bar read `Theme::table_active_border`. `oneterm_theme::apply_list_style_override` sets that token to transparent (the data tables want no selection border) and runs after **every** theme application, so after a live theme change the token the bar reads is transparent. The horizontal strip's top bar and the active Space's cue (`terminal-view/src/space/render.rs`) read the same token and lose their colour the same way; the walk saw the strip lose its bar. Fix: `oneterm_theme::selection_accent(cx)` keeps the theme's own `table.active.border` (recorded by the override before it blanks the token, and not overwritten by a second run of the override), and the rail row, the strip's top bar and the Space cue read it. Nothing patched in gpui-component. Tests: `the_selection_accent_survives_the_list_style_override` (two themes of both modes: the accent equals the theme file's colour before the override, after it and after a second run, while the table token stays blanked).
- **D2 fixed.** The drop line sits on the edge where the tab lands (`drop_edge`): below the hovered row for a tab dragged forward within its group, above it otherwise, none on the dragged row. Tests: `the_drop_line_is_where_the_tab_lands` (every pair in groups of 1-5 against a model of base's insert rule) and `a_forward_reorder_lands_after_the_hovered_row_and_a_backward_one_before_it` (the real group).
- **D3 fixed.** The row list sits in a relative wrapper with the kit's vertical scrollbar (`ScrollableElement::vertical_scrollbar`). Rows overflowing the list reserve the scrollbar's gutter (`Scrollbar::width()`, 16 px) so the close buttons are not under it; the overflow is read from the previous frame and, when it changes, the next frame is requested with `Window::request_animation_frame` (a `refresh` inside a draw is ignored by gpui, advisor R1). Tests: the 40-tab test asserts rows clear the gutter, the 3-tab test asserts they fill the list.
- **D4 fixed.** The grip-drag preview draws `tab.active.foreground` (the pair the contrast check measures on `tab.active.background`) at 0.95 opacity, content-sized between 96 and 240 px instead of a fixed 96 px.
- **F1.** `TabRowLayout::scope` returns a `Drop` guard (`TabRowScope`, derefs to `App`) that restores the previous layout on every exit; replaces `draw`. Tests include one that unwinds a panic through it.
- **F2.** The elevated-write claim in the acceptance list no longer promises a setter-level assertion.
- **F3.** The HLD "Readers" paragraph, the accent token and the drop-line rows are amended.
- **Gate after the rework:** `pwsh scripts/ci-local.ps1` (`CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=2`): all checks passed, 141 suites, 5067 passed, 0 failed, 33 ignored; `check-theme-contrast` 1638 pairings and 624 hierarchy comparisons pass. Debug app rebuilt (`target\\debug\\oneterm.exe`). The UI fixes (D1 on a live theme switch, D3 scrollbar, D4 chip) are for `ui-verifier` to re-walk on screen.
- **F4 (info, for US-0197).** `draws_rail` is evaluated three times per group per frame (`frame`, `content_frame`, `render_tab_bar`); each upgrades the area and walks the centre tree. Negligible now; evaluate once per frame when `US-0197` adds per-frame work to it.

## Rework 2026-10-21 (owner): no close button in a vertical row, status at the trailing end

Acceptance rework of this packet (reopened, not a new BUG). Owner request, 2026-10-21 (translated from Vietnamese): "In vertical tab mode, remove the x button at the end of the tab and show the OSC 7501 status there instead."

**Outcome.** A vertical rail row (`TabRowLayout::Vertical`) draws no `tab-close` element. The OSC 7501 status icon moves from before the label to the trailing end of the row, right-aligned where the `x` was; with no status nothing is drawn there and the label keeps the full width. The horizontal strip's row is exactly as before (status icon before the label, `x` at the end). Closing a tab in the rail still works by middle-click, the right-click menu (Close, Close Others, Close Tabs Below) and the panel's keyboard paths (`ClosePanel`, Close Space on the last Space).

**Choices left open by the request.**

- Channel chips and the recording dot stay where they are (before the label): the request names only the close button and the status icon, and they fit the row as before.
- Scrollbar gutter (D3): unchanged mechanism. The list reserves `Scrollbar::width()` once the rows overflow, which was written to keep the `x` clear of the overlay scrollbar; the status icon is the right-most element now and rides the same reservation, so it stays clear. Only the comment and the test wording changed.
- The status icon keeps its tooltip and its `flex_shrink_0`; the row keeps its 4 px right padding. The icon is part of the row content, so a drag started on it is the tab drag (it is no longer a "starts neither" element; only the grip differs).

**Acceptance.**

- [x] A vertical row has no `tab-close` element (test in `oneterm-terminal-view`).
- [x] With a status, the status icon is drawn after the label in a vertical row, inside the row and right of the label's box; with none, nothing is drawn and the label spans to the row's right padding (test).
- [x] The horizontal row still has the `x` and the status icon before the label (test).
- [x] The "x starts neither drag" assertions are replaced: in the rail, the trailing status icon is row content and starts the tab drag; the grip still starts the reorder (terminal-view and workspace tests).
- [x] The D3 gutter test stands (rows clear the scrollbar gutter once they overflow), re-worded for the status icon.
- [ ] Middle-click still closes a vertical row's tab: the handler is the row's own and untouched; no gpui test drives it, so it is for the `ui-verifier` walk below.
- [ ] On screen, by `ui-verifier`: no `x` in any rail row in a light and a dark theme; the status icon at the right end for Working, Blocked, Done and Error; the label ellipsis with and without a status; the icon clear of the scrollbar with 40 tabs; middle-click and the menu close a tab; the strip (switch off) unchanged.

**Evidence.** Tests: `the_vertical_row_has_no_close_button_and_its_status_icon_trails_the_label` and `the_horizontal_row_is_content_sized_as_before` (extended: status before the label, `x` after it) and `the_vertical_row_fills_its_width_and_only_its_content_drags_the_tab` (the former `x` spot now drags the tab) in `oneterm-terminal-view`; `the_title_row_drags_into_a_space_and_only_the_grip_reorders` (the status icon stand-in drags the title row's own drag) and the 40-tab gutter test in `oneterm-workspace`. Mutation check: drawing the close button in the vertical row, or putting the status icon first, fails the new vertical test. Gate: `cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings`, `cargo test -p oneterm-terminal-view` (407 passed, 3 ignored), `-p oneterm-workspace` (90 passed, 3 ignored), `check-english`, `check-doc-paths`, `check-theme-contrast` (1638 pairings, 624 hierarchy comparisons), `verify-dependency-graph`, then the full `pwsh scripts/ci-local.ps1` (`CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=2`): all checks passed, 141 suites, 5068 passed, 0 failed, 33 ignored. Debug app rebuilt (`target\debug\oneterm.exe`). No UI verdict here.

**Documentation.** `high-level-design.md` (diagram note, Rail anatomy row, wireframe, Interactions rows for drag, close and status icon, contrast table); `IN-0055.md` (two walk sentences); `docs/gui-layout.md` "Vertical tabs" (anatomy, row description, scrollbar sentence).

**Verification.** `cargo test -p oneterm-terminal-view`, `-p oneterm-workspace`; clippy; fmt; `check-english`, `check-doc-paths`, `check-theme-contrast`; full `pwsh scripts/ci-local.ps1` (`CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=2`); debug app rebuilt for `ui-verifier`.

## Rework 2 2026-10-21 (owner): two-line rows, 64 px, current directory on line 2

Acceptance rework of this packet (reopened, not a new BUG). Owner request, 2026-10-21 (translated from Vietnamese): "The height of each tab in vertical mode is a bit small, so it looks out of place. Make each tab twice the current height. Show the information on 2 lines. Line 1 is the title. Line 2 shows the current directory aligned left (cut from the LEFT side if too long), and the OSC 7501 status icon aligned right."

**Outcome.** A vertical rail row is 64 px high (the header stays 32 px; the horizontal strip is unchanged). Line 1: the reorder grip (on hover), the channel chips, the recording dot, then the title, which ellipsizes at its end as before. Line 2: the terminal's current directory, left-aligned in muted text and cut at its **start** when too long (`…\src\panel`), then the OSC 7501 status icon right-aligned at the end of the line. Still no close button.

**Assumptions made for the owner (recorded as assumptions, to be confirmed in the walk).**

- (a) An unknown cwd (no OSC 7 yet) shows no path on line 2, only the status icon if any.
- (b) A **local** tab abbreviates the user's home directory to `~` (`~\src` on a Windows path, `~/src` on a POSIX one); an **SSH** tab shows the remote path exactly as reported.
- (c) A split tab uses the active Space's cwd (its status icon stays absent, as today).
- (d) The path updates live: `TerminalView` now emits a `CwdChanged` event on the session's OSC 7 event, and the panel, already subscribed to every leaf, repaints its row.

**Choices left open by the request.**

- *Source.* `TerminalSession::cwd()` (`crates/terminal/src/session.rs`; set by the OSC router from OSC 7 after `TerminalSecurityPolicy::sanitize_cwd` strips controls and bounds the length). Terminal-view reads it through the active Space's session, as `breadcrumb_label` / `local_cwd` do; a new `TerminalPanel::tab_cwd` returns the display text.
- *Start truncation.* gpui-pre 0.3.9 has `TextOverflow::TruncateStart` (`Styled::text_ellipsis_start`), which the text element measures against the live available width, so the path follows the rail width (including `US-0197`'s resize) with no measuring of our own. No hand-written truncation function; the path element also uses `whitespace_nowrap`, like the title.
- *Home abbreviation and separators* are a pure string function (`cwd_display`): a drive-letter path is shown with `\` for a local tab, a prefix match on the home directory is made on whole components (`C:\Users\a` is not the home of `C:\Users\ab`), case-insensitive for drive paths only. No `std::path` (code-style: Windows-shaped strings are not built with `Path`).
- *Geometry.* The row is a column: 10 px top padding, line 1 (24 px), line 2 (20 px), 10 px bottom padding = 64 px. The grip stays the row's leading edge but its glyph is centred on line 1 (shared constants in `oneterm_state::TabRowLayout`, so the shell and the panel cannot drift apart). Line 2 is `text_xs` in `muted.foreground`, a pair the contrast script already measures on both row surfaces (and the hierarchy against `tab.active.foreground`), so no new surface is added.
- *Gutter.* Unchanged: the list reserves the scrollbar's gutter once rows overflow; the status icon at the end of line 2 rides it.

**Acceptance.**

- [x] Row height is 64 px; the header 32 px (workspace test).
- [x] Title on line 1, cwd on line 2 below it (terminal-view test: label above path, same left edge).
- [x] A long cwd is cut at the start (`TextOverflow::TruncateStart` on the path element, and the drawn path is narrower than the full text); the title still ellipsizes at the end.
- [x] Status icon at the right end of line 2, level with the path; with no cwd the icon is still right-aligned; with no cwd and no status line 2 is empty.
- [x] Unknown cwd shows no path; a local home path shows `~`; an SSH path is unchanged (pure-function tests).
- [x] An OSC 7 event repaints the row (event test).
- [x] 40-tab scroll, scroll into view, gutter clearance, drag priority and the drop line still pass at 64 px (existing workspace tests).
- [x] The horizontal row is unchanged (existing test).
- [ ] On screen, by `ui-verifier`: both themes, the two lines, start truncation while the path changes and while the window narrows, `~` on a local tab, a remote path on an SSH tab, the grip aligned to line 1, the icon clear of the scrollbar with 40 tabs.

**Evidence.** Tests: `oneterm-terminal-view` `the_vertical_row_shows_the_title_then_the_directory_and_the_status`, `a_long_directory_is_cut_at_its_start_and_follows_the_width` (the path element asks for `TruncateStart`; its drawn width shrinks with 900 / 220 / 160 px slots), `the_directory_line_abbreviates_the_local_home_only` (pure function), `a_reported_directory_repaints_the_row`, `cwd_event_emits_cwd_changed`, and the reworked `the_vertical_row_has_no_close_button_and_its_status_icon_trails_the_label` (the icon now on line 2); `oneterm-workspace` `the_rail_has_one_stacked_row_per_tab_and_a_click_selects_it` (64 px rows) and `the_grip_glyph_sits_on_the_title_line_and_the_header_keeps_its_height`; `oneterm-state` `the_vertical_row_is_two_lines_between_equal_padding`. The fake session got `FakeSessionProbe::set_cwd` for these. Gate in the worktree: `cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings`, `cargo test -p oneterm-terminal-view` (412 passed), `-p oneterm-state` (76), `-p oneterm-workspace tab_rail` (18), `check-doc-paths`, `check-english`. The full `pwsh scripts/ci-local.ps1` was run once after `US-0197` (owner request): all checks passed, 5084 tests passed, 0 failed, 33 ignored (`US-0197` packet, Evidence). Debug app rebuilt (`target\debug\oneterm.exe`). No UI verdict here.

**Documentation.** `high-level-design.md` (Rail anatomy rows, row content, wireframe, Interactions rows), `IN-0055.md` (walk sentence), `docs/gui-layout.md` "Vertical tabs".

**Verification.** `cargo test -p oneterm-terminal-view`, `-p oneterm-workspace`, `-p oneterm-state`; clippy; fmt; the doc and contrast scripts; the full `pwsh scripts/ci-local.ps1` once after `US-0197` (owner request: one full gate at the end of both parts).

## Rework 3 2026-10-21 (owner): the tab's directory is the status bar's, and a divider between rows

Acceptance rework of this packet (reopened, not a new BUG). Owner request, 2026-10-21 (translated from Vietnamese): (1) "When I open a new cmd terminal, the cwd on the tab shows `~` while the status bar shows the full path. Make the tab's cwd the same as the status bar's." (2) "Add a bottom border to separate the tabs more clearly."

**Outcome.** Line 2 of a vertical row shows exactly the string the status bar's breadcrumb shows for that tab: `TerminalPanel::breadcrumb_label` (the session's OSC 7 working directory, `PathBuf::display`). There is no `~` substitution, no separator rewriting and no local/SSH difference any more; `cwd_display` and `is_drive_path` are deleted. The start-cut (`TextOverflow::TruncateStart`) stays. Each vertical row has a 1 px bottom border in the theme's `border` token (the colour the rail's own header divider and right edge already use), inside the row's 64 px (the rail does not grow); the horizontal strip is unchanged.

**Choices left open by the request.**

- *Where the one formatter lives.* The status bar reaches the label through `oneterm_state::active_terminal::breadcrumb`, whose provider (`terminal-view/src/status.rs`) calls `TerminalPanel::breadcrumb_label`. The tab row is drawn by the same `TerminalPanel` in the same crate, so no code moves: `tab_cwd` is now a one-line call to `breadcrumb_label`. The status bar's width-driven `PathTail` shortening is presentation of the same string and stays the bar's.
- *Row budget (advisor A2).* The row's lines and padding summed to 64 px inside a 63 px content box. The bottom padding is now 9 px (`TabRowLayout::VERTICAL_ROW_DIVIDER` is carved out of `VERTICAL_ROW_PADDING`), so 10 + 24 + 20 + 9 + 1 = 64 and nothing is squeezed.
- *Divider width (UI defect U2).* The scrollbar's gutter was padding on the list, so every row, and its divider, stopped short of the rail's edge once the rows overflowed. The gutter is now padding inside each row, so the row and its divider span the whole rail.
- *The border and the drop line.* The row's border is the row's own `border_b_1` (taffy sizes are border-box, so the row stays 64 px). While a `DragPanel` hovers a row, the drop line (2 px, `drag_border`, above or below) replaces it on that edge; "above" also drops the row's own divider so the single border colour is not mistaken for a second drop line.

**Acceptance.**

- [x] Line 2 equals the status bar's text for a local drive path, a path in the user's home and an SSH path (unit tests compare the tab's string with `breadcrumb_label` and with the path the session reports; none contains `~`).
- [x] `cwd_display` and its test are gone; a long path is still cut at its start (existing test).
- [x] Each vertical row draws a 1 px bottom border in `theme.border`; the row is still 64 px and the header 32 px (workspace test).
- [x] The horizontal row is unchanged (existing test).
- [ ] On screen, by `ui-verifier`: a new `cmd` tab shows the same path in the tab and in the status bar; the divider in a light and a dark theme; the drop line still reads clearly on a hovered row.

**Documentation.** `docs/gui-layout.md` "Vertical tabs" (row description, anatomy), `high-level-design.md` (Rail anatomy row content, wireframe note, Interactions), this packet.

**Evidence.** Tests: `oneterm-terminal-view` `the_rows_directory_is_the_status_bars` (a local drive path, a path under the home folder, the home folder itself and an SSH path: the row's string equals `breadcrumb_label` equals the path the session reported, and none starts with `~`; it replaces `the_directory_line_abbreviates_the_local_home_only`); `a_long_directory_is_cut_at_its_start_and_follows_the_width` unchanged and green; `oneterm-workspace` `the_rail_has_one_stacked_row_per_tab_and_a_click_selects_it` now also asserts the row's content box (the grip column) is 63 px inside the 64 px row, which only the 1 px border explains. The border's colour (`theme.border`) is code-reviewed, not pixel-tested. Gate: `pwsh scripts/ci-local.ps1` on Windows (`CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=2`, worktree `target`): all checks passed (`cargo test --workspace`: 2509 passed, 0 failed, 19 ignored in that step; the three `--ignored --test-threads=1` lines, `check-ignored-tests.py`, the `oneterm-vt` steps, `verify-dependency-graph.py`, `check-doc-paths.py`, `check-english.py`, `check-theme-contrast.py` (1638 pairings, 624 hierarchy comparisons) and the notices check included). `bash scripts/ci-local.sh` was tried first and stops at the `oneterm-vt` six-dependency check on this machine only because Windows' `sort.exe` shadows GNU `sort` in that shell; the PowerShell script is the same set of checks.

## Rework 4 2026-10-21 (owner): no zoom button, `+` on the right, no scrollbar; the OSC 7501 icon

Acceptance rework of this packet (reopened, not a new BUG). Owner requests, 2026-10-21 (translated from Vietnamese): (1) "Remove zoom in/out from the vertical tab rail; it has no effect." (2) "I no longer see the OSC 7501 status icon animating like before. Keep it static." (4) "Move the + button to the right side (currently left)." (5) "Hide the scrollbar in vertical tabs." The collapse button (request 3) moved to the status bar; see the `US-0198` Rework 4 entry.

**Outcome.**

- *Zoom button gone.* The rail header (expanded and collapsed) has no Zoom In / Zoom Out button; `ZoomButton`, `zoom_button` and the `ToggleZoom` tooltip wiring are deleted. The horizontal strip keeps its kit zoom button. Zoom itself is untouched and still reachable by its key binding (`ToggleZoom`), so a zoomed group (restored from `docks.json`) is not stranded (owner ruling 2026-10-22: no "Zoom Out" item is added to the tab context menu either; zoom stays key-only (`ToggleZoom`) in the vertical rail); the rail keeps drawing in a zoomed group as before.
- *`+` on the right.* The header is one 32 px line with the `+` (the active panel's `title_suffix`, the New Terminal menu) right-aligned. Collapsed, the header holds only `+`, centred in the 44 px strip, at the same 32 px height, so the cells below start at 32 px in both states. `COLLAPSED_HEADER_HEIGHT` (the three-button stacking floor) is deleted.
- *No scrollbar in the rail.* The expanded rail draws no scrollbar either (the collapsed strip already had none). The list still scrolls by wheel or touchpad, and the displayed tab is still scrolled into view when it changes and when the rail collapses or expands. The per-row scrollbar gutter (`RowPlace::gutter`, `pr(Scrollbar::width())`), the `overflowing` flag and the `request_animation_frame` that tracked it are deleted, so row content uses the full width up to the row's own right padding. Why: owner request.

**Item 2, the OSC 7501 icon: investigated, behaviour NOT changed.** The owner's sentence ("no longer see ... animating like before. Keep it static") can be read either way, so no behaviour was touched; the evidence is here for the coordinator to put to the owner.

- The animation code (`tab_status_icon`, `crates/terminal-view/src/panel/tab_title.rs`) is what `701a4b16` (2026-10-15, US-0193) introduced: a repeating 1 s `with_animation` rotation for `Working` and a 1.2 s opacity pulse for `Blocked`. No later commit touched it (`git log -S"with_animation"` finds only `701a4b16`), and IN-0055 did not move it: the strip, the vertical row and the collapsed cell all call the same function.
- New test `the_working_icon_animates_in_every_row_layout` proves the icon asks for the next frame in `Horizontal`, `Vertical` and `Collapsed` rows alike (it passes), so the rail did not lose the animation.
- What stops it is gpui's reduced-motion flag. `gpui-base` (`reduce_motion.rs`, in the dependency since GPUI Kit 0.7.0, 2026-09-28) reads the Windows **Animation effects** switch (`SPI_GETCLIENTAREAANIMATION`) at start-up and sets `App::reduce_motion`; `AnimationElement::request_layout` (`gpui-pre` `elements/animation.rs:407`) then draws one static frame and asks for no more. On this machine that switch reads **off** (checked with `SystemParametersInfo`), and the US-0193 walk already recorded "the icon was static on screen ... Windows animation effects off". So the icon is static here in every layout and animates when the Windows switch is on (remote-desktop sessions also turn animations off).
- The two readings: if the owner wants the animation back, turn Windows Settings > Accessibility > Visual effects > Animation effects on (no code change; the app follows the system), or decide OneTerm should ignore the system switch for this icon (a deliberate departure from the accessibility default). If the owner wants it to stay static, it already is on this machine, and a setting to force it would be new scope.

**Owner ruling on item 2, 2026-10-21:** keep it static. OneTerm keeps following the Windows Animation effects switch (gpui reduced motion); no code change. To see the icon move, turn Animation effects on and restart OneTerm.

**Choices left open by the requests.**

- *Header height collapsed.* Fixed at the expanded header's 32 px (`HEADER_HEIGHT`), not the old stacked floor.
- *Zoom keeps its action.* Only the rail's button went; `ToggleZoom` and the horizontal strip's button stay, because the request is about the rail's control and zoom state is persisted.
- *Scrollbar and the 40-tab test.* Without a gutter the 40-tab test now asserts the opposite of before: row content may reach the row's right padding, and the rows are as wide as the list.

**Acceptance.**

- [x] The header (expanded and collapsed) has no zoom button; the horizontal strip is unchanged (workspace test: no `zoom-in` / `zoom-out` in the rail, also while the group is zoomed).
- [x] Expanded, the `+` is the right-most thing in the 32 px header; collapsed it is centred in the 44 px strip and the first cell starts at 32 px.
- [x] With 40 tabs the rail draws no scrollbar, the rows are as wide as the list, the wheel scrolls, and the displayed tab is revealed.
- [x] The working status icon asks for frames in the strip, the vertical row and the collapsed cell (terminal-view test).
- [ ] On screen, by `ui-verifier`: the header in both states and two themes, no scrollbar with 40 tabs, the status icon clear of the row's edge.

**Evidence.** Tests: `oneterm-workspace` `the_header_holds_only_the_plus_on_the_right_and_zoom_keeps_it` (replaces the zoom-button test: header 32 px, `+` right-aligned, no other control, unchanged while zoomed), `the_setter_collapses_the_rail_to_a_strip_and_expands_it` (header 32 px with the `+` centred when collapsed; the cells start at 32 px), `forty_tabs_scroll_and_the_active_one_is_brought_into_view` (no gutter: row content reaches into the old 16 px), `a_click_across_an_expanded_row_of_an_overflowing_rail_selects_it` (after a wheel scroll a press 5, 8, 15 and 16 px from the right edge selects its tab and the list does not move; the scrollbar track would have taken the last three); `oneterm-terminal-view` `the_working_icon_animates_in_every_row_layout`. `tab_rail::zoom_button` and its unit test are deleted with the code. Gate: `pwsh scripts/ci-local.ps1` (`CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=2`, worktree `target`): all checks passed.

**Documentation.** `docs/gui-layout.md` "Vertical tabs" (anatomy, header, scrolling, collapsing) and the Status bar section, `high-level-design.md` (wireframes, rail anatomy, zoom row, collapse step), this entry.

## Rework 5 2026-10-21 (owner): a hover background on every rail row

Acceptance rework of this packet (reopened, not a new BUG). Owner request, 2026-10-21 (translated from Vietnamese): "When hovering a vertical tab, I want that tab's background to change colour as a highlight. Today only the title text changes." The other rework 5 requests (status bar separator, the collapsed cell's hover card) are in the `US-0198` Rework 5 entry.

**Root cause.** The row's own `.hover(text_color)` (`tab_rail.rs`) set only the text colour; nothing ever gave the row a hover background. (The row is `gpui_base::Tab`, whose `render`, `reference/gpui-kit/crates/base/src/tabs.rs:150-182`, registers no hover style of its own. An earlier version of this record said the kit's `Tab` replaced the row's hover style; that was wrong, and a second `hover` on one element would trip gpui's debug assertion, `div.rs:842-848`.) A second `hover` therefore cannot carry the fill, which is why it is a child that follows the row's hover group.

**Outcome.** Every unselected row of the rail, expanded or collapsed, gets the theme's list hover fill (`theme.list_hover`, the colour the session tree and SFTP rows use for hover) across the whole row, behind the content. It is a first child of the row, an `absolute` fill that is transparent until the row's hover group is hovered (`group_hover`), so it needs no second `hover` on the row. The selected row keeps its active background and accent bar (no hover fill is added to it, so hovering it changes nothing); the 1 px divider is the row's own bottom border and the fill sits inside it, so the divider stays. The grip, number, badges and text are drawn after the fill.

**Contrast.** The fill is translucent over the rail's `tab_bar.background`, and the row's text is `tab.active.foreground` (the kit's hover text colour) and `muted.foreground` (directory, grip). The OSC 7501 bell (`warning`) is a non-text glyph. At 4.5:1 on the hover fill it fails in 11 built-in variants (worst 4.0:1), and a weaker fill still failed 2. **Owner ruling, 2026-10-21 (F2):** the bell is measured as a non-text glyph at WCAG 1.4.11's 3:1 on **every** surface it is drawn on, one rule and not a per-surface exception: `check-theme-contrast.py` has a `GLYPH_SURFACES` tier (`GLYPH_FLOOR = 3.0`) with `warning.background` on `tab_bar.background`, `tab.background`, `tab.active.background` and `tab_bar.hover.background`. Its text uses (title bar suffix, notes, notifications) stay at 4.5:1 in `SURFACES`. `--report` and the failure summary name both floors, and the self-test drives `rows()` with a bell fixture (about 3.4:1 passes its glyph row, about 2.4:1 fails it) to pin the tier itself. AGENTS.md does not enumerate the bell and is unchanged. `scripts/check-theme-contrast.py` gets a new surface, `tab_bar.hover.background` (= `list.hover.background` composited over `tab_bar.background`), listed for `tab.active.foreground` and `muted.foreground`; the hierarchy rule therefore also compares `tab.active.foreground` against `muted.foreground` on it.

**Acceptance.**

- [x] An unselected row (expanded and collapsed) has the hover fill element; the selected row does not (workspace test).
- [x] The contrast check measures the hover surface in every built-in theme and passes.
- [ ] On screen, by `ui-verifier`: hovering a row in two themes, the active row unchanged, the divider and accent bar visible, collapsed cells likewise.

## Rework 7 2026-10-22 (owner): visual polish of the expanded rail

Acceptance rework of this packet (reopened, not a new BUG), together with `US-0198` Rework 7 (the collapsed strip). A design walkthrough of the running build measured, in Zed One Dark: rail `#1E2227`, terminal `#23272E`, the selected row filled with the terminal's own colour (so it read as the page, not as a row), the hover fill `#2C313C` *lighter* than the selected one, an idle title in a muted tone, a 2 px accent bar, rows of 64 px with 36 px of nothing when the tab has no directory, a 1 px divider under every row (the selected row boxed by two), the reorder grip at x 3..9 touching the title at x 12, a header that is a lone `+`. In Zed One Light the hovered row (`#E5E5E6`) was darker than the selected one (`#FAFAFA`) and the idle title blue-grey. Screenshots: the scratchpad `vt-walk\` (`vtw-04-15tabs.png`, `vtw-09-zoom-rows.png`, `vtw-20-light-expanded.png`; `vtw-40-mock-proposal.png` is a hand-drawn target, whose shell icon in every row and close `x` the owner did **not** choose).

**Owner choices (implemented as given).**

1. **Rows are 48 px**: 6 px top and bottom padding, line 1 = 20 px (the title), line 2 = 16 px (the directory, `text_xs`, `muted.foreground`). A tab with **no directory** has no blank second line: line 1 is vertically centred in the 48 px. The status icon stays at the right end of line 2 when there is a directory and sits at the right end of the centred line 1 when there is none. The row is padded 14 px on the left and 10 px on the right (the title no longer starts under the grip).
2. **The selected row stands out**: selected > hover > idle in both themes. Selected fill = `Theme::list_hover`; hover fill = `Theme::list_hover` at 50 % alpha over the rail; a selected row takes no hover fill. The accent is a **3 px rounded pill inset 8 px top and bottom** on the left edge, in `selection_accent`. No divider or border boxes the selected row.
3. **No row dividers.** The 1 px bottom border and `TabRowLayout::VERTICAL_ROW_DIVIDER` are deleted; only the rail's own right border remains (a drop line still draws on the edge a dragged tab will land). **The grip is a hover-only overlay** at the far left (an 8 px wide absolute strip, its glyph ink at about x 2..6): it reserves no column and the title starts at 14 px.
4. **The header has a label**: "Terminals" on the left (`text_xs`, `muted.foreground`, not upper-cased: no other header in the chrome is), the `+` on the right as a 24 x 24 ghost button (the kit's `Size::Small`; the terminal panel's `title_suffix` takes it while a rail draws it, the strip keeps `xsmall`), and **no bottom border**. The collapsed strip's header is the `+` alone, centred.
5. and 6. are the collapsed strip's: `US-0198` Rework 7.

**Choices the request left open.**

- *Which token is the selected fill.* `tab.active.background` equals the terminal's `background` in both Zed One themes, so it cannot make the row stand out from the page. `list.active.background` is the kit's "selected list item" token, but OneTerm shows selection as hover everywhere (`apply_list_style_override`: the session tree and the SFTP rows), and `list_hover` is the owner's choice. `Theme::list_active` is the kit's token for the same thing, but the kit clamps its own token to 0.2 alpha and OneTerm's override wrote only the colours, not the kit's `tokens` (`BUG-0088`, fixed with this rework); the first build, which read `list_active`, showed the selected row at `#21252b` in a screenshot, exactly a fifth of the way from the rail to `#2c313c` (the cause was not isolated: the advisor shows `list_active` equals `list_hover` after every theme path, so a reset after the override is **not** the explanation, and the sample is kept only as an observation). The rail reads the colour it means, `list_hover`, which in both Zed One themes is `#2c313c` (dark) and `#e5e5e6` (light): visibly different from the rail (`#1e2227`, `#f0f0f1`) and from the terminal (`#23272e`, `#fafafa`), and takes hover as `list_hover` at **half alpha**, which by construction lies between the rail and the selected fill in every theme: selected > hover > idle holds for any theme, not only these two. A test applies both Zed One themes and checks the order against the rail.
- *Contrast.* `scripts/check-theme-contrast.py` gets a `tab_bar.selected.background` surface (`list.hover.background` over `tab_bar.background`, the selected-item colour) for `tab.active.foreground`, `muted.foreground` and the bell glyph, and `tab_bar.hover.background` is now the hover colour **at 50 %** over the bar. Results are in Evidence.
- *Idle title colour.* `tab.foreground` is kept. Zed One Light has no neutral muted token: `muted.foreground`, `tab.foreground` and the sidebar's text are all `#5d677a` (blue-grey), and `foreground` `#383a42` is the selected title's colour. A computed grey would be a colour the theme did not choose, and retuning a theme is not this packet's to do (owner rule), so the idle title stays the theme's own `tab.foreground`.
- *The grip glyph is centred on the whole 48 px row*, not on line 1 (the row has one line when there is no directory, and the overlay is a strip the full height of the row).
- *Dead code removed*: `TabRowLayout::VERTICAL_ROW_DIVIDER`, `vertical_first_line_centre`, `report_cell_fill` / `take_cell_fill` (they told the rail what colour the collapsed cell filled itself with, only to colour the grip glyph on it; the collapsed cell has no grip any more) and `grip_glyph_color`.

**Acceptance.**

- [x] Rows are 48 px; with a directory the lines are 20 + 16 px between 6 px of padding; without one, line 1 is centred in the row (tests, `oneterm-state` and `oneterm-terminal-view`, on a drawn frame).
- [x] The status icon is at line 2's right end with a directory and at line 1's right end without one (test).
- [x] No divider: the title element is as tall as the row, rows touch (workspace test); `VERTICAL_ROW_DIVIDER` is gone.
- [x] The selected fill is `list_hover`, the hover fill `list_hover` at 50 %, and in Zed One Dark and Zed One Light selected is stronger than hover, which is stronger than the rail (workspace test with both themes applied); the selected row draws no hover fill.
- [x] The accent is a 3 px pill inset 8 px top and bottom (workspace test on the drawn bounds).
- [x] The grip is an overlay: the title starts at the row's left padding and not after a grip column; the reorder drag still starts from it (workspace tests).
- [x] The header has the "Terminals" label on the left, the `+` on the right, no bottom border, and the collapsed header is the `+` alone (workspace tests); the `+` is 24 px in a rail and 20 px in the strip (terminal-view unit test of the size rule).
- [x] `scripts/check-theme-contrast.py` measures the new selected and hover surfaces; results and any failure reported, no theme retuned.
- [x] Full `pwsh scripts/ci-local.ps1` passes; `cargo build -p oneterm-app` builds.
- [ ] On screen, by `ui-verifier`: rows, selected / hover / idle in a dark and a light theme, the pill, the grip overlay on hover, the header, with and without a directory.

**Evidence (coder).** `pwsh scripts/ci-local.ps1` (`CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=2`): all checks passed (`cargo test --workspace`: 5125 passed, 0 failed, 33 ignored across the gate's steps), then, after the token fix below, `cargo fmt --check`, `cargo clippy -p oneterm-workspace -p oneterm-terminal-view -p oneterm-state --all-targets -- -D warnings`, `cargo test -p oneterm-workspace --lib` (114 passed, 3 ignored) and the four Python checks again. `cargo build -p oneterm-app` builds (`target\debug\oneterm.exe` in the worktree).

- Tests: `oneterm-state` `the_vertical_row_is_two_lines_between_equal_padding` (48 = 6 + 20 + 16 + 6; the cell-fill hand-over test is deleted with its code); `oneterm-terminal-view` `the_vertical_row_shows_the_title_then_the_directory_and_the_status` (48 px, the title centred without a directory, line 1 / line 2 centres with one, the 14 px left padding, the status at line 1's end without a directory), `the_vertical_row_has_no_close_button_and_its_status_icon_trails_the_label`, `the_new_terminal_button_is_larger_in_the_rail`; `oneterm-workspace` `the_rail_has_one_stacked_row_per_tab_and_a_click_selects_it` (48 px), `the_grip_overlays_the_row_and_no_divider_cuts_it` (title as wide and tall as the row, 8 px grip at the left edge, glyph centred on the row), `the_selected_row_has_an_inset_accent_pill`, `an_unselected_row_has_a_hover_fill_over_the_whole_row` (the fill is the whole row, none on the selected one), `selected_outranks_hover_outranks_idle_in_both_zed_themes`, `the_header_holds_the_label_and_the_plus_and_zoom_keeps_it`, and the collapse test now asserts the collapsed header has no label.
- **Observation from the coder's own screenshot.** The first build read `Theme::list_active` for the selected fill; the running build painted the selected row at `#21252b` (sampled), not `#2c313c`, a fifth of the strength, so the hovered row outranked it. The stated cause (a theme change resets `list_active` after the override) did not survive the advisor's review (`evidence/US-0196-verify.md`) and is withdrawn; the cause was not isolated. The rail reads `list_hover` (see the choice above), and the override's missing reach into the kit's tokens is fixed as `BUG-0088`.
- Contrast: `scripts/check-theme-contrast.py` passes (2028 pairings, 702 hierarchy comparisons, the five accepted `KNOWN_BELOW` glyph rows unchanged, nothing new failing, no theme retuned). Zed One Dark: `tab.active.foreground` on the selected surface 13.03:1, the bell glyph 7.54:1; idle `tab.foreground` 5.38:1. Zed One Light: 9.01:1, 4.18:1 (glyph floor 3:1), idle 5.00:1. The selected fill is `#2c313c` / `#e5e5e6`, the hover fill `#252a32` / `#eaeaeb` over the rail (sampled from the running build: `#252a32` in dark).
- Screenshots (coder's, no verdict): `C:\Users\trunglt\AppData\Local\Temp\claude\D--TrungKFC-Research-Rust-myTerm2\fa2d0135-9c28-4a69-a255-ba7479e604cb\scratchpad\vt-rework7\` (`r7-dark-expanded-hover.png`, `r7-light-expanded-hover.png`, `r7-dark-collapsed-card.png`, `r7-light-collapsed-card.png`, `...-collapsed-hover.png`).

### Rework 7 after the advisor review and the UI walk (2026-10-22)

Advisor PASS with findings (`evidence/US-0196-verify.md`, "Advisor code review (rework 7)"), ui-verifier PASS (same file).

- **Finding 1 (records).** The claim that a theme change resets `list_active` after the override does not reproduce; reworded in this packet, the HLD, `docs/gui-layout.md` and the code comment on `row_fills`: `list_hover` is the owner's choice, and the kit paints from its own tokens, which the override did not reach.
- **Finding 2 (pre-existing bug).** `apply_list_style_override` wrote only `Theme`'s colours, so the kit's `tokens.table_active` / `tokens.list_active` kept their 0.2-alpha derivation and the SFTP tables drew the kit's selection overlay over their own highlight. Fixed as `BUG-0088` (`IN-0042`): the override's writes now run inside `Theme::update`, after `record_selection_accent`. The advisor's pin passes.
- **Finding 3.** See `US-0198` Rework 7 after the review: a selected filled cell no longer shows notches.
- Records: `docs/gui-layout.md` no longer says "2 px accent bar" for a collapsed cell.

## Rework 8 2026-10-22 (owner): five polish items from the rework 7 UI walk

Acceptance rework of this packet (reopened, not a new BUG). Owner ruling, 2026-10-22: do the five polish items the ui-verifier recorded under "Defects" of `evidence/US-0196-verify.md` ("UI verification (rework 7)"). The collapsed strip's drag split stays as it is (the icon drags the tab into a Space, the rest of the cell reorders): not touched.

**Outcome.**

1. **Light theme: selected, hover and idle are clearly three steps.** In Zed One Light the fills were `#f0f0f1` (rail) / `#eaeaeb` (hover) / `#e5e5e6` (selected), 5 to 6 levels apart. Target strength: about `#e6e6e8` hover and `#dcdde1` selected.
2. **Dark theme: hover is stronger.** In Zed One Dark the fills were `#1e2227` / `#252a32` / `#2c313c`; hover is stronger (10 levels over the rail instead of 7; see the choices for why it is 5 % and not 62 % of the selected fill).
3. **A hovered title is one step dimmer than the selected one.** Idle is `tab.foreground`, selected is `tab.active.foreground`, hover is the theme-derived midpoint of the two (it was the selected colour).
4. **A right-clicked row keeps its hover highlight while its context menu is open** (fill and title colour), until the menu is dismissed.
5. **A 1 px line under the "Terminals" header, only while the list is scrolled** (scroll offset above 0), in the `border` token. It is an overlay, so the header does not change height.

**Choices the request left open (all recorded here).**

- *Fills from the foreground, not from `list_hover`.* Across the 39 built-in variants the rework 7 rule (selected = `list_hover`, hover = `list_hover` at 50 %) puts the selected fill at a WCAG contrast of 1.00 to 1.33 against the rail (Aurora Light and Gruvbox Light: exactly the rail; Catppuccin Latte 1.01, Solarized Light 1.03): the weakness is the themes' `list.hover.background`, which several themes set barely off the list background, not only Zed One Light. The rail's fills are therefore the theme's `foreground` at a fixed alpha over the rail (`tab_bar.background`): **selected = 10 %, hover = 5 %**. Zed One Light: rail 240, hover 231 (`#e7e7e8`), selected 222 (`#dedee0`) against the targets `#e6e6e8` / `#dcdde1`. Zed One Dark: rail 30, hover 40 (`#282c31`, was `#252a32`), selected 51 (`#33373b`, was `#2c313c`; the selected row got lighter by 7 levels, a side effect of one formula for both families). Every variant keeps selected stronger than hover stronger than the rail (test over the whole theme registry). The session tree and the SFTP rows keep `list_hover` for selection (not this packet's).
- *Why hover is 5 % and not 62 % of selected (the owner's "60-65 %").* `muted.foreground`, the directory line, is only about 5:1 on the rail in Aurora Light, macOS Classic Light and Solarized Dark; a hover fill above about 5 % drops it under the 4.5:1 floor there (measured: at 6.2 % it was 4.39:1, 4.39:1 and 4.42:1), and no theme may be retuned here. So the hover alpha is capped at 5 % and the selected fill is the rest of the range; hover is half of selected, but in absolute terms it is stronger than before in dark (10 levels over the rail, was 7) and in light (9, was 5). If the owner wants hover at 62 % of selected, the selected alpha would have to drop to 8 % and the light steps shrink again.
- *Secondary text on the selected row.* `muted.foreground` on the 10 % selected fill is under 4.5:1 in eight variants (Zed One Light 4.23:1). The selected row's directory line and grip are `muted.foreground` halfway to `tab.active.foreground` (`oneterm_theme::selected_row_secondary`), at 4.69:1 or more in every variant and still under the title; the terminal panel's row applies it to the active tab's directory line. The script's `tab_bar.selected.foreground` token measures it and `HIERARCHY` keeps it under `tab.active.foreground`.
- *Hover title colour.* `tab.foreground` blended 50 % towards `tab.active.foreground` (`tab_foreground.blend(tab_active_foreground.opacity(0.5))`). Both ends clear 4.5:1 on the hover fill, so the midpoint does; `scripts/check-theme-contrast.py` gets a `tab_bar.hover.foreground` token (that blend) measured on `tab_bar.hover.background` so a theme cannot break it silently. The hover text style is not applied to the selected row (it would dim the selected title under the pointer).
- *How the rail learns a menu is open.* The kit's `ContextMenu` keeps its open state in private element state and the menu is built by the terminal panel's title (`tab_context_menu`), not by the rail. `oneterm-state` gets a small global, `TabMenuOpen` (the panel whose tab menu is open), beside `TabRowLayout`, which already carries row state between the rail and the panel; the menu builder sets it when the menu is built and clears it on the menu's `DismissEvent` (Escape, an outside click, a chosen item). The rail reads it per row. The drawing rule is one pure function over a row state (idle / hover / menu open / selected) that returns the fill and title colour, so the tests pin it without a pointer.
- *The header line is an overlay, not a border.* A bottom border would shrink the 32 px header's content box by 1 px and nudge the label and the `+`; a 1 px absolute child at the bottom edge moves nothing.
- *The hover state of the collapsed strip's cells* is the same code (`TabRail::row`), so they take items 1 to 4 too.

**Acceptance.**

- [x] The row fills come from one function: selected = `foreground` at 10 %, hover = `foreground` at 5 %, idle = none; for every embedded theme the order idle < hover < selected holds against the rail (workspace test over the whole theme registry), and in Zed One Dark / Zed One Light the three steps are measurably apart (test with thresholds in levels).
- [x] The hover title colour lies between the idle and the selected title colour in every embedded theme, and is a step of its own where the theme draws those two apart (workspace test); the hover colour is not applied to the selected row.
- [x] A row whose context menu is open draws the hover fill and the hover title colour (pure-function test of the row state; `TabMenuOpen` set by the menu builder and cleared on dismiss: terminal-view test with a real menu).
- [x] The header has the 1 px `border` line when the list is scrolled and none at offset 0 (pure-function test, and a drawn-frame workspace test with 40 tabs).
- [x] `scripts/check-theme-contrast.py` measures the new fills (`tab_bar.hover.background`, `tab_bar.selected.background`) and the new hover title colour in every variant and passes; no theme retuned.
- [ ] Full `pwsh scripts/ci-local.ps1` passes.
- [ ] On screen, by `ui-verifier`: the three steps in a dark and a light theme, the hovered vs selected title, a right-clicked row with its menu open (and after Escape), the header line appearing on scroll and vanishing at the top.

**Evidence (coder, rework 8).** `pwsh scripts/ci-local.ps1` (`CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=2`): all checks passed. Per crate after the last edit: `oneterm-workspace --lib` 120 passed, 3 ignored; `oneterm-terminal-view --lib` 438 passed, 3 ignored; `oneterm-theme --lib` 10 passed; `oneterm-state --lib` 78 passed. `cargo build -p oneterm-app` builds.

- New tests: `oneterm-state` `the_newest_menu_wins_and_a_stale_close_is_ignored`; `oneterm-terminal-view` `a_tab_menu_records_that_it_is_open_until_it_is_dismissed` (a real right-click opens the menu and records the panel; Escape and a click outside clear it; the recorded id is the panel handle's `panel_id`, which the rail compares); `oneterm-workspace` `selected_outranks_hover_outranks_idle_in_every_embedded_theme` (all 39 variants), `the_zed_one_themes_have_the_steps_the_owner_asked_for`, `a_hovered_title_sits_between_the_idle_and_the_selected_title`, `a_row_with_its_menu_open_looks_hovered_until_it_closes`, `the_header_rule_shows_only_while_the_list_is_scrolled`, and the drawn-frame `the_header_has_a_rule_only_while_the_list_is_scrolled` (40 tabs: no rule at the top, a 1 px rule on the header's bottom edge when the last tab is shown, the header and the `+` unmoved, gone again at the top). `selected_outranks_hover_outranks_idle_in_both_zed_themes` is replaced by the all-themes test.
- Zed One Light, levels of the red channel over the rail: rail 240, hover 231, selected 222 (`#f0f0f1` / `#e7e7e8` / `#dedee0`); Zed One Dark: 30, 40, 51 (`#1e2227` / `#282c31` / `#33373b`). Hover titles: Zed One Dark `#8f96a3` (idle) -> `#c7cbd1` -> `#ffffff` (selected); Zed One Light `#5d677a` -> `#4a505e` -> `#383a42`.
- Contrast: `scripts/check-theme-contrast.py` passes (2067 pairings, the five accepted `KNOWN_BELOW` glyph rows unchanged, no theme retuned). New surfaces and tokens: `tab_bar.hover.background` (`foreground` at 5 %), `tab_bar.selected.background` (`foreground` at 10 %), `tab_bar.hover.foreground` (hover title) and `tab_bar.selected.foreground` (selected secondary text), with `tab.active.foreground` compared against the latter in `HIERARCHY`. First attempt at 6.2 % hover and plain `muted.foreground` on the selected fill failed 8 and 3 variants respectively, which is why hover is 5 % and the selected secondary text is blended.

**Review of rework 8 (2026-10-22).** Advisor PASS with two Low findings and ui-verifier PASS with no UI defects (`evidence/US-0196-verify.md`, "Advisor code review (rework 8)" and "UI verification (rework 8)"). Fixes: (1) `TabMenuOpen` is also cleared when the menu entity is released (a menu replaced or abandoned without a dismissal), and the record is now per menu (panel id plus the menu's entity id), so the release of a replaced menu never clears the record of the menu that replaced it, also on the same tab; test `a_tab_menu_records_that_it_is_open_until_it_is_dismissed` right-clicks the same tab twice; (2) records: the `docs/gui-layout.md` "Colours" bullet and the HLD "Row content", grip and colour-table rows now state the rework 8 colours (hover title, `tab_bar.selected.foreground`).

## Rework 9 2026-10-23 (owner): the whole expanded row reorders; the grip is gone

Acceptance rework of this packet (reopened, not a new BUG). Owner report, 2026-10-23: "tabs can no longer be reordered like before"; the owner does not see the 8 px hover-only grip at the left edge of an expanded row.

**Owner ruling.** In the **expanded** rail, dragging the **whole row** reorders tabs (as the horizontal strip did). To move a tab into a Space the user drags the row out of the rail and drops it on the terminal area (the existing Space drop targets). The 8 px grip is removed. The **collapsed** strip stays exactly as it is (the shell icon drags the tab into a Space, the rest of the cell reorders).

**Outcome.**

1. An expanded row starts the kit's `DragPanel` from any pixel of it (title, chips, directory line, status icon, padding), the drag the collapsed cell already starts outside its icon. The row's own `DragTerminalTab` drag is gone in the vertical layout (it is kept for the collapsed cell's icon and for the horizontal strip).
2. The grip, its glyph, its `GRIP_*` selectors and the row paint's `secondary` colour (which only the grip read) are deleted. The reorder drop line, the drop slot below the last row and the drag preview chip are unchanged.
3. An **empty Space accepts a dragged terminal tab in either payload**: the strip's and the collapsed icon's `DragTerminalTab`, and now the dock's `DragPanel` of a terminal panel. The panel behind a `DragPanel` is resolved through the workspace dock area (`DockArea::panel(PanelId)`, public API, already used by `spaces.rs`), the same terminal-leaf move runs (`handle_tab_drop` body), and a `DragPanel` of any other panel kind is neither highlighted nor accepted.

**Choices the request left open.**

- **Option (a), not (b).** Weighed: (a) the row starts `DragPanel` and the empty Space also accepts `DragPanel`; (b) keep the row's `DragTerminalTab` and make the rail accept it for reorder. (b) needs the shell to name `DragTerminalTab` (`pub(crate)` in `oneterm-terminal-view`, R4), so the payload would move into a shared crate with an erased panel handle; it also loses everything the kit's `DragPanel` gives the rail today: a drop between two rails of a split centre, a split of the centre by dragging to a content edge (the UI walk of `US-0196` used it), the drop placeholder flight and the preview size. (a) changes two small places and keeps all of that. What (a) costs: `docs/terminal-split/03-drag-drop.md` § 6 rejected accepting `DragPanel` in a Space because it "could move a whole `TerminalPanel` instead of one active terminal leaf". That risk is answered here, not ignored: the accepted `DragPanel` is resolved to its terminal panel and goes through the **same** leaf move as `DragTerminalTab` (only the active terminal leaf moves; the Space still has to be empty), and the doc is amended.
- **What the dock does with a `DragPanel` over the terminal area, and how this stays consistent with the strip.** The kit's content frame tracks every `DragPanel` over it and, on release, merges into the group (centre zone: a no-op for the dragged panel's own group) or splits the centre (edge zones). That is what a strip tab dragged by its edge (outside its label) does today, and what a rail row dragged by the old grip did; (a) leaves it as it is. The strip's label drag (`DragTerminalTab`) never triggered it, so a label drag onto a **non-empty** Space still does nothing; the rail row now behaves as the strip's edge does there. Recorded as a known difference from the strip's label, not hidden.
- **The drop indicator after a drop on an empty Space needs no code.** An empty Space's `on_drop` is innermost, so it wins and stops the content frame's own drop (the group's `drop_indicator` is then never taken), but `TabGroup::context` hands a skin an indicator only while a drag is active, so none outlives the drop. A first draft guarded `render_drop_indicator` in `OneTermTabGroupSkin` against exactly this; a test that disabled the guard still passed, which is how the kit's own filter was found, and the guard was removed.
- **No grab cursor on an expanded row.** A whole row that is also a click target keeps the default cursor (the strip's tab has the same); the collapsed cell keeps its open hand.
- **`grip-vertical.svg` and `AppIcon::GripVertical` stay**: the icon is a generated enum variant of the shared set; deleting an asset is not this packet's. `TabRowLayout::VERTICAL_ROW_PADDING_LEFT` (14 px) stays: the accent pill (3 px) still needs the clearance and the header label starts at the same x.

**Why the owner could not see the grip.** Not a rework 8 regression and no layer over it: it was the row's last child (drawn after the hover fill and the title) and the ui-verifier saw it on hover in reworks 7 and 8; it was undiscoverable by design (six dots of `muted.foreground` in the far 8 px, shown only on hover, overlapping the selected row's pill). From the code and the earlier walks; not reproduced on the owner's machine.

**Acceptance.**

- [x] An expanded row starts the reorder drag (`DragPanel`) from its title, from its trailing status icon and from its padding; no row starts a `DragTerminalTab` in the vertical layout (workspace test with a probe panel that mirrors the real row; terminal-view test on the real row).
- [x] Dropping an expanded row on another row reorders (forward lands after, backward before), below the last row appends, a lone tab has nothing to reorder, a zoomed group is neither draggable nor droppable (the existing tests, re-pointed from the grip to the row).
- [x] There is no grip: no `GRIP_*` selector or glyph, the title element fills the row to its left edge, and a press at 2 px from the left edge starts the reorder (workspace tests).
- [x] Dropping a terminal row's `DragPanel` on an empty Space moves its active terminal into it and removes the emptied source tab; a `DragPanel` of another panel kind is ignored; dropping on a non-empty Space moves nothing (terminal-view tests on a real dock area).
- [x] A row dropped on content that takes the drop (an empty Space, stood in for by the probe's content) changes nothing in the dock: same tab order, still one group, even on the edge where the dock would split (workspace test).
- [x] The collapsed strip is unchanged: its tests pass unedited (`a_collapsed_cell_reorders_from_the_cell_and_drags_the_tab_from_its_icon` and the collapsed-cell module).
- [x] `docs/gui-layout.md`, the HLD rows, `docs/terminal-split/03-drag-drop.md`, the intake and this packet say the same thing.
- [x] Full `pwsh scripts/ci-local.ps1` passes; the debug app builds.
- [ ] On screen, by `ui-verifier`: reorder by dragging the title area and the status icon of a row (drop line and preview chip), the drop below the last row, drag a row onto an empty Space and onto a terminal, the collapsed strip unchanged, no grip on hover.

**Evidence (coder, rework 9).** `pwsh scripts/ci-local.ps1` (`CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=2`, worktree `target`): all checks passed, including the three `--ignored --test-threads=1` lines, `check-ignored-tests.py`, `check-theme-contrast.py` and `check-doc-paths.py` / `check-english.py`. Per crate after the last edit: `oneterm-workspace --lib` 121 passed, 3 ignored; `oneterm-terminal-view --lib` 442 passed, 3 ignored. `cargo build -p oneterm-app` builds (`target\debug\oneterm.exe` in the worktree); smoke launch with a private profile (scratch cwd and `USERPROFILE`, `vertical_tabs.enabled` on) ran 12 s without a panic, then the pid was stopped. No UI verdict here.

- **Code.** `tab_rail.rs`: the grip, `GRIP_*`, `RowPaint::secondary` and three imports deleted; the kit `Tab` takes `on_drag(DragPanel)` for every row (the open hand only on a collapsed cell). `tab_title.rs`: `row_drag` / `cell_drag` now follow the layout (strip: the whole row; collapsed: the icon; vertical: none). `panel/spaces.rs`: `handle_tab_drop` and the new `handle_panel_drop` share `move_tab_into`; `TerminalPanel::dragged_by` resolves a `DragPanel` through `DockArea::panel` and keeps terminal panels only. `space/render.rs`: the empty-Space placeholder registers `drag_over` and `on_drop` for `DragPanel` next to the `DragTerminalTab` pair.
- **New or re-pointed tests.** `oneterm-workspace`: `the_whole_expanded_row_is_the_reorder_drag` (title, trailing status icon and the old grip spot each start the reorder and move the tab as the kit's insert rule says; the probe's `title_drags` stays 0), `a_row_dropped_on_a_content_that_takes_it_changes_nothing_in_the_dock` (a drop on the content's right edge, where the dock would split, goes to the content and leaves one group and the order), `the_title_fills_the_row_and_nothing_cuts_it` (was the grip-overlay geometry test), and the former grip tests now start from the row: `a_row_dropped_below_the_last_row_moves_the_tab_to_the_end`, `a_lone_tab_has_nothing_to_reorder`, `a_forward_reorder_lands_after_the_hovered_row_and_a_backward_one_before_it`. The probe panel now mirrors the real row: its own drag exists only in the horizontal layout (and on a collapsed cell's icon). `oneterm-terminal-view`: `the_vertical_row_fills_its_width_and_starts_no_drag_of_its_own`, `the_strip_row_still_drags_the_tab_into_a_space`, `the_collapsed_cell_icon_still_drags_the_tab_and_the_rest_of_the_cell_does_not`, `a_dragged_dock_panel_dropped_on_an_empty_space_moves_its_terminal` (a real dock area with two terminal tabs: an unknown panel id and an occupied Space move nothing; the real drop fills the empty Space, removes the emptied source tab and does not close its session).
- **Collapsed strip unchanged.** `a_collapsed_cell_reorders_from_the_cell_and_drags_the_tab_from_its_icon` and the rest of the collapsed-cell tests pass as they were, except that the one assertion that a collapsed cell has no `GRIP_*` selector was removed with the selector.
- **A guard that was not needed.** The first draft also guarded `OneTermTabGroupSkin::render_drop_indicator` against the dock's drop placeholder outliving a drop that an empty Space takes. A test that removed the guard still passed: `TabGroup::context` already hands a skin an indicator only while a drag is active. The guard and its test hook were dropped; the finding is under the choices above.

**Weak points (for the reviewer and the UI walk).** (1) While a row hovers an empty Space the kit's split placeholder (a half or the whole of the content, by pointer zone) is drawn under the Space's own `drop_target` fill, and the drop is a move into the Space whatever zone it shows: the two cues can disagree near an edge. (2) A `DragPanel` of a non-terminal panel dropped on an empty Space is consumed by the Space's drop (gpui takes the drag before the type check can decline), so the dock's merge or split does not happen there. (3) Dropping a row on a non-empty terminal area splits or merges the centre as the strip's edge always did; the strip's label never did. (4) The grip's root cause is reasoned from the code and the earlier walks, not reproduced.

### Rework 9 after the reviews (2026-10-23): "no split by drop"

Advisor PASS with one Minor, ui-verifier partly PASS (`evidence/US-0196-verify.md`, "Advisor review (rework 9)" and "UI verification (rework 9)": expanded reorder, Space drop and the collapsed strip PASS; the horizontal strip's reorder is not confirmable with posted messages, an owner check).

**Owner ruling, 2026-10-23 ("Chan tach").** Dropping a dragged tab (`DragPanel`) on a **non-empty** terminal must not split or merge the dock's centre (ui-verifier D2: an early release made a second group with its own rail). Only an empty Space (a terminal tab moves into it) and the rail or strip (reorder) accept the drop. One rule for the expanded row, the collapsed cell and the strip's edge drag: terminal content never splits the dock by a drop. This supersedes weak point 3 above and turns the advisor's Defect 1 (a non-terminal panel's drag swallowed by an empty Space) into the rule, fix shape (a): documented, not forwarded.

**How the block is done.** `TerminalPanel`'s root element takes `on_drop::<DragPanel>` and does nothing. gpui gives a drop to the consumer closest to the pointer and takes the drag before it asks anything (`gpui-pre-0.3.9 src/elements/div.rs:2905-2931`), so the dock's content-frame handler never runs for a drop on a terminal panel; an empty Space, being inside the panel, is still innermost and still moves the terminal in. No kit or gpui change. Foreign panels (SFTP, session) dropped on a terminal tab's content, empty Space or not, do nothing. The strip's edge drag dropped on an empty Space moves the terminal in (advisor observation 1; kept, documented in `docs/terminal-split/03-drag-drop.md` section 6).

**How the placeholder is done.** `TabGroup::on_panel_drag_move` keeps tracking the pointer and sets the group's `drop_indicator` for any `DragPanel` over the content; that state is private to the kit and cannot be switched off. What the skin controls is the drawing: `OneTermTabGroupSkin::frame` (which base calls before it asks for the indicator) notes whether the group's active panel is a terminal, and `render_drop_indicator` returns nothing in that case. So no kit split placeholder tints a Space during such a drag (ui-verifier D1), and an empty Space shows only its own `drop_target` fill. Other panels keep the kit's placeholder (the control in the test). This does not touch base or the kit.

**Acceptance (rework 9 after review).**

- [x] A foreign tab dropped on the empty Space, and on a terminal that holds a session in the left edge zone and in the middle, leaves the centre one group (terminal-view test `a_foreign_dock_panel_dropped_on_terminal_content_never_splits_the_centre`, which replaces the advisor's pin `pin_a_foreign_dock_panel_dropped_on_an_empty_space_is_swallowed_by_it`; with the terminal panel's handler removed the second drop makes two groups and the test fails).
- [x] The kit's split placeholder is drawn over another panel's content and not over a terminal's (workspace test `the_split_placeholder_is_drawn_over_other_content_but_not_over_a_terminal`; the SFTP control proves the selector sees it).
- [x] The earlier tests are unchanged: a row dropped on content that takes it leaves the dock alone, an empty Space still takes a terminal tab, the collapsed strip's tests pass.
- [x] `docs/gui-layout.md`, the HLD rows, `docs/terminal-split/03-drag-drop.md` section 6 state the rule.
- [ ] On screen, by `ui-verifier`: a row released on a non-empty terminal (edge zones and middle) changes nothing; no tint on a neighbouring Space while hovering an empty Space; the horizontal strip's edge drag with a real mouse.

Weak points now: (1) is closed by the placeholder rule; (2) is the rule; (3) is closed by the block. Left: the kit's drop state is still tracked and only its drawing is suppressed; a group that displays a terminal and later something else shows the placeholder again, as intended.

**Verification plan.** `cargo test -p oneterm-workspace`, `-p oneterm-terminal-view`, `-p oneterm-state`, `-p oneterm-theme` (comment only), `python scripts/check-theme-contrast.py` (comment only), the full `pwsh scripts/ci-local.ps1`; a smoke launch of the debug build with a private profile for `ui-verifier`.

## Handoff

Implemented and gate-green; `ui-verifier` next (debug binary `target\debug\oneterm.exe` in this worktree). After acceptance tick `US-0196` in the intake. `US-0197` (resize, `clamp_tab_rail_width`, done) and `US-0198` (collapse) build on `tab_rail.rs` and the stored `vertical_tabs` fields.
