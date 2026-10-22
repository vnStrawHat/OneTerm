# High-Level Design: Vertical tabs

Intake: IN-0055
Lane: normal
Date: 2026-10-20

## Idea

Terminal Tabs are drawn today by the GPUI Kit tab strip along the top of the centre tab group.
With **Vertical Tabs** on (Settings > General > Interface, off by default) OneTerm's own
tab-group renderer draws them instead as a **rail** down the left side of that group: a header
with the `+` menu, then one row per tab. The row content is the same element
the strip shows today (`TerminalPanel::title`), in a vertical variant, so every per-tab behaviour
comes along without a second implementation. The switch applies live, persists in
`ui_config.json`, and changes nothing about panels, sessions, docks or `docks.json`. Later
packets make the rail resizable (`US-0197`) and collapsible to a narrow strip (`US-0198`).

## Where the tabs live today

```text
OneTermWorkspace (layout/workspace/mod.rs:562-594)        column: title bar / dock area / status bar
└── DockArea "main-dock"  (renderer: OneTermDockSkin, dock_skin.rs:39-98)
    ├── Center: v_split
    │   └── TabGroup (base, tab_group.rs:683-757)          ← one per centre group (usually one)
    │       │   renderer: OneTermTabGroupSkin (dock_skin.rs:100-170)
    │       │             └── delegates to the kit TabGroupSkin (tab_panel.rs:690-792)
    │       ├── render_tab_bar  → kit TabBar (tab_panel.rs:433-687)
    │       │     ├── per panel: kit Tab ── child: TerminalPanel::title()
    │       │     │                         = render_tab_strip (tab_title.rs:207-345):
    │       │     │                           chips · ● · status icon · label · ×
    │       │     │                           (vertical row, two lines: chips · ● · label / cwd · status icon, no ×)
    │       │     │                           drag: DragTerminalTab   menu: Rename/Duplicate/Close...
    │       │     └── suffix: `+` (title_suffix) · zoom · `...` · right-dock toggle
    │       └── content_frame → active TerminalPanel (cached) → Space tree
    └── Right: tabs → SshClientPanel (its group draws no tab bar: owns_header, dock_skin.rs:105-110)
```

**What the vertical layout replaces:** only `render_tab_bar` of a rail group, plus that group's
content-frame offset. The SSH Client group, the docks, the title bar, the status bar and the
panels are untouched.

