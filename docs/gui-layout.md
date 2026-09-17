# GUI layout — current implementation

This document describes the shipped workspace shell. Historical design sketches live in Git history; current dependency and crate boundaries are in [`architecture.md`](architecture.md) and [`agents/dependencies.md`](agents/dependencies.md).

## Frame and ownership

`OneTermWorkspace` (`crates/workspace/src/layout/workspace/`) renders three vertical regions:

```text
TitleBar
DockArea
StatusBar
```

The feature-agnostic workspace crate owns the frame, dock layout, dock persistence, right-dock mode switching, and status widgets. Feature crates register their panels by stable names from `oneterm_state::panel_names`; the shell constructs panels through `PanelRegistry` and does not import feature implementations. `oneterm-app` is the composition root and owns the composite SSH Client right-dock panel because it may depend on both Session and SFTP feature crates.

## Dock composition

The shell uses the GPUI Kit 0.6 dock tree:

```text
DockArea (id = "main-dock", version = 3)
├── Center: DockLayout::v_split
│   └── DockLayout::tabs
│       └── one or more TerminalPanel values
└── Right: DockLayout::tabs
    └── exactly one mode panel
        ├── SshClientPanel (SSH Client mode), or
        └── AgentListView (Agent mode)
```

`RightDockMode::None` closes the existing right dock without replacing its content. Selecting SSH Client or Agent mode builds the registered panel, replaces the right-dock layout, preserves its width, and opens it.

`SshClientPanel` owns a vertical `v_resizable` split containing `SessionPanel` and `SftpPanel`, each with its own header. Its section headers show the hosted panel's `title_suffix` (the SFTP Browser's expand/collapse toggle) in a control group framed like the center tab bar's trailing buttons. It is zoomable: expanding the SFTP Browser (IN-0025) zooms this node so the browser fills the workspace like a zoomed terminal tab, with the Session section hidden until it collapses. The right dock still has a tab-group node internally, but `OneTermDockSkin` suppresses that outer tab bar for the single `ssh_client_panel` or `agent_panel` leaf. Center terminal groups keep the standard GPUI Kit tab chrome.

Layout construction uses `DockLayout::{tabs,v_split}` and `DockArea::{set_center,set_dock,set_dock_size,set_dock_collapsible}`. Runtime traversal uses `DockArea::layout`, `PaneNode`/`PaneRef`, and stable `PanelId`/`NodeId` values. Tab behavior is provided by `TabGroup`; rendering customization is isolated behind `DockSkin`, `DockAreaRenderer`, and `TabGroupRenderer`.

## Settings window

`oneterm-settings-ui` composes the GPUI Kit 0.6 `Settings` widget in a standalone window. It uses `GroupBoxVariant::Outline`, matching the v0.6.0 Settings story so every setting group has the theme border, radius, and content padding supplied by `GroupBox`. Items use GPUI Kit's standard group gap without custom divider rows. Editable fields declare their built-in default or a custom reset handler; GPUI Kit therefore shows the page-level `Reset All` action only while that page differs from its defaults and persists the reset through the same field setters.

Every OneTerm-owned focus target exposes an ID and accessibility role. The Settings root and feature-panel content roots are named `Pane` nodes, the Key Bindings capture target is a `TextInput`, and repository targets are `Link` elements. GPUI Base focuses the host-supplied `DockAreaRenderer`, `TabGroupRenderer`, and `TilesRenderer` frames directly, so `OneTermDockSkin` also assigns pane roles at those seams; this keeps the published dependency unmodified while preventing node-less focus targets. Docked panels return a separate navigation focus proxy to `TabGroup` and forward it to their independently tracked content focus after that frame; a tab-group frame and panel content must never track the same handle because both accessibility nodes would claim focus in one frame.

The upstream sidebar numbers only titled groups, while page scrolling indexes every group. OneTerm therefore keeps untitled groups after all titled groups on a page; the About-page ordering regression protects that alignment without adding a heading to its identity block.

