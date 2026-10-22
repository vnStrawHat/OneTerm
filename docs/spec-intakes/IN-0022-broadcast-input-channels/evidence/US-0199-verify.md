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

## Advisor re-review

Date: 2026-10-21. Reviewed head: `2bfa0b69` (`081b425f` findings 1, 3, 4; `2bfa0b69` findings 2
and 5, owner rulings), same branch.

**Verdict: PASS.** Findings 1 to 5 are closed; two new low notes below, neither blocking.

Gate: `cargo clippy -p oneterm-workspace --all-targets -- -D warnings` clean;
`cargo test -p oneterm-workspace` 111 passed, 3 ignored (the pin is gone, as asked);
`check-theme-contrast.py` 1755 pairings / 429 rows, all at their floor, 663 hierarchy
comparisons; `--self-test` passes; `check-english.py` 1144 files; `check-doc-paths.py` 219
paths. No other crate changed.

Measured-pair set, before (`6952618c`) vs after, computed from both modules' tables:
removed `warning.background` x {`tab_bar.background`, `tab.background`, `tab.active.background`}
at 4.5:1; added the same three plus `tab_bar.hover.background` at 3.0:1. Nothing else: every
other `SURFACES` pair, `HIERARCHY`, `PARENTS` and `FALLBACKS` are identical. So the only
loosening is the bell on the three tab surfaces, which is the owner's ruling (one rule for
the glyph everywhere), and the text uses of `warning.background` stay at 4.5:1. The worst
glyph row is still 4.01:1 (Hybrid Dark on the hover fill), well above 3:1.

Finding 1 text: the comment at `tab_rail.rs:665-668`, the US-0196 Rework 5 "Root cause" and
`docs/gui-layout.md` now say the row's own hover set only the text colour and that gpui
refuses a second `hover` on one element; both statements are true (`gpui_base::Tab::render`
registers no hover, `reference/gpui-kit/crates/base/src/tabs.rs:150-182`; gpui-pre 0.3.9
`div.rs:842-848`). Finding 3: the stateful id is gone, the fill is a plain `div` with the
selector. Finding 4: the acceptance line now says what the card test pins and what it does
not. Finding 5: the US-0199 Plan records the owner's acceptance.

New, low, not blocking:

- `scripts/check-theme-contrast.py` `--report` summary still prints
  `"{n} below {FLOOR}:1"`; a glyph failure would be counted there under the 4.5:1 wording.
  One f-string.
- The new self-test lines assert `measure()` on two fixtures, which proves the ratio maths
  (already covered) rather than that `rows()` applies `GLYPH_FLOOR` to a glyph row and
  `FLOOR` to the rest. A fixture variant run through `rows()` with one glyph at 3.5:1 would
  pin the tier itself.

Re-run once the desktop is unlocked; the driver `scratchpad\uivrw5-gui.ps1` and `uivrw5.sh` are ready (use `pwsh -File`, Windows PowerShell 5.1 mis-binds the coordinate arguments).

## UI verification (rework 5 + US-0199), re-run

Binary copy built from `081b425f` (UI identical to `dbacc969`), desktop unlocked, 1280x800 window, private profile. Coordinates below are screenshot pixels of the full window (about 8 px left frame; the custom title bar is inside the client area, so client x = shot x - 8, client y = shot y). Screenshots: `scratchpad\uivrw5-shots\uivrw5-<nn>-*.png`.

| # | Item | Result | Evidence |
|---|---|---|---|
| 1 | Row hover fills the whole row; active row unchanged; divider and accent bar visible; text readable | PASS | `03` dark expanded: hovered row (44,49,60) at x=30, 200, 225 and y=200, 250, idle rows (30,34,39), dividers (62,68,81) at the row edges, active row (35,39,46) with the 2 px accent (82,139,255). `04`: hovering the active row leaves it unchanged. `18` dark collapsed: whole 44 px cell (44,49,60), bar and dividers kept. `22` light expanded: hover (229,229,230) vs idle (240,240,241) vs active white, dividers kept. `25`/`26` light collapsed: cell filled, accent bar (82,111,255). The grip appears at the left of the hovered row/cell. |
| 2 | Collapsed cell hover card | PASS | `18`, `20`, `25`/`26`: card about 220 px wide (x 39..258), two-line layout (line 1 chip + title; line 2 blank here, since cwd and status icon are empty in this sandbox), popover fill and border, themed in both themes. It appears just below-right of the pointer, overlapping the right edge of the strip by about 12 px and the cell underneath; it is never clipped off-window (checked at cell 2 and cell 6). Right-click on the cell hides it while the context menu is open (`19`). cwd line and status icon: NOT WALKED (no live cwd/OSC 7501 here). |
| 3 | Status bar separator between Collapse/Expand button and clock | PASS | Separator at x=44 between button (x=26) and clock, in dark (`24` zoom) and light (`23` zoom). Absent with Vertical Tabs off (`27`: clock first, no button, no separator). Narrow (window clamps to about 656 px, `34-narrow-330`/`420`): button, separator, clock and right group do not overlap. |
| 4 | Input Channel in the tab menu | PARTLY | Order Rename..., Duplicate, separator, Input Channel >, separator, Close, Close Others, Close to the Right / Close Tabs Below: PASS in the horizontal strip (`30`), a vertical row (`05`, `11`, `13`) and a collapsed cell (`19`). Right-click in the terminal area: no Input Channel item (`08`). Right-click on an INACTIVE tab, join Channel A (vertical, `06`/`07`) and Channel B (horizontal, `31`/`33`): only that tab gets the chip, the active tab is unchanged and stays selected. A split tab: NOT WALKED (Split Right creates an empty Space, not a split tab; `09`). See D1. |

