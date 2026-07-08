# GUI Layout — OneTerm

> Design document for the OneTerm GUI layout, based on the reference
> `reference/gpui-component/crates/story/examples/dock.rs`.
>
> All gpui-component APIs are extracted from the reference (see
> [`docs/agents/dependencies.md` § 5](agents/dependencies.md)), no web_search used.

## Table of contents

1. [Overview & diagram](#1-overview--diagram)
2. [Reference → OneTerm mapping](#2-reference--oneterm-mapping)
3. [DockArea architecture](#3-dockarea-architecture)
4. [Panel trait — implementation requirements](#4-panel-trait--implementation-requirements)
5. [Panel serialization registry](#5-panel-serialization-registry)
6. [Layout state save/load](#6-layout-state-saveload)
7. [Title bar & App menu bar](#7-title-bar--app-menu-bar)
8. [Status bar & DateTimeClock](#8-status-bar--datetimeclock)
9. [Resizable behavior](#9-resizable-behavior)
10. [File structure](#10-file-structure)
11. [Implementation checklist](#11-implementation-checklist)

---

## 1. Overview & diagram

OneTerm keeps the reference `dock.rs` three-block vertical frame:
**TitleBar → DockArea → StatusBar**, only changing the *contents* of DockArea and
StatusBar.

```
┌─────────────────────────────────────────────────────────────────┐
│  TitleBar  [OneTerm ▾] [Edit] [Window] [Help]   [⚙][🐙][🔔]      │
├───────────────────────────────────────────────┬─────────────────┤
│                                                │                 │
│   CENTER  (70% width)                          │  RIGHT DOCK     │
│   ┌──────────────────────────────────────────┐ │  (30% width)    │
│   │Tab1│Tab2│Tab3│ + │info│zoom│collapse│ ✕  │ │  ┌────────────┐  │
│   ├──────────────────────────────────────────┤ │  │ Session    │  │
│   │                                          │ │  │ [placeholder]│  │
│   │           [Terminal view]                │ │  └────────────┘  │
│   │      (terminal only, nothing else)       │ │  ↕ v_split       │
│   │                                          │ │  ┌────────────┐  │
│   │                                          │ │  │ SFTP       │  │
│   └──────────────────────────────────────────┘ │  │ [placeholder]│  │
│                                                │  └────────────┘  │
│            ↔ resizable center ↔ right_dock    │  [collapse ▸]   │
├─────────────────────────────────────────────────────────────────┤
│  🕐 2025-01-15 14:32:07                  [Toggle Right Dock]      │
└─────────────────────────────────────────────────────────────────┘
```

### Finalized design decisions

| # | Decision | Rationale |
|---|---|---|
| 1 | **Right = `set_right_dock`** (side dock) = `v_split([Session, SFTP])` | Matches "as currently (Image/Icon)" — reference also uses `right_dock` = `v_split([Image, Icon])`. Keeps the native toggle button + collapse/resize. |
| 2 | **Drop `set_left_dock` and `set_bottom_dock`** entirely | Only need Center (Terminals) + Right (Session/SFTP). StatusBar keeps only toggle Right + datetime. |
| 3 | **Add Panel menu** → only "New Terminal Tab" + "Show/Hide Dock Toggle Button" | Keeps the "title bar functionality unchanged" spirit, with content rationalized for "terminals only". |

---

## 2. Reference → OneTerm mapping

| Reference `dock.rs` component | OneTerm | Notes |
|---|---|---|
| `StoryWorkspace { title_bar, dock_area, last_layout_state, toggle_button_visible, _save_layout_task }` | `OneTermWorkspace` (rename, keep fields) | `app/src/app.rs` |
| `AppTitleBar::new("Examples", ...)` | `AppTitleBar::new("OneTerm", ...)` | Change title |
| `AppMenuBar` (`app_menus.rs`: Appearance/Theme/Language + Edit/Window/Help) | Keep 100% | Themes + Language + Appearance |
| `FontSizeSelector` (font-size, gutter toggle) | Keep (drop radius/scrollbar/list-highlight) | radius=0px, scrollbar=Scrolling fixed in `theme::init`; list.active_highlight=true fixed; gutter toggle → `TerminalSettings.show_gutter` |
| `DockArea::new("main-dock", Some(version), window, cx)` | `version = 1` (bump) | Triggers reset prompt when old layout differs from version |
| Center = `DockItem::v_split` with 19 story tabs | Center = `DockItem::tabs([TerminalPanel, ...])` | Terminal only, nothing else |
| `set_left_dock(...)` | **Drop** | — |
| `set_bottom_dock(...)` | **Drop** | — |
| `set_right_dock(DockItem::v_split([Image, Icon]), Some(px(320.)), true, ...)` | `set_right_dock(DockItem::v_split([Session, Sftp]), Some(px(480.)), true, ...)` | 30% of ~1600px window |
| `set_dock_collapsible(Edges{left:true,bottom:true,right:true})` | `set_dock_collapsible(Edges{right:true, ..Default::default()})` | Only right_dock remains |
| `DockAreaState` save/load `STATE_FILE`, version check, reset prompt | Keep | `STATE_FILE = "target/docks.json"` (debug) |
| `AddPanel` action + dropdown (add random story) | Dropdown only "New Terminal Tab" + "Show/Hide Dock Toggle Button" | Drop Add to Left/Bottom/Right + menu check Sidebar/Dialog/... |
| StatusBar: 3 toggle buttons (left/bottom/right) | StatusBar: `.left(DateTimeClock)` + `.right(toggle-right-dock)` | — |

---

## 3. DockArea architecture

### 3.1 DockItem structure

```
DockArea (id="main-dock", version=1)
├── center:  DockItem::tabs([TerminalPanel, TerminalPanel, ...])
│            • each tab = 1 Terminal, no other panel added
│            • zoom/close/info/collapse unchanged (via Panel trait)
│            • placeholder "No terminal session" when empty
│
└── right_dock: DockItem::v_split([
        DockItem::tab(SessionPanel),    size auto   ← top half
        DockItem::tab(SftpPanel),                     ← bottom half
    ])
    • set_right_dock(panel, Some(window_w * 0.30), true, window, cx)
    • set_dock_collapsible(Edges{ right:true, .. })
    • collapse/expand button in corner + toggle button in status bar
    • v_split resizable (draggable divider between Session/SFTP)
    • placeholder "No active session" / "No SFTP connection"
```

### 3.2 Constructor API (extracted from reference)

`DockItem` constructors (`reference/.../dock/mod.rs`):

```rust
// Tabs — center (multiple terminals)
DockItem::tabs(
    items: Vec<Arc<dyn PanelView>>,
    dock_area: &WeakEntity<DockArea>,
    window: &mut Window, cx: &mut App,
) -> DockItem

// Single tab — used for leaf inside v_split
DockItem::tab<P: Panel>(
    item: Entity<P>,
    dock_area: &WeakEntity<DockArea>,
    window: &mut Window, cx: &mut App,
) -> DockItem
// = DockItem::new_tabs(vec![Arc::new(item.clone())], None, ...)

// Vertical split — right_dock (Session on top, SFTP on bottom)
DockItem::v_split(
    items: Vec<DockItem>,
    dock_area: &WeakEntity<DockArea>,
    window: &mut Window, cx: &mut App,
) -> DockItem
// = DockItem::split(Axis::Vertical, items, ...)
```

DockArea setters (`reference/.../dock/mod.rs`):

```rust
dock_area.set_center(center: DockItem, window, cx)
dock_area.set_right_dock(panel: DockItem, size: Option<Pixels>, open: bool, window, cx)
dock_area.set_dock_collapsible(edges: Edges<bool>, window, cx)
dock_area.toggle_dock(placement: DockPlacement, window, cx)   // status bar button
dock_area.set_toggle_button_visible(visible: bool, cx)
dock_area.add_panel(panel: Arc<dyn PanelView>, placement, bounds, window, cx)
dock_area.set_version(version: usize, window, cx)
dock_area.dump(cx) -> DockAreaState
dock_area.load(state: DockAreaState, window, cx) -> Result<()>
```

---

## 4. Panel trait — implementation requirements

Terminal/Session/Sftp must implement the `Panel` trait (`reference/.../dock/panel.rs:46`).
`Panel` requires 3 super-traits: `EventEmitter<PanelEvent> + Render + Focusable`.

### 4.1 Trait signature (abbreviated)

```rust
pub trait Panel: EventEmitter<PanelEvent> + Render + Focusable {
    fn panel_name(&self) -> &'static str;                    // ⭐ stable, used for deserialize
    fn tab_name(&self, cx: &App) -> Option<SharedString> { None }
    fn title(&mut self, window, cx) -> impl IntoElement { t!("Dock.Unnamed") }
    fn title_style(&self, cx: &App) -> Option<TitleStyle> { None }
    fn title_suffix(&mut self, window, cx) -> Option<impl IntoElement> { None }
    fn closable(&self, cx: &App) -> bool { true }
    fn zoomable(&self, cx: &App) -> Option<PanelControl> { Some(PanelControl::Menu) }
    fn visible(&self, cx: &App) -> bool { true }
    fn set_active(&mut self, active: bool, window, cx) {}
    fn set_zoomed(&mut self, zoomed: bool, window, cx) {}
    fn on_added_to(&mut self, tab_panel: WeakEntity<TabPanel>, window, cx) {}
    fn on_removed(&mut self, window, cx) {}
    fn dropdown_menu(&mut self, menu: PopupMenu, window, cx) -> PopupMenu { menu }
    fn toolbar_buttons(&mut self, window, cx) -> Option<Vec<Button>> { None }
    fn dump(&self, cx: &App) -> PanelState { PanelState::new(self) }
    fn inner_padding(&self, cx: &App) -> bool { true }
}
```

### 4.2 `PanelControl` — controls zoom/menu/toolbar

```rust
pub enum PanelControl { Both, Menu, Toolbar }
// Both    → show dropdown menu + toolbar buttons
// Menu    → dropdown menu only (default)
// Toolbar → toolbar buttons only
```

`zoomable` returns `Some(PanelControl::...)` to enable zoom. `None` → zoom disabled.
Tab panel renders toolbar (zoom/info/close) when `PanelControl::Both | Toolbar`,
renders dropdown menu when `PanelControl::Both | Menu` (see `tab_panel.rs:479`).

### 4.3 Actions kept as-is

```rust
// dock/mod.rs
actions!(dock, [ToggleZoom, ClosePanel]);

// Bind in init()
KeyBinding::new("shift-escape", ToggleZoom, None)
KeyBinding::new("ctrl-w", ClosePanel, None)
```

`TabPanel` handles `ToggleZoom` (fullscreen zoom) and `ClosePanel` (close tab) itself.
Panel only needs to declare `zoomable()` + `closable()`.

### 4.4 Sample implementation for TerminalPanel

```rust
pub struct TerminalPanel {
    focus_handle: FocusHandle,
    // TODO: TerminalSession handle, scrollback grid, etc.
}

impl TerminalPanel {
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self { focus_handle: cx.focus_handle() }
    }
    pub fn new_entity(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }
}

impl EventEmitter<PanelEvent> for TerminalPanel {}
impl Focusable for TerminalPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle { self.focus_handle.clone() }
}

impl Panel for TerminalPanel {
    fn panel_name(&self) -> &'static str { "terminal" }
    fn title(&mut self, _window, _cx) -> impl IntoElement {
        "Terminal".into()
    }
    fn closable(&self, _: &App) -> bool { true }
    fn zoomable(&self, _: &App) -> Option<PanelControl> { Some(PanelControl::Both) }
    // dump uses default (PanelState::new) — no auxiliary state yet
}

impl Render for TerminalPanel {
    fn render(&mut self, _window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("terminal-panel")
            .size_full()
            .track_focus(&self.focus_handle)
            .flex()
            .items_center()
            .justify_center()
            .text_color(cx.theme().muted_foreground)
            .child("No terminal session. Press Ctrl+N to open.")
    }
}
```

SessionPanel/SftpPanel are similar, change `panel_name` to `"session"`/`"sftp"` and
the placeholder text.

---

## 5. Panel serialization registry

> This section is the result of reading `reference/.../dock/state.rs` + `panel.rs:293` +
> the `layout.json` fixture. This is the **core** mechanism that must be understood correctly.

### 5.1 Deserialize flow

```
DockAreaState (JSON)
    │  dock_area.load(state, window, cx)
    ▼
DockArea::load
    ├── center:     state.center.to_item(dock_area, window, cx)  → DockItem
    ├── left_dock:  state.left_dock.map(DockState::to_dock)        → Entity<Dock>  (dropped)
    ├── right_dock: state.right_dock.map(DockState::to_dock)       → Entity<Dock>
    └── bottom_dock: state.bottom_dock.map(DockState::to_dock)     (dropped)
```

`DockState::to_dock` (`state.rs:36`) calls `self.panel.to_item(...)` then builds
`Dock::from_state(placement, size, item, open, ...)`.

### 5.2 `PanelState::to_item` — dispatch by `PanelInfo` (NOT by `panel_name`)

```rust
// state.rs:168
match info {
    PanelInfo::Stack { sizes, axis } => {
        // RECREATE DockItem::split_with_sizes — NOT via registry
        // children = self.children.iter().map(|c| c.to_item(...))
        DockItem::split_with_sizes(axis, items, sizes, dock_area, window, cx)
    }
    PanelInfo::Tabs { active_index } => {
        // RECREATE DockItem::tabs — NOT via registry
        // If only 1 child → return items[0].clone() (unwrap tab wrapper)
        DockItem::tabs(items, dock_area, window, cx).active_index(active_index, cx)
    }
    PanelInfo::Panel(value) => {
        // ⭐ ONLY here does it call PanelRegistry::build_panel(panel_name, ...)
        let view = PanelRegistry::build_panel(&self.panel_name, dock_area, self, &info, window, cx);
        DockItem::tabs(vec![view.into()], dock_area, window, cx)
    }
    PanelInfo::Tiles { metas } => DockItem::tiles(items, metas, dock_area, window, cx),
}
```

**Important consequences:**

- The **house structure** (split/tabs/tiles) is **recreated automatically** by gpui-component based on
  `PanelInfo` — no registration needed.
- `panel_name = "StackPanel"` / `"TabPanel"` are **only labels** in JSON (used for debug/test);
  they **do not** go through `PanelRegistry`. `to_item` ignores `panel_name` on the
  Stack/Tabs/Tiles branches.
- **Only leaf panels** (`PanelInfo::Panel`) need to be registered via `register_panel`.

### 5.3 JSON structure on save

Right dock of form `DockItem::v_split([tab(Session), tab(Sftp)])` serializes to:

```json
"right_dock": {
  "panel": {
    "panel_name": "StackPanel",
    "children": [
      {
        "panel_name": "TabPanel",
        "children": [
          {
            "panel_name": "session",              // ⭐ LEAF → PanelInfo::Panel
            "children": [],
            "info": { "panel": null }
          }
        ],
        "info": { "tabs": { "active_index": 0 } }
      },
      {
        "panel_name": "TabPanel",
        "children": [ { "panel_name": "sftp", "children": [], "info": { "panel": null } } ],
        "info": { "tabs": { "active_index": 0 } }
      }
    ],
    "info": { "stack": { "sizes": [...], "axis": 1 } }   // axis 1 = Vertical
  },
  "placement": "right",
  "size": 480.0,
  "open": true,
  "resizeable": true
}
```

Center of form `DockItem::tabs([TerminalPanel, TerminalPanel])`:

```json
"center": {
  "panel_name": "TabPanel",
  "children": [
    { "panel_name": "terminal", "children": [], "info": { "panel": null } },
    { "panel_name": "terminal", "children": [], "info": { "panel": null } }
  ],
  "info": { "tabs": { "active_index": 0 } }
}
```

### 5.4 `PanelRegistry` — register 3 leaf panels

`panel.rs:293`:

```rust
pub struct PanelRegistry {
    items: HashMap<String, Arc<dyn Fn(WeakEntity<DockArea>, &PanelState, &PanelInfo, &mut Window, &mut App) -> Box<dyn PanelView>>>,
}

pub fn register_panel<F>(cx: &mut App, panel_name: &str, deserialize: F)
where F: Fn(...) -> Box<dyn PanelView> + 'static

pub fn build_panel(panel_name, dock_area, panel_state, panel_info, window, cx) -> Box<dyn PanelView> {
    // if in registry → call fn
    // else → InvalidPanel (shows "The `{}` panel type is not registered")
}
```

**OneTerm registers 3** (in `ui::init` or `app::init`):

```rust
register_panel(cx, "terminal", |_, _, _, window, cx| {
    Box::new(cx.new(|cx| TerminalPanel::new(window, cx)))
});
register_panel(cx, "session", |_, _, _, window, cx| {
    Box::new(cx.new(|cx| SessionPanel::new(window, cx)))
});
register_panel(cx, "sftp", |_, _, _, window, cx| {
    Box::new(cx.new(|cx| SftpPanel::new(window, cx)))
});
```

These panels initially have **no auxiliary state** (only placeholder) so they ignore `PanelInfo`
— the constructor returns a fresh panel.

> `PanelRegistry::init` is already called by `gpui_component::init(cx)` (`mod.rs:27`).
> No need to call it again.

### 5.5 `Panel::dump` — leaf panel serializes itself

Default (`panel.rs`):

```rust
fn dump(&self, cx: &App) -> PanelState {
    PanelState::new(self)   // panel_name + info = PanelInfo::Panel(Null)
}
```

When TerminalPanel has state (host_id, cwd, scrollback hash...) — override:

```rust
fn dump(&self, _cx: &App) -> PanelState {
    let mut s = PanelState::new(self);
    s.info = PanelInfo::panel(serde_json::json!({
        "host_id": self.host_id,
        "cwd": self.cwd,
    }));
    s
}
```

and parse in the registry constructor:

```rust
register_panel(cx, "terminal", |_, _, info, window, cx| {
    let value = match info { PanelInfo::Panel(v) => v.clone(), _ => json!(null) };
    Box::new(cx.new(|cx| TerminalPanel::restore(value, window, cx)))
});
```

This pattern mirrors `StoryContainer::dump` + `StoryState::to_value`/`from_value`
in `reference/.../story/src/lib.rs:603,242`.

### 5.6 Version check

`state.rs:8`:

```rust
#[serde(default)]
pub version: Option<usize>,
```

`dock.rs` `load_layout` compares `state.version != Some(MAIN_DOCK_AREA.version)` →
prompts "reset to default?". OneTerm sets `version = 1`, bumps it when the panel structure
changes in a way that invalidates old JSON (e.g. changing `panel_name`, adding a required field).

---

## 6. Layout state save/load

Keep the `dock.rs` mechanism as-is. Place it in `OneTermWorkspace` (`app/src/app.rs` or
`ui/src/layout/workspace.rs`).

### 6.1 Constants

```rust
const MAIN_DOCK_AREA: DockAreaTab = DockAreaTab { id: "main-dock", version: 1 };

#[cfg(debug_assertions)]
const STATE_FILE: &str = "target/docks.json";
#[cfg(not(debug_assertions))]
const STATE_FILE: &str = "docks.json";
```

### 6.2 Constructor — load or reset

```rust
impl OneTermWorkspace {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let dock_area = cx.new(|cx| DockArea::new(MAIN_DOCK_AREA.id, Some(MAIN_DOCK_AREA.version), window, cx));
        let weak_dock_area = dock_area.downgrade();

        match Self::load_layout(dock_area.clone(), window, cx) {
            Ok(_) => println!("load layout success"),
            Err(err) => {
                eprintln!("load layout error: {:?}", err);
                Self::reset_default_layout(weak_dock_area, window, cx);
            }
        };

        // Subscribe DockEvent::LayoutChanged → save_layout (debounce 10s)
        cx.subscribe_in(&dock_area, window, |this, dock_area, ev: &DockEvent, window, cx| match ev {
            DockEvent::LayoutChanged => this.save_layout(dock_area, window, cx),
            _ => {}
        }).detach();

        // Save layout before quit
        cx.on_app_quit({
            let dock_area = dock_area.clone();
            move |_, cx| {
                let state = dock_area.read(cx).dump(cx);
                cx.background_executor().spawn(async move {
                    Self::save_state(&state).unwrap();
                })
            }
        }).detach();

        let title_bar = cx.new(|cx| AppTitleBar::new("OneTerm", window, cx).child(...));

        Self { dock_area, title_bar, last_layout_state: None, toggle_button_visible: true, _save_layout_task: None }
    }
}
```

### 6.3 `save_layout` — debounce 10s, skip when unchanged

```rust
fn save_layout(&mut self, dock_area: &Entity<DockArea>, window: &mut Window, cx: &mut Context<Self>) {
    let dock_area = dock_area.clone();
    self._save_layout_task = Some(cx.spawn_in(window, async move |story, window| {
        window.background_executor().timer(Duration::from_secs(10)).await;
        _ = story.update_in(window, move |this, _, cx| {
            let state = dock_area.read(cx).dump(cx);
            if Some(&state) == this.last_layout_state.as_ref() { return; }
            Self::save_state(&state).unwrap();
            this.last_layout_state = Some(state);
        });
    }));
}

fn save_state(state: &DockAreaState) -> Result<()> {
    let json = serde_json::to_string_pretty(state)?;
    std::fs::write(STATE_FILE, json)?;
    Ok(())
}
```

### 6.4 `load_layout` — version check + reset prompt

```rust
fn load_layout(dock_area: Entity<DockArea>, window: &mut Window, cx: &mut Context<Self>) -> Result<()> {
    let json = std::fs::read_to_string(STATE_FILE)?;
    let state = serde_json::from_str::<DockAreaState>(&json)?;

    if state.version != Some(MAIN_DOCK_AREA.version) {
        let answer = window.prompt(
            PromptLevel::Info,
            "The default main layout has been updated.\nDo you want to reset the layout to default?",
            None, &["Yes", "No"], cx,
        );
        let weak_dock_area = dock_area.downgrade();
        cx.spawn_in(window, async move |this, window| {
            if answer.await == Ok(0) {
                _ = this.update_in(window, |_, window, cx| Self::reset_default_layout(weak_dock_area, window, cx));
            }
        }).detach();
    }

    dock_area.update(cx, |dock_area, cx| {
        dock_area.load(state, window, cx).context("load layout")?;
        dock_area.set_dock_collapsible(Edges { right: true, ..Default::default() }, window, cx);
        Ok::<(), anyhow::Error>(())
    })
}
```

### 6.5 `reset_default_layout` — build the default OneTerm layout

```rust
fn reset_default_layout(dock_area: WeakEntity<DockArea>, window: &mut Window, cx: &mut App) {
    let weak = dock_area.clone();

    let center = DockItem::tabs(
        vec![ Arc::new(TerminalPanel::new_entity(window, cx)) ],   // 1 default terminal
        &weak, window, cx,
    );

    let right = DockItem::v_split(
        vec![
            DockItem::tab(SessionPanel::new_entity(window, cx), &weak, window, cx),
            DockItem::tab(SftpPanel::new_entity(window, cx),   &weak, window, cx),
        ],
        &weak, window, cx,
    );

    _ = dock_area.update(cx, |view, cx| {
        view.set_version(MAIN_DOCK_AREA.version, window, cx);
        view.set_center(center, window, cx);
        view.set_right_dock(right, Some(px(480.)), true, window, cx);  // 30% of 1600
        view.set_dock_collapsible(Edges { right: true, ..Default::default() }, window, cx);
        Self::save_state(&view.dump(cx)).unwrap();
    });
}
```

---

## 7. Title bar & App menu bar

### 7.1 Keep `AppTitleBar` + `AppMenuBar` as-is

- `AppTitleBar::new("OneTerm", window, cx)` — structure unchanged (`title_bar.rs`).
- `AppMenuBar` via `app_menus::init(title, cx)` (`app_menus.rs`):
  - "OneTerm" menu: About, Open..., Appearance (Light/Dark), **Theme submenu**
    (list from `ThemeRegistry::global(cx).sorted_themes()`), Language, Quit.
  - Edit: Undo/Redo/Cut/Copy/Paste/Find/SelectAll.
  - Window: Toggle Search.
  - Help: Documentation, Open Website.
- `cx.observe_global::<Theme>` → refresh menu check-state when theme changes.
- `FontSizeSelector` (font-size, gutter toggle) — keep; drop radius/scrollbar (default radius=0px, scrollbar=Scrolling fixed in `theme::init`) and list highlight (default active_highlight=true, not toggleable). Gutter toggle changes the global `TerminalSettings.show_gutter`.

### 7.2 Theme system — keep 100%

- `Theme::global_mut(cx)` + `ThemeRegistry` + `SwitchTheme`/`SwitchThemeMode` action.
- Register themes via `crates/ui/src/theme.rs` (pattern like `reference/.../story/src/themes.rs`).
- No hardcoded colors — read from `cx.theme()`.

### 7.3 Add Panel dropdown — simplified

Child of `AppTitleBar` (replacing the random-story "add-panel" button):

```rust
AppTitleBar::new("OneTerm", window, cx).child(move |_, cx| {
    Button::new("add-panel")
        .icon(IconName::LayoutDashboard)
        .small()
        .ghost()
        .dropdown_menu(move |menu, _, cx| {
            menu.menu("New Terminal Tab", Box::new(AddPanel(DockPlacement::Center)))
                .separator()
                .menu("Show / Hide Dock Toggle Button", Box::new(ToggleDockToggleButton))
        })
        .anchor(Anchor::TopRight)
})
```

`on_action_add_panel` only adds a TerminalPanel:

```rust
fn on_action_add_panel(&mut self, action: &AddPanel, window, cx) {
    let panel = Arc::new(TerminalPanel::new_entity(window, cx));
    self.dock_area.update(cx, |dock_area, cx| {
        dock_area.add_panel(panel, action.0, None, window, cx);
    });
}
```

---

## 8. Status bar & DateTimeClock

### 8.1 StatusBar wiring

```rust
StatusBar::new()
    .left(DateTimeClock::new(window, cx))                     // left corner: clock
    .right(
        Button::new("toggle-right-dock").ghost().xsmall()
            .icon(IconName::PanelRight)
            .tooltip("Toggle Right Dock")
            .on_click(cx.listener(|this, _, window, cx| {
                this.dock_area.update(cx, |area, cx| {
                    area.toggle_dock(DockPlacement::Right, window, cx);
                });
            }))
    )
```

> `StatusBar` (`reference/.../ui/src/status_bar.rs`): `.left(...)` pins left,
> `.right(...)` pins right, `.child(...)` adds center. With both left+right → center
> justify_center; left only → center justify_end.

### 8.2 `DateTimeClock` component

File `crates/ui/src/components/datetime_clock.rs`. `Entity` + `Render` + `Focusable`,
updates via a `cx.spawn` 1s interval.

```rust
pub struct DateTimeClock {
    focus_handle: FocusHandle,
    now: chrono::DateTime<chrono::Local>,
    _timer: Task<()>,
}

impl DateTimeClock {
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let timer = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                _ = this.update(cx, |this, cx| {
                    this.now = chrono::Local::now();
                    cx.notify();
                });
            }
        });
        Self { focus_handle, now: chrono::Local::now(), _timer: timer }
    }
}

impl Focusable for DateTimeClock {
    fn focus_handle(&self, _: &App) -> FocusHandle { self.focus_handle.clone() }
}

impl Render for DateTimeClock {
    fn render(&mut self, _window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("datetime-clock")
            .track_focus(&self.focus_handle)
            .child(self.now.format("%Y-%m-%d %H:%M:%S").to_string())
    }
}
```

> **Dep**: add `chrono` to `crates/ui/Cargo.toml` (or the `time` crate).
> Recommended `chrono` (popular, easy format string).

---

## 9. Resizable behavior

GPUI Dock handles resize itself — **no extra code needed**:

- **Center ↔ right_dock**: draggable vertical divider between the two regions. right_dock width
  initialized to `Some(px(480.))`, user drags to change at runtime → saved into
  `DockState.size`.
- **v_split inside right_dock** (Session ↔ SFTP): `DockItem::v_split` creates a
  `StackPanel` with Vertical axis, draggable horizontal divider. `sizes` auto (50/50) or
  set via `DockItem::split_with_sizes`. Drag changes → saved into
  `PanelInfo::Stack.sizes`.
- **Collapse/expand right_dock**: arrow button in the right_dock corner (from
  `set_dock_collapsible(Edges{right:true})`) + toggle button in status bar
  (`toggle_dock(DockPlacement::Right)`).

Tab panel also has toolbar zoom (ToggleZoom → temporary fullscreen) + close (ClosePanel).

---

## 10. File structure

Per [`docs/agents/structure.md`](agents/structure.md):

| File | Responsibility |
|---|---|
| `crates/app/src/main.rs` | Entry point — calls `run_app()` |
| `crates/app/src/app.rs` | `OneTermWorkspace` (title_bar, dock_area, save_layout, reset_default_layout) |
| `crates/app/src/window.rs` | `new_local`, `WindowOptions`, titlebar options |
| `crates/app/src/actions.rs` | Global actions / key bindings |
| `crates/ui/src/lib.rs` | Re-exports + `init(cx)` (calls `register_panel`) |
| `crates/ui/src/root.rs` | Root view wrapper |
| `crates/ui/src/theme.rs` | Theme registration (pattern `themes.rs`) |
| `crates/ui/src/layout/workspace.rs` | `OneTermWorkspace::render` (title_bar + dock_area + status_bar) |
| `crates/ui/src/layout/statusbar.rs` | StatusBar wiring |
| `crates/ui/src/views/terminal/terminal_panel.rs` | `TerminalPanel` `impl Panel` + placeholder |
| `crates/ui/src/views/session_tabs/tabs.rs` | `SessionPanel` `impl Panel` + placeholder |
| `crates/ui/src/views/sftp/file_browser.rs` | `SftpPanel` `impl Panel` + placeholder |
| `crates/ui/src/components/datetime_clock.rs` | `DateTimeClock` |
| `crates/ui/src/state/app_state.rs` | `AppState` |

---

## 11. Implementation checklist

### Step 1 — Skeleton & registration

- [ ] Create the `OneTermWorkspace` struct (copy fields from `StoryWorkspace`).
- [ ] `register_panel("terminal"/"session"/"sftp", ...)` in `ui::init`.
- [ ] Constants `MAIN_DOCK_AREA.version = 1`, `STATE_FILE`.

### Step 2 — Panels (placeholder)

- [ ] `TerminalPanel`: `impl Panel + Render + Focusable + EventEmitter<PanelEvent>`,
      `panel_name = "terminal"`, placeholder "No terminal session...".
- [ ] `SessionPanel`: `panel_name = "session"`, placeholder "No active session...".
- [ ] `SftpPanel`: `panel_name = "sftp"`, placeholder "No SFTP connection...".
- [ ] Helper `new_entity(window, cx) -> Entity<Self>` for each panel.

### Step 3 — Layout wiring

- [ ] `reset_default_layout`: center = tabs([TerminalPanel]), right_dock = v_split([Session, Sftp]).
- [ ] `set_dock_collapsible(Edges{right:true, ..})`.
- [ ] `load_layout` + version check + reset prompt.
- [ ] `save_layout` (debounce 10s) + `save_state` + `on_app_quit` save.

### Step 4 — Title bar & menu

- [ ] `AppTitleBar::new("OneTerm", ...)` keep structure.
- [ ] `app_menus::init` (Appearance/Theme/Language/Edit/Window/Help) — keep.
- [ ] `FontSizeSelector` — keep (only font-size + Gutter toggle; radius=0px & scrollbar=Scrolling fixed; list.active_highlight=true fixed).
- [ ] Add Panel dropdown → "New Terminal Tab" + "Show/Hide Dock Toggle Button".
- [ ] `on_action_add_panel` → only adds TerminalPanel.

### Step 5 — Status bar

- [ ] `DateTimeClock` component (chrono, 1s timer).
- [ ] `StatusBar::new().left(clock).right(toggle-right-dock button)`.
- [ ] Add `chrono` to `crates/ui/Cargo.toml`.

### Step 6 — Window & entry

- [ ] `new_local`: WindowOptions, titlebar options, `Root::new(OneTermWorkspace, ...)`.
- [ ] `main()`: `gpui_platform::application().with_assets(Assets).run(...)`.

### Step 7 — Quality gate

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo build --workspace`

### Version bump notes

Bump `MAIN_DOCK_AREA.version` (1 → 2 → ...) when:
- Changing the `panel_name` of a leaf panel.
- Adding a required field to `Panel::dump` that breaks deserializing old JSON.
- Changing the dock structure (e.g. re-adding left_dock).