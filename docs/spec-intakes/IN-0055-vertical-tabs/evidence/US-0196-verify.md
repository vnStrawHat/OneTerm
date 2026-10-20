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
