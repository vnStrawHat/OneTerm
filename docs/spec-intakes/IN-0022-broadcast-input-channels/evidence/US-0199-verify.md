# Advisor code review (rework 5 + US-0199)

Date: 2026-10-21. Reviewed head: `dbacc969` (three commits on `main` @ `9ae95ff4`): `e20c5057`
(US-0199 packet), `897f8e7f` (IN-0055 rework 5: status bar separator, row hover fill),
`dbacc969` (collapsed cell hover card, Input Channel in the tab menu). Verify branch
`verify/in-0055-rework5-us-0199` in the coder's worktree.

## Verdict

**PASS** with two record corrections to make before merge (findings 1 and 2 are text: a code
comment and two record paragraphs; no product code changes). Findings 3 to 6 are low.

## Gate, re-run one crate set per command

| Command | Result |
| --- | --- |
| `cargo clippy -p oneterm-terminal-view -p oneterm-workspace -p oneterm-settings -p oneterm-state --all-targets -- -D warnings` | clean |
| `cargo test -p oneterm-terminal-view -p oneterm-workspace -p oneterm-settings -p oneterm-state` | settings 56, state 77, terminal-view 418 (3 ignored), workspace 111 (3 ignored); all pass |
| `python scripts/check-theme-contrast.py` | 1716 pairings, all >= 4.5:1; hierarchy 663 comparisons pass |
| `python scripts/check-english.py` | 1143 files pass |
| `python scripts/check-doc-paths.py` | 219 paths pass |
| `cargo clippy -p oneterm-workspace --all-targets -- -D warnings` after the pinned test | clean |

`CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2`, the worktree's own `target`.

## The coder's weakest points, attacked first

1. **Hover fill as an absolute child inside the kit Tab.** `crates/workspace/src/layout/workspace/tab_rail.rs:666-678`.
   The row is `gpui_base::Tab` (`tab_rail.rs:23`), `relative()` (`:611`), `overflow_hidden`
   is not set by the base Tab, and the fill is the first child, so it paints under `lead`,
   `title` and `trail` (`:679-681`) and under the collapsed grip overlay. The inset is the
   padding box, so the row's 1 px bottom divider and the 2 px drag-over lines
   (`:627-645`) stay outside it; the workspace test pins `fill.bottom + 1 == row.bottom`,
   expanded and collapsed. The selected row gets no fill (`:670`). No layout shift: absolute
   children take no flex room. Sound.
