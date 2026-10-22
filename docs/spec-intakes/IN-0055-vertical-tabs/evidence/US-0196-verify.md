# US-0196 verification (advisor)

- Packet: `US-0196-vertical-tab-rail.md`; coder head `75cf9cc1` on base `c2b7792e`; diff `git diff c2b7792e..75cf9cc1`.
- Worktree: `.claude/worktrees/agent-a9322689c5c9f01fe`, branch `verify/US-0196`.
- Read: the full diff, `tab_rail.rs`, `tab_rail_tests.rs`, `dock_skin.rs`, `tab_row_layout.rs`, `ui_config.rs`, `tab_title.rs`, `panel/tests.rs`, `general.rs`, the intake, HLD and spec review; the kit's `base/src/dock/tab_group.rs` (`TabGroupContext`, the column layout), `base/src/dock/dock_area.rs:1127-1135` (one renderer per group, cached), `base/src/tabs.rs:149-234`, `component/src/dock/tab_panel.rs:433-687`; gpui 0.3.9 `app.rs:1308` / `:2131-2137`, `view.rs:530-557` (refresh and cached views), `elements/div.rs:4039-4065` (group hitbox stack).
- No code was changed. No pinned test was added: no defect in code was found (the findings below are documentation).

## Verdict

**PASS.** Every gate re-run green; the five weak points the coder named hold; the six deviations are sound and recorded. Two low documentation findings and two informational notes.

## Gate re-run (one crate per command, `CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=2`)

| Command | Result |
| --- | --- |
| `cargo clippy -p oneterm-settings --all-targets -- -D warnings` | green |
| `cargo clippy -p oneterm-state --all-targets -- -D warnings` | green |
| `cargo clippy -p oneterm-workspace --all-targets -- -D warnings` | green |
| `cargo clippy -p oneterm-terminal-view --all-targets -- -D warnings` | green |
| `cargo clippy -p oneterm-settings-ui --all-targets -- -D warnings` | green |
| `cargo test -p oneterm-settings` | 55 passed, 0 failed |
| `cargo test -p oneterm-state` | 74 passed, 0 failed, 1 ignored (doc-test) |
| `cargo test -p oneterm-workspace` | 88 passed, 0 failed, 3 ignored (the `IN-0043` elevation tests) |
| `cargo test -p oneterm-terminal-view` | 406 passed, 0 failed, 3 ignored |
| `cargo test -p oneterm-settings-ui` | 58 passed, 0 failed, 1 ignored |
| `cargo fmt --all -- --check` | clean |
| `python scripts/check-theme-contrast.py` | 1638 pairings all >= 4.5:1; 624 hierarchy comparisons pass (matches the packet) |
| `python scripts/verify-dependency-graph.py` | passed, 20 packages; no manifest changed in the diff |
| `python scripts/check-english.py` | passed, 1135 files |
| `python scripts/check-doc-paths.py` | passed, 219 paths in 12 documents |