### Defects

**D1 (medium): the "PowerShell 7" tab menu has no Input Channel item and no separator pair.**
- Screen: tab context menu of the tab opened from the New Terminal menu as "PowerShell 7", in the vertical rail (`10`, `14`, `15c`). Expected: Input Channel on every terminal tab (US-0199). Observed: Rename..., Duplicate, separator, Close, Close Others, Close Tabs Below only; reproducible whether the tab is active or inactive, before and after a Space split, while Command Prompt and PowerShell tabs show the item. Likely the PowerShell 7 shell did not start in this sandbox, so the panel may not be a live terminal; if so the omission may be intended and the owner should confirm with a working pwsh.
- Steps: Vertical Tabs on, + > PowerShell 7, right-click that row.

**D2 (low, observation, probably pre-existing): two empty separators after the clock.** Status bar, left group: `button | clock | | ` (x=186 and 203 dark and light, `23`/`24`), an empty cell between them; with Vertical Tabs off the same pair follows the clock (x=149, 166). Not part of rework 5 but visible next to the new separator.

### Owner-only checks
- Hover card with a real cwd, the OSC 7501 status icon, channel chips and the recording dot (needs a live shell with a prompt that reports its directory).
- Input Channel on a live split tab and the pane frame of a joined channel (no split tab can be made here; the joined tab was inactive so no frame is visible).
- D1 with a working PowerShell 7.
- Keyboard-only access to the menu, elevated window, physical-key behaviour.

## Advisor re-review 2

Date: 2026-10-22. Reviewed head: `15f6b19f` (`179ff17f` UI evidence, `15f6b19f` D1, D2 and
the two script nits), same branch. No conflict markers in `crates/`, `docs/`, `scripts/`.

**Verdict: PASS.** One informational note, nothing blocking.

Gate: `cargo clippy -p oneterm-terminal-view -p oneterm-workspace -p oneterm-settings -p oneterm-state --all-targets -- -D warnings`
clean; `cargo test` on the same four: settings 56, state 77, terminal-view 419 (3 ignored),
workspace 112 (3 ignored), all pass; `check-theme-contrast.py` 1755 pairings / 429 rows at
their floor; `--self-test` passes; `check-english.py` 1144 files; `check-doc-paths.py` 219
paths. Disk D at 6.3 GB free after the run.

**D1, the disabled row, against IN-0022.** `channel_menu.rs:92-103,138-147`: the only
condition is `active_view()` being `None` (a `SpaceTree::new_empty` tab, `terminal_panel.rs:274-276`,
or an emptied Space); then the row is drawn disabled between the same two separators, so
every tab's menu has one shape. Membership needs a view and its session (`registry.join(id,
channel, session, ..)`), so an empty Space really cannot join: the rule follows the data.
An exited session keeps its view, so it may join and stay a member; that is the behaviour
IN-0022 already had (nothing evicts a member on `SessionEvent::Exited`, `view.rs:417-424`;
only `shutdown` leaves, `view.rs:486`), and a broadcast reaching it is a failed write
logged once per delivery through `report_generated_input` (`crates/terminal/src/session.rs:64-68`,
`log::warn`), no notification, no panic, the origin unaffected (`fan_out_..` tests in
`crates/state`, including a dead member). So the rule is safe and consistent with the intake.
Test `only_a_tab_without_a_terminal_loses_the_channel_submenu` pins both halves.

**D2, separators only between drawn items.** `statusbar.rs:265-309`. Walked every
combination: left group is `[toggle] | clock` (both always drawn when the toggle is, so its
separator is unconditional, correct); centre leads with a separator only when the cwd or
the branch has text and puts the middle one only when both do, so `clock | cwd | branch`,
`clock | cwd`, `clock | branch`, `clock` are all right; right group puts a separator after
the network figure only when it is drawn and after the resource figure only when it is, and
the dock button is always last, so `net | res | btn`, `net | btn`, `res | btn`, `btn` are
all right. The test covers all-on, all-off and branch-only through `item_if` (a `None`
label, the same path production takes for an empty cwd). The width budget still counts the
four separators in `BAR_CHROME` when some are hidden, which only leaves room unused; the
record says so.

**Script nits.** `--report` summary names both floors. The self-test now runs a fixture
through `rows(only=..)` and checks that the glyph row carries `GLYPH_FLOOR`, the text row
`FLOOR`, that a bell near 3.4:1 passes the glyph row and would fail the text floor, and
that one near 2.4:1 fails; `rows()` keeps its default path for `main()`. Meaningful.

**Records.** US-0199 acceptance and Plan state the D1 rule and its cause; `gui-layout.md`
and `terminal-split.md` say "disabled while the tab's active Space holds no terminal";
US-0198 lists D2 with the cause and the test name. All match the code.

Info, not blocking: a channel member whose process exits stays a member until its tab is
closed (pre-existing, not introduced here). If the owner wants exited members out of the
fan-out, that is a one-line `leave_channel` in the `Exited` arm plus a test, as a separate
packet.