2. **`Tooltip::element` reading the panel in a draw.** `crates/terminal-view/src/panel/tab_title.rs:816-828`.
   The builder runs inside the kit `Tooltip::render` (`reference/gpui-kit/crates/component/src/tooltip.rs:94-131`),
   which gpui draws in the tooltip pass after the root element's prepaint; the panel's
   lease (`render_tab_strip` runs inside the panel's render) is closed by then, and the
   tooltip-builder closure itself only creates `CellTooltip`. No re-entrancy. The content is
   rebuilt on every tooltip draw from the live panel, so a title, directory or status change
   while hovering is shown on the next frame. Cost: `tab_channels`, `breadcrumb_label`,
   `tab_status` once per frame while a cell is hovered; negligible. `TOOLTIP_CHROME_X = 18`
   matches the kit's `border_1` + `px_2` (`tooltip.rs:118,124`) for the pinned 0.7.3.
3. **The bell contrast exclusion.** Reproduced: with `tab_bar.hover.background` added to the
   `warning.background` surfaces, 11 variants fail (worst Hybrid Dark 4.01:1, Adventure Time
   4.02:1), none below 3:1. See finding 2 for why the exclusion as written is inconsistent
   with the script's own contract.
4. **US-0199 semantics.** `crates/terminal-view/src/panel/channel_menu.rs:389-474`. The menu
   closure captures the right-clicked tab's `Entity<TerminalPanel>` (`tab_title.rs:835-844`),
   and every item resolves `self.active_view()` of **that** panel (`terminal_panel.rs:431`,
   the tab's own Space tree), not the workspace's focused terminal, so an inactive tab is
   addressed correctly; the kit runs the handler **instead of** the action when both are
   set (`reference/gpui-kit/crates/component/src/menu/popup_menu.rs:862-872`), so the
   `.action(..)` hint cannot double-apply to the focused terminal. Membership stays per
   Space (`DEC-0009`): the Join rows call `TerminalView::join_channel` on one view. No
   remaining reader of `MenuContext.{channel,tab_spaces,tab_has_member}` or
   `BroadcastOrigin::channel` in `crates/` (search of `crates/` for `tab_spaces`,
   `tab_has_member`, `origin.channel(`); `TerminalView::channel` stays for
   `close_active_channel` (`terminal_panel.rs:523-537`). The key-bound actions and the
   Key Bindings catalogue (`crates/settings-ui/src/key_bindings/key_bindings_actions.rs:286-345`)
   are untouched.

## Findings

### 1. Medium. The recorded root cause of the missing hover fill is false

- `crates/workspace/src/layout/workspace/tab_rail.rs:662-665` (comment), `docs/spec-intakes/IN-0055-vertical-tabs/US-0196-vertical-tab-rail.md` "Rework 5" "Root cause" paragraph.
- Claim: the kit's `Tab::render` registers its own `.hover(..)` after the row's, and gpui's
  `hover` replaces the earlier style, so the row's `.hover(text_color)` never set a background.
- Fact: the row is `gpui_base::Tab` (`tab_rail.rs:23`), not `gpui_component::tab::Tab`. The
  base Tab's `render` (`reference/gpui-kit/crates/base/src/tabs.rs:150-182`) registers no hover
  style. The row's own `.hover(..)` at `tab_rail.rs:626` is the live one and is what changed
  the title colour; the background never changed because that closure never set one.
- Independent reproduction: gpui-pre 0.3.9 `src/elements/div.rs:842-848` carries
  `debug_assert!(hover_style.is_none(), "hover style already set")`, active in this build
  (`cargo test` profile). A kit that registered a second `.hover` would panic every rail
  test. Pinned by `a_second_hover_on_a_base_tab_panics_so_the_kit_cannot_replace_the_rows`
  (`crates/workspace/src/layout/workspace/tab_rail_tests.rs`, `should_panic`).
- Consequence: the fill works, but for a reason the comment and the record do not give,
  and the absolute child plus its stateful id exist only because of the false premise:
  `.hover(move |s| if selected { s.text_color(..) } else { s.text_color(..).bg(hover_fill) })`
  on the row would have done it with no extra element (the hover style is the element's
  own background, under children and inside borders). Either shape is acceptable; the
  record is not.
- Fix shape: rewrite the comment and the record's "Root cause" to "the row's hover style
  set only the text colour"; keep the absolute fill if the debug-bounds test is wanted
  (a hover `bg` is not observable through `debug_bounds`), and say so. Delete the pinned
  test when done.

### 2. Medium. The bell exception contradicts the contrast script's own contract

- `scripts/check-theme-contrast.py:228-270` (the `warning.background` entry), `AGENTS.md` section 3.4.
- The entry already measures the bell at 4.5:1 on `tab_bar.background`, `tab.background` and
  `tab.active.background` as "a glyph read as a marker" (`:246-248`), so the file has decided
  the bell is text-grade on tab surfaces. The new comment (`:267-270`) calls the same glyph
  non-text on the one surface where it fails, and `AGENTS.md` section 3.4 says a listed
  token drawn on an unlisted background gets the surface added. The fill is drawn under
  the bell on every hovered unselected row with a Blocked status.
- Reproduction: add `"tab_bar.hover.background"` to the tuple; the script reports 11
  variants between 4.01:1 and 4.48:1 (Adventure Time, Ayu Light, Catppuccin Latte,
  Everforest Light, Fahrenheit, Flexoki Light, Hybrid Dark, Mellifluous Light, Molokai Dark,
  Solarized Dark, Zed One Light).
- Owner decision, three shapes: (a) treat the bell as non-text on **all** surfaces and give
  the script a 3:1 non-text tier for it (consistent, one new constant); (b) keep 4.5:1 and
  retune `warning.background` in the 11 variants; (c) accept the exception as written and
  record it as a decision, not a script comment. The rework record already states the
  numbers; what is missing is the inconsistency with lines 246-248.

### 3. Low. Stateful id on the hover fill

- `tab_rail.rs:669`: `.id(("tab-rail-hover", ix))`. `debug_selector` and `group_hover` need no
  element state; the id adds a per-row state entry and a hit-test participant for nothing.
  Drop the id.

### 4. Low. Record claims a test assertion that does not exist

- `docs/spec-intakes/IN-0055-vertical-tabs/US-0198-collapsible-rail.md` Rework 5 acceptance:
  "test draws the card: title, directory, status icon, and a chip and the recording dot are in
  it". `the_collapsed_cells_hover_card_has_the_expanded_rows_content`
  (`crates/terminal-view/src/panel/tests/tab_menu_and_card.rs:1050-1102`) asserts the title,
  the directory and the status icon only; `channel_chip` has no debug selector. Reword the
  line, or add a selector and the two assertions.

### 5. Low. Known loss is an owner call, not a coder call

- `channel_menu.rs:400-409`; `US-0199` Plan "Which Space, in a split tab". A non-active Space of
  a split tab can no longer be put in a channel from any menu (focus it first, or use the
  tab-wide rows). `IN-0022.md` "Requested Outcome" (a split tab holding members of one channel,
  of different channels and non-members at once) is still reachable, with one more click per
  Space. The packet records it as "decided here, for review"; the owner must confirm. Fix
  shape if refused: per-Space rows in the tab submenu for a split tab (`Space 2 > Channel A`).

### 6. Info. What the tests do not cover

- The menu wiring (`channel_submenu` -> `channel_item` -> `on_click`) is not exercised; the
  US-0199 test calls `apply_channel_item` directly (`PopupMenu` items are private). The hover
  card is drawn by a test host, not through the tooltip path. Both are for `ui-verifier`.
- `TOOLTIP_CHROME_X` is a kit-internal number; re-check on the next kit bump.

## Owner must still check by hand

- Hover a row in two themes: whole-row fill, title colour, divider and accent bar; the selected
  row unchanged; a collapsed cell likewise; a drag over a row keeps its drop line.
- The hover card on a collapsed cell: chips, a long directory cut at its start, the pulsing
  bell, hidden while the cell's context menu is open.
- The status bar separator between the toggle and the clock, in a light and a dark theme.
- The tab menu's "Input Channel" from a horizontal tab, a vertical row and a collapsed cell;
  right-click an inactive tab and confirm the chip appears on that tab; the terminal menu
  without the submenu.
- Finding 2: which of (a), (b), (c).