Not re-run: the full `ci-local.ps1` (the coder's run is recorded in the packet; the five touched crates and every script the diff affects were re-run here), the `oneterm-app` binary (left for the UI walk).

## The coder's weak points

1. **Live switch through `cx.refresh_windows()`.** Holds in the real tree, not only in the test host. `refresh_windows` queues `Effect::RefreshWindows` (gpui `app.rs:1308`), which sets `window.refreshing = true` and marks the window dirty (`app.rs:2131-2137`); a cached view (`AnyView::cached`, used by the workspace for the title bar and status bar and by the kit for the active panel) re-renders whenever `window.refreshing` is set (`view.rs:535`). Independently of that, the tab-group render reads the `UiConfig` entity (`dock_skin.rs:130`), which `detect_accessed_entities` records, so the `cx.notify()` in the setter would re-render it anyway. The test `the_settings_setter_flips_the_layout_on_the_next_frame` drives the real `OneTermTabGroupSkin` and the real `DockArea` inside a window; the only thing the test host lacks is the workspace's title and status bar, which do not sit above the dock area.
2. **Scoped global `TabRowLayout`.** Nothing else builds a title inside the scope: `TabRowLayout::Vertical.draw` (`tab_rail.rs:288-301`) wraps exactly one `handle.title(window, cx)` (or a `tab_name` label), `Tabs::children` is eager, and `render_tab_strip` builds elements only; the closures it registers (context menu, drags, clicks) run later, outside the scope, which is why the menu wording is captured into the closure (`tab_title.rs:357`, `:485`). The grip's own preview (`RailDragPreview::render`, `tab_rail.rs:407-412`) and the kit's `DragPanelPreview` therefore draw the horizontal row, as intended and as `the_rail_draws_its_rows_in_the_vertical_layout_and_nothing_else_does` proves. Early return: not possible (closure form). Panic: the previous value is not restored on unwind (`tab_row_layout.rs:30-36`) — immaterial at runtime because a panic inside a render is fatal to the window, but the packet calls it an "RAII guard", which it is not (finding 1).
3. **Drag arbitration.** The grip's `DragPanel` (`tab_rail.rs:334-344`) and the title row's `DragTerminalTab` (`tab_title.rs:285-296`) have disjoint hitboxes, so the arbitration no longer depends on bubble order at all, which is simpler than the strip. Base `Tab` stops only left mouse-down (`base/src/tabs.rs:172`) after the children have seen it. The split across crates is forced by R4 and is complete: the workspace test proves grip -> reorder and title row -> its own drag with no reorder, the terminal-view test proves the real row starts `DragTerminalTab` (accepted by a `DragTerminalTab` drop target) and the `x` starts nothing. `close_button_visible` is the kit's default `false` (`component/src/dock/mod.rs:158`) and OneTerm never sets it, so the row's own `x` is the only close control in both layouts, as before.
4. **`width` / `collapsed` inert.** Parsed, clamped, round-tripped and omitted while default (tests `the_vertical_tab_width_is_clamped_on_read`, `vertical_tabs_survive_a_load_and_a_save`); nothing reads them (`RAIL_WIDTH` is the constant, `tab_rail.rs:34`). `docs/agents/persistence.md` and `docs/gui-layout.md` both say so. Acceptable for this packet.
5. **Rail rule.** `dock_skin.rs:129-144` reads the switch, excludes a collapsed group, locates the group's node in `DockArea::layout(Center)` and applies `tab_rail::draws_rail` (non-empty, all `terminal`). The right dock keeps the kit strip (`other_groups_keep_what_they_draw_today`). The `DockArea` read from inside a tab group render is safe: the area's lease ends when its `render` returns, before child views are laid out, and the test exercises exactly this path.

## Persistence (`ui_config.json`)

- Absent -> default; `{"vertical_tabs":{"enabled":true}}` -> on; `"x"`, `5`, `true`, `[true]`, `null`, wrong-typed fields -> that field's default with a warning, the rest of the document intact, no quarantine (`a_garbage_vertical_tabs_value_loads_as_the_default`, which goes through `parse_document`, the function `load_from` quarantines on).
- Default omitted on write (field-level `skip_serializing_if` plus the object-level `is_default`); `{enabled:false, width:300}` writes `{"width":300}`.
- Width: `as_f64`, rounded, floored at 0, clamped to 140..=400 on read, never rewritten by the read.
- Older build: `UiConfig` has no `deny_unknown_fields`, so the field is ignored and dropped on that build's next write (the existing `a_config_from_the_agent_panel_era_loads_and_heals` proves unknown fields pass).
- Elevated window: the setter changes the entity then calls `persist`, which asks `write_refusal(is_restricted())` and returns before any write (`ui_config.rs:476-481`); the layout flips in memory. No setter-level test exists (finding 2).

## Crate rules, script, docs

- No `Cargo.toml` changed; `TabRowLayout` lives in `oneterm-state` (L2), the lowest crate both L3 consumers reach (R10); the shell names no feature type (R4). `verify-dependency-graph.py` passes.
- `check-theme-contrast.py`: `tab_bar.background` added to `tab.active.foreground`; the rail draws that pair on hover (`tab_rail.rs:372`) and the strip already did; numbers match the packet.
- `docs/gui-layout.md` "Vertical tabs" section matches the code (rule, seam, anatomy, grip, scroll, menu wording, colours, elevated window, inert fields); the dock-button sentence is qualified. `docs/agents/persistence.md` row and `docs/agents/structure.md` entries are accurate.
- Deviations 1-6 in the packet: each checked against the code; all sound. Deviation 4 (the `DragPanel` on the grip only) gives the HLD's visible behaviour with no overlapping hitbox.

## Findings

1. **Low — packet text: "RAII guard" is not what was built.** `US-0196-vertical-tab-rail.md:77` says the layout is set "(RAII guard)"; `crates/state/src/tab_row_layout.rs:30-36` is set / call / set-back with no `Drop` type, so the previous layout is not restored if the closure unwinds. No runtime effect (a render panic is fatal), but the record should say what the code does, or the code should match the record (a 6-line guard struct). Reproduction: read the two lines.
2. **Low — packet acceptance claims a test that does not exist.** `US-0196-vertical-tab-rail.md:46` ticks "an elevated window writes nothing (existing `write_refusal` test plus a setter-level assertion)"; the Evidence section (`:114`) says the setter has no write path of its own and names only the existing `write_refusal` test. There is no assertion on `set_vertical_tabs_enabled` under elevation (it would need the process-global flip of the `IN-0043` ignored tests). Drop the words "plus a setter-level assertion" or add the test to the ignored set.
3. **Info — HLD "Readers" paragraph is stale after deviation 1.** `high-level-design.md:265-267` still says `render_tab_strip` reads `UiConfig::global(cx)`; the packet records the deviation but the HLD was not amended. One sentence: the row reads `oneterm_state::TabRowLayout`, set by the rail.
4. **Info — `draws_rail` is evaluated three times per group per frame** (`dock_skin.rs:153`, `:167`, `:178`), each upgrading the area and walking the centre tree. Negligible today; worth a single evaluation per frame if `US-0197` adds per-frame work to it.

## For the owner to check by hand (the UI walk)

- The real window: switch on and off from Settings with two or more tabs open; the sessions keep running; the terminal is resized, not covered.
- Row behaviours in the rail: click, middle-click, `x`, double-click rename, right-click menu (reads "Close Tabs Below"), status icon, channel chips, recording dot; the grip appears on hover with the open-hand cursor; drag by grip reorders; drag by label into an empty Space works and dropped on another row does nothing.
- 40 tabs: wheel scroll in the rail; `+` scrolls the new tab into view.
- Split centre: two rails, one per group; a terminal tab dropped into the right dock keeps the strip there.
- Zoom and lock screen with the rail on; restart persistence; an elevated window (switch works, `ui_config.json` untouched).
- Light and dark themes: hovered and selected row text readable.

## UI verification (US-0196, vertical tab rail)

Build under test: scratchpad copy `us0196-bin\oneterm.exe` (head 75cf9cc1), private profile (scratch cwd and USERPROFILE), Windows 11, 1280x800 window, posted-message driver + PrintWindow. Screenshots: `scratchpad\us0196-shots\us0196-NN-*.png`. Note: the shell prompt text never rendered in any pane, in the vertical or the horizontal layout (baseline `us0196-09`), so this environment cannot show terminal text; reflow is judged from the cursor position and pane edges.

### Acceptance

| # | Item | Verdict | Evidence |
|---|------|---------|----------|
| 1 | Settings > General > Interface > Vertical Tabs (default off), live flip both ways, shells/tabs kept, survives restart | PASS | `us0196-11`..`13c` (switch present, English label and help text, default off), `15` (off: strip, 3 tabs), `16` (on again: same 3 tabs, same selection), `ui_config.json` held `{"vertical_tabs":{"enabled":true}}`, restart `58` / `60` (rail still on). "Shells keep running" is judged by tabs, titles and selection surviving; no shell output is visible in this environment. |
| 2 | 220 px rail, 32 px header (`+`, zoom only), 32 px rows, selected fill + 2 px accent bar, terminal narrower | PASS (bar: see D1) | `01`, `03`, `14`. Measured: rail x 8..228 (220 px), header and rows 32 px, bar 2 px (82,139,255) in Zed One Dark, no `...` menu, no dock toggle. Cursor starts at x=238 (vertical) vs x=18 (strip): resized, not overlapped. `+` opens the New Terminal menu (`02`). |
| 3 | Click selects; middle-click and x close; double-click renames; context menu; chips, status icon, recording dot | PARTLY | PASS: click, middle-click (`19`), x (`20`), menu Rename.../Duplicate/Close/Close Others/Close Tabs Below (`04b`), Rename dialog and a long name truncated with an ellipsis (`05`, `06`), Close Tabs Below confirm "Close 37 terminal tabs? Their sessions end." and result (`25`..`27`), channel chip A in the row (`66`). NOT WALKED: double-click rename (the posted double-click does not rename on the horizontal strip either, `10`, so a driver limit), status icon (needs OSC 7501 output), recording dot. |
| 4 | Drag row content to an empty Space; drop on a row does nothing; grip reorder; x starts no drag; lone tab no reorder | PASS | Content drag to a Space moved the tab out of the rail (`33`, `34`); content dropped on another row changed nothing (`30`); grip drag shows a drop line and a preview chip and reorders (`23`, `24`); grip drop below the last row appends (`29`); a drag started on x moves nothing and shows no preview (`35`, `36`); a lone tab shows no grip and no drag (`61`..`63`). 12 px grip on the hovered row (`03`, `22`). Open-hand cursor: NOT WALKED (not visible in PrintWindow). |
| 5 | ~40 tabs scroll under the header; new tab scrolls into view | PASS | `21` (40 tabs, newest active row visible at the bottom, header fixed), `22` (wheel scrolls, header stays). No scrollbar or other overflow cue is drawn (D3). |
| 6 | Zoom keeps the rail; tooltip Zoom In / Zoom Out | PASS | `17` ("Zoom In  Shift+Esc"), `18` (icon changes, "Zoom Out  Shift+Esc", rail kept). |
| 7 | SSH Client group has no rail; terminal tab dropped in the right dock keeps the strip; split centre one rail per group | PASS for 3 of 4 | `39` (Session and SFTP groups: kit headers, no rail), `41` (terminal tab dropped into the right dock: kit strip with `+`, zoom, `...`), `38` (centre split by grip drag to the right edge: two groups, one rail each). An SSH tab opening the right dock: NOT WALKED (no sftp-dev-server copy in the scratchpad). |
| 8 | Lock screen covers the rail; rail back after unlock; elevated window | PASS / NOT WALKED | `55` (locked: no rail, no terminal pixels), `56` (unlocked: rail and rows as before), `57` (locked at restart). Elevated window: NOT WALKED (owner only). |
| 9 | Both layouts, a light and a dark theme: hover label, grip, x, selected text contrast | PARTLY | Dark: Zed One Dark `03`, Catppuccin Mocha `49`/`58`; light: Zed One Light `43`, `44` (strip), `60`; Catppuccin Latte `47`. Hovered label, grip, x and selected text are legible in all. The accent bar is missing after a live theme switch (D1). |

### Defects

**D1 (medium): the 2 px accent bar on the selected row is absent after a live theme change.**
- Screen: rail, selected row. Expected: 2 px bar in `table_active_border` in every theme (HLD Rail anatomy). Observed: Zed One Dark at launch shows it (pixels x=8..9 = 82,139,255, `us0196-03`). After switching Settings > General from Dark to Light (Zed One Light, then Catppuccin Latte) and from One Dark to Catppuccin Mocha, the selected row has no bar (pixels x=8..11 equal the row fill: `43`, `47`, `49`, `59`). After a restart in the same theme the bar is back (Mocha `58`: 77,97,136; Light `60`: 82,111,255).
- The horizontal strip loses its top accent bar the same way after the live light switch (`44`), so the cause is probably the shared token not refreshing on a live theme change, not the rail. It is still a missing selection cue for the user. In Mocha the restart colour is also low contrast against the fill.
- Steps: launch with `vertical_tabs.enabled`, open Settings (Ctrl+,), General > Mode > Light; look at the selected row.

**D2 (low, observation): a forward reorder lands after the hovered row while the drop line is drawn above it.** Dragging the first row's grip onto the 8th row (`23`) drew the line above that row, and the tab ended up at the 8th position (`24`), i.e. after it. This matches the kit's insert-at-slot rule but reads as "before this row". Not compared against the strip.

**D3 (low): no scroll cue.** With 40 tabs the rows scroll by wheel with no scrollbar or fade (`21`, `22`).

**D4 (low): drag preview chip is faint.** The grip-drag preview text is drawn grey on dark (`23`) and truncated ("Comman..." in `37`). Legible but dimmer than the row label.

### Not walked / owner-only
- Double-click rename (the driver cannot produce a double-click gpui accepts, on the strip either).
- Status icon (OSC 7501) and recording dot in a rail row.
- Open-hand cursor on the grip.
- SSH tab opening the right dock (needs a server; none built in the scratchpad).
- Elevated window (UAC).
- Real terminal output, and "shells keep running" beyond tab/session survival (no prompt text renders in this environment, strip or rail).
- Sleep/resume, taskbar thumbnail, IME.
- Flaky: twice a freshly opened window (first Settings window, one relaunched main window) painted black under PrintWindow and never recovered; a relaunch was fine. Not tied to the rail, but worth an owner eye.


## Re-review (advisor): rework `a90f894f..36d2c8d9`

- Reviewed: `5ba12664` (fix), `eee4817d` (UI evidence), `36d2c8d9` (gate evidence); sources as in the first review plus gpui 0.3.9 `window.rs:2376-2381` / `:3423-3470` / `:3669-3706`, `app.rs:1976-1992` / `:2140-2145` / `:2403-2405` / `:2450-2463`, kit `theme/mod.rs:285-400`, `theme/registry.rs:1-72`, `theme/schema.rs:682-700` / `:1000-1012`.
- Gate re-run (one crate per command, `CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=2`): `cargo clippy --all-targets -- -D warnings` green for `oneterm-theme`, `oneterm-state`, `oneterm-workspace`, `oneterm-terminal-view`; `cargo test` 6 / 75 / 90 / 406 passed, 0 failed; `python scripts/check-theme-contrast.py` 1638 pairings and 624 hierarchy comparisons pass; `cargo fmt --all -- --check` clean; `check-english` passes.
- No code changed. A pinned test for finding R1 was written and withdrawn: the gpui test platform redraws a dirty window on its own after every update, so the one-frame lag cannot be isolated there (details under R1).

### Verdict

**PASS.** D1, D2, D4 and F1 are fixed at the right place; D3 works, but for a different reason than the code and the records say (R1, low). The launch-time colour the coder could not explain is not explained by the code either (R2, info): the one kit path that re-applies a theme after OneTerm's override is inert at start-up, so the owner's hand check below decides whether anything is left.

### D1 - `oneterm_theme::selection_accent`

- **Correct after a live switch to another theme.** Every switch path (`SwitchTheme` `theme.rs:222-234`, `SwitchThemeMode` `:237-242`, Settings mode `appearance.rs:85-88`, Settings theme `appearance.rs:158-163`) runs `Theme::apply_config` or `Theme::change` first and `apply_list_style_override` right after; `record_selection_accent` (`theme.rs:91-96`) reads the live token at the top of the override, which at that moment is the new theme's `table.active.border`, and only then blanks it. Nothing is stale.
- **Mode switches and reloads.** `Theme::change` reloads the mode's registered file (`mod.rs:389-400`, `edit(reload_mode = true)`), so the live token is the new file's before the override records it. A kit-side re-application without the override (the registry observer, `registry.rs:45-72`) leaves the live token coloured, and `selection_accent` prefers the live token whenever its alpha is above zero (`theme.rs:100-107`), so that case reads the real colour, not the record.
- **Stale case.** The record is used only while a theme's own token is transparent. The kit resolves an absent `table.active.border` to `list.active.border` and an absent one of those to the default theme's colour (`schema.rs:682-700`, `:1008`), never to transparent; a scan of `crates/theme/themes/*.json` finds one theme that names neither (Catppuccin Frappe), which therefore takes the kit default, a visible colour. No shipped theme can make the accent stale.
- **Global lifetime.** `SelectionAccent` is an `App` global written only by the override and read only by `selection_accent`; it is never removed and never needs to be. Correct.
- **Other readers of `table_active_border`.** `crates/sftp-ui/src/table_delegate.rs:341` (a comment relying on it being transparent, which still holds) and the kit's tables. The three OneTerm accent readers (rail row `tab_rail.rs:376`, strip top bar `tab_title.rs:223`, Space cue `space/render.rs:172`) now use `selection_accent`. Complete.
- **Test.** `the_selection_accent_survives_the_list_style_override` covers two themes of both modes, before and after the override and after a second run. Sound.

### D2 - drop line

`drop_edge` (`tab_rail.rs:98-105`) matches base's rule: `drop_panel(Some(ix), ..)` removes the panel, then inserts at `ix`, so a tab from earlier in the same group lands after the hovered row and anything else before it; the dragged row itself draws no line. The unit test models the rule for every pair in groups of 1..=5 and the gpui test `a_forward_reorder_lands_after_the_hovered_row_and_a_backward_one_before_it` proves it on the real group. Correct.

### D3 - scrollbar gutter

- **No loop, no flicker.** `overflowing` flips only when `max_offset().y` crosses zero; reserving the gutter changes row widths, never row heights, so the content height and `max_offset` do not move and the flag cannot oscillate. The steady state draws nothing extra.
- **R1 (low): the "one refresh" is not requested.** `TabRail::render` calls `window.refresh()` (`tab_rail.rs:194-197`) to draw the frame that applies the new gutter. A view renders inside the draw: `draw_roots` sets `DrawPhase::Prepaint` before the root's `request_layout` (gpui `window.rs:3670-3706`), and `Window::refresh` does nothing unless `not_drawing()` (`window.rs:2376-2381`). So the call is dropped every time; the gutter appears or disappears with the next frame anything else causes (in the app: cursor blink, output, hover; in the gpui test platform: the automatic redraw after any update, which is why the 40-tab and 3-tab tests pass and why a pinned test could not isolate it). The behaviour is right in practice, one frame late at worst; the code comment, the packet ("one refresh is requested") and `docs/gui-layout.md` describe a mechanism that is not there. Fix is one call: `window.request_animation_frame()` (`window.rs:2730-2733`, which schedules a `notify` of the current view for the next frame) in place of `window.refresh()`; the test platform's `simulate_next_frame` then delivers it in a test.

### D4 - preview chip

`tab.active.foreground` on `tab.active.background` is the measured pair; `min_w_24` / `max_w(240)` with `overflow_hidden` and `whitespace_nowrap` is content-sized. `DRAG_PREVIEW_SIZE` (140 x 30) is only base's placeholder hint. Fine.

### F1 - `TabRowScope`

`TabRowLayout::scope` (`tab_row_layout.rs:34-38`) sets the global and returns a guard that owns the `&mut App`; `Deref` / `DerefMut` hand it back (`:49-61`), so nothing can touch `cx` except through the guard while it lives, and `Drop` restores the previous layout (`:63-67`). Sound. The panic test (`the_layout_is_restored_when_the_drawing_unwinds`) is meaningful: the workspace profile is `panic = "unwind"` (root `Cargo.toml:288`), `catch_unwind` runs the guard's `Drop` during the unwind inside the same `cx.update`, and the assertion afterwards reads the restored global. The nested-scope test covers the stack order.

### R2 (info) - why the accent showed at launch and vanished after a switch

The code reads: at launch `init` applies the theme, then the override, then nothing re-applies the theme. The only kit path that re-applies one outside OneTerm's switch handlers is the `ThemeRegistry` observer (`registry.rs:45-72`, `Theme::change(mode)` on any registry mutation), and it is inert at start-up: gpui queues the registry notification when `registry::init` first calls `global_mut` (`app.rs:2403-2405`), drops every later one while that first is pending (`:1983-1986`), and the observer registered afterwards is activated by a deferred effect that runs after the queued notification (`:2450-2462`, `:2140-2145`); a probe test confirmed the observer fires zero times across `gpui_component::init` plus the `init` sequence, and no OneTerm code mutates the registry later. So the start-up state the code predicts is a blank token (no accent bar), which contradicts the walk's launch screenshot. The coder's fix is right either way (it reads the live colour when there is one and the record when there is not), so nothing in this packet depends on the answer, but one of two things is true and the owner can tell them apart on screen: either some start-up path still re-applies the theme after the override (then the data tables and the session list draw a selection border at launch that disappears after the first theme switch, i.e. `apply_list_style_override` is not in force at launch), or the walk's launch frame was taken after a theme change the driver made. Hand check: launch, select a row in the SFTP table or the session list, look for a selection border; switch theme; look again.

### Findings

1. **R1, Low** - `crates/workspace/src/layout/workspace/tab_rail.rs:194-197`: `window.refresh()` inside a render is a no-op (gpui `window.rs:2376-2381`, phase set at `:3670`); the gutter frame comes from whatever else redraws. Replace with `window.request_animation_frame()`; amend the comment, the packet's D3 line and `docs/gui-layout.md` ("one refresh is requested"). Reproduction: read the two gpui lines; or, in a `#[gpui::test]`, after a draw that flips `overflowing`, `window.simulate_next_frame(cx)` returns 0.
2. **R2, Info** - the launch colour is not explained by the code (above). The owner's hand check decides whether a separate start-up-override defect exists; outside this packet.

### Owner hand checks added by the re-review

- Table / session-list selection border at launch versus after a theme switch (R2).
- With 40 tabs, close tabs until they fit and watch the gutter leave within a moment (R1: it should follow on the next natural frame, not stick).

## UI re-walk (ui-verifier, head 36d2c8d9)

### UI re-walk (US-0196 fixes, head 36d2c8d9)

Build: scratchpad `us0196-bin2\oneterm.exe`, fresh private profile (label r1), 1280x800. Screenshots `scratchpad\us0196-shots\us0196r-*.png`. Pixel values are from PrintWindow captures (x = client+8).

| Defect | Verdict | Evidence |
|---|---|---|
| D1 accent bar after live theme switch | PASS | Rail, selected row bar pixels (x=8..9) after live switches: Zed One Dark 82,139,255 at launch; Mocha 77,97,136; Latte 114,135,253 (`tv-modelight`, `tv-latte`); Zed One Light 82,111,255 (`tv-onelight`); Default Dark 29,78,216 (`tv-defaultdark`); every value is the new theme's accent, none the previous theme's. Horizontal strip top bar (y=35..36 at x=340): Default Dark 29,78,216, Latte 114,135,253, One Light 82,111,255, Default Dark again, Mocha 77,97,136 (`th-*`). Active Space border follows the theme in a split tab (`us0196r-12` Mocha grey-blue, `us0196r-13` Latte blue). One gap: the final "back to Zed One Dark" step was NOT WALKED, the theme list cannot be scrolled by posted input (Zed One Dark sits off screen once another dark theme is selected); One Dark at launch was the baseline. |
| D3 scrollbar and gutter | PASS | 40 tabs: scrollbar shows (x~218-222), rows end before it, x buttons move from x=214 to x=198 and the selected fill stops at x=210 (`us0196r-23`). A renamed row's x next to the scrollbar closed it (`24`, `25`). 3 tabs: no scrollbar, x at x=214 (`us0196r-22`, `19-crop`). |
| D2 drop line vs landing | PASS | Forward: pointer on row 2, line at the bottom of that row, tab landed after it (`18-crop`, `19-crop`). Backward: pointer on row 1, line at its top, tab landed first (`20-crop`, `21-crop`). |
| D4 grip-drag preview text | PARTLY | Chip text is now full-contrast and larger, readable (`18-crop`, `20-crop`). A 46-character title is still truncated with an ellipsis ("build-server-with-a-very-l...") in the chip; if "not truncated" meant the full title, this fails. |

### Quick regression
- Switch on/off live, both ways (Default Dark horizontal and back, `10-horizontal-defaultdark`): PASS.
- Click select, x close, rename through the context menu, Close Tabs Below not repeated, long name truncated with ellipsis in the row: PASS.
- Lock (Ctrl+Shift+L) covers the rail (`27`), unlock returns the rail with scroll position kept (`28`): PASS.

### Notes
- Capture driver: with Windows PowerShell 5.1 the typed-int arguments silently failed; pwsh 7 works (`us0196r-do.ps1`, `us0196r-item.ps1`).
- Mid-drag captures can show a blank frame right after a dialog closes; recaptured after a hover pause.

Coordinator: D4 accepted as is; the chip is content-sized to at most 240 px, so a long title is ellipsised by design.

## Review (owner rework 2026-10-21): no close button in a vertical row, status icon at the trailing end

- Reviewed `git diff 91297cfd..99e63050` (8 files) on branch `verify/US-0196-r2` in worktree `agent-aece93a0e0e48ea0e`; read `tab_title.rs:186-240` / `:328-360`, the two test files, `tab_rail.rs:188-197`, the packet's rework section, the HLD and `docs/gui-layout.md` amendments; gpui 0.3.9 `elements/div.rs:878-900` (`debug_selector`), root `Cargo.toml:24` (`resolver = "3"`).
- Disk D: 7.9 GB free at start, 7.7 GB after the builds (never near the 3 GB stop line). App binary not built.
- No code changed, no pinned test added: no defect found.

### Verdict

**PASS.** The vertical row omits the `x`, puts the status icon last and leaves the strip byte-for-byte as before; the three points the coder asked about hold.

### The coder's three points

1. **Trailing status icon versus the scrollbar gutter.** It is clear by containment, not by a separate assertion. The row element is a child of the list; once the rows overflow, the list reserves `Scrollbar::width()` as right padding (`tab_rail.rs:214`), so every row's right edge is at or left of the gutter (asserted by `forty_tabs_scroll_and_the_active_one_is_brought_into_view`: `first.right() <= list.right() - Scrollbar::width()`). Inside the row the title element is `flex_1().min_w_0().pr_1()`, the label is `flex_1().min_w_0()` with `text_ellipsis` (`tab_title.rs:212`), and the status icon is `flex_shrink_0` and the last child: the label is the only item that gives way, so the fixed items (grip 12 px, up to five chips, the dot, the 16 px icon, 4 px padding) cannot push the icon past the row's edge in a 204 px row. The terminal-view test asserts the other half, `status.right() <= row.right()` and `status.right() >= row.right() - 8` (so it is really at the end, inside the 4 px padding). Together the two tests pin `icon.right <= list.right - gutter` in a 40-tab frame; a single cross-crate assertion is not possible (R4: the shell cannot draw the real row), and is not needed.
2. **`debug_selector` on the shared status icon and label.** A no-op outside tests: gpui compiles the recording body only under `cfg(any(test, all(debug_assertions, feature = "test-support")))` and otherwise an inlined identity that never calls the closure (`div.rs:884-900`). `test-support` is a dev-dependency feature of `oneterm-terminal-view` and `oneterm-workspace`; with `resolver = "3"` dev-dependency features do not unify into the `oneterm-app` build, so the strip in the shipped binary pays nothing. In a `cargo test` build each drawn row allocates three short strings per frame, which is what every other selector in the file already does. Duplicate names across rows (every tab's icon is `oneterm-tab-status`) matter only to `debug_bounds`, and the terminal-view host draws exactly one row.
3. **Drag priority with the `x` gone.** The former "starts neither" element is now row content; the tests say so and still prove the split: `the_vertical_row_fills_its_width_and_only_its_content_drags_the_tab` drags from the former `x` spot and lands a `DragTerminalTab` on the Space drop target (`dropped` 1 -> 2); `the_title_row_drags_into_a_space_and_only_the_grip_reorders` drags from the probe's status stand-in (plain content, no stop-propagation) and asserts it is the title row's own drag that reorders nothing, then that only the grip reorders. The horizontal strip keeps its `x` with its left mouse-down stop, unchanged and untouched by the diff.

### Everything else

- **Strip unchanged.** `leading_status` / `trailing_status` resolve to the old order when `!vertical` (`tab_title.rs:238-243`), and the `x` is gated by `tab_panel.filter(|_| !vertical)` (`:349`); `the_horizontal_row_is_content_sized_as_before` now also pins status-before-label and `x`-after-label on the strip.
- **Close paths left in the rail.** Middle-click (`on_mouse_down(Middle)` on the whole row, untouched), the context menu rows and the panel's keyboard paths; the packet is right that no gpui test drives middle-click, and lists it for the UI walk.
- **Status icon behaviour.** Same element (`tab_status_icon`), same tooltip, id and rules; only its slot moves. With no status nothing is drawn and the label runs to the padding (asserted).
- **Docs.** HLD (diagram, Rail anatomy, wireframe, Interactions rows, contrast table), `IN-0055.md` walk sentences, `docs/gui-layout.md` anatomy and row description, and the packet (reopened, rework section, acceptance) all say what the code does. The drag preview chip still renders the horizontal row (so it shows an `x`); that is the existing design and not touched here.

### Gate (one crate per command, `CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=2`, worktree target reused)

| Command | Result |
| --- | --- |
| `cargo clippy -p oneterm-terminal-view --all-targets -- -D warnings` | green |
| `cargo test -p oneterm-terminal-view` | 407 passed, 0 failed, 3 ignored |
| `cargo clippy -p oneterm-workspace --all-targets -- -D warnings` | green |
| `cargo test -p oneterm-workspace` | 90 passed, 0 failed, 3 ignored |
| `python scripts/check-english.py` | passed |

Not re-run: the full `ci-local.ps1` (recorded in the packet), the app binary (disk).

### Findings

None. For the `ui-verifier` walk the packet already lists: no `x` in any rail row in a light and a dark theme, the status icon at the right end for each state, the label ellipsis with and without a status, the icon clear of the scrollbar with 40 tabs, middle-click and the menu closing a tab, the strip unchanged.

## UI walk (owner rework 2026-10-21, ui-verifier, head 99e63050)

### UI verification: US-0196 owner rework (× removed from rail rows), head 99e63050

Build `us0196-bin3\oneterm.exe`, fresh private profile (label o1), pwsh 7 driver, shots `us0196o-*.png` (small crops).

**Environment limit:** every shell started in this environment exits within seconds ("terminal session is closed" in the log, no cmd/pwsh child under the app), so no OSC 7501 state, chip-with-status, recording dot or SSH could be produced. All status-icon items are NOT WALKED. This is not a verdict on the rework; the owner (or a session where local shells stay alive) must walk them.

| # | Item | Verdict | Evidence |
|---|------|---------|----------|
| 1 | No × in any rail row, light and dark | PASS | Dark (Zed One Dark): idle, hovered, selected rows (`02`); light (Zed One Light): idle, hovered, selected (`03`). Only the 12 px grip shows on hover. |
| 2 | Status icon at the right end for Working/Blocked/Done/Error, not before the label | NOT WALKED | No status producible. Idle rows show nothing before the label (`02`, `03`). |
| 3 | Label full width when no status; ellipsis before the icon when status | PARTLY | No-status case PASS: a 46-char title ellipsizes at the far right of the row (`03`, `04`), a short one is unaffected. With-status case NOT WALKED. |
| 4 | Chips, recording dot, status icon on one row | NOT WALKED | Needs status; chip alone was verified in the previous walk. |
| 5 | 40 tabs: status icons clear of scrollbar/gutter | NOT WALKED (icons) | 40 tabs open and scroll (`13`); icon clearance not testable. The earlier re-walk showed the gutter and scrollbar with no trailing element. |
| 6 | Middle-click closes; Close / Close Others / Close Tabs Below | PASS | Middle-click removed the row (`04`); menu lists Rename, Duplicate, Close, Close Others, Close Tabs Below (`05`); Close removed the row (`06`); Close Others confirm then 1 row left (`07`, `08`); Close Tabs Below after adding tabs left 1 row (`09`; the added rows were not individually counted before the close). |
| 7 | Drag label/trailing icon into an empty Space; grip reorders; icon tooltip | PARTLY | Label drag into a Space moved the tab out of the rail (`10`, `11`; dragging the active tab that owns the Space does nothing, as before); grip reorder to the bottom works (`12`). Dragging from the trailing icon and the icon tooltip: NOT WALKED. |
| 8 | Switch off: strip as before (icon before label, × at end), × closes; live both ways | PARTLY | Live flip off and on again worked, strip has × at each tab's end (`14`), rail returned (`16`). The × click was sent (`15`) but the strip scrolled, so the closed tab was not individually confirmed. Status icon before the label: NOT WALKED. |
| 9 | Split tab shows no status icon | NOT WALKED | No status producible; the split tab row is plain (`10`). |

### Defects
None found in what could be walked.

### Needs an owner or a working-shell session
Items 2, 4, 5 (icon part), 7 (icon drag, tooltip), 8 (icon before label), 9, the recording dot, and confirming the strip × close.

Coordinator: merged on the advisor PASS. The status-icon items were not walkable (local shells exit at once under the scratch profile); the icon slot is pinned by the terminal-view layout test, and the owner checks it on screen with scripts/osc7501-test.sh.

## Review (rework 2 + US-0197): Part A, two-line 64 px rows with the cwd

- Reviewed commits `188d8c2d` (packet) and `ffa63aae` (code) of `git diff afb39845..96270e82`, branch `verify/IN-0055-r3` in worktree `agent-aa7d464a81f2a206b`; read `tab_title.rs` (`tab_cwd`, `cwd_display`, `tab_cwd_label`, the two-line `render_tab_strip`), `tab_row_layout.rs` (geometry constants), `view.rs:434-441`, `terminal_panel.rs:404-418`, `osc_router.rs:219-226`, `test_support.rs`, the new tests, gpui 0.3.9 `styled.rs:97-98` / `style.rs:414` / `elements/text.rs:679`; the HLD, `IN-0055.md`, `docs/gui-layout.md` and the packet's "Rework 2" section. Part B is in `evidence/US-0197-verify.md`.
- No code changed, no pinned test added: no defect.

### Verdict (Part A)

**PASS.** The row is 64 px, the title on line 1 keeps its end ellipsis, line 2 is the cwd cut at its start with the status icon right-aligned, the strip is unchanged, and the records say so.

### The coder's points on Part A

- **cwd_display edge cases** (`tab_title.rs:98-150`). UNC `\\server\share\x`: not a drive path, so it is shown untouched (backslashes kept, no `~`); the home prefix cannot match. Git Bash `/c/Users/dev/src` on a local tab: not a drive path either, so it is shown as reported, with `/` and **without** `~` (the home is `C:\Users\dev`, which does not prefix it); correct and safe, just not abbreviated, noted below for the owner. Drive matching is case-insensitive only for drive paths (`eq_ignore_ascii_case`), POSIX paths compare exactly; `shown.get(..home.len())` returns `None` on a non-char boundary, so no panic on a multi-byte path; a trailing separator on the home is trimmed and a root home (`/`) is refused; the whole-component check stops `C:\Users\a` from matching `C:\Users\ab`. Home abbreviation and the `\` rewrite apply only when `local` is true, which `tab_cwd` derives from `SessionKind::Local`, so an SSH path is never touched. All of this is pinned by `the_directory_line_abbreviates_the_local_home_only`.
- **CwdChanged repaints in horizontal mode** (Info, not a defect). The panel's one subscription notifies on *every* `TerminalViewEvent` (`terminal_panel.rs:413-415`), and the router emits `SessionEvent::Cwd` on every OSC 7 without comparing to the previous value (`osc_router.rs:219-226`); most shells send OSC 7 at every prompt, so the strip's panel now re-renders once per prompt whether or not the cwd changed or is shown. It is one more notify in the same frame as the per-prompt OSC 0/2 title notify the panel already takes, so no new frames in practice; a cheap narrowing, if wanted, is to emit `CwdChanged` only when the path differs from the last one the view saw.
- **Start-truncation test asserts the style flag and the width.** Adequate: `text_ellipsis_start` sets `TextOverflow::TruncateStart(ELLIPSIS)` (gpui `styled.rs:97-98`) and the text element honours it with `TruncateFrom::Start` (`elements/text.rs:679`); which glyphs survive is gpui's, and the three slot widths (900 / 220 / 160) prove the element is measured against the live width. The owner sees the `…\src\panel` shape in the walk.

### Everything else on Part A

- **Geometry.** 10 + 24 + 20 + 10 = 64 (`TabRowLayout` constants, asserted); the `Tab` is `h(ROW_HEIGHT)`, the row `v_flex` is `py(10)` with fixed line heights, so nothing depends on font metrics. The grip glyph is offset to line 1's centre (`tab_rail.rs`, asserted at 22 px).
- **Line 2 layout.** `[path][flex_1 spacer][status]`: the path has `min_w_0` and no `flex_1`, so it is content-sized and shrinks (and is cut) before the spacer, keeping the icon at the end whether or not a path exists; the icon is `flex_shrink_0`. Tested for cwd+status, status only, and neither.
- **Source and live update.** `tab_cwd` reads the active Space's `TerminalSession::cwd()` (already sanitised by the security policy); `SessionEvent::Cwd` now emits `TerminalViewEvent::CwdChanged`, and `cwd_event_emits_cwd_changed` / `a_reported_directory_repaints_the_row` cover the chain. `FakeSessionProbe::set_cwd` is test support only.
- **Strip unchanged.** The horizontal branch keeps chips, dot, status, label, `x` in that order and its 100-220 px band with the `-5 px` margin; `h_flex` carries the `items_center` the old row had; `the_horizontal_row_is_content_sized_as_before` still passes.
- **Contrast.** Line 2 is `muted.foreground` on the two row fills, pairs the script already measures.
- **Records.** HLD (Rail anatomy rows, wireframes, Interactions), `IN-0055.md`, `docs/gui-layout.md` and the packet's assumptions (a)-(d) match the code.

### For the owner (Part A)

- A Git Bash local tab reports `/c/Users/...`; it is shown exactly so, with `/` and no `~`. Say if it should read `~\...` like a PowerShell tab (then `cwd_display` learns the `/c/` form).
- The walk items the packet lists (both themes, start truncation while the path changes and the window narrows, `~` on a local tab, a remote path on SSH, the grip on line 1, the icon clear of the scrollbar with 40 tabs).

Gate for both parts: see `US-0197-verify.md` (same run, five crates).

## Advisor code review (rework 4)

Head `e5f98b62` (three commits on main `ab69703e`), reviewed on `verify/in-0055-rework4` in the coder's worktree, 2026-10-21. Covers the `US-0196` Rework 4 and `US-0198` Rework 4 entries together (one diff).

### Verdict: PASS

No defect in the code; two points for the owner (1 and 3 below). No pinned test added: nothing found that a test could flip when fixed.

### Gate (one crate set per command, `CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=2`, worktree target reused)

- `cargo test -p oneterm-terminal-view -p oneterm-workspace -p oneterm-settings -p oneterm-state`: 659 passed, 7 ignored (8 suites).
- `cargo clippy` on the same four crates, `--all-targets -- -D warnings`: clean.
- `cargo fmt --all -- --check`: clean. `python scripts/check-english.py`: 1140 files pass. `python scripts/check-doc-paths.py`: 219 paths pass.
- `python scripts/check-ignored-tests.py`: not re-run (its whole-workspace `cargo test --list` ran out of memory on this machine); the diff adds no `#[ignore]`, so the census is unchanged. The coder's full `ci-local.ps1` run stands for it.

### The coder's attack list

1. **Zoom stranding: Medium, owner decision, not a code defect.** With the rail drawn there is now no pointer path to zoom in or out: the rail has no button (`tab_rail.rs:452-477`, the header holds only the `+`), the rail draws no `...` menu, the rail rows have no context menu of their own (no `context_menu` / `popup_menu` in `tab_rail.rs`), and no application menu carries `ToggleZoom` (`app_menus.rs` has no zoom entry). The only way in or out is the `ToggleZoom` key binding (default `shift-escape`, `key_bindings_actions.rs:64-70`, allowed in an elevated window, `elevated_policy.rs:85`). Zoom is persisted (`docks.json` `zoomed_panel`), so a user who zoomed on the horizontal strip, then switched Vertical Tabs on, or who restarts into a zoomed layout, sees a zoomed terminal with no visible way back. The owner asked for the button's removal, so this is the consequence to confirm, not a defect. Fix shape if wanted: a `Zoom Out` row in the terminal tab's own context menu while `group.is_zoomed()` (the menu `tab_title.rs` already draws, the rail re-uses it), or a `View > Toggle Zoom` menu item; either is a few lines and keeps the header bare.
2. **Status-bar toggle gated on the switch only: Low, documented choice.** `build_status_bar` shows the button on `UiConfig::read_vertical_tabs(cx).enabled` alone (`statusbar.rs:243-244`). A centre group that is not all-terminal (a panel dragged into the centre; `draws_rail`, `tab_rail.rs:97-107`) draws the strip, yet the bar still offers Collapse / Expand, and a click flips the persisted `collapsed` with nothing on screen moving. The `US-0198` Rework 4 entry records this as deliberate (the button is a switch for the stored preference). Fix shape if the owner minds: also require that some centre group draws the rail (the bar holds `dock_area`; `layout(DockPlacement::Center)` and `panel_names` are enough).
3. **The OSC 7501 icon claim: verified; "it animated before" is not explained by this change or by the kit bump.** The animation code is the `701a4b16` (2026-10-15) version, untouched; the new `the_working_icon_animates_in_every_row_layout` passes for the strip, the vertical row and the collapsed cell. Reduced motion: `gpui-base` `reduce_motion.rs` reads `SPI_GETCLIENTAREAANIMATION` once at `init` (Windows `follow` is a no-op, `reduce_motion.rs:141`) and `AnimationElement::request_layout` draws one static frame under the flag (`gpui-pre-0.3.9` `elements/animation.rs:407-413`). That file exists in `gpui-base` 0.7.0, 0.7.1 and 0.7.3 and not in 0.6.0; the 0.7.0 bump (`5a5bcff1`, 2026-09-28) is an ancestor of the animation commit, so the icon was born under a reduce-motion-aware kit and the bump cannot be what changed. The repo's own walk record already says it: `US-0193-tab-status-icon.md:94`, "The icon was static on screen: this machine has Windows animation effects off". Other explanations for having seen it animate: the flag is read once per launch, so an instance started while Animation effects were on kept animating until it was closed (the setting is also turned off by Performance Options "Adjust for best performance", by some Remote Desktop experience profiles and by some power-saver profiles); or it was seen on another machine or user account. Recommendation: the owner checks Settings > Accessibility > Visual effects > Animation effects; if it is on and the icon is still static, that is a new bug. Cheap aid either way: one `log::info!` of `cx.reduce_motion()` after `gpui_component::init` in `crates/app/src/init.rs`, so a walk log says which state the app read.
4. **28 px chrome vs the real layout: verified.** An icon-only `xsmall` kit button is `size_5` (`gpui-component-0.7.3` `button/button.rs:674`), 20 px; the bar's left region is an `h_flex` with `gap_2` (`status_bar.rs:84`), 8 px; the test pins both edges (`statusbar.rs:519-528`).
5. **Right-edge presses vs the 4 px handle: by design.** The handle is `right(-1)` and 4 px wide (`tab_rail.rs:271-272`), so it covers the rail's last 3 px and a press there starts a resize, not a selection. The new test's smallest offset is 5 px (`tab_rail_tests.rs:1246`), which is the first pixel a press can select at.
6. **The scrollbar-click test against the old code: done, it discriminates.** With `.vertical_scrollbar(&self.scroll_handle)` put back on the expanded list (local patch, not committed), `a_click_across_an_expanded_row_of_an_overflowing_rail_selects_it` fails at `tab_rail_tests.rs:1259` ("5 px from the edge: left 0, right 5": the track took the press and no tab was selected) while `forty_tabs_scroll_and_the_active_one_is_brought_into_view` still passes. The file was restored to `e5f98b62` afterwards (`git status` clean).

### Everything else

- **Cached status bar follows the setting.** All three `vertical_tabs` setters end with `cx.refresh_windows()` (`ui_config.rs:462, 483, 499`); a cached view re-renders while `window.refreshing` (`gpui-pre-0.3.9` `view.rs:534-535`), so the button appears, flips and disappears without the bar observing the config. The test drives the Settings switch's path by hand (`cx.refresh_windows()` in its `draw`), which matches what `set_vertical_tabs_enabled` does.
- **Dead code.** None: `ZoomButton`, `zoom_button`, `COLLAPSED_HEADER_HEIGHT`, `RowPlace::gutter`, `TabRail::overflowing` and the `Scrollbar` / `v_flex` / `Button` / `IconName` / `ToggleZoom` imports left with their uses; clippy is clean.
- **Crate rules.** No new edge: `oneterm-workspace` already used `oneterm-settings` (`layout/workspace/mod.rs:351`).
- **Records.** `US-0196` Rework 4, `US-0198` Rework 4, `IN-0055.md`, the HLD (wireframes, rail anatomy, Zoom row, collapse step) and `docs/gui-layout.md` (anatomy, header, scrolling, Collapsing, Status bar) all describe the code as it is: 32 px header in both states, `+` right-aligned / centred, no scrollbar, status-bar button left of the clock, gated on the switch.

### For the owner

1. Confirm the zoom consequence in point 1 (key binding only while the rail is drawn), or pick one of the two small affordances.
2. Decide the icon question in point 3 after checking the Windows Animation effects switch.
3. The ui-verifier walk items the packets list: header in both states and two themes, 40 tabs without a scrollbar, the status icon clear of the row's edge, the status-bar button next to the clock (two themes, tooltip, elevated window, narrow window).

## UI verification (rework 4)

Build: copy of the debug binary at head e5f98b62, private profile and cwd, window 1280x800 (also resized down to 900 and 640; the window will not go below ~656 px). Coordinates below are screenshot (full-window) space unless marked client; client = screenshot x minus 8, y unchanged. Screenshots: `<scratchpad>\uivrw4-shots\uivrw4-*.png`.

| # | Item | Result | Evidence |
|---|---|---|---|
| 1 | Rail header holds only `+`, 32 px (y 34..65); expanded `+` right-aligned (x 209), collapsed centred (x 28); no zoom button in rail; strip (Vertical Tabs off) still has zoom | PASS | 01-launch, 09-collapsed, 17-vt-off |
| 2 | Status bar button first in the left group before the clock; tooltip "Collapse Tabs" (PanelLeftClose) / "Expand Tabs" (PanelLeftOpen); click flips rail live; hidden when Vertical Tabs off | PASS | 08, 10, crop-08, crop-10, 17-vt-off |
| 2b | Narrow widths: nothing overlaps at 900 / 656 expanded and collapsed. The centre breadcrumb could not be exercised (child shells die here, no cwd) | PASS (breadcrumb NOT WALKED) | 12-narrow-exp-*, 13-narrow-coll-* |
| 3 | 40 tabs: no scrollbar expanded or collapsed; wheel scrolls; selected tab revealed after expanding (tab 40 at bottom) and collapsing (17 and 29 visible); row content uses the full width | PASS | 05, 06, 09, 13-narrow-coll-640 |
| 3b | Clicks select the right row: expanded at 5 px from the right (client x 214), centre, left (client x 3); collapsed at client x 38, 20, 1, 42 | PASS | 07-click-*, 11-collclick-* |
| 4 | Collapsed persists across restart (`ui_config.json` has `collapsed: true`; relaunch shows the strip); light theme collapsed and expanded legible, tooltip themed | PASS | 14, 15, 16 |

Defects: none blocking.

1. (Info, by design) Expanded rail: a click at client x 216 (screenshot x 224), inside the 4 px resize handle over the right border, does not select a row. 5 px in works.

Not walked: OSC 7501 icon and animation, cwd line, centre breadcrumb yielding to the buttons (no cwd without a live shell), hover-only states beyond tooltips, High-DPI.

Owner-only checks: with a real shell, narrow the window until the breadcrumb is long and confirm it truncates before touching the new button and the clock; check the OSC 7501 icon and cwd line in a row; try keyboard focus on the status bar button.

## Advisor code review (rework 7)

Coder head `e033390c` (three commits on `main` @ `4272956d`), reviewed on `verify/in-0055-rework7` in the coder's worktree with its target reused. Covers `US-0196` Rework 7 and `US-0198` Rework 7 together (one diff).

### Verdict: PASS, with one record correction required (finding 1) and one pre-existing latent bug pinned (finding 2)

No defect in the rework's own code blocks acceptance. The records state a root cause that does not reproduce; the owner's question about `list_active` has a different answer than the packet gives.

### Gate (one crate set per command, `CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=2`)

- `cargo clippy -p oneterm-workspace -p oneterm-terminal-view -p oneterm-state --all-targets -- -D warnings`: clean.
- `cargo test -p oneterm-state --lib`: 77 passed. `cargo test -p oneterm-workspace --lib -- tab_rail`: 40 passed (41 with the advisor test). `cargo test -p oneterm-terminal-view --lib -- panel::tests` and `-- hover_card right_press`: all passed (the six hover-card tests included). `cargo test -p oneterm-theme --lib`: 8 passed, 1 failed, the failure being the advisor pin of finding 2.
- `python scripts/check-theme-contrast.py`: 2028 pairings, 702 hierarchy comparisons, the five accepted `KNOWN_BELOW` rows, nothing new; `--self-test` passed. `cargo fmt --all -- --check` and `python scripts/check-english.py` pass.
- Disk: 4.33 GB free before, 3.83 GB after (the `oneterm-theme` lib-test profile rebuilt `gpui-component`); no `cargo clean`.

### Findings

1. **Medium, records.** The packet (`US-0196` Rework 7 "Choices the request left open" and "Defect found by the coder's own screenshot"), the HLD rows row, `docs/gui-layout.md` (colours paragraph), `crates/workspace/src/layout/workspace/tab_rail.rs:110-111` and the comment in `scripts/check-theme-contrast.py` all state that "a theme change after OneTerm's list-style override resets `Theme::list_active`". That does not reproduce. Every theme path the app has re-applies `apply_list_style_override` right after the change: `oneterm_theme::init` (`crates/theme/src/theme.rs:238-239`, `288-289`, `316`), `SwitchThemeMode` (`251-252`), the Appearance page (`crates/settings-ui/src/appearance.rs:85-86`, `162-163`); `UiConfig::observe_theme` (`crates/settings/src/ui_config.rs:413`) only persists; nothing in `crates/` or the kit calls `Theme::sync_system_appearance`. The kit's `Theme::edit` reconcile (`reference/gpui-kit/crates/component/src/theme/theme_color.rs:371-386`) leaves an untouched field alone. Advisor test `crates/theme/src/theme.rs` `list_active_is_list_hover_after_every_app_theme_path` drives all four paths (init, `Theme::change` + override, `apply_config` + override, `Theme::update`) and passes: `list_active == list_hover` after each. The `#21252b` sample of the first build (a fifth of the way from the rail to `list_hover`, which is exactly the kit's `clamp_alpha(..., 0.2)` of `list_active` at `schema.rs:1025-1043`) is therefore unexplained by the stated cause; the first build (`cc1f73a1`) read `theme.list_active` (the colour, not the token), which the override does set. Reading `list_hover` remains the right choice for the stated owner decision (selected = `list_hover`) and for the proven reason that `tab.active.background` equals the terminal's `background` in both Zed One themes. Fix shape: in the five places, replace the reset claim with "`list_hover` is the owner's choice; `list_active` is the kit's token for the same thing but is clamped to 0.2 alpha by `apply_config` and the override's write does not reach the kit's `tokens` (finding 2), so the rail reads the colour it means directly"; drop "a theme change resets it" and "found by sampling pixels" as a root cause, or re-sample the first build before keeping it.

2. **Low, pre-existing latent bug (out of this packet; new BUG candidate), pinned.** `apply_list_style_override` (`crates/theme/src/theme.rs:69-78`) writes `Theme::list_active`, `table_active` and `table_active_border` through `Theme::global_mut`, i.e. the `colors` only. GPUI Kit 0.7 paints the data table's selected row and selected cell from `cx.theme().tokens.table_active` (`reference/gpui-kit/crates/component/src/table/state.rs:2279`, `:2106`, `:1443`), and `tokens` are synced from `colors` only inside `Theme::update` (`theme/mod.rs:298-335`). So after every theme application the kit's token keeps its own derivation at 0.2 alpha and the SFTP tables (`row_selectable(true)`, `crates/sftp-ui/src/panel.rs:152`, `local_pane.rs:500`) get the kit's overlay on top of OneTerm's `render_tr` highlight, which the comment at `crates/sftp-ui/src/table_delegate.rs:339-341` believes is disabled. `tokens.list_active` is likewise left at the clamped value (no kit reader outside the theme today; `ListItem` reads the colour). Pinned by `crates/theme/src/theme.rs` `the_list_style_override_reaches_the_tokens_the_kit_reads`, which fails on this head (`tokens.table_active.color.a` is 0.2, expected 0). Fix shape: perform the override's writes inside `Theme::update(cx, |theme| { ... })` so the kit's reconcile copies them to the tokens (keep `record_selection_accent` before it), or set `theme.tokens.table_active` / `tokens.list_active` alongside the colours.

3. **Low, visual (this packet).** A selected collapsed cell with a status fill now shows two 3 x 8 px notches of plain row background at its top-left and bottom-left: the fill still starts at `ACCENT_BAR_WIDTH` (`crates/terminal-view/src/panel/tab_title.rs:511-519`, `left(px(3.))` when selected) because the pill is painted before the title (`tab_rail.rs:702-713` then `:722`), while the pill itself is now inset 8 px top and bottom. Fix shape: paint the pill after the title (`.child(title)` then the pill child) and let the fill run the cell's full width, so the pill sits on the fill as it does on the selected fill. Not pinned: the layout is the owner's to choose.

4. **Info.** Collapsed-cell drag split: correct by test (`a_collapsed_cell_reorders_from_the_cell_and_drags_the_tab_from_its_icon`), and the gpui dispatch order makes the icon's `DragTerminalTab` win where it is (innermost listener first, `stop_propagation` in `start_tab_drag`). Discoverability: the open-hand cursor is set on the whole cell (`tab_rail.rs:664-666`), including the 16 px icon whose drag goes somewhere else; the packet records the icon-vs-margin question as open for the owner. No action.

### The coder's attack list, verified

- **Token mapping and alpha compositing (script vs runtime).** Runtime: the base `Tab` is unstyled (`reference/gpui-kit/crates/base/src/tabs.rs:150-176`, no default background), so a hovered row is `list_hover.opacity(0.5)` over `tab_bar` and the selected row is `list_hover` over `tab_bar`; `Hsla::opacity` multiplies alpha. Script: `tab_bar.hover.background = ("alpha", "list.hover.background", 0.5)` multiplies the stops' alpha and `surface()` composites over `PARENTS` = `tab_bar.background`; `tab_bar.selected.background = list.hover.background` over the same parent. The two models agree. The workspace test `selected_outranks_hover_outranks_idle_in_both_zed_themes` applies the real theme configs rather than the override.
- **Selected-row hover suppression.** The `group_hover` fill child exists only for `!selected` (`tab_rail.rs:716-731`); the row's own `hover` sets text colour only. Verified by `an_unselected_row_has_a_hover_fill_over_the_whole_row` (no fill on row 0, fill equals the row on row 1).
- **Drop line without dividers.** `DropEdge::Above` is `border_t_2`, `Below` `border_b_2`, in `drag_border` on the row itself (`tab_rail.rs:677-681`); the row has no other border now, so the old `border_b_0` is rightly gone. The absolute children (pill, hover fill, grip) are positioned in the padding box, so the 2 px line stays visible above or below them. Order and slot are tested; the line's look is for the ui-verifier.
- **Grip overlay vs title drag.** The grip is an 8 px absolute strip painted after the title (`tab_rail.rs:598-630`, `:723`), so its `DragPanel` listener runs first in the bubble phase and takes x 0..8; x 8..14 is the title row's own padding and starts the tab drag. The glyph box is shifted 2 px left so its dots fall at about x 1..7. Verified by `the_grip_overlays_the_row_and_no_divider_cuts_it` and `the_title_row_drags_into_a_space_and_only_the_grip_reorders`.
- **Hover card placement.** gpui-pre 0.3.9 puts a tooltip at `mouse_position + (1, 1)` (`window.rs:3816-3817`), reads the pointer at build time (`elements/div.rs:3928`, the same call the builder reads), flips only by the root's measured size (`window.rs:3823-3845`), and clears the tooltip on a press or a wheel scroll (`div.rs:3745-3750`, `:3795-3800`), so a scrolled strip rebuilds the card with fresh cell bounds. With a `size_0` root the flip never fires: the card can leave the window only if the window is narrower than 44 + 7 + 280 = 331 px (the minimum is 640, `crates/app/src/window.rs:38-41`) and vertically it stays within 1 px of the 44 px cell. Positions are logical pixels throughout, so DPI does not enter. Verified by the two coder tests plus the advisor test `the_hover_card_is_not_drawn_while_a_right_press_hides_it` (the `hidden` flag path through the new `CellTooltip`).
- **Header at 140 px.** Advisor test `the_header_label_and_the_plus_fit_the_narrowest_rail`: at the floor the label ends at least 4 px before the 24 px `+`, the `+` keeps its 8 px right inset, the label starts at the row's 14 px.
- **Dead code.** `VERTICAL_ROW_DIVIDER`, `vertical_first_line_centre`, `report_cell_fill` / `take_cell_fill`, `grip_glyph_color` and the `CellFill` global are gone from code and tests; no reference remains in `crates/` (grep). Clippy is clean, so no unused import.
- **Tests.** Not constant swaps: the rail tests assert the title element's full width and height, the pill's inset, the fill equal to the row, the label and the 24 px `+`; the terminal-view tests assert centring without a directory, line centres with one, the 16 px icon, the card's one and two line heights and the 280 px cap; the probe cell was remodelled on the real one (icon-only tab drag).
- **Records.** `US-0196` / `US-0198` Rework 7, the HLD and `docs/gui-layout.md` match the code except finding 1 and one stale sentence: `docs/gui-layout.md:149` still says "The fill never lies under the 2 px accent bar of the selected cell" (3 px pill now; see also finding 3).

### Tests added (verify branch)

- `crates/theme/src/theme.rs`: `list_active_is_list_hover_after_every_app_theme_path` (passes; finding 1), `the_list_style_override_reaches_the_tokens_the_kit_reads` (fails; finding 2 pin).
- `crates/workspace/src/layout/workspace/tab_rail_tests.rs`: `the_header_label_and_the_plus_fit_the_narrowest_rail` (passes).
- `crates/terminal-view/src/panel/tests/tab_menu_and_card.rs`: `the_hover_card_is_not_drawn_while_a_right_press_hides_it` (passes).

### For the owner

1. Finding 1: have the coder correct the five records (and the two code comments) before merge; decide whether the first build's pixel sample is re-taken or dropped.
2. Finding 2: open a BUG for the override not reaching the kit's tokens (SFTP selected-row overlay), outside this intake.
3. Finding 3: say whether the pill should sit on a full-width status fill (notches gone) or the notches are acceptable.
4. ui-verifier: the drop line on rows without dividers (above and below, on the selected row too), the selected filled cell's corners, the card at the bottom-most cell of an overflowing strip.

## UI verification (rework 7)

Build: debug binary copy at head e033390c, private profile (scratch USERPROFILE and cwd), Zed One Dark and Zed One Light, window 1280x800 (shot x = client x + 8). Measured with pixels. Screenshots (prefix `uivrw7-`): `C:\Users\trunglt\AppData\Local\Temp\claude\D--TrungKFC-Research-Rust-myTerm2\fa2d0135-9c28-4a69-a255-ba7479e604cb\scratchpad\vt-rework7-walk`.

| Item | Result | Evidence |
|---|---|---|
| Rows 48 px, line 1 centred without a directory | PASS (y66..113 = 48 px; title ink centre 138.5 vs row centre 137.5) | d01, d03 |
| No row dividers | PASS (rows touch; only the rail's right border #3e4451) | d02, d15 |
| Selected > hover > idle, dark | PASS: rail #1e2227, hover #252a32, selected #2c313c; idle title #8f96a3, hover and selected title #ffffff | d03 |
| Selected > hover > idle, light | PASS but weak: rail #f0f0f1, hover #eaeaeb, selected #e5e5e6; idle title #5d677a, hover and selected #383a42 | l02 |
| Hover on the selected row changes nothing | PASS (fill stays #2c313c dark / #e5e5e6 light) | d03, l02 |
| 3 px pill, inset 8 px | PASS (x client 0..2, y 74..105 in a 66..113 row, #528bff) | d01 |
| Grip: hover-only overlay, far left, clear of title | PASS (ink x client 1..6, y centred on the row; title ink starts client 15; 8 px gap) | d03, d03z |
| Drag from x 0 and x 4 reorders; drop line drawn | PASS (blue 3 px line on the row boundary) | d04, d05 |
| Title-row drag is the tab drag, not a reorder | PASS as far as walkable (different ghost, no reorder); dropping into an empty Space NOT WALKED (no empty Space made) | d06 |
| Header "Terminals" + 24 px `+`, no bottom border | PASS (label x client 13..64, `+` glyph centred 198.5; no line under the header) | d03z |
| Header at 140 px | PASS (label and `+` both fit, no clipping) | d07-narrow140 |
| Collapsed: icon 16 px, centred, no grip | PASS (ink 15 x 12 px, centred on the 43 px cell both axes; no grip in cells) | d08 |
| Card: 6 px gap, centred, rounded, shadow, one line | PASS (border x58 vs strip border x51 = 6 px; card centre y equals cell centre; dark and light; first and last cell) | d09, d17, l03 |
| Right-click hides the card, menu opens | PASS | d10 |
| Cell-margin drag reorders; icon drag is the tab drag | PASS | d11, d12, d13, d14 |
| Click selects (expanded, collapsed), context menu, status bar toggle | PASS | d10, d18, d16 |
| Two-line row with a directory, status icon at line 2's end | NOT WALKED: no directory reaches the rail in a private profile (no OSC 7 from the shells; typed input did not render in the pane) | |
| Card clipped for the cell at the window bottom | NOT WALKED: 15 cells fit without reaching the bottom (last cell y682..725 of 763) | d17 |
| 15 tabs, both themes | PASS (list scrolls; the top row is cut by the header edge) | d15, l04 |

### Defects

1. LOW, light theme, rows and cells. The three steps are 5 to 6 levels apart (#f0f0f1 / #eaeaeb / #e5e5e6), so a hovered row is almost invisible and the selected row reads mainly through the pill and the darker title. Steps: hover any idle row in Zed One Light (l02).
2. LOW, dark theme, same rows. Steps are 7 levels (#1e2227 / #252a32 / #2c313c); hover is visible but faint (d19).
3. LOW, hover and selected share the full-strength title colour (#ffffff dark, #383a42 light), so only the fill and the pill tell them apart, and the idle to hover change is a large jump.
4. LOW, a right-clicked row (menu open) keeps the bright title but loses the hover fill (d18).
5. INFO, with the header border gone a scrolled row is cut flat under the header (d15, l04 at y66).

### Owner-only checks

A tab with a directory (two lines); an empty-Space drop of a title-row or icon drag; real-host status icons; the card on a cell touching the window bottom.
