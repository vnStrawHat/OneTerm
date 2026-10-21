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

## Handoff

Implemented and gate-green; `ui-verifier` next (debug binary `target\debug\oneterm.exe` in this worktree). After acceptance tick `US-0196` in the intake. `US-0197` (resize, `clamp_tab_rail_width`, done) and `US-0198` (collapse) build on `tab_rail.rs` and the stored `vertical_tabs` fields.
