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

`SshClientPanel` owns a vertical `v_resizable` split containing `SessionPanel` and `SftpPanel`, each with its own header. The right dock still has a tab-group node internally, but `OneTermDockSkin` suppresses that outer tab bar for the single `ssh_client_panel` or `agent_panel` leaf. Center terminal groups keep the standard GPUI Kit tab chrome.

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

`TerminalPanel` retains the final empty tab instead of removing it, preserving the tab bar and `+` creation entry point. When sibling tabs exist, terminal close routes remove the panel through `DockArea::remove_panel`. Adding a terminal to a normalized empty center recreates the center `DockLayout`; otherwise it uses `DockArea::add_panel_view`.

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

A loaded document feeds the right-dock layout, width, and open state. Startup intentionally resets the center to one terminal tab while retaining right-dock state and then restores zoom by panel name. Writes use `update_dock_document_at`, preserve `sftp_table_state`, make a backup, and quarantine invalid data rather than overwriting it. See [`agents/persistence.md`](agents/persistence.md) for storage and ownership rules.

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
scrollbar track, including the lone Space of an unsplit tab, and is framed in its channel
colour whether or not it is the active Space. A Space in no channel keeps the theme's
active/inactive border rule, and the single Space of an unsplit tab stays borderless. The
badge takes no focus and its clicks activate the Space like any other click in it. Both read the `InputChannelRegistry`, and every `TerminalPanel` observes it,
so a `Close Channel` performed in one tab repaints the others.

## Status bar

The status bar contains the clock, active-terminal network speed, breadcrumb, CPU/memory indicator, and right-dock controls. Terminal-derived widgets resolve the active panel through the dock tree and then the active Space inside `TerminalPanel`; an empty Space yields no terminal metrics.

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
- Channel submenu, chips, and Space badge: `crates/terminal-view/src/input/menu.rs`,
  `crates/terminal-view/src/panel/tab_title.rs`, `crates/terminal-view/src/space/render.rs`
- Focused layout regressions: `crates/workspace/src/layout/workspace/layout_tests.rs`
