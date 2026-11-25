# Low-Level Design: Dock migration (DockItem → DockLayout, TabPanel → TabGroup, Panel split)

Intake: IN-0017
HLD: ../high-level-design.md
Topic: dock-migration
Date: 2026-09-07

## Concern

The v0.6.0 dock redesign, which is the substance of this migration: 46 `gpui_component::dock`
references, 5 `Panel` impls, 5 `register_panel` sites, and the workspace shell that builds and
traverses the layout tree.

## Design

### What upstream changed

v0.6.0 split the dock in two. `gpui_base::dock` owns behavior: a `PaneTree` of pure-data
`PaneNode`s (`Split` | `Tabs` | `Tiles`), addressed by stable `NodeId` / `PanelId`, reconciled
into container entities by `DockArea`. `gpui_component::dock` is a *skin* over it — it draws the
tab bar, dock chrome and drop indicators, and re-exports everything base exports so a consumer
need not name `gpui_base::dock` for types.

Consequences for OneTerm, each verified against the v0.6.0 source:

| 0.5.2 | 0.6.0 |
| --- | --- |
| `DockItem::{Split, Tabs, Panel}` enum, `DockItem::v_split/tabs/panel(..)` | `DockLayout` builder: `DockLayout::v_split()/h_split().child(..)`, `DockLayout::tabs().panel(..)/.panel_view(..)`, `DockLayout::tiles()` |
| `TabPanel` / `StackPanel` entities | `TabGroup` entity (a `Tabs` node's projection); splits have no entity |
| `DockArea::new(..)` draws chrome by itself | `DockArea::new(..).with_renderer(DockSkin::new(cx))`, or `DockSkin::dock_area(id, version, window, cx) -> (Entity<DockArea>, Rc<DockSkin>)` |
| `.panel_style(PanelStyle::TabBar)` on `DockArea` | `DockSkin::set_panel_style(PanelStyle::TabBar, cx)` |
| `da.left_dock()/right_dock()/bottom_dock()` | `da.layout(DockPlacement::Left)` → `Option<&PaneTree>`; `is_dock_open`, `dock_size`, `set_dock`, `set_dock_size`, `remove_dock`, `toggle_dock` |
| `trait Panel` (one trait: name + title + closable + zoomable + dump) | split: `gpui_base::dock::Panel` (name, `visible`, `closable`, `zoomable`, `set_active`, `set_zoomed`, `on_added_to`, `on_removed`, `dump`) + `gpui_component::dock::Panel` (`tab_name`, `title`, `title_style`, `title_suffix`, `toolbar_buttons`, `dropdown_menu`, `zoom_control`, `inner_padding`) |
| `fn zoomable(&self) -> Option<PanelControl>` | `zoomable(&self) -> bool` (base) **and** `zoom_control(&self) -> Option<PanelControl>` (component) |
| `register_panel(cx, name, \|dock_area, state, info, window, cx\| ..)` | `register_panel(cx, name, \|ctx: PanelBuildContext, window, cx\| -> Arc<dyn PanelView>)`, with `ctx.dock_area()/state()/info()` |
| `PanelRegistry::build_panel(name, dock_area, &state, &info, window, cx)` | `PanelRegistry::build_panel(name, PanelBuildContext::new(dock_area, &state, &info), window, cx)` |
| `DockEvent::DragDrop(item)` | `DockEvent::DragDrop { item, target }` |
| `PanelState { panel_name, children, info }` built literally | same public fields, plus `PanelState::new(name)` |

**The presentation seam is the subtle part.** Base holds panels as
`Arc<dyn gpui_base::dock::PanelView>` and Rust cannot recover a sub-trait object from a
super-trait object, so a panel handed to base *directly* (`DockLayout::tabs().panel(entity)`,
`DockArea::add_panel`) loses its presentation: it still docks, drags and persists, but the skin
draws its `panel_name` where its title should be. Every OneTerm panel must therefore go in
wrapped: `panel_handle(entity)` → `DockLayout::tabs().panel_view(..)` /
`DockArea::add_panel_view(..)`, and every `register_panel` builder must return
`Arc::new(PanelHandle::new(entity))`. Using the bare-entity entry points compiles cleanly and
fails only visually — tabs showing `Terminal` instead of the session title. This is the single
most likely silent regression in the whole migration and needs an explicit assertion.

### Per-site mapping

**`crates/workspace/src/layout/workspace/mod.rs`** — construct the area with a skin, keep
`MAIN_DOCK_ID` / `MAIN_DOCK_VERSION`, move `panel_style` onto the skin handle, and store the
`Rc<DockSkin>` on `OneTermWorkspace` (the skin is the handle for later settings changes):

```rust
let (dock_area, skin) = DockSkin::dock_area(MAIN_DOCK_ID, Some(MAIN_DOCK_VERSION), window, cx);
skin.set_panel_style(PanelStyle::TabBar, cx);
```

The zoom-restore path (`restore_zoom`, `zoomed_panel`) currently finds the `TabPanel` whose
active panel matches a name and re-zooms it; it moves to `TabGroup` + `PanelId`. Note
`zoomable()` and `zoom_control()` are now two separate answers and base refuses a zoom that
fails `zoomable()` however it was requested.

**`crates/state/src/dock_util.rs`** — `collect_tab_panels` walks `DockItem::{Tabs,Split}` to
collect `Entity<TabPanel>`; it becomes a `PaneTree`/`PaneNode` walk collecting `Entity<TabGroup>`
over `layout(Center)` + the three placements. `set_right_dock_open` moves from
`right_dock()` + manual open flag to `is_dock_open` / `toggle_dock`.

**`crates/workspace/src/layout/workspace/layout.rs` + `actions.rs`** — all `DockItem::v_split` /
`::tabs` / `::panel` construction becomes `DockLayout`. `actions.rs` additionally contains the
IN-0015 "ghost TabPanel" workaround: it inspects the center `DockItem` for a `Tabs` whose active
panel is `None` and recreates the center. v0.6.0 normalizes the tree after every edit and drops
empty containers by construction, so **that workaround is probably obsolete**. Do not delete it
on that reasoning alone — re-run the IN-0015 and IN-0016 regression tests, and remove it only if
they pass without it, recording the removal in the packet.

**`crates/terminal-view`** — `panel/terminal_panel.rs` holds a `WeakEntity<TabPanel>` for
close-policy and Agent click-to-focus, and uses the vendor patches `set_active_panel` and
`panel_count`. Both are replaceable with public 0.6.0 API: `TabGroup::panels()` + `select_tab(ix)`
for activation, `TabGroup::panels().len()` for the count. `on_added_to(group, ..)` now delivers
the group handle to the panel directly, which is cleaner than the current manual wiring.
`space/drag.rs` and `agent.rs` follow the same `TabPanel` → `TabGroup` rename.

**`crates/app/src/ssh_client_panel.rs`** — a raw `DockItem::Panel` right dock that draws its own
title bars and splits internally, precisely because `DockItem::Panel` was rendered without
chrome and could not be a `v_split` child. In 0.6.0 there is no `Panel` node variant at all — a
panel can only live in a `Tabs` or a `Tiles` node. The equivalent is a single-panel
`DockLayout::tabs()` whose group draws no tab bar; whether that is `PanelStyle::Auto`, a
`zoom_control` of `None`, or `inner_padding(false)` needs to be settled against the skin's
actual rendering. This is the one site where a faithful visual match is not a mechanical rename,
and its `dump()` currently persists `PanelInfo::panel(Null)` deliberately to control the reload
path — so it interacts with `03-persistence-compatibility.md`.

**Panel impls (5 crates)** — split each into two impls. Mechanical except `zoomable`:

```rust
impl gpui_base::dock::Panel for TerminalPanel {
    fn panel_name(&self) -> &'static str { panel_names::TERMINAL }
    fn closable(&self, _: &App) -> bool { .. }
    fn zoomable(&self, _: &App) -> bool { true }
    fn set_active(&mut self, active: bool, window: &mut Window, cx: &mut Context<Self>) { .. }
    fn dump(&self, cx: &App) -> PanelState { .. }
}

impl gpui_component::dock::Panel for TerminalPanel {
    fn title(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement { .. }
    fn zoom_control(&self, _: &App) -> Option<PanelControl> { Some(PanelControl::Both) }
}
```

### Order of work inside P2

1. `crates/state/src/dock_util.rs` (tree traversal — everything else reads through it).
2. The five `Panel` impls + `register_panel` builders (independent of each other).
3. `crates/workspace` layout construction, skin install, zoom restore.
4. `crates/app/src/ssh_client_panel.rs` (needs the skin's real rendering behavior).
5. `terminal-view` tab-group wiring, dropping the vendor-patch calls.
6. Tests: `workspace/layout_tests.rs`, `terminal-view/panel/tests.rs`, `space/tests.rs`.

## Interfaces

```rust
// gpui_base::dock (re-exported from gpui_component::dock)
DockArea::new(id, version, window, cx) -> Self
DockArea::with_renderer(self, Rc<dyn DockAreaRenderer>) -> Self
DockArea::layout(&self, DockPlacement) -> Option<&PaneTree>
DockArea::set_center(&mut self, DockLayout, &mut Window, &mut Context<Self>)
DockArea::set_dock(&mut self, DockPlacement, DockLayout, &mut Window, &mut Context<Self>)
DockArea::add_panel_view(&mut self, Arc<dyn PanelView>, DockPlacement, Option<Pixels>, ..)
DockArea::is_dock_open / dock_size / set_dock_size / remove_dock / toggle_dock
DockArea::dump(&self, &App) -> DockAreaState
DockArea::load(&mut self, DockAreaState, &mut Window, &mut Context<Self>) -> Result<()>

TabGroup::panels(&self) -> &[Arc<dyn PanelView>]
TabGroup::active_ix(&self) -> usize
TabGroup::active_panel(&self, &App) -> Option<Arc<dyn PanelView>>
TabGroup::select_tab(&mut self, usize, &mut Window, &mut Context<Self>)
TabGroup::close_panel(&mut self, PanelId, &mut Context<Self>)

// gpui_component::dock
DockSkin::dock_area(id, version, window, cx) -> (Entity<DockArea>, Rc<DockSkin>)
DockSkin::set_panel_style(&self, PanelStyle, &mut App)
panel_handle<P: Panel>(Entity<P>) -> Arc<dyn gpui_base::dock::PanelView>
PanelHandle::new / ::of / ::panel
```

## Edge Cases and Failure Modes

- [ ] A panel installed with `panel(..)`/`add_panel(..)` instead of the `_view` variants loses
      its title. Compiles fine. Proof: assert rendered tab titles, not just that panels exist.
- [ ] `zoomable() -> bool` defaults to `true` in base; a panel that previously returned
      `zoomable() -> None` to refuse zoom must now return `false` **and** `zoom_control() -> None`.
- [ ] The `SshClientPanel` right dock renders a tab bar it never had, or loses its internal split.
- [ ] The IN-0015 ghost-`TabPanel` workaround is removed on the assumption normalization covers
      it, and the "SSH connect after closing all tabs" bug returns.
- [ ] IN-0016's "keep the last terminal tab alive" placeholder behavior depends on close policy
      that now lives partly in `TabGroupConstraints::is_closable` and base's "a dock's last group
      must stay" rule; re-verify rather than assume.
- [ ] `set_active` semantics tightened upstream: exactly one notification per edge, delivered on
      the next tick, and a removed panel is told `on_removed` instead of `set_active(false)`.
      OneTerm's active-terminal tracking may depend on the old timing.
- [ ] `DockEvent::LayoutChanged` firing frequency may differ; the statusbar deliberately emits it
      manually because `toggle_dock` did not. Re-check whether it still needs to.

## Verification

- [ ] `crates/workspace/src/layout/workspace/layout_tests.rs` passes unchanged in intent.
- [ ] `crates/terminal-view/src/panel/tests.rs` (last-tab, sibling-tab, duplicate-action) passes.
- [ ] `crates/terminal-view/src/space/tests.rs` passes.
- [ ] New focused test: every panel installed by the shell reports a non-`panel_name` title via
      the recovered `PanelHandle`, proving presentation survived the base seam.
- [ ] IN-0015 and IN-0016 regression scenarios re-run manually (close all tabs → SSH connect;
      close last tab → placeholder Space with a working `+` button).
- [ ] Manual UAT: drag a tab between groups, split R/L/U/D, zoom in/out, collapse the right dock.
