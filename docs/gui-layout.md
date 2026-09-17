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

A mode click decides against the panel the right dock **contains**, never against the persisted mode: `None` hides the dock, a mode whose panel is already there is shown (opened if it was collapsed, with the same panel instance and the same width), and only a mode the dock does not have is built. So an SFTP connection and the session list survive every reopen, and an explicit click always means "show me this", never "hide this".

The dock buttons in the tab bar and the status bar toggle the dock through the kit, so the workspace mirrors the dock's open state back into `UiConfig.right_dock_mode`: collapsing the dock with either button selects `None` in the title bar's segmented control, and reopening it with the same button reselects the mode of the panel that comes back. The dock's open state is the truth; the persisted mode follows it. Clicking **SSH Client** after such a collapse is therefore a mode change back from `None` — and it still takes the show path, because the panel never left the dock.

`SshClientPanel` owns a vertical `v_resizable` split containing `SessionPanel` and `SftpPanel`, each with its own header. Its section headers show the hosted panel's `title_suffix` (the SFTP Browser's expand/collapse toggle) in a control group framed like the center tab bar's trailing buttons. It is zoomable: expanding the SFTP Browser (IN-0025) zooms this node so the browser fills the workspace like a zoomed terminal tab, with the Session section hidden until it collapses. The right dock still has a tab-group node internally, but `OneTermDockSkin` suppresses that outer tab bar for the single `ssh_client_panel` or `agent_panel` leaf. `AgentListView` draws its own header in the same shape (title `Agents`, tab-bar background, trailing control group) in both its empty and its populated state, so either mode shows exactly one header and no tab bar. Center terminal groups keep the standard GPUI Kit tab chrome.

The right dock's width is bounded by a share of the window: `clamp_right_dock_width` caps a requested width at 35 % of the window width, with a 240 px floor so a very narrow window keeps a usable dock rather than collapsing it. The workspace remembers the width the user last set (`preferred_right_dock_width`) and applies the clamp of that value at each seam that sets the width — both layout builders, the mode swap, and the window-resize observer — so narrowing the window narrows the dock and widening it brings the user's width back. A drag inside the allowed range is never fought and becomes the new preference; a drag past the ceiling is capped where it happens rather than left to be snapped back later. `docks.json` keeps the kit's own flattened width field and needs no schema change, but what is written there is the **preference** (see §Persistence), so a session at a narrow window never costs the user their wide dock.

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

`TerminalPanel` retains the final empty tab instead of removing it, preserving the tab bar and `+` creation entry point. That `+` is the panel's `title_suffix`: a dropdown that reads, top to bottom — the platform's local shells (`AddPanelWithShell`); a separator labelled "SSH Sessions"; the sessions saved in `ssh_session.json` that have no group; then, per group in the order the groups appear in the store, a dashed separator labelled with the group name followed by that group's sessions; a plain separator; and "New SSH Session" (`NewSession`) last. Session rows show an 8px square in the session's own colour followed by the session title — the same square the right dock's tree draws, with the same `#56B6C2` default when the session has no colour saved, so the two surfaces read alike (`US-0110`). The title is the only text, so two saved sessions sharing a label are indistinguishable here by design (the right dock's tree still shows their `user@host:port`); a blank hand-edited label falls back to `host:port`. The colour reaches the menu as a hex string in the same `WorkspaceCommands` row tuple as the id and the title, resolved by the session feature's `session_color_hex` — the one function the right dock's tree also goes through, so a value neither surface can parse (only a hand-edited `ssh_session.json` holds one) falls back to the same `#56B6C2` in both, and the two can never draw one session differently. With nothing saved the heading carries a disabled "No saved sessions" hint. The two labelled separators are composed from a disabled `PopupMenuItem::element` — a theme-coloured rule, the label, another rule, so the label sits in the middle — because the kit's `PopupMenu` has a separator and a label but not one item that is both. The popup scrolls (and so shows a scrollbar, which the app theme keeps always visible) only when the rows exceed the kit's height cap of half the window or 450px, whichever is smaller; a normal-sized menu carries no scrollbar. This order is the owner's, fixed during the acceptance of `US-0094` (`IN-0033`). Clicking a saved session opens the same connect dialog the `SessionPanel` opens for it. The list and the open call reach the terminal feature through `WorkspaceCommands` rather than a crate edge, because `oneterm-session-ui` already depends on `oneterm-terminal-view` (R5) and the reverse edge would be a cycle. The menu builder closure re-reads the store on every open, so the list is never stale. When sibling tabs exist, terminal close routes remove the panel through `DockArea::remove_panel`. Adding a terminal to a normalized empty center recreates the center `DockLayout`; otherwise it uses `DockArea::add_panel_view`.

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