## Panel registration and presentation

The persisted panel-name contract is:

| Name | Owner | Panel |
|---|---|---|
| `terminal` | `oneterm-terminal-view` | `TerminalPanel` |
| `session` | `oneterm-session-ui` | `SessionPanel` |
| `sftp` | `oneterm-sftp-ui` | `SftpPanel` |
| `ssh_client_panel` | `oneterm-app` | `SshClientPanel` |
| `agent_panel` | `oneterm-agent-ui` | `AgentListView` |

Each registration returns a `panel_handle(...)`, preserving the component-layer `Panel` implementation and tab presentation across the `gpui-base` seam. A bare base panel can compile but would lose its title and controls, so `workspace/layout_tests.rs` verifies all persisted names resolve with their presentation handle.

Panels implement both layers introduced by GPUI Kit 0.6:

- `gpui_base::dock::Panel` supplies base identity and layout behavior such as `panel_name`, `zoomable`, and `set_active`.
- `gpui_component::dock::Panel` supplies presentation behavior such as `tab_name`, `zoom_control`, toolbar/menu content, and `on_added_to(WeakEntity<TabGroup>)`.

`TerminalPanel` retains the final empty tab instead of removing it, preserving the tab bar and `+` creation entry point. That `+` is the panel's `title_suffix`: a dropdown that reads, top to bottom — the platform's local shells (`AddPanelWithShell`); a separator labelled "SSH Sessions"; the sessions saved in `ssh_session.json` that have no group; then, per group in the order the groups appear in the store, a dashed separator labelled with the group name followed by that group's sessions; a plain separator; and last the two rows that each name the dialog they open (`US-0114`) — "Quick Connect..." (`NewSession`, the host/port/user/auth dialog) and beside it "New Saved Session...", which opens the full "New SSH Session" dialog (label, colour, group, jump host, port forwards, logging), the same one the right dock's session tree opens. Only the wording of the closing rows changed and one row was added beside them; nothing above the closing separator moved. Session rows show an 8px square in the session's own colour followed by the session title — the same square the right dock's tree draws, with the same `#56B6C2` default when the session has no colour saved, so the two surfaces read alike (`US-0110`). The title is the only text, so two saved sessions sharing a label are indistinguishable here by design (the right dock's tree still shows their `user@host:port`); a blank hand-edited label falls back to `host:port`. The colour reaches the menu as a hex string in the same `WorkspaceCommands` row tuple as the id and the title, resolved by the session feature's `session_color_hex` — the one function the right dock's tree also goes through, so a value neither surface can parse (only a hand-edited `ssh_session.json` holds one) falls back to the same `#56B6C2` in both, and the two can never draw one session differently. With nothing saved the heading carries a disabled "No saved sessions" hint. The two labelled separators are composed from a disabled `PopupMenuItem::element` — a theme-coloured rule, the label, another rule, so the label sits in the middle — because the kit's `PopupMenu` has a separator and a label but not one item that is both. The popup scrolls (and so shows a scrollbar, which the app theme keeps always visible) only when the rows exceed the kit's height cap of half the window or 450px, whichever is smaller; a normal-sized menu carries no scrollbar. This order is the owner's, fixed during the acceptance of `US-0094` (`IN-0033`). Clicking a saved session opens the same connect dialog the `SessionPanel` opens for it. The list and the open call reach the terminal feature through `WorkspaceCommands` rather than a crate edge, because `oneterm-session-ui` already depends on `oneterm-terminal-view` (R5) and the reverse edge would be a cycle. The menu builder closure re-reads the store on every open, so the list is never stale. A local-shell tab is named after the shell it runs — the row label and the tab label are the same string, `ShellKind::display_name` (`crates/core/src/config/shell.rs`), so "PowerShell" in the menu opens a tab reading "PowerShell"; a custom shell is named after its program's file stem, and "Terminal" survives only as the reset-tab fallback. A live OSC 0/2 title still wins over the shell name wherever a shell sets one, and a manual rename wins over both (`US-0114`). A terminal tab answers a right-click with its own menu — Rename..., Duplicate, a separator, Close, Close Others, Close to the Right (`US-0116`). Every row acts on the tab that was right-clicked, not on the active one, and the two bulk rows are disabled when they would close nothing and confirm before closing more than one tab; Rename opens the same dialog the unadvertised double-click already opened. The menu lives on the tab *content* OneTerm renders (`crates/terminal-view/src/panel/tab_title.rs`) because the kit attaches no right-click handler to a tab and stops only left mouse-down (`reference/gpui-kit/crates/base/src/tabs.rs:172`). The tab bar's `...` menu opens with the **tab list** — one row per terminal tab, the active one checked, clicking one shows it — contributed through `Panel::dropdown_menu`, the one menu a panel may add rows to (`reference/gpui-kit/crates/component/src/dock/tab_panel.rs:334-336`), so a tab that has scrolled out of the strip is still reachable. **Recorded upstream limits, so the next reader does not re-derive them:** the `...` menu's own "Zoom In" / "Zoom Out" row is appended by the kit after the panel's rows and cannot be removed — `zoom_control()` only enables or disables it (`tab_panel.rs:338-345`) — and its wording is a compiled-in kit locale string (`Dock.Zoom In`, `reference/gpui-kit/crates/component/locales/ui.yml:163`), which a consuming crate cannot override because `rust_i18n` stores translations per crate; renaming it to say it zooms the panel and not the font needs an upstream change, and `docs/PROJECT.md` forbids patching `gpui-component`. The strip itself has no chevrons and scrolls only when the active tab changes (`tab_panel.rs:462-466`); the kit's own `TabBar::menu` overflow list (`crates/component/src/tab/tab_bar.rs:521-552`) is not enabled by the dock and would label every dock tab `Dock.Unnamed`, which is why the tab list is OneTerm's own. Tab width, by contrast, was ours: the kit's `Tab` is content-sized and `flex_shrink_0`, so OneTerm's `w_full()` on the title row contributed nothing and every tab collapsed to its `min_w`, which is how the leftmost tab rendered as a bare close button; the title row is now sized by its label between a 100px floor and a 220px ceiling. When sibling tabs exist, terminal close routes remove the panel through `DockArea::remove_panel`. Adding a terminal to a normalized empty center recreates the center `DockLayout`; otherwise it uses `DockArea::add_panel_view`.

## Zoom

Zoom is a tab-group operation. The shell persists the active panel name in the top-level `zoomed_panel` field. On load it finds the tab-group node whose active panel has that name and calls `DockArea::set_zoomed_in(node, ...)`. The `DockArea` reports the current zoom through `zoomed_group`; the shell maps that node back to its active panel name before saving.

The whole `TerminalPanel` is zoomed, including its internal Space tree and tab-group chrome. Individual terminal Spaces are not dock nodes and are not persisted by the dock layout.

## Persistence

`oneterm_state::dock_persistence::DockDocument` is the only read/update API for `docks.json`. It owns:

- `schema_version` (currently `1`),
- flattened GPUI Kit dock fields (`version`, `center`, and optional side docks),
- shell-owned `zoomed_panel`, and
- SFTP-owned `sftp_table_state`.

`MAIN_DOCK_VERSION` remains `3`. GPUI Kit 0.6 retains the shipped JSON container names `"StackPanel"` and `"TabPanel"`; these are persisted schema labels, not current Rust type names. OneTerm preserves the pre-0.6 single-leaf right-dock encoding when saving so older files round-trip without semantic drift.

A loaded document feeds the right-dock layout, width, and open state. Startup intentionally resets the center to one terminal tab while retaining right-dock state and then restores zoom by panel name. Because that reset always replaces the center, `load_layout` replaces the document's center with an empty stack node before handing the state to `DockArea::load`, so the saved center's panels are never built. Building a panel is what starts its session — a `terminal` panel spawns a local shell — so building the center here would start a shell only to discard it milliseconds later, which on Windows can kill a `cmd.exe` mid-initialisation and raise the `0xc0000142` "Application Error" dialog. Writes use `update_dock_document_at`, preserve `sftp_table_state`, make a backup, and quarantine invalid data rather than overwriting it. See [`agents/persistence.md`](agents/persistence.md) for storage and ownership rules.

## Broadcast input channels

A terminal Space joins one of five channels (A..E) from its context menu: an "Input Channel"
submenu sits after the Split items and lists `Channel A`..`Channel E` (the current one marked
with a `* ` prefix), then `Leave Channel` and `Close Channel <X>` for a member, then
`Join All Spaces In Tab To <X>` and `Leave With All Spaces In Tab` when the tab holds more
than one Space. The five joins, the leave, and the close are also actions in
Settings > Key Bindings (group "Input Channel"), shipped unbound.

Membership is painted in two places. The tab strip shows one chip per distinct channel of
the tab's Spaces, in A..E order, before the recording dot; the chip is a 16 px square with
square corners holding the channel letter in the theme's `chart_1..chart_5`, centred by the
text node rather than by flex so the glyph sits in the middle of the square. Each member
Space carries the same chip as a badge in its top-right corner, inset past the 12 px
scrollbar track, including the lone Space of an unsplit tab. While a member Space is the
selected one, its active-Space highlight takes the channel colour; unselected Spaces keep
the plain border, and the single Space of an unsplit tab stays borderless. Since `US-0117` the badge shares that corner with the Space's own number chip — `#N`, the
same 16px footprint as the badge, on inactive Spaces only, with what the Space holds on its
tooltip: one row, the chip left of the badge, so neither hides the other and neither is wide
enough to cover live output. The
badge takes no focus and its clicks activate the Space like any other click in it. Both read the `InputChannelRegistry`, and every `TerminalPanel` observes it,
so a `Close Channel` performed in one tab repaints the others.

## Status bar

The status bar contains the clock, breadcrumb, git status of the active local terminal's cwd (branch, `*` when dirty, `(+added -removed)` line counts against `HEAD` in the success/danger colours, ahead/behind counts; polled every 2 s on the background executor, hidden for SSH sessions and non-repositories), active-terminal network speed, CPU/memory indicator, and right-dock controls. Each text indicator carries a leading icon (clock, folder, git branch, network, CPU) that hides with its label, and all indicator text uses the theme foreground colour (only the diffstat counts are coloured). Terminal-derived widgets resolve the active panel through the dock tree and then the active Space inside `TerminalPanel`; an empty Space yields no terminal metrics.

## Secondary text contrast floor

Secondary text — host addresses, search placeholders, empty-state copy, key-binding chips,
`Default:` hints, SFTP dates, column headers, inactive tab labels — is drawn in one of three
theme tokens: `muted.foreground`, `tab.foreground` and `table.head.foreground`. In every
variant of every built-in theme each of those clears **4.5:1** (WCAG AA for body text) on
every surface the `SURFACES` table in `scripts/check-theme-contrast.py` lists for it — for
`muted.foreground` that is the window body, popovers and a hovered menu row, the key-binding
chip's own fill, the sidebar, the title and status bars, list and table rows in their plain,
alternating and hovered/selected states, and both an inactive and the active tab. The quality
gate runs the check, so a new theme in `crates/theme/themes/` cannot ship below the floor.

That table is the contract, and it is only as complete as its last review: a surface missing
from it is not measured, so a component that starts drawing one of these tokens on a new
background adds the surface there (each entry cites the line that draws the pair) instead of
assuming an existing entry covers it. The floor is also a minimum, not a target — raised
values land near 5:1 so that secondary text still reads as secondary, while a few themes
carry untouched tokens far above the floor (`Molokai Light`'s `tab.foreground` inherits the
primary `foreground` at 19:1).

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
- Built-in themes and the contrast floor: `crates/theme/themes/`, `scripts/check-theme-contrast.py`