**Which groups draw a rail.** A group draws the rail only when it is in the **centre** and every
panel in it is a `terminal` panel. Placement is read the way the kit's own dock toggle reads it
(the group's node found in `DockArea::layout(DockPlacement::Center)`, `tab_panel.rs:229-241`);
the panel check is `owns_header`'s `panel_name` test (`dock_skin.rs:105-110`). Every other group
draws what it draws today: the SSH Client group nothing, and a group with a panel of any other
kind the kit strip. The right dock is not locked, so a terminal tab can be dropped into it
(`layout.rs:50-55`; `droppable` is `!locked`, `tab_group.rs:454-460`): that group keeps the kit
strip inside the dock, as today.

## Diagram

```text
UiConfig (oneterm-settings, global entity)
  vertical_tabs { enabled, width*, collapsed* }      * = US-0197 / US-0198
        │  read every render                              ▲ one setter per field
        │                                                 │
        ├──────────────► OneTermTabGroupSkin (oneterm-workspace)
        │                  frame():          + relative()             (vertical, rail group)
        │                  render_tab_bar(): TabRail  (absolute, left, full height, width W)
        │                  content_frame():  + margin-left W
        │                        │
        │                        │ for each visible panel: PanelHandle::of(p).title(window, cx)
        │                        │ header: PanelHandle::of(active).title_suffix(..)
        │                        │ callbacks: TabGroupContext::{select_tab,
        │                        │            drag_panel, drop_panel}
        │                        ▼
        └──────────────► TerminalPanel::title() → render_tab_strip (oneterm-terminal-view)
                           vertical variant: fill row width, no top bar, no -5 px margin,
                           no ×, two lines (title; cwd + status icon at the end),
                           "Close Tabs Below" in its menu

Settings > General > Interface > [Vertical Tabs]  ──► UiConfig::set_vertical_tabs_enabled
                                                       → persist (background) → refresh windows
```

## Rendering seam

Base composes every tab group as a column: the skin's frame, then `render_tab_bar`'s element,
then the content frame, with `.flex().flex_col()` applied **after** the skin's frame
(`reference/gpui-kit/crates/base/src/dock/tab_group.rs:691-709`). A skin cannot make the group a
row, and the component `TabBar` has no vertical mode. The design works inside those facts:

- **Option B1 (chosen): absolute rail.** In vertical mode `OneTermTabGroupSkin`:
  - `frame`: the kit frame (it carries the `ToggleZoom` / `ClosePanel` actions,
    `tab_panel.rs:691-727`) plus `.relative()`;
  - `render_tab_bar`: the rail, positioned `absolute`, `top 0`, `bottom 0`, `left 0`, width `W`.
    An absolute child takes no space in the column, so base's layout is unchanged;
  - `content_frame`: the kit content frame plus `margin-left W`. Base still installs its drop
    handling and the active-panel slot on this element, so a dock drop is measured against the
    region beside the rail, never under it.
  A **margin**, not padding: padding would put the rail inside the content frame's bounds and
  under its drop handling. Base's own test proves a drag is measured against wherever the skin
  puts the content frame (`the_content_frame_is_what_a_drag_is_measured_against`,
  `tab_group.rs:1764-1784`); while a drag hovers the rail, the content frame's move listener sees
  the pointer outside its bounds and clears the indicator (`tab_group.rs:523-526`). The active
  panel is `cached(absolute().size_full())` inside the content frame (`tab_panel.rs:785-791`),
  so the terminal is laid out narrower, not overlapped: one resize per visible terminal.
  Groups that draw no rail (§ Which groups draw a rail) are excluded before any of this.
- **Option B2 (fallback if the Q8 spike fails):** `render_tab_bar` returns nothing and
  `render_active_panel` returns a row of rail and panel. Weaker, in three ways that B2 would have
  to answer: (1) the rail sits inside the content frame, so a `DragPanel` released on any rail
  pixel with no drop handler of its own (the header, the gaps between rows) falls through to the
  content frame's `on_drop` and **splits the centre** with the placement resolved over the rail
  (`tab_group.rs:626-669`, `:729-742`); every rail pixel would need its own handler (header =
  append); (2) the split placeholder is painted over the rail while a tab is dragged, so the
  indicator must be suppressed there; (3) an empty group (`render_empty`) needs the rail drawn a
  second way.
- **Rejected:** (A) a sidebar outside the `DockArea`, owned by the workspace: it would have to
  re-implement selection, drop slots and per-group scope that `TabGroupContext` already hands a
  skin, and it cannot say which group's tabs it shows once the centre is split; (C) patching or
  forking `gpui-component` (forbidden, `docs/PROJECT.md`).

The rail is OneTerm code built from the unstyled base `Tabs` (role `TabList`, vertical
orientation) and `Tab` (role `Tab`, `aria_selected`, position in set) elements
(`.../base/src/tabs.rs:149-234`), styled with theme tokens. It keeps its own `ScrollHandle` and
the last active index per group, like the kit strip (`tab_panel.rs:191-198`, `:464-470`): a
change of active tab scrolls the row into view; a clicked row is not snapped.

### Rail anatomy

| Part | Content | Behaviour |
| --- | --- | --- |
| Header (32 px, the strip's height; the same collapsed; amended 2026-10-21, owner rework 4: no zoom button, the collapse button moved to the status bar, `+` on the right) | `+` = the active panel's `title_suffix` (OneTerm's New Terminal menu, `terminal_panel.rs:721`), right-aligned (centred in the collapsed strip) | No zoom button (the `ToggleZoom` key binding and the horizontal strip's button stay), no `...` menu (Q5), no right-dock toggle, no collapse button (the status bar holds it, just left of the clock, while Vertical Tabs is on) |
| Rows (64 px each, two lines; was 32 px until the 2026-10-21 owner rework 2) | base `Tab` wrapping `PanelHandle::of(panel).title(window, cx)` | click: `group.select_tab(ix)`; selected: `tab.active.background` + 2 px bar on the left edge in `oneterm_theme::selection_accent` (the theme's `table.active.border`, kept past OneTerm's own blanking of `Theme::table_active_border`; the strip's top bar reads the same, amended at `US-0196`) |
| Row content (amended 2026-10-21, owner; two lines at rework 2) | **Line 1** (24 px): chips, recording dot, label (ellipsis at its end). **Line 2** (20 px, `text_xs`, `muted.foreground`): the terminal's current directory left-aligned and cut at its **start** when too long (`…\src\panel`, gpui's `TextOverflow::TruncateStart`, so it follows the live width), then the OSC 7501 status icon right-aligned at the end of the line. 10 px above line 1 and below line 2, the last pixel of which is the row's 1 px divider (so 10 + 24 + 20 + 9 + 1 = 64 and the lines are never squeezed). **No `×` close button**; with no cwd line 2 shows no path, with no status nothing is drawn at its end. The strip's row is unchanged (chips, dot, status icon, label, `×`) | the same row element, selected by `TabRowLayout`; the close paths left in the rail are middle-click, the right-click menu (Close, Close Others, Close Tabs Below) and the panel's keyboard paths (`ClosePanel`, and Close Space on the last Space); the icon sits inside the row's 4 px right padding and, once the rows overflow, clear of the scrollbar's gutter. The cwd is the active Space's `TerminalSession::cwd()` (OSC 7, sanitised by `TerminalSecurityPolicy::sanitize_cwd`); the very string the status bar's breadcrumb shows (`TerminalPanel::breadcrumb_label`, no `~` abbreviation; owner rework 3, 2026-10-21); the row has a 1 px `border` divider on its bottom edge, inside the 64 px; it updates live (`TerminalViewEvent::CwdChanged`) |
| Row leading edge, the grip (12 px, the strip tab's padding, kit `tab/tab.rs:72-89`; Q9) | a `grip-vertical` glyph, centred on line 1 (not on the whole 64 px row), in `muted.foreground` and the `OpenHand` cursor while the row is hovered (a new `crates/theme/assets/icons/grip-vertical.svg`, so `AppIcon::GripVertical`; the kit's `IconName` has no grip) | starts `DragPanel` (`group.drag_panel(ix)`, guarded by `is_draggable`), the reorder and dock-move drag, as the kit tab's padding does today. The rest of the row is the title row, whose own drag (`DragTerminalTab`) wins, as on the strip |
| Row as drop target | 2 px line in `drag_border` while a `DragPanel` hovers, on the edge where the tab will land: below the hovered row for a tab dragged forward within its own group, above it otherwise, none on the dragged row itself (amended at `US-0196`) | drop: `group.drop_panel(drag, Some(ix), true)` (mirrors `tab_panel.rs:594-612`) |
| Space below the last row | `drop_target` fill while dragged over | drop: append, or last slot for a panel from this group (mirrors `tab_panel.rs:630-651`) |

The kit's tab bar also accepts a host-owned `AnyDrag` on tabs and the empty space. The rail does
not: no OneTerm crate creates an `AnyDrag` or handles `TabGroupEvent::DragDrop`, so such a handler
could be neither used nor tested here. A future host drag adds it with its first user.
| Right edge | 1 px `border` | `US-0197`: a 4 px resize handle over it (column-resize cursor); the drag sets the width live in memory and the release stores the preference once (`set_vertical_tabs_width`) |

## UI Wireframe

Horizontal (today, unchanged when the switch is off):

```text
+--------------------------------------------------------------------------+
| >_ OneTerm  [menu]                                              _  □  x  |
+--------------------------------------------------------------------------+
| [A ● PowerShell  x][ ⟳ web-01  x][ cmd  x]          [+] [⛶] [...] [▣]  |
+----------------------------------------------------------+---------------+
|                                                          | Session    [+]|
|  PS C:\> _                                               |  ...          |
|                                                          |---------------|
|                                                          | SFTP Browser  |
+----------------------------------------------------------+---------------+
| 12:04  C:\work  main*                      ↓1 KB/s  CPU 1%  MEM 60 MB [▣]|
+--------------------------------------------------------------------------+
```

Vertical (`US-0196`, rail 220 px):

```text
+--------------------------------------------------------------------------+
| >_ OneTerm  [menu]                                              _  □  x  |
+------------------+---------------------------------------+---------------+
|              [+] |                                       | Session    [+]|
|------------------|  PS C:\> _                            |  ...          |
|▌A ● PowerShell   |                                       |               |
|▌ C:\work\src    ⟳|                                       |---------------|
|⠿ web-01          |                                       | SFTP Browser  |
|  /srv/…/releases✓|                                       |               |
|  cmd             |                                       |               |
|  (no cwd yet)    |                                       |               |
|  (drop space)    |                                       |               |
+------------------+---------------------------------------+---------------+
| 12:04  C:\work  main*                      ↓1 KB/s  CPU 1%  MEM 60 MB [▣]|
+--------------------------------------------------------------------------+
  each row is 64 px, two lines: line 1 = chips, ●, title; line 2 = the current directory
  (left, cut at its START when too long: "…\panel") and the status icon (right)
  ▌ = selected row: tab.active.background + 2 px accent bar on its left edge
  ⠿ = the reorder grip, shown on the hovered row (here web-01), level with line 1, open-hand cursor
  ⟳ ✓ = the OSC 7501 status icon at the right end of line 2 (none: nothing drawn); no × in a row
  rows scroll vertically (wheel or touchpad, no scrollbar) when they do not fit; the header stays
```

Vertical, resizing (`US-0197`):

```text
+------------------++---------------------
|              [+] ||   drag the right edge: 140 px .. min(400 px, 35 % of the tab group)
|▌PowerShell       ||
| C:\work\src      ⟳||
|  web-01          ||   the width the user sets is remembered and
+------------------++   re-applied (clamped) when the window grows again
```

Vertical, collapsed (`US-0198`, 44 px):

```text
+----+------------------------------------
| [+]|
|----|
|▌>_ |  ← 44 x 44 cell: the shell's icon centred (cmd / PowerShell / generic terminal);
| ▒▒ |     the OSC 7501 state is the cell's background, no badge (rework 6, 2026-10-22);
|● >_|     recording dot top-left; hover card: the expanded row's content (with its
        status icon); click / middle-click / right-click as expanded; chips and cwd not drawn
+----+
```

Settings window, General page (`US-0196`):

```text
Interface
  UI Font Size          [ 16 ]
  Show FPS Monitor      [  o ]
  Vertical Tabs         [  o ]   Show terminal tabs in a list on the left of the
                                 terminal area instead of a strip above it.
```

## Interactions

| Area | Vertical layout behaviour | Why / source |
| --- | --- | --- |
| Spaces and splits (`docs/terminal-split.md`) | Unchanged. A Space tree lives inside the panel's content; the rail sits outside it | the rail replaces only the group's tab bar |
| Drag a tab into an empty Space | Drag the row's content (chips, dot, status icon, label; anything but the grip): the same `DragTerminalTab` source as on the strip (`tab_title.rs:271-281`) | the row content is the same element |
| Reorder / move between groups | Drag the row's **grip** (its 12 px leading edge) onto another row or the space below; `DragPanel` as on the strip. **Dropping a row's content on another row does nothing**: that drag only lands on an empty Space, and the shell cannot accept its type (`pub(crate)` in `oneterm-terminal-view`, R4). Label-drag reorder is a follow-up (intake Q9) | `TabGroupContext::drag_panel` / `drop_panel` |
| Right dock, `IN-0054` | Unchanged: it follows the active tab's kind; the dock buttons in the status bar and `Ctrl+Shift+B` work. The strip's right-dock toggle has no counterpart in the rail (Q5), so `docs/gui-layout.md`'s "dock buttons in the tab bar and the status bar" becomes a statement about the horizontal layout (amended by `US-0196`) | `layout/workspace/mod.rs:346-379` reads `AppState`, not the strip |
| Right dock width | Unchanged formula (35 % of the window, 240 px floor); the rail is taken from the centre's share. At 900 px with the dock open: dock 315, rail 220 (`US-0196`) or 204.75 (`US-0197`), terminal 365 or 380.25 px | `clamp_right_dock_width`, `mod.rs:76-85` |
| Title bar | Unchanged; nothing moves into it | `title_bar.rs` |
| Status bar, FPS HUD | Unchanged. The HUD keeps its position (title bar + 32 + 12 px from the top, 12 px from the right), which in vertical mode lies over the terminal's top-right rows or, with the dock open, the right dock's session search field, as it does today | `widgets/fps_hud.rs:26-27`, `:159` |
| Zoom | The zoomed group keeps its rail, as it keeps its strip today ("the whole TerminalPanel is zoomed, including its tab-group chrome"); the rail has no zoom button (owner rework 4), so Zoom Out is the `ToggleZoom` key binding. A zoomed group is neither draggable nor droppable (`tab_group.rs:441-460`), so reorder and drops are off while zoomed, as on the strip. Zoom persistence (`docks.json` `zoomed_panel`) is unchanged | `docs/gui-layout.md` § Zoom |
| Lock screen | Covered like everything else (the lock replaces the window's whole element tree); after unlock the rail returns as it was, scroll position included | `docs/gui-layout.md` § Lock screen |
| Broadcast channels | Chips stay in the row; the Space badge is unchanged | same row element |
| OSC 7501 status icon (`US-0193`) | Same icon, same rules, drawn at the right end of line 2 of a vertical row (amended 2026-10-21) and before the label in the strip (single-Space tabs only, acknowledge on `set_active(true)`, which a row click reaches through `select_tab` like a strip click). Of its colours only the bell's `warning` is measured on the tab surfaces; `primary`, `success` and `danger` are not, as for the strip | same row element; `terminal_panel.rs:670-684` |
| Close a tab (`×`, amended 2026-10-21, owner) | A vertical row has **no `×`**; line 2 shows the current directory and the OSC 7501 status icon at its right end instead. A tab still closes by middle-click on the row, the right-click menu (Close, Close Others, Close Tabs Below) and the panel's keyboard paths (`ClosePanel`, Close Space on the last Space). The strip keeps its `×` | `render_tab_strip` omits the `tab-close` element when the layout is vertical |
| Tab context menu | Same rows; "Close to the Right" reads "Close Tabs Below" while vertical (Q6) | `tab_title.rs:469-507` |
| `...` menu | Not drawn in vertical mode (Q5); its tab list is the rail itself | `tab_title.rs:520-548` |
| Keyboard | No tab navigation exists in either layout; rows are not tab stops (like the strip's). Follow-up packet proposed in the intake | `crates/actions/src/lib.rs` |
| Overflow | The rows scroll vertically under the header; the active row is scrolled into view when it changes. Rows are not virtualised, like the strip's tabs (every tab's row renders each frame in both layouts) | kit pattern `tab_panel.rs:464-470` |
| Several centre groups (Q7) | Each all-terminal group draws its own rail beside its own content; from `US-0197` each rail is bounded by its own group's width (§ Width). In `US-0196` each rail is a fixed 220 px | the renderer is per group |
| A terminal tab dropped into the right dock | That group keeps the kit strip, as today (§ Which groups draw a rail) | not a centre group |
| Elevated window | Shows the saved layout; the switch works in memory, the write is refused (`DEC-0019` M4) | `ui_config.rs:239-247` |
| Accessibility | Rail = base `Tabs` (`TabList`, vertical orientation, as `toggle_group.rs:60` sets one); row = base `Tab` with `aria_selected` and position `ix + 1` of `n`; header buttons carry tooltips. The group frame keeps its "Terminal tabs" pane label (`dock_skin.rs:114-119`) | `.../base/src/tabs.rs:149-234` |
| Reduced motion | The status icon's animations already stop; the rail adds no animation | `US-0193` |

## Themes and contrast

The rail draws only tokens the strip draws, on the surfaces the strip uses:

| Element | Foreground | Surface | In `SURFACES` today |
| --- | --- | --- | --- |
| Rail fill, header | icons only | `tab_bar.background` | n/a (no text) |
| Inactive row label | `tab.foreground` | `tab_bar.background` | yes |
| Hovered row label | `tab.active.foreground` | `tab_bar.background` | **no: added by `US-0196`** (the strip draws the same pair on hover, kit `tab.rs:172-184`, so the addition also covers the strip) |
| Selected row | `tab.active.foreground` | `tab.active.background` | yes |
| `×` (strip only), grip, cwd line 2 of a vertical row | `muted.foreground` | `tab_bar.background`, `tab.active.background` | yes (the cwd line draws no new pair) |
| Bell (OSC 7501 blocked) | `warning` | `tab_bar.background`, `tab.background`, `tab.active.background` | yes (`US-0193`) |
| Channel chips, recording dot, other status icons | `chart_1..5`, `danger`, `primary`, `success` | `tab_bar.background`, `tab.active.background` | no, as for the strip (no new gap: same pairs the strip draws) |
| Accent bar, drop border, drop fill | `selection_accent`, `drag_border`, `drop_target` | non-text | n/a |

No hover fill is added: a hovered row changes only its label colour, as a strip tab does.
Adding `tab_bar.background` to `tab.active.foreground`'s surfaces also turns on the script's
`HIERARCHY` comparison `("tab.active.foreground", "muted.foreground")` on that fill, because
`muted.foreground` is already listed there (`check-theme-contrast.py:200`, `:264`). The advisor
measured both on a scratch copy: every pairing and every hierarchy comparison passes in all
built-in themes. If a later theme fails, its token is corrected by lightness only, as `US-0193`
did. A new fill drawn later must be added to `SURFACES` with the line that draws it.

## Data Ownership and Interfaces

| Data | Owner | Lifetime | Writers |
| --- | --- | --- | --- |
| `UiConfig.vertical_tabs.enabled` | `oneterm-settings` (`ui_config.json`) | persisted | `UiConfig::set_vertical_tabs_enabled` (Settings switch) |
| `UiConfig.vertical_tabs.width` (`US-0197`) | same | persisted; the user's preference, clamped only when applied | `UiConfig::set_vertical_tabs_width` (end of a rail drag) |
| `UiConfig.vertical_tabs.collapsed` (`US-0198`) | same | persisted | `UiConfig::set_vertical_tabs_collapsed` (header button) |
| Rail scroll position, last active index, last painted group width (`US-0197`) | `OneTermTabGroupSkin` instance (one per group) | memory | the renderer (`on_prepaint` on its frame for the width) |
| Tabs, selection, order | base `TabGroup` / `LayoutTree` | as today | as today |

Readers: `OneTermTabGroupSkin` (`frame`, `render_tab_bar`, `content_frame`) reads the flag on
each render through `UiConfig::read_vertical_tabs`. `render_tab_strip` does **not** read it
(amended at `US-0196`): a panel draws its row without knowing its group, and a terminal tab in
the right dock must keep the horizontal row even with the switch on, so the rail wraps its own
`title()` calls in `oneterm_state::TabRowLayout::scope` and the row reads
`TabRowLayout::current`. `oneterm-state` is below both crates (L2), so no crate edge is added
(R2, R4).

Persisted shape (all fields optional, the object omitted while it equals the default):

```json
"vertical_tabs": { "enabled": true, "width": 260, "collapsed": false }
```

`width` and `collapsed` exist only from `US-0197` / `US-0198`. The object is parsed leniently:
a value that is not an object, or a field of the wrong type, reads as that field's default with
a warning, and `width` is clamped to 140..=400 on read; the document is never quarantined for
it and the file is not rewritten by the read.

## State Machine

```text
                  switch on                          [«] collapse (US-0198)
  Horizontal  ───────────────►  Vertical, expanded  ─────────────────────►  Vertical, collapsed
  (default)   ◄───────────────   (width W)          ◄─────────────────────   (44 px)
                  switch off                          [»] expand
       ▲                                                                         │
       └───────────────────────────── switch off ─────────────────────────────────┘

  enabled:    false              true                                      true
  collapsed:  kept as remembered false                                     true
  every transition: setter → UiConfig entity updated → persist (background) → refresh windows
  no transition rebuilds a panel, ends a session or writes docks.json
```

Turning the switch off from the collapsed state keeps `collapsed = true` remembered, so turning
it on again returns to the collapsed rail.

## Width (`US-0197`)

```text
MIN = 140 px    MAX = 400 px    SHARE = 0.35    DEFAULT = 220 px    COLLAPSED = 44 px

preference  = clamp(dragged width, MIN, MAX)                 (stored as vertical_tabs.width)
applied     = clamp_tab_rail_width(preference, group width)

clamp_tab_rail_width(w, group):
    group <= 0 (not painted yet)  ->  clamp(w, MIN, MAX)
    otherwise                     ->  clamp(w, MIN, ceiling)
        where ceiling = max(MIN, min(MAX, group * SHARE))
```

`group` is the painted width of the rail's own tab group, rail included, so the rail's width does
not feed back into its own bound. The renderer reads it through `on_prepaint` on its frame
(`tab_group.rs:910-911`) and keeps it per group; when it differs from the last frame's, the
renderer asks for one more frame, so a window resize, the right dock opening or a centre split
re-clamps every rail on the next frame. `SHARE` is the right dock's share
(`RIGHT_DOCK_MAX_SHARE`, `layout/workspace/mod.rs:37`). Bounding by the window instead would let
two rails in a split centre take 2 x 250 px of a 1000 px window and leave the two terminals 150 px
between them.

Worked examples (each is a unit test in `US-0197`):

| Group width | Requested | Ceiling | Applied |
| --- | --- | --- | --- |
| 1920 | 220 | max(140, min(400, 672)) = 400 | 220 |
| 1920 | 520 | 400 | 400 |
| 800 | 300 | max(140, min(400, 280)) = 280 | 280 |
| 600 | 220 | max(140, min(400, 210)) = 210 | 210 |
| 380 | 220 | max(140, min(400, 133)) = 140 | 140 |
| 0 (not painted yet) | 100 | n/a | 140 |

In a window (right dock at its own ceiling, 35 % of the window with a 240 px floor):

| Window | Right dock | Centre groups | Rail per group | Terminal per group |
| --- | --- | --- | --- | --- |
| 1000 | 350 | one of 650 | 220 (ceiling 227.5) | 430 |
| 1000 | 350 | two of 325 | 140 (floor) | 185 |
| 900 | 315 | one of 585 | 204.75 (ceiling) | 380.25 |
| 600 | 240 (floor) | one of 360 | 140 (floor) | 220 |

A terminal keeps at least 65 % of its group whenever the group is 400 px or wider
(140 / 0.35 = 400); below that the 140 px floor applies. `US-0196` uses the fixed 220 px without
this clamp: on a 600 px window with the dock open the terminal has 140 px, and a split centre
spends 440 px on rails, until `US-0197` lands.

## Data Flow

1. **Start-up.** `UiConfig::init` loads `vertical_tabs` (lenient). The workspace builds the
   layout as today. On the first frame each centre group's renderer reads the flag and draws the
   strip or the rail.
2. **Switch.** The Settings switch calls `UiConfig::set_vertical_tabs_enabled(on, cx)`: the
   entity is updated, a snapshot is persisted on the background executor, and every window is
   refreshed (`ui_config.rs:326-333` is the pattern). The main window re-renders its dock area;
   no view is cached above the tab groups, so the next frame draws the other layout. The terminal
   area changes width and each visible terminal gets one resize.
3. **Row actions.** A click calls `TabGroupContext::select_tab`, which activates the panel exactly
   as a strip click does (`set_active`, `AppState` publish, `IN-0054` dock rule). Row content
   handles middle-click, `×` (strip only), rename, the context menu and the label drag itself.
4. **Drops.** A `DragPanel` dropped on a row or the space below goes to `drop_panel`; on the
   content region beside the rail, base's own split-or-merge handling runs as today.
5. **Width (`US-0197`).** Dragging the handle sets the applied width each frame (memory only);
   mouse-up stores the preference through its setter (one write). Any change of a group's painted
   width (window resize, dock open or close, a split) re-applies the clamp from the preference on
   the next frame.
6. **Collapse (`US-0198`).** The status bar button (just left of the clock; it was in the header until owner rework 4) flips `collapsed` through its setter; the row
   element renders its compact form when it reads `collapsed`.

## Detail Design

- [x] Detail design: not needed
- Reason: normal lane; the only seam is OneTerm's own `TabGroupRenderer`, specified above
  (Rendering seam, Rail anatomy, Interactions), and the only persisted change is one optional
  lenient field following the `show_fps` / `lock_after_idle_minutes` precedents. The advisor
  review ([`evidence/IN-0055-spec-review.md`](evidence/IN-0055-spec-review.md)) settled the drag
  order and the contrast pair; the remaining uncertainty is empirical and is settled by the
  `US-0196` spike (intake Q8), not by more design.
