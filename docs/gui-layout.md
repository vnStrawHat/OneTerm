# GUI layout — current implementation

This document describes the shipped workspace shell. Historical design sketches live in Git history; current dependency and crate boundaries are in [`architecture.md`](architecture.md) and [`agents/dependencies.md`](agents/dependencies.md).

## Frame and ownership

`OneTermWorkspace` (`crates/workspace/src/layout/workspace/`) renders three vertical regions:

```text
TitleBar
DockArea
StatusBar
```

The feature-agnostic workspace crate owns the frame, dock layout, dock persistence, the right dock following the active tab's kind, and status widgets. Feature crates register their panels by stable names from `oneterm_state::panel_names`; the shell constructs panels through `PanelRegistry` and does not import feature implementations. `oneterm-app` is the composition root and owns the composite SSH Client right-dock panel because it may depend on both Session and SFTP feature crates.

## Dock composition

The shell uses the GPUI Kit 0.6 dock tree:

```text
DockArea (id = "main-dock", version = 3)
├── Center: DockLayout::v_split
│   └── DockLayout::tabs
│       └── one or more TerminalPanel values
└── Right: DockLayout::tabs
    └── exactly one mode panel: SshClientPanel (SSH Client mode)
```

**The right dock follows the active tab's kind** (`IN-0054`; the title bar's former `SSH Client | None` control is gone). It opens when an SSH tab becomes the active tab and closes when a local-shell tab does — only when the kind changes, so moving between two tabs of the same kind, or between Spaces, never moves it. The dock button in the status bar, the one in the horizontal tab bar (the vertical rail has none, see [Vertical tabs](#vertical-tabs)), and **Toggle Right Dock** (`Ctrl+Shift+B` by default, Settings > Key Bindings), open or close it, and that choice is remembered for the kind of tab it was made on and persisted as `ui_config.json`'s `right_dock` (`local` default closed, `ssh` default open). Opening and closing never rebuilds the panel, so an SFTP connection, the session list and the dock's width survive. At start-up the dock is set for the active tab's kind, whatever open state `docks.json` carried. An elevated window has no right dock and none of this applies.

`SshClientPanel` owns a vertical `v_resizable` split containing `SessionPanel` and `SftpPanel`, each with its own header. Its section headers show the hosted panel's `title_suffix` in a control group framed like the center tab bar's trailing buttons: the SFTP Browser's expand/collapse toggle, and — since `US-0119` — a `+` on the Session header that opens the full New SSH Session dialog. That `+` is the panel's one unconditional add affordance: the empty-list hint ("Right-click → New Session") disappears as soon as the list fills, and the header does not. A right-click on a session row answers with that session's own menu — Open, Properties, Duplicate Saved Session, Move to Group ▸, a separator, New Session, a separator, and a danger-styled, confirmed Delete (`US-0119`, `US-0129`). **Duplicate Saved Session** copies a row of `ssh_session.json` and opens the copy's Properties dialog; it is deliberately not called "Duplicate", because the centre tab bar's tab menu already has a row by that name which reopens a *running* connection. **Move to Group ▸** lists the groups already in use plus "No group" and "New group…", and moves that one session. With the Password method the Properties / New SSH Session dialog also shows a password input with **Save password** (sealed under the master password, set up or unlocked first; `US-0158`), or "Saved password" with **Replace** / **Remove** once one is saved; the connect dialog then shows "Using saved password" rows ([`ssh-client-connect.md`](ssh-client-connect.md) §4.8). It is zoomable: expanding the SFTP Browser (IN-0025) zooms this node so the browser fills the workspace like a zoomed terminal tab, with the Session section hidden until it collapses. The right dock still has a tab-group node internally, but `OneTermDockSkin` suppresses that outer tab bar for the single `ssh_client_panel` leaf, so it shows exactly one header and no tab bar. Center terminal groups keep the standard GPUI Kit tab chrome, or draw the vertical tab rail when Vertical Tabs is on (see [Vertical tabs](#vertical-tabs)).

The right dock's width is bounded by a share of the window: `clamp_right_dock_width` caps a requested width at 35 % of the window width, with a 240 px floor so a very narrow window keeps a usable dock rather than collapsing it. The workspace remembers the width the user last set (`preferred_right_dock_width`) and applies the clamp of that value at each seam that sets the width — both layout builders and the window-resize observer — so narrowing the window narrows the dock and widening it brings the user's width back. A drag inside the allowed range is never fought and becomes the new preference; a drag past the ceiling is capped where it happens rather than left to be snapped back later. `docks.json` keeps the kit's own flattened width field and needs no schema change, but what is written there is the **preference** (see §Persistence), so a session at a narrow window never costs the user their wide dock. **An elevated OneTerm window has no right dock at all** (`IN-0043`, `DEC-0019` M1): it does not read `docks.json`, so it starts from the fixed default layout every time, `ssh_client_panel` is never built, and **Toggle Right Dock** does nothing. **Not reading the saved layout is the load-bearing half**, and it is worth saying why: `load_layout` restores the side docks *by name*, so a saved `ssh_client` dock would be built before the layout builders run, and a builder that declines to call `set_dock(Right, ..)` leaves a dock that already exists rather than removing it. Gating the builders alone left every user who had run OneTerm once with a full SSH Client dock in their administrator window. That window is local shells only: no SSH, no SFTP, no saved sessions. It is also marked, from its process token and never from its command line: `OneTerm (Administrator)` in the OS title bar and in the in-app one — the title text is the whole marker, deliberately (owner ruling, `DEC-0019` M5 as amended; a warning-coloured title-bar *border* was built and then removed). In the **app** title bar the suffix is drawn as its own span beside the application name, in the theme's `warning` and bold, so it reads as a marker rather than as part of a long title; the **OS** title bar carries the same words plain, because a window title has no spans. The two cannot drift — the split is a view of the one title function, and a test concatenates the spans and asserts the result is the OS title. `warning` is checked against `title_bar.background` by `scripts/check-theme-contrast.py` in all 39 theme variants. A token query that fails reads `OneTerm (elevation unknown)` and is restricted all the same.

Layout construction uses `DockLayout::{tabs,v_split}` and `DockArea::{set_center,set_dock,set_dock_size,set_dock_collapsible}`. Runtime traversal uses `DockArea::layout`, `PaneNode`/`PaneRef`, and stable `PanelId`/`NodeId` values. Tab behavior is provided by `TabGroup`; rendering customization is isolated behind `DockSkin`, `DockAreaRenderer`, and `TabGroupRenderer`.

## Settings window

`oneterm-settings-ui` composes the GPUI Kit 0.7 `Settings` widget in a standalone window. It uses `GroupBoxVariant::Outline`, matching the v0.7.0 Settings story so every setting group has the theme border, radius, and content padding supplied by `GroupBox`. Items use GPUI Kit's standard group gap without custom divider rows. Editable fields declare their built-in default or a custom reset handler; GPUI Kit therefore shows the page-level `Reset All` action only while that page differs from its defaults and persists the reset through the same field setters.

Every OneTerm-owned focus target exposes an ID and accessibility role. The Settings root and feature-panel content roots are named `Pane` nodes, the Key Bindings capture target is a `TextInput`, and repository targets are `Link` elements. GPUI Base focuses the host-supplied `DockAreaRenderer`, `TabGroupRenderer`, and `TilesRenderer` frames directly, so `OneTermDockSkin` also assigns pane roles at those seams; this keeps the published dependency unmodified while preventing node-less focus targets. Docked panels return a separate navigation focus proxy to `TabGroup` and forward it to their independently tracked content focus after that frame; a tab-group frame and panel content must never track the same handle because both accessibility nodes would claim focus in one frame.

The pages, in sidebar order:

| Page | Groups |
|---|---|
| General | Theme (mode + colour theme), Interface (UI font size, Show FPS Monitor), Shell |
| Key Bindings | App Menu, Edit Menu, Terminal Context Menu, Input Channel, Session Tabs Context Menu, SFTP Context Menu |
| Terminal | Font, Cursor, Layout, Scroll, Bell, Mouse, Security, Logging |
| Completion | Completion |
| SSH | Connection, SFTP Editor, SFTP Edit Limit |
| Security | Master Password, Screen Lock |
| Network | GitHub Connection |
| About | Application, Links, Updates |

`General` is the page the window opens on and carries what a new user reaches Settings for first — the theme, the UI font size, and the shell new local terminals start. There is no separate Appearance page: two controls did not earn one, and the theme was the thing hardest to find (`US-0122`). `Network` holds the updater's proxy and TLS-verification settings (`docs/auto-update.md`); `About` holds the application identity, the repository link and the update preferences together with the **Check Now** action that the About dialog also offers. `Security` (`IN-0050`, `US-0157`) shows the master password's status (Not set up / Set, locked / Set, unlocked / why it is unavailable) and only the actions that apply: **Set Up...**, **Unlock...**, **Change...**, **Lock Now** (shown whenever a vault exists: covers every window with the lock screen and drops the key, see § Lock screen) and **Reset...** (type `RESET`; deletes `vault.json` and every saved password, keeps sessions). Each opens one of the shared master-password dialogs in `oneterm-state` (`vault_dialogs.rs`); Argon2id runs off the UI thread while the confirm button spins, a wrong password is shown under the field, and from the fifth wrong one in a row the button waits 2, 4, 8, 16, then 30 seconds with a countdown. A second group, **Screen Lock**, holds Lock After Idle (minutes, 1-480, default 15; `US-0159`). In an elevated window the page is read-only: the status ends with "Managed from the normal OneTerm window." and only Unlock and Lock Now remain (`DEC-0019` M4, intake Q5). The page has no resettable fields, so it shows no Reset All.

**A page may be longer than the window** (`US-0195`). `US-0122` had split Terminal into four pages and Key Bindings into three, because the kit then scrolled a sidebar sub-item by *measured* group heights and under-shot every group below the fold. The 0.7.3 kit scrolls by index (`reference/gpui-kit/crates/component/src/setting/page.rs:170-180`, `ListState::scroll_to` with `item_ix`), which needs no measurement, so a sub-item lands wherever its group is, and the owner asked for the pages back together. Sub-items appear on any page with more than one group and only for **titled** groups (`settings.rs:178-183`), which is why the Logging group carries a title and its old page description. `Reset All` is per page (`page.rs:111-119`): on Key Bindings it resets every binding, on Terminal every terminal setting except Completion's. `terminal/mod.rs`'s `TERMINAL_PAGES` and `key_bindings_ui.rs`'s `KEY_BINDING_PAGES` hold the composition, and each is asserted against its own group source in tests.

Every setting is named at most twice: once by its group (a title, plus a description only where the description says something the titles do not) and once by its item (a label, plus a description that says what the value *does*). A description never restates a title. This is the rule `US-0122` applied across every page after `F16` found `Interface / UI font size. → UI Font Size → Interface font size in px.` on the landing page.

The theme list on **General** is built by `appearance::theme_rows`, not handed to the kit in registry order. It is sectioned `Light themes` / `Dark themes`, the section holding the selected theme comes first, and the selected theme leads that section, so the checked row is the first selectable one. That ordering is how the current theme is visible the moment the list opens: `gpui_component`'s popup always opens at its top and exposes no way to scroll it to the checked row (see `US-0121` Gaps for the file and line). The picker is a `SettingField::element` rather than the kit's `scrollable_dropdown`, because the dropdown field accepts only `(value, label)` pairs and a section heading would have to be a selectable row that does nothing; building the menu in OneTerm lets the headings be `PopupMenuItem::label`, which the kit renders disabled and never treats as clickable, and steps over in `select_up`/`select_down` once a row is selected. The first Down-arrow of all still highlights the heading, because the kit sets index 0 without checking (`popup_menu.rs:906-911`); Enter there does nothing. Ordering the list has one visible consequence: choosing a theme in the other mode moves that whole section to the top for the next open.

A key-binding row prints its keystroke once. The `Default: …` line under the label appears only while the row differs from its shipped default — the same test `overrides_from_effective` applies when deciding what reaches `ui_config.json` (`key_bindings/state.rs`'s `is_at_default`), so the line and the persisted override can never disagree about what "changed" means. On a row at its default, **Reset** is a no-op and the row says nothing about defaults.

### Sidebar navigation, and the two things upstream owns

The kit indexes a page's groups twice, and the two indexes are not the same one.

1. **The sidebar numbers only titled groups** (`reference/gpui-kit/crates/component/src/setting/settings.rs:209-216`), while the page hands that number to a `list` that indexes **every** group matching the search query (`.../setting/page.rs:131-137`). They agree only while no untitled group precedes a titled one. OneTerm therefore keeps untitled groups after all titled groups on a page; the About-page ordering regression protects that alignment without adding a heading to its identity block. **No test guards the rule in general**, and none can from this side: `SettingPage::groups` (`.../setting/page.rs:29`) and `SettingGroup::title` (`.../setting/group.rs:19`) are both `pub(super)`, so a built page cannot be read back to see whether an untitled group precedes a titled one. It is a convention a reviewer has to hold, and today every OneTerm group on every page is titled.

2. **The scroll used to under-shoot on a long page; 0.7.3 fixed it.** Up to 0.7.2 the page called `ListState::scroll_to_reveal_item`, whose forward branch sums the heights of items already laid out, so a group below the fold weighed zero and the jump landed short; OneTerm worked around it by keeping every page inside the window (`US-0122`). The 0.7.3 page calls `ListState::scroll_to(ListOffset { item_ix, .. })` instead (`page.rs:170-180`), which is exact on an unmeasured list, and the long pages came back (`US-0195`).

The sidebar's chevron is upstream too. The caret is its own button and **does** collapse a group (`reference/gpui-kit/crates/component/src/sidebar/menu.rs:312-333`); it is the row that never closes, because `Settings` passes `click_to_open(true)` (`settings.rs:193`), which forces open rather than toggling. OneTerm cannot change that argument and instead keeps the row count down.

## Panel registration and presentation

The persisted panel-name contract is:

| Name | Owner | Panel |
|---|---|---|
| `terminal` | `oneterm-terminal-view` | `TerminalPanel` |
| `session` | `oneterm-session-ui` | `SessionPanel` |
| `sftp` | `oneterm-sftp-ui` | `SftpPanel` |
| `ssh_client_panel` | `oneterm-app` | `SshClientPanel` |

Each registration returns a `panel_handle(...)`, preserving the component-layer `Panel` implementation and tab presentation across the `gpui-base` seam. A bare base panel can compile but would lose its title and controls, so `workspace/layout_tests.rs` verifies all persisted names resolve with their presentation handle.

Panels implement both layers introduced by GPUI Kit 0.6:

- `gpui_base::dock::Panel` supplies base identity and layout behavior such as `panel_name`, `zoomable`, and `set_active`.
- `gpui_component::dock::Panel` supplies presentation behavior such as `tab_name`, `zoom_control`, toolbar/menu content, and `on_added_to(WeakEntity<TabGroup>)`.

`TerminalPanel` retains the final empty tab instead of removing it, preserving the tab bar and `+` creation entry point. That `+` is the panel's `title_suffix`: a dropdown that reads, top to bottom — the platform's local shells (`AddPanelWithShell`); on Windows one `Run as administrator ›` submenu row listing those same three shells (`IN-0043`); a separator labelled "SSH Sessions"; the sessions saved in `ssh_session.json` that have no group; then, per group in the order the groups appear in the store, a dashed separator labelled with the group name followed by that group's sessions; a plain separator; and last the two rows that each name the dialog they open (`US-0114`) — "Quick Connect..." (`NewSession`, the host/port/user/auth dialog) and beside it "New Saved Session...", which opens the full "New SSH Session" dialog (label, colour, group, jump host, port forwards, logging), the same one the right dock's session tree opens. Only the wording of the closing rows changed and one row was added beside them; nothing above the closing separator moved. Session rows show an 8px square in the session's own colour followed by the session title — the same square the right dock's tree draws, with the same `#56B6C2` default when the session has no colour saved, so the two surfaces read alike (`US-0110`). The title is the only text, so two saved sessions sharing a label are indistinguishable here by design (the right dock's tree still shows their `user@host:port`); a blank hand-edited label falls back to `host:port`. The colour reaches the menu as a hex string in the same `WorkspaceCommands` row tuple as the id and the title, resolved by the session feature's `session_color_hex` — the one function the right dock's tree also goes through, so a value neither surface can parse (only a hand-edited `ssh_session.json` holds one) falls back to the same `#56B6C2` in both, and the two can never draw one session differently. With nothing saved the heading carries a disabled "No saved sessions" hint. The two labelled separators are composed from a disabled `PopupMenuItem::element` — a theme-coloured rule, the label, another rule, so the label sits in the middle — because the kit's `PopupMenu` has a separator and a label but not one item that is both. The popup scrolls (and so shows a scrollbar, which the app theme keeps always visible) only when the rows exceed the kit's height cap of half the window or 450px, whichever is smaller; a normal-sized menu carries no scrollbar. This order is the owner's, fixed during the acceptance of `US-0094` (`IN-0033`), which is why the elevated entries went **with the shell block at the top**, as one submenu row rather than three top-level rows: nothing below the shells moves, and "open a shell" never shares a hit target with "open an elevated shell". Picking one shows a UAC prompt and then a **second, marked OneTerm window** with that shell in it — Windows cannot attach an elevated child to a pseudo-console this process owns, so the elevated shell cannot live in this window (`DEC-0019`, [`terminal-backend.md`](terminal-backend.md) §6.1). The window the user was in keeps its tabs, its connections and its transfers, unelevated; declining the prompt does nothing at all. **In an elevated window the submenu is absent** — not disabled — along with the whole SSH block and the right dock: that menu is the three Windows shells and nothing else, because an elevated window runs local shells only (`DEC-0019` M1) and a `runas` from an elevated process would only produce a second identical window. The scroll estimate counts the rows each mode actually emits, so neither menu carries a scrollbar it does not need. Clicking a saved session opens the same connect dialog the `SessionPanel` opens for it. The list and the open call reach the terminal feature through `WorkspaceCommands` rather than a crate edge, because `oneterm-session-ui` already depends on `oneterm-terminal-view` (R5) and the reverse edge would be a cycle. The menu builder closure re-reads the store on every open, so the list is never stale. A local-shell tab is named after the shell it runs — the row label and the tab label are the same string, `ShellKind::display_name` (`crates/core/src/config/shell.rs`), so "PowerShell" in the menu opens a tab reading "PowerShell"; a shell whose settings name a `program` is named after that program's last path component with a trailing `.exe` removed and nothing else (both `/` and `\` separate, so the label never depends on the host OS), and "Terminal" survives only as the reset-tab fallback. A live OSC 0/2 title still wins over the shell name wherever a shell sets one, and a manual rename wins over both (`US-0114`). A terminal tab answers a right-click with its own menu — Rename..., Duplicate, a separator, an **Input Channel** submenu followed by its own separator (disabled while the tab's active Space holds no terminal; `US-0199`, see [Broadcast input channels](#broadcast-input-channels)), Close, Close Others, Close to the Right (`US-0116`). Its **Duplicate** reopens the tab's *live* connection in a second tab (`oneterm_core::SshDuplicateConfig`) and is a different command from the session tree's **Duplicate Saved Session**, which copies a saved row and connects nothing (`US-0129`); the two are named apart for that reason and share no code. Every row acts on the tab that was right-clicked, not on the active one, and the two bulk rows are disabled when they would close nothing and confirm before closing more than one tab; Rename opens the same dialog the unadvertised double-click already opened. The menu lives on the tab *content* OneTerm renders (`crates/terminal-view/src/panel/tab_title.rs`) because the kit attaches no right-click handler to a tab and stops only left mouse-down (`reference/gpui-kit/crates/base/src/tabs.rs:172`). The tab bar's `...` menu opens with the **tab list** — one row per terminal tab, the active one checked, clicking one shows it — contributed through `Panel::dropdown_menu`, the one menu a panel may add rows to (`reference/gpui-kit/crates/component/src/dock/tab_panel.rs:334-336`), so a tab that has scrolled out of the strip is still reachable. **Recorded upstream limits, so the next reader does not re-derive them:** the `...` menu's own "Zoom In" / "Zoom Out" row is appended by the kit after the panel's rows and cannot be removed — `zoom_control()` only enables or disables it (`tab_panel.rs:338-345`) — and its wording is a compiled-in kit locale string (`Dock.Zoom In`, `reference/gpui-kit/crates/component/locales/ui.yml:163`), which a consuming crate cannot override because `rust_i18n` stores translations per crate; renaming it to say it zooms the panel and not the font needs an upstream change, and `docs/PROJECT.md` forbids patching `gpui-component`. The strip itself has no chevrons and scrolls only when the active tab changes (`tab_panel.rs:462-466`); the kit's own `TabBar::menu` overflow list (`crates/component/src/tab/tab_bar.rs:521-552`) is not enabled by the dock and would label every dock tab `Dock.Unnamed`, which is why the tab list is OneTerm's own. Tab width, by contrast, was ours: the kit's `Tab` is content-sized and `flex_shrink_0`, so OneTerm's `w_full()` on the title row contributed nothing and every tab collapsed to its `min_w`, which is how the leftmost tab rendered as a bare close button; the title row is now sized by its label between a 100px floor and a 220px ceiling. When sibling tabs exist, terminal close routes remove the panel through `DockArea::remove_panel`. Adding a terminal to a normalized empty center recreates the center `DockLayout`; otherwise it uses `DockArea::add_panel_view`.

## Zoom

Zoom is a tab-group operation. The shell persists the active panel name in the top-level `zoomed_panel` field. On load it finds the tab-group node whose active panel has that name and calls `DockArea::set_zoomed_in(node, ...)`. The `DockArea` reports the current zoom through `zoomed_group`; the shell maps that node back to its active panel name before saving.

The whole `TerminalPanel` is zoomed, including its internal Space tree and tab-group chrome. Individual terminal Spaces are not dock nodes and are not persisted by the dock layout.

## Vertical tabs

Off by default (`IN-0055`, `US-0196`). **Settings > General > Interface > Vertical Tabs** moves the Terminal Tabs from the strip above the terminal into a list down its left side. The switch applies on the next frame in both directions and persists as `ui_config.json`'s `vertical_tabs` (see `docs/agents/persistence.md`); it rebuilds no panel and ends no session, only the group's chrome changes. The terminal area becomes narrower, so each visible terminal is resized once, as when the right dock opens.

**Which groups draw the rail.** A tab group draws it only when it is in the **centre**, holds at least one panel and every panel is a `terminal` panel. The SSH Client group keeps drawing no tab bar, and a group holding any other panel, or sitting in the right dock (a terminal tab can be dropped there), keeps the kit strip. A split centre draws one rail per group, each beside its own terminals. The rule is `tab_rail::draws_rail`; the group's place is read from `DockArea::layout(DockPlacement::Center)`.

**Rendering seam.** GPUI Kit lays every tab group out as a column (tab bar, then content) and applies that after the skin's frame, and its `TabBar` has no vertical mode. `OneTermTabGroupSkin` therefore draws the rail itself (`crates/workspace/src/layout/workspace/tab_rail.rs`): an absolutely placed element in the group frame (`render_tab_bar`) whose width is the user's stored preference clamped to its own group (see **Resizing** below), and a left **margin** of the same width on the content frame (`content_frame`). A margin, not padding: base installs its drop handling on the content frame, and the rail must stay outside those bounds so a tab dragged over the rail is never measured as a split of the content under it. The active terminal panel is laid out inside the narrower content frame, so it is resized, never overlapped (test `the_rail_is_beside_the_content_and_the_terminal_is_resized_not_overlapped`).

**Anatomy.**

```text
+------------------+----------------------------+
|              [+] |                            |   header (32 px): `+` on the right
|------------------|  terminal                  |
|▌A ● PowerShell   |                            |   row (64 px, two lines): grip | tab element
|▌ C:\work\src   ⟳ |                            |   line 1: chips, ●, title
|⠿ web-01          |                            |   line 2: cwd (cut at its start), status icon
|  /srv/…/releases✓|                            |   ▌ selected: tab.active.background
|  (drop space)    |                            |      and a 2 px bar in the theme's accent
|                  |                            |   ⠿ grip, shown on the hovered row
+------------------+----------------------------+
```

- The header holds the active panel's `title_suffix` (the New Terminal `+` menu), right-aligned. There is **no zoom button** (owner request, 2026-10-21: it had no effect in the rail; the horizontal strip keeps the kit's, and the `ToggleZoom` key binding still works), **no collapse button** (it lives in the status bar, see **Collapsing**), **no `...` menu** (its tab list is the rail) and **no dock toggle** (the status bar and `Ctrl+Shift+B` own it).
- Each row is the panel's own tab element (`TerminalPanel::title`, i.e. `render_tab_strip`) drawn in its vertical layout: it fills the row, has no top bar and no negative margin, and **has no `x` close button**. A row is **64 px and two lines** (`TabRowLayout::VERTICAL_ROW_HEIGHT`, shared with the rail), with a 1 px `border` divider on its bottom edge inside those 64 px (it takes the last pixel of the 10 px bottom padding, 10 + 24 + 20 + 9 + 1; the drop line replaces it on the edge it is drawn on): line 1 is the channel chips, the recording dot and the title (ellipsis at its end); line 2 is the terminal's current directory, left-aligned in `muted.foreground`, and the OSC 7501 status icon right-aligned at its end. The directory is the active Space's OSC 7 working directory (`TerminalSession::cwd()`, sanitised by the session's security policy; a split tab uses the active Space) and updates live; none reported yet means no path (the icon, if any, stays at the right). It is the very string the status bar's breadcrumb shows for that tab (`TerminalPanel::breadcrumb_label`): the path as the shell reported it, with no `~` abbreviation, local or SSH (owner rework 3, 2026-10-21). A path that does not fit is cut at its **start** (`…\src\panel`) by gpui's `TextOverflow::TruncateStart`, measured against the live row width. The grip's glyph is centred on line 1. The row says so through `oneterm_state::TabRowLayout`, set only around the rail's own `title()` calls, so the kit's drag preview and a terminal tab in the right dock still draw the horizontal row. Everything else a strip tab does works unchanged: click selects, middle-click closes (so does the menu's Close, Close Others and Close Tabs Below, and the panel's keyboard paths), double-click renames, right-click opens the tab menu, the OSC 7501 status icon, channel chips and recording dot are drawn, and the title row drags the tab into an empty Space.
- The tab menu's bulk row reads **Close Tabs Below** while the layout is vertical and "Close to the Right" in the strip: the same command on the same tabs.
- **Reorder.** The 12 px leading edge of a row is a grip (`AppIcon::GripVertical`, shown while the row is hovered, with the open-hand cursor). Dragging it starts the kit's `DragPanel`, so a row is dropped on another row (inserted at that row's slot: a tab dragged forward lands after the hovered row, one dragged backward before it, and the 2 px drop line is drawn on the edge where it will land) or below the last row (appended; the last slot for a tab of the same group). The rest of the row is the title row: its own drag (`DragTerminalTab`) wins there and lands only on an empty Space, and **dropping a row's content on another row does nothing**. The trailing status icon is row content and starts the tab drag. A zoomed group is neither draggable nor droppable, and a group with a single tab has nothing to reorder, as with the strip.
- The list scrolls vertically under the header (`overflow_y_scroll` on a base `Tabs`, role `TabList`, vertical orientation) by wheel or touchpad, and **the rail draws no scrollbar, expanded or collapsed** (owner request, 2026-10-21), so no gutter is reserved: a row, its content and its divider use the whole rail width, and the status icon on line 2 sits inside the row's 4 px right padding. The row of a tab that becomes active from elsewhere is scrolled into view; a clicked row is left where it is. Rows are base `Tab`s with `aria_selected` and position `n` of `size`. No keyboard tab navigation exists in either layout.
- Zoom keeps the rail (it is the group's chrome, as the strip is). The lock screen covers it with everything else.
- The selected row's accent bar (and the strip's top bar, and the active Space's cue) is `oneterm_theme::selection_accent`, **not** `Theme::table_active_border`: OneTerm blanks that token after every theme application so the data tables draw no selection border, which used to make the bars vanish after a live theme change. `selection_accent` keeps the theme's own `table.active.border`. The grip-drag preview chip draws `tab.active.foreground` on `tab.active.background` and is as wide as its label up to 240 px.
- Colours are the strip's: `tab.foreground` on `tab_bar.background`, `tab.active.foreground` on a hovered or selected row, `muted.foreground` for the grip, the directory on line 2 (and the strip's `x`). `scripts/check-theme-contrast.py` now measures `tab.active.foreground` on `tab_bar.background` too (the hover pair the strip also draws), floor and hierarchy. **Hover** (owner rework 5 of `US-0196`): an unselected row, expanded or collapsed, is filled across its whole width with the theme's list hover colour (`list.hover.background`, the session tree's and the SFTP rows' hover) behind its content, through a `group_hover` fill inside the row, because the row's own hover style sets only the text colour and gpui refuses a second `hover` on one element. The selected row keeps its active fill and accent bar and takes no hover fill; the 1 px divider is the row's own border and stays. The check measures `tab.active.foreground` and `muted.foreground` on that fill (`tab_bar.hover.background`: the hover colour composited over the bar), hierarchy included; the OSC 7501 bell glyph is a non-text marker, held to WCAG 1.4.11's 3:1 on every surface it is drawn on (an inactive, the selected and a hovered tab or row) by one rule in the script's `GLYPH_SURFACES` tier (owner ruling, 2026-10-21). The collapsed cell's glyph on its status fill (`primary` / `warning` / `success` / `danger`, `US-0198` rework 6) is measured the same way, at the same 3:1: for every variant and fill, the better of `background` and `foreground` against it (the rule `legible_on` applies at run time). That tier gates; five (variant, fill) pairs in Catppuccin Latte, Everforest Light and Solarized Light cannot reach 3:1 with either token (2.61 to 2.99:1) and are recorded in the script's `KNOWN_BELOW`, printed on every run, until the owner retunes those fills.

**Resizing** (`US-0197`). A 4 px handle on the rail's right edge (over its 1 px border) has the column-resize cursor; dragging it sets the width live, in memory, and releasing it stores the preference once through `UiConfig::set_vertical_tabs_width` (`vertical_tabs.width`; a press that never moved, or a width equal to the stored one, writes nothing, and an elevated window applies it in memory and writes nothing). The drag is tracked by a window-level mouse listener the rail registers only while it is on, which also holds the resize cursor for the whole window. The width drawn is `clamp_tab_rail_width(preference, group width)`: at least 140 px and at most `max(140, min(400, 35 % of the rail's own tab group's painted width))`. The group width is read through `on_prepaint` on the group frame (rail included, so the rail never feeds its own bound); when it changes (a window resize, the right dock opening or closing, a centre split) the rail asks for one more frame and re-applies the clamp from the preference, so undoing the change gives the preference back, and in a split centre each rail is bounded by its own group. The stored preference of a drag is the dragged width clamped to 140..=400, so a drag past a narrow window's ceiling stops at the ceiling on screen and a wider window later honours the wider choice. A terminal keeps at least 65 % of its group once the group is 400 px or wider. The rows, and so the directory on each row's second line, follow the live width. The rail rule and the width are worked out once per group per frame (`frame` settles them; `render_tab_bar` and `content_frame` read them).

**Collapsing** (`US-0198`). A button in the **status bar**, just left of the clock and followed by a separator (owner rework 5; `PanelLeftClose`, tooltip "Collapse Tabs"; owner rework 4, 2026-10-21, it was in the rail's header), collapses the rail to a **44 px strip**; while collapsed the same button is `PanelLeftOpen` ("Expand Tabs"). It is shown only while Vertical Tabs is on (the switch, not whether the active tab group draws a rail) and in the elevated window too, where the click changes memory only. The choice is `vertical_tabs.collapsed`, written once per click through `UiConfig::set_vertical_tabs_collapsed` (a value equal to the stored one writes nothing; the elevated window applies it in memory and writes nothing), applied on the next frame, remembered across a restart, and **kept while the Vertical Tabs switch is off** (turning it on again returns to the strip). The collapsed width is `VerticalTabs::COLLAPSED_WIDTH` whatever the stored width preference or the group's width is (`tab_rail::rail_width`), the content frame's margin follows, and **the resize handle is not drawn**; expanding returns the preference's width.

```text
+----+------------------
| [+]|    header (32 px): `+`, centred
|----|
|▌>_  |   a 44 x 44 cell per tab: the shell's icon centred (cmd, PowerShell, else
| ▒▒▒ |   the generic terminal glyph); the OSC 7501 state is the cell's background
|● >_ |   (▒ = filled), the recording dot top-left; ▌ selected as expanded
+----+------------------
```

- A cell is the panel's own tab element again (`render_tab_strip`), drawn under `TabRowLayout::Collapsed`. **It shows the shell's icon, not a position number** (`US-0198` rework 6, owner, 2026-10-22): `AppIcon::Cmd` for `cmd`, `AppIcon::Powershell` for Windows PowerShell and PowerShell 7, the generic `AppIcon::Terminal` for everything else (Bash and Git Bash, Zsh, Sh, WSL, SSH, a custom shell, an empty tab). The shell comes from the active terminal's launch data (`duplicate_config`: the local shell's `program` name when it sets one, else its `ShellKind`; never the window title), classified by `tab_title::shell_icon`. `cmd.svg` and `powershell.svg` are original strokes, not vendor logos. **The OSC 7501 state is the cell's background, and the cell has no status badge**: Working `primary`, Blocked `warning`, Done `success`, Error `danger`, with the glyph (and the recording dot on it) in whichever of the theme's `background` and `foreground` reads better on the fill (`oneterm_theme::legible_on`, a computed pick, owner ruling 2026-10-22: the status `*_foreground` tokens are not used); Idle or no state leaves the cell as it was. The fill never lies under the 2 px accent bar of the selected cell (it starts after it) and it hides the hover fill, so a filled cell shows no hover tint (the hover card is its hover feedback). Expanded rows and the horizontal strip keep their status icon, and the hover card keeps the full row with it. It has no title, directory, channel chips or close button in the cell itself; hovering it shows a **hover card** with the expanded row's content (`US-0198` rework 5): line 1 the channel chips, the recording dot and the title, line 2 the directory (the status bar's string, cut at its start) and the OSC 7501 status icon at the end. It is the kit's tooltip (popover fill, `popover.foreground`, `border`) 220 px wide, built by the same function as the expanded row's two lines (`vertical_row_lines`) from the live tab, so the two cannot drift apart (the status icon keeps its own tooltip). Click selects, middle-click closes, the right-click menu is the expanded one ("Close Tabs Below" included, "Rename..." in the menu), and the cell's content drags the tab into an empty Space. **Rename by double-click is gone** (there is no label to double-click). Broadcast channel chips are not drawn in the strip.
- The reorder grip is the same 12 px column, but on a cell it overlays the left edge (painted after the content, so it still wins the press there) and its glyph is centred on the cell.
- Collapsing or expanding re-reveals the displayed tab (the row heights change, so the old pixel scroll offset would point elsewhere). The collapsed header is the expanded one's 32 px with only the `+`, centred. A right-press on a cell hides its hover card until the pointer returns to the cell, so the card does not sit on top of the menu. A middle or right press on the 12 px grip overlay reaches the cell's own close / menu handlers (accepted: a larger target).
- Cells are 44 px high and stack and scroll like rows. **The strip draws no scrollbar, and neither does the expanded rail** (see the list bullet above). For the strip the reason is that the kit's scrollbar takes every press inside its track (16 px by default, 6 px with a custom style) to page the list and passes nothing on (`gpui-base` `scrollbar.rs`), and OneTerm shows it always (`ScrollbarMode::Always`), so any track over a 43 px cell takes clicks from it, and a thumb thin enough to leave the cell clickable cannot be dragged. The list still scrolls by wheel or touchpad, and the displayed tab is scrolled into view whenever it changes or the rail collapses or expands.

An elevated window reads the saved layout and shows it; the switch works in memory there and the write is refused.

## Persistence

`oneterm_state::dock_persistence::DockDocument` is the only read/update API for `docks.json`. It owns:

- `schema_version` (currently `1`),
- flattened GPUI Kit dock fields (`version`, `center`, and optional side docks),
- shell-owned `zoomed_panel`, and
- SFTP-owned `sftp_table_state`.

`MAIN_DOCK_VERSION` remains `3`. GPUI Kit 0.6 retains the shipped JSON container names `"StackPanel"` and `"TabPanel"`; these are persisted schema labels, not current Rust type names. OneTerm preserves the pre-0.6 single-leaf right-dock encoding when saving so older files round-trip without semantic drift.

A loaded document feeds the right-dock layout, width, and open state. The width in that document is the user's **preference**, not the width that was on screen: the workspace writes `preferred_right_dock_width` into every save and clamps only when applying (see §Dock composition). Storing the applied width instead would ratchet the preference down — one session at a narrow window and the wide dock would be gone for good. Startup intentionally resets the center to one terminal tab while retaining right-dock state and then restores zoom by panel name. Because that reset always replaces the center, `load_layout` replaces the document's center with an empty stack node before handing the state to `DockArea::load`, so the saved center's panels are never built. Building a panel is what starts its session — a `terminal` panel spawns a local shell — so building the center here would start a shell only to discard it milliseconds later, which on Windows can kill a `cmd.exe` mid-initialisation and raise the `0xc0000142` "Application Error" dialog. Writes use `update_dock_document_at`, preserve `sftp_table_state`, make a backup, and quarantine invalid data rather than overwriting it. See [`agents/persistence.md`](agents/persistence.md) for storage and ownership rules.

## Broadcast input channels

A terminal Space joins one of five channels (A..E) from its **tab's** context menu (`US-0199`;
it was the terminal area's menu until 2026-10-21, and that menu no longer has it). The tab menu
(the strip, a vertical row and a collapsed cell share it) has an "Input Channel" submenu between
Duplicate and Close: `Channel A`..`Channel E` (the current one marked with a `* ` prefix), then
`Leave Channel` and `Close Channel <X>` for a member, then `Join All Spaces In Tab To <X>` and
`Leave With All Spaces In Tab` when the tab holds more than one Space. Membership is still per
Space (`DEC-0009`), so the single-Space rows act on the right-clicked tab's **active Space**
(the Space its directory, status icon and recording dot report, and the one the key bindings act
on); to put another Space of a split tab in a channel, focus it first or use the tab-wide rows.
While the active Space holds no terminal (its shell failed to spawn) the row stays, disabled, so the menu keeps its shape; a terminal whose process exited still offers the submenu. The five joins, the leave, and the close
are also actions in Settings > Key Bindings (group "Input Channel"), shipped unbound.

### Program status icon (OSC 7501)

A Terminal Tab with a single Space shows its terminal's OSC 7501 program status as an icon
after the channel chips and the recording dot, before the label on the strip and at the right end of the second line of a row in the vertical rail (`IN-0049`, `DEC-0022`, `IN-0055`): a
rotating `loader-circle` in `primary` while **working**, a `bell` pulsing between 35 % and full
opacity in `warning` while **blocked** (waiting on the user), a `circle-check` in `success` when
**done**, a `circle-x` in `danger` on **error**, and nothing for **idle** or no report. Several
records (subagents) show the highest of `blocked > error > working > done > idle`. A **split
tab shows no icon** (owner ruling): it has no room to say which Space reports. The icon is on
the selected tab too; **done** and **error** clear once seen — when the tab is selected, or
when the user types into a terminal whose tab is already selected — and **working** /
**blocked** end when the process exits or the next shell prompt starts (OSC 133 `A`). The
tooltip says `Working`, `Waiting for you`, `Done` or `Error`; the program's own `title` /
`msg` text is never shown. Under reduced motion both animations stand still. The bell's
`warning` is held to 4.5:1 on the tab surfaces by `check-theme-contrast.py`.

Membership is painted in two places. The tab strip shows one chip per distinct channel of
the tab's Spaces, in A..E order, before the recording dot; the chip is a 16 px square with
square corners holding the channel letter in the theme's `chart_1..chart_5`, centred by the
text node rather than by flex so the glyph sits in the middle of the square. Each member
Space carries the same chip as a badge in its top-right corner, inset past the 12 px
scrollbar track, including the lone Space of an unsplit tab. While a member Space is the
selected one, its active-Space highlight takes the channel colour; unselected Spaces keep
the plain border, and the single Space of an unsplit tab stays borderless. The badge is the
only thing in that corner — `US-0117` briefly put a `#N` chip beside it and the owner removed
the chip on 2026-09-17, leaving the active-Space ring as the whole cue. The
badge takes no focus and its clicks activate the Space like any other click in it. Both read the `InputChannelRegistry`, and every `TerminalPanel` observes it,
so a `Close Channel` performed in one tab repaints the others.

## Status bar

The status bar contains, at its left end, the **Collapse / Expand Tabs button** and a 1 px vertical separator after it (only while Vertical Tabs is on; see [Vertical tabs](#vertical-tabs), `US-0198` rework 4 and 5; the separator is the bar's own, the same `border`-coloured rule that divides its other items) just before the clock, then the clock, breadcrumb, git status of the active local terminal's cwd (branch, `*` when dirty, `(+added -removed)` line counts against `HEAD` in the success/danger colours, ahead/behind counts; polled every 2 s on the background executor, hidden for SSH sessions and non-repositories), active-terminal network speed, CPU/memory indicator (sampled every 2 s on the background executor, not the UI thread — F1, 2026-09-28), and right-dock controls. The indicator's `MEM` is the figure Task Manager's "Memory" column shows, the process's private working set; where the OS does not give one (before Windows 10 22H2 or Windows 11 22H2 with the September 2023 cumulative update) it is the full working set, shared pages included (about 136 MB idle instead of 51), and on Linux and macOS it is the resident set size. It is not commit, which counts pages never touched (`US-0137`). Hovering the indicator shows a table from the same 2 s sample (`US-0148`): under **Memory** the private working set, the working set, commit (private bytes) and the peak working set on Windows (a figure the OS does not give is left out), or the resident set size and the virtual size on Linux and macOS; under **CPU** the usage with its denominator (`1.2% of 16 logical cores`), CPU time (user plus kernel), the thread count (`n/a` on macOS) and the process uptime. Row names are `muted.foreground`, titles and values the kit tooltip's `popover.foreground`. The table tracks the item's own 2 s cadence for as long as the tooltip stays open: its content is a small entity (`DetailsTooltip`, `crates/workspace/src/widgets/status_text.rs`) that observes the indicator and re-renders on every tick, rather than a snapshot baked in when the pointer entered (2026-09-28 acceptance rework of `US-0148`; the kit only calls a tooltip's builder closure once, when it is first shown, so a snapshot would have frozen there). Each text indicator carries a leading icon (clock, folder, git branch, network, CPU) that hides with its label, and all indicator text uses the theme foreground colour (only the diffstat counts are coloured). Terminal-derived widgets resolve the active panel through the dock tree and then the active Space inside `TerminalPanel`; an empty Space yields no terminal metrics.

The bar neither wraps nor scrolls, so something has to give when the window is narrow. The two indicators that can grow without bound — the cwd breadcrumb and the git branch — sit in the bar's **centre** region, which GPUI Kit lays out as `flex-1` with a zero basis: it takes what the pinned ends leave and can never push them. The clock on the left and the network, CPU/memory and dock-toggle controls on the right therefore keep their width at any window size, which is what keeps `MEM 577.0 MB` whole (a value and its unit are one token, and every pinned indicator is `Shorten::Never`).

Inside that region `build_status_bar` divides the room between the two: it **measures** every label through the window's text system, gives the branch what it asks for while the path keeps at least 80 px, and takes it out of the branch below that (down to 40 px of branch). The path then elides from the **left**, at a path separator, behind a leading ellipsis (`…\scratchpad\ux\home`), keeping the directory the user is in; the branch elides from the **right**, keeping the head that identifies it (`worktree-agent-a18…`). Only a single component longer than the whole budget is cut inside itself. A shortened indicator shows its full value in a tooltip, and click-to-copy still copies the sampled path, not the shortened one.

## FPS HUD

Off by default (`IN-0048`, `US-0151`). `ui_config.json`'s `show_fps` switches it; three entry
points write it through `UiConfig::set_show_fps`: the app menu's checked **Show FPS Monitor**
item, the `ToggleFpsMonitor` action (Settings > Key Bindings > App Menu, "Toggle FPS
Monitor", shipped unbound per `DEC-0018`), and the **Show FPS Monitor** switch in Settings >
General > Interface.

When on, `OneTermWorkspace` renders the `FpsHud` view (`crates/workspace/src/widgets/fps_hud.rs`)
at its root, pinned 12 px from the right edge and 12 px below the tab strip
(`TITLE_BAR_HEIGHT` + 32 px), so it covers no caption button, right-dock toggle, tab-strip `+`
or `...`; it lies over the top-right of whatever panel is there. It is OneTerm's own (GPUI
Kit's `gpui-fps` was dropped on the owner's acceptance ruling of 2026-09-29, `US-0151`) and
has four rows, in theme tokens only (`popover` fill, `border`, `muted.foreground` labels,
`popover.foreground` values):

| Row | Value | Source |
|---|---|---|
| `FPS` | Frames the window actually drew in the last second | The HUD view is not cached, so it renders on every frame the window draws; it counts its renders and turns the count into a rate once a second on the `until_next_tick` grid. It never asks for a frame of its own beyond that once-a-second refresh, so an idle window reads about 2 and a window under output reads its real rate. It is not a refresh rate or a "max". |
| `GPU` | The adapter gpui renders with, on one line (a long name widens the HUD leftwards from its 220 px minimum) | `Window::gpu_specs().device_name` (the DXGI adapter description on Windows), asked once when the HUD is shown; `(software)` for an emulated device, `n/a` where gpui returns none (macOS in gpui-pre 0.3.9). |
| `API` | `Direct3D 11`, `Metal` or `Vulkan/GL (wgpu)` | A compile-time constant per platform: gpui-pre 0.3.9 reports neither the backend nor the Direct3D feature level at run time. |
| `GPU usage` | This process' GPU share | Windows: PDH `\GPU Engine(*)\Utilization Percentage`, this pid's busiest single engine across all GPUs, clamped to 100 % — how Task Manager defines its GPU column. The query is opened on the background executor when the HUD is shown (the row reads `-` until the first sample) and closed when it is hidden, and sampled every 2 s on the background executor. `n/a` on Linux and macOS. |

The HUD is opaque to the mouse except for the wheel (`block_mouse_except_scroll`): a click or
a drag on it never reaches the terminal underneath (no selection change, nothing sent to a
mouse-reporting TUI), while the wheel still scrolls it. The other side of that: whatever the HUD
covers cannot be clicked while it is shown (with the SSH Client dock, the right part of
"Search sessions..."); switch the HUD off to reach it.

Off, the workspace holds no HUD: no ticker, no PDH query, no per-frame count. On, the HUD
costs one `u64` add per frame, one refresh a second (which lands on the status bar clock's
own tick, so an idle window stays at about 2 frames a second), one PDH collection every 2 s off
the UI thread, and about 4-5 MB for PDH's counter data. The first show in a process also pays
PDH's one-time initialisation, off the UI thread: ~100-165 ms on a background thread and
~4 MB that Windows keeps for the life of the process (no growth on later shows).
Measurements: `US-0151`.

## Lock screen

Once a master password exists (`IN-0050`, `US-0159`), every OneTerm window — main, Settings,
and any later `Root` — shows only the lock screen after **Lock After Idle** minutes without
keyboard or mouse input (Settings > Security > Screen Lock; 15 by default, 1-480, no "Never"),
or at once on **Lock OneTerm** (app menu; Settings > Security > Lock Now; the `LockOneTerm`
action, `ctrl-shift-l` by default and rebindable under Key Bindings > App Menu). It also
starts locked: whenever OneTerm starts with a usable vault, every window's first frame is the
lock screen and the application is not drawn before the master password is typed (intake Q1
re-ruled 2026-10-13; always on, no setting); the idle timer starts at that unlock. Without a
vault (not set up, or `vault.json` unusable) nothing locks, at start or later, and the action
says why. The lock
drops the vault key; the master password lifts it and unlocks the vault. Terminal output,
timers and notifications are not input; a key, a mouse move, click or scroll, or switching to a
OneTerm window is. Input that arrives later than the timeout (the machine slept, measured on
the wall clock as well as the monotonic one) locks before it is delivered.

`crates/workspace/src/lock_screen.rs` registers `LockScreen`, a `gpui_base::RootPlugin`, after
`gpui_component::init`, so its `decorate` runs last in every window. While locked it returns
`window_border()` around the lock view **in place of** the window's surface: the workspace (or
the Settings panel), its dialogs, toasts, tooltips and the FPS HUD are not rendered, laid out
or painted, so no frame and no screenshot holds a terminal pixel and a hidden terminal's output
asks for no frame (gpui invalidates only windows whose last frame rendered the notifying view).
Sessions keep running underneath; open dialogs, inputs, terminals and the dock come back
unchanged after the unlock, and each window gets its previous focus back. Element-keyed state
does not survive a lock: the Settings window comes back on General, whatever page it showed
(known limit, the kit keeps the page in element state). A native OS dialog (file picker, Save
dialog) is not covered by the lock and stays usable over it; whatever is picked in it while
locked is dropped.

The lock view has its own title bar (drag, minimise, maximise, close; the elevated window's
`(Administrator)` marker), the lock icon, "OneTerm is locked", a masked password input focused
on show (Enter submits), the error line ("Wrong master password.", cleared input) and the
back-off countdown shared with the master-password dialogs (2, 4, 8, 16, then 30 s from the
fifth wrong password in a row), an Unlock button that spins while Argon2id runs off the UI
thread, a paragraph on how to recover a forgotten master password (the lock screen cannot reset
it: quit OneTerm and delete `vault.json` in the config folder, whose absolute path the
paragraph prints; OneTerm then starts without a lock and every saved password is lost, while
saved sessions are kept and ask for their passwords again), and **Quit OneTerm**. The password
input keeps the keyboard while locked, whatever is clicked or opened under the lock; a dialog
opened under it (a crash report at start) gets the focus at the unlock, and no Unlock dialog is
ever stacked under the lock screen. It names no
host, tab, session or notification. While locked, one app-wide keystroke interceptor stops
every keystroke except text and the password input's own editing keys (a keystroke passes only
inside the lock view, and only when every binding it resolves to there is `input::*` or
`root::*`), so no workspace or global shortcut fires, `ctrl-shift-q` included; an empty Enter
is not an attempt. Closing a window with its close button still works, and closing the main
window quits (Alt+F4 passes through the same keystroke filter as an unbound key and was not
walked). The lock screen checks the password against the vault header held in memory (read at
start, replaced by a set-up, change or rehash in this process), never against `vault.json` on
disk, so a vault reset or replaced by another OneTerm cannot open it.

## Text contrast floor and hierarchy

Text in the built-in themes obeys two rules, both enforced by
`scripts/check-theme-contrast.py` in the quality gate, so a new theme in
`crates/theme/themes/` cannot ship breaking either.

**The floor.** Every checked token clears **4.5:1** (WCAG AA for body text) on every surface
the script's `SURFACES` table lists for it, in every variant of every built-in theme.
Secondary text — host addresses, search placeholders, empty-state copy, key-binding chips,
`Default:` hints, SFTP dates, column headers, inactive tab labels — is `muted.foreground`,
`tab.foreground` and `table.head.foreground`; for `muted.foreground` the surfaces are the
window body, popovers and a hovered menu row, the key-binding chip's own fill, the sidebar,
the title and status bars, list and table rows in their plain, alternating and
hovered/selected states, and both an inactive and the active tab. Primary text — every label,
name and value beside them — is `foreground` plus the tokens the kit routes specific surfaces
through, each falling back to `foreground` when a theme omits it: `popover.foreground`,
`sidebar.foreground`, `tab.active.foreground` (the selected tab, and the hovered tab or rail row on the bar's own fill and under the rail's hover fill), `accent.foreground` (the label of a hovered or
selected menu row) and `secondary.foreground` (text on a secondary fill — a `Secondary` button
or tag). `foreground` itself is measured on the window body, popovers, the title and status
bars, the **tab strip** (the dock panel headers "Session" and "SFTP Browser" inherit it: the
kit colours a header only when a panel supplies a `title_style`, and none does) and list and
table rows in their plain, alternating, hovered/selected and header states.

**The hierarchy.** On every surface where a primary token and `muted.foreground` are both
drawn, the primary token's ratio is **strictly greater** than the secondary's. Clearing the
floor is not enough on its own: raising only `muted.foreground` satisfies it and still leaves
a page where the host address shouts louder than the session name (`US-0111` did exactly
that, and `US-0127` is the correction). A tie fails too — text as loud as the label beside it
has no hierarchy left.

That `SURFACES` table is the contract, and it is only as complete as its last review: a
surface missing from it is not measured, so a component that starts drawing one of these
tokens on a new background adds the surface there (each entry cites the line that draws the
pair) instead of assuming an existing entry covers it. Pair each foreground with the token the
kit actually paints on that surface, and remember that one surface can carry more than one:
the tab strip shows an inactive tab's `tab.foreground` *and* a dock header's `foreground`, so
both are listed against it. Declaring a surface out of scope is the one move this table cannot
make safely — an omission reads exactly like a pair that was never drawn. The floor is a
minimum, not a target:
secondary values land near 5:1 so that secondary text still reads as secondary, primary ones
sit about half a ratio point or more above them, and a few themes carry untouched tokens far
above both (`Molokai Light`'s `foreground` is 19:1).

## Frames and re-rendering

gpui draws a frame only after something calls `cx.notify()` (or `window.refresh()`), and
every frame is built from the window root down: `Root`, `OneTermWorkspace`, the kit's
title bar, the dock area with every tab group and tab strip all run their `render` and are
laid out, prepainted and painted again (the title bar's content and the status bar are
cached views since `US-0150`, below). `cx.notify()` marks the notified view and
each of its ancestors dirty; a child view is skipped only when it is embedded with
`.cached(style)` and is not dirty. GPUI Kit caches each tab group's active panel, so a
terminal frame reuses the right dock's contents. Measured cost (`IN-0046`, `US-0145`): about
4 ms of UI-thread time per idle frame, about 2,100 heap allocations outside every OneTerm
render, with taffy layout, heap traffic and the title bar's toggle group and window
controls the largest parts; OneTerm's own render functions are about 5 % of it.

The rules that follow from it:

- **Fewer frames is the lever.** A notify costs a whole-window frame, so a view notifies
  only when what it draws changed. A terminal in a tab its group is not showing does not
  notify on output (the tab strip reads that view, so the window tracks it); showing the
  tab re-renders it, because `set_active(true)` notifies the panel. What a removed frame
  buys depends on the load: an idle or uncapped window draws fewer frames and spends less
  CPU; a window already at its frame-rate cap under output spends the freed frames on the
  shown terminal instead, which refreshes more often (smoother, lower latency) at no CPU
  saving (`US-0145`: two-tab TUI, -51 % UI cycles uncapped, +39 % with twice the shown-tab
  refreshes when capped).
- **Repainting timers share one grid.** The cursor blink and every status-bar indicator
  sleep with `oneterm_state::until_next_tick`, which wakes on a multiple of the interval on
  the wall clock, so the blink (500 ms), the clock (1 s) and the resource indicator (2 s)
  land in one frame: an idle, focused window draws about 2 frames a second, an unfocused
  one about 1.25 (measured 2.04-2.11 and 1.23-1.25, before the Agent panel was withdrawn
  by `DEC-0022`). Animations keep their own cadence.
- **What OneTerm caches (`US-0150`).** Two views are embedded with `.cached(style)`, so
  a frame another view asked for (a blink, a status tick, terminal output) reuses them:
  - the title bar's content (`TitleBarContent` in `crates/workspace/src/layout/title_bar.rs`:
    icon, app menu, elevation suffix), laid out to fill
    the kit's `bar` row (`flex_1`, `h_full`);
  - the status bar (`StatusBarView` in `crates/workspace/src/layout/statusbar.rs`), at
    `status_bar_height(rem)` = 1.75 rem + 1 px.

  A cached view re-renders only when it or a view inside it notifies, when the window
  is refreshed (theme switch, resize, DPI change: `Window::bounds_changed` refreshes), or
  when its bounds change. So **whatever a cached view reads must notify it**: the status bar's indicators are child views, so an indicator's notify re-renders the
  bar (where the labels are measured and the budgets set). A cached view is laid out from
  its style, never measured: the title bar's content takes the kit row's size, and the
  status bar's height is pinned against the uncached layout at four rem sizes by
  `status_bar_height_matches_the_kit_layout`, so a kit change that resizes the bar fails
  the test instead of shifting pixels. Caching the tab bar's `+` button was measured and
  dropped (no gain above noise); the rest of the tab-bar chrome is the kit's.
- **A cached view falls back to uncached while accessibility is active.** gpui-pre 0.3.9
  builds AccessKit nodes during prepaint and does not replay them for a reused view, so a
  cached view's menus and buttons leave the UI Automation tree on every frame that reuses
  it. Embed through `crate::layout::cached_unless_a11y` (`crates/workspace/src/layout/mod.rs`),
  which caches only while `window.is_a11y_active()` is false and otherwise lays the view
  out uncached in a box of the same style (AccessKit activation refreshes the window, so
  the switch applies on the next frame). Keep the fallback until gpui replays AccessKit
  nodes for reused views. The kit's own cached right-dock panel has the same gap, on main
  as well (upstream).
- **Do not cache the kit's `TitleBar` itself or the dock area.** gpui does not replay a
  cached view's window-control hitboxes, so a cached `TitleBar` stops moving the window
  and its caption buttons stop working after its first reused frame; the drag region
  (the kit's `bar` row) and min/max/close must stay in an uncached view (`AppTitleBar`
  renders the kit's `TitleBar` uncached around the cached content). Re-rendering a cached
  view sets gpui's `refreshing` flag for its whole subtree, so a cached dock area that a
  terminal frame dirties re-renders every panel in it, including the right dock the kit
  had cached: measured worse than not caching it. For the same reason a cached view
  inside another cached view is only reused while the outer one is clean.

## Source map

- Workspace state and zoom: `crates/workspace/src/layout/workspace/mod.rs`
- Default/reset layouts: `crates/workspace/src/layout/workspace/layout.rs`
- Add-panel and right-mode actions: `crates/workspace/src/layout/workspace/actions.rs`
- GPUI Kit rendering seam: `crates/workspace/src/layout/workspace/dock_skin.rs`
- Persistence compatibility: `crates/workspace/src/layout/workspace/persistence.rs`
- Shared dock traversal: `crates/state/src/dock_util.rs`
- Persisted document owner: `crates/state/src/dock_persistence.rs`
- Registered names: `crates/state/src/panel_names.rs`
- Input channel membership: `crates/state/src/input_channel_registry.rs`
- The tab bar's `+` menu (shells, saved sessions, Quick Connect / New Saved Session): `crates/terminal-view/src/panel/terminal_panel.rs`
- Saved-session menu rows and the commands behind them: `crates/session-ui/src/tree_builder.rs`, `crates/session-ui/src/lib.rs`
- Channel submenu, chips, and Space badge: `crates/terminal-view/src/input/menu.rs`,
  `crates/terminal-view/src/panel/tab_title.rs`, `crates/terminal-view/src/space/render.rs`
- Focused layout regressions: `crates/workspace/src/layout/workspace/layout_tests.rs`
- Vertical tab rail: `crates/workspace/src/layout/workspace/tab_rail.rs` (tests: `tab_rail_tests.rs`), the vertical row layout `crates/state/src/tab_row_layout.rs` and `crates/terminal-view/src/panel/tab_title.rs`; its setting: `crates/settings/src/ui_config.rs`, `crates/settings-ui/src/general.rs`
- FPS HUD overlay: `crates/workspace/src/widgets/fps_hud.rs`; its switch: `crates/settings/src/ui_config.rs`, `crates/workspace/src/layout/app_menus.rs`
- Built-in themes and the contrast floor: `crates/theme/themes/`, `scripts/check-theme-contrast.py`