A loaded document feeds the right-dock layout, width, and open state. The width in that document is the user's **preference**, not the width that was on screen: the workspace writes `preferred_right_dock_width` into every save and clamps only when applying (see §Dock composition). Storing the applied width instead would ratchet the preference down — one session at a narrow window and the wide dock would be gone for good. Startup intentionally resets the center to one terminal tab while retaining right-dock state and then restores zoom by panel name. Because that reset always replaces the center, `load_layout` replaces the document's center with an empty stack node before handing the state to `DockArea::load`, so the saved center's panels are never built. Building a panel is what starts its session — a `terminal` panel spawns a local shell — so building the center here would start a shell only to discard it milliseconds later, which on Windows can kill a `cmd.exe` mid-initialisation and raise the `0xc0000142` "Application Error" dialog. Writes use `update_dock_document_at`, preserve `sftp_table_state`, make a backup, and quarantine invalid data rather than overwriting it. See [`agents/persistence.md`](agents/persistence.md) for storage and ownership rules.

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
the plain border, and the single Space of an unsplit tab stays borderless. The
badge takes no focus and its clicks activate the Space like any other click in it. Both read the `InputChannelRegistry`, and every `TerminalPanel` observes it,
so a `Close Channel` performed in one tab repaints the others.

## Status bar

The status bar contains the clock, breadcrumb, git status of the active local terminal's cwd (branch, `*` when dirty, `(+added -removed)` line counts against `HEAD` in the success/danger colours, ahead/behind counts; polled every 2 s on the background executor, hidden for SSH sessions and non-repositories), active-terminal network speed, CPU/memory indicator, and right-dock controls. Each text indicator carries a leading icon (clock, folder, git branch, network, CPU) that hides with its label, and all indicator text uses the theme foreground colour (only the diffstat counts are coloured). Terminal-derived widgets resolve the active panel through the dock tree and then the active Space inside `TerminalPanel`; an empty Space yields no terminal metrics.

The bar neither wraps nor scrolls, so something has to give when the window is narrow. The two indicators that can grow without bound — the cwd breadcrumb and the git branch — sit in the bar's **centre** region, which GPUI Kit lays out as `flex-1` with a zero basis: it takes what the pinned ends leave and can never push them. The clock on the left and the network, CPU/memory and dock-toggle controls on the right therefore keep their width at any window size, which is what keeps `MEM 577.0 MB` whole (a value and its unit are one token, and every pinned indicator is `Shorten::Never`).

Inside that region `build_status_bar` divides the room between the two: it **measures** every label through the window's text system, gives the branch what it asks for while the path keeps at least 80 px, and takes it out of the branch below that (down to 40 px of branch). The path then elides from the **left**, at a path separator, behind a leading ellipsis (`…\scratchpad\ux\home`), keeping the directory the user is in; the branch elides from the **right**, keeping the head that identifies it (`worktree-agent-a18…`). Only a single component longer than the whole budget is cut inside itself. A shortened indicator shows its full value in a tooltip, and click-to-copy still copies the sampled path, not the shortened one.

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
- The tab bar's `+` menu (shells, New SSH Session, saved sessions): `crates/terminal-view/src/panel/terminal_panel.rs`
- Saved-session menu rows and the commands behind them: `crates/session-ui/src/tree_builder.rs`, `crates/session-ui/src/lib.rs`
- Channel submenu, chips, and Space badge: `crates/terminal-view/src/input/menu.rs`,
  `crates/terminal-view/src/panel/tab_title.rs`, `crates/terminal-view/src/space/render.rs`
- Focused layout regressions: `crates/workspace/src/layout/workspace/layout_tests.rs`
- Built-in themes and the contrast floor: `crates/theme/themes/`, `scripts/check-theme-contrast.py`
