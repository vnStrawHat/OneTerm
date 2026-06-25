# GUI Layout — myTerm2

> Tài liệu thiết kế layout GUI cho myTerm2, dựa trên reference
> `reference/gpui-component/crates/story/examples/dock.rs`.
>
> Mọi API gpui-component được trích từ reference (xem
> [`docs/agents/dependencies.md` § 5](agents/dependencies.md)), không dùng web_search.

## Mục lục

1. [Tổng quan & sơ đồ](#1-tổng-quan--sơ-đồ)
2. [Ánh xạ reference → myTerm2](#2-ánh-xạ-reference--myterm2)
3. [Kiến trúc DockArea](#3-kiến-trúc-dockarea)
4. [Panel trait — yêu cầu triển khai](#4-panel-trait--yêu-cầu-triển-khai)
5. [Panel serialization registry](#5-panel-serialization-registry)
6. [Layout state save/load](#6-layout-state-saveload)
7. [Title bar & App menu bar](#7-title-bar--app-menu-bar)
8. [Status bar & DateTimeClock](#8-status-bar--datetimeclock)
9. [Resizable behavior](#9-resizable-behavior)
10. [Cấu trúc file](#10-cấu-trúc-file)
11. [Implementation checklist](#11-implementation-checklist)

---

## 1. Tổng quan & sơ đồ

myTerm2 giữ nguyên khung 3 khối xếp dọc của reference `dock.rs`:
**TitleBar → DockArea → StatusBar**, chỉ thay đổi *nội dung* của DockArea và
StatusBar.

```
┌─────────────────────────────────────────────────────────────────┐
│  TitleBar  [myTerm2 ▾] [Edit] [Window] [Help]   [⚙][🐙][🔔]      │
├───────────────────────────────────────────────┬─────────────────┤
│                                                │                 │
│   CENTER  (70% width)                          │  RIGHT DOCK     │
│   ┌──────────────────────────────────────────┐ │  (30% width)    │
│   │Tab1│Tab2│Tab3│ + │info│zoom│collapse│ ✕  │ │  ┌────────────┐  │
│   ├──────────────────────────────────────────┤ │  │ Session    │  │
│   │                                          │ │  │ [placeholder]│  │
│   │           [Terminal view]                │ │  └────────────┘  │
│   │      (chỉ terminal, không gì khác)       │ │  ↕ v_split       │
│   │                                          │ │  ┌────────────┐  │
│   │                                          │ │  │ SFTP       │  │
│   └──────────────────────────────────────────┘ │  │ [placeholder]│  │
│                                                │  └────────────┘  │
│            ↔ resizable center ↔ right_dock    │  [collapse ▸]   │
├─────────────────────────────────────────────────────────────────┤
│  🕐 2025-01-15 14:32:07                  [Toggle Right Dock]      │
└─────────────────────────────────────────────────────────────────┘
```

### Quyết định thiết kế đã chốt

| # | Quyết định | Lý do |
|---|---|---|
| 1 | **Right = `set_right_dock`** (side dock) = `v_split([Session, SFTP])` | Khớp "như hiện tại (Image/Icon)" — reference cũng dùng `right_dock` = `v_split([Image, Icon])`. Giữ được toggle button + collapse/resize native. |
| 2 | **Bỏ `set_left_dock` và `set_bottom_dock`** hoàn toàn | Chỉ cần Center (Terminals) + Right (Session/SFTP). StatusBar chỉ còn toggle Right + datetime. |
| 3 | **Add Panel menu** → chỉ "New Terminal Tab" + "Show/Hide Dock Toggle Button" | Giữ tinh thần "title bar chức năng giữ nguyên", nội dung hợp lý hóa cho "terminals only". |

---

## 2. Ánh xạ reference → myTerm2

| Thành phần reference `dock.rs` | myTerm2 | Ghi chú |
|---|---|---|
| `StoryWorkspace { title_bar, dock_area, last_layout_state, toggle_button_visible, _save_layout_task }` | `MyTermWorkspace` (đổi tên, giữ field) | `app/src/app.rs` |
| `AppTitleBar::new("Examples", ...)` | `AppTitleBar::new("myTerm2", ...)` | Đổi title |
| `AppMenuBar` (`app_menus.rs`: Appearance/Theme/Language + Edit/Window/Help) | Giữ nguyên 100% | Themes + Language + Appearance |
| `FontSizeSelector` (font-size, gutter toggle) | Giữ nguyên (bỏ radius/scrollbar/list-highlight) | radius=0px, scrollbar=Scrolling cố định ở `theme::init`; list.active_highlight=true cố định; gutter toggle → `TerminalSettings.show_gutter` |
| `DockArea::new("main-dock", Some(version), window, cx)` | `version = 1` (bump) | Trigger reset prompt khi layout cũ khác version |
| Center = `DockItem::v_split` chứa 19 story tabs | Center = `DockItem::tabs([TerminalPanel, ...])` | Chỉ terminal, không gì khác |
| `set_left_dock(...)` | **Bỏ** | — |
| `set_bottom_dock(...)` | **Bỏ** | — |
| `set_right_dock(DockItem::v_split([Image, Icon]), Some(px(320.)), true, ...)` | `set_right_dock(DockItem::v_split([Session, Sftp]), Some(px(480.)), true, ...)` | 30% của ~1600px window |
| `set_dock_collapsible(Edges{left:true,bottom:true,right:true})` | `set_dock_collapsible(Edges{right:true, ..Default::default()})` | Chỉ còn right_dock |
| `DockAreaState` save/load `STATE_FILE`, version check, prompt reset | Giữ nguyên | `STATE_FILE = "target/docks.json"` (debug) |
| `AddPanel` action + dropdown (thêm random story) | Dropdown chỉ "New Terminal Tab" + "Show/Hide Dock Toggle Button" | Bỏ Add to Left/Bottom/Right + menu check Sidebar/Dialog/... |
| StatusBar: 3 toggle button (left/bottom/right) | StatusBar: `.left(DateTimeClock)` + `.right(toggle-right-dock)` | — |

---

## 3. Kiến trúc DockArea

### 3.1 Cấu trúc DockItem

```
DockArea (id="main-dock", version=1)
├── center:  DockItem::tabs([TerminalPanel, TerminalPanel, ...])
│            • mỗi tab = 1 Terminal, không thêm panel nào khác
│            • zoom/close/info/collapse giữ nguyên (qua Panel trait)
│            • placeholder "No terminal session" khi rỗng
│
└── right_dock: DockItem::v_split([
        DockItem::tab(SessionPanel),    size auto   ← top half
        DockItem::tab(SftpPanel),                     ← bottom half
    ])
    • set_right_dock(panel, Some(window_w * 0.30), true, window, cx)
    • set_dock_collapsible(Edges{ right:true, .. })
    • nút collapse/expand ở góc + toggle button ở status bar
    • v_split resizable (draggable divider giữa Session/SFTP)
    • placeholder "No active session" / "No SFTP connection"
```

### 3.2 Constructor API (trích từ reference)

`DockItem` constructors (`reference/.../dock/mod.rs`):

```rust
// Tabs — center (nhiều terminal)
DockItem::tabs(
    items: Vec<Arc<dyn PanelView>>,
    dock_area: &WeakEntity<DockArea>,
    window: &mut Window, cx: &mut App,
) -> DockItem

// Tab đơn — dùng cho leaf trong v_split
DockItem::tab<P: Panel>(
    item: Entity<P>,
    dock_area: &WeakEntity<DockArea>,
    window: &mut Window, cx: &mut App,
) -> DockItem
// = DockItem::new_tabs(vec![Arc::new(item.clone())], None, ...)

// Vertical split — right_dock (Session trên, SFTP dưới)
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

## 4. Panel trait — yêu cầu triển khai

Terminal/Session/Sftp phải implement `Panel` trait (`reference/.../dock/panel.rs:46`).
`Panel` yêu cầu 3 super-trait: `EventEmitter<PanelEvent> + Render + Focusable`.

### 4.1 Trait signature (rút gọn)

```rust
pub trait Panel: EventEmitter<PanelEvent> + Render + Focusable {
    fn panel_name(&self) -> &'static str;                    // ⭐ stable, dùng deserialize
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

### 4.2 `PanelControl` — kiểm soát zoom/menu/toolbar

```rust
pub enum PanelControl { Both, Menu, Toolbar }
// Both    → hiện dropdown menu + toolbar buttons
// Menu    → chỉ dropdown menu (mặc định)
// Toolbar → chỉ toolbar buttons
```

`zoomable` trả `Some(PanelControl::...)` để bật zoom. `None` → tắt zoom.
Tab panel render toolbar (zoom/info/close) khi `PanelControl::Both | Toolbar`,
render dropdown menu khi `PanelControl::Both | Menu` (xem `tab_panel.rs:479`).

### 4.3 Actions giữ nguyên

```rust
// dock/mod.rs
actions!(dock, [ToggleZoom, ClosePanel]);

// Bind trong init()
KeyBinding::new("shift-escape", ToggleZoom, None)
KeyBinding::new("ctrl-w", ClosePanel, None)
```

`TabPanel` tự handle `ToggleZoom` (zoom fullscreen) và `ClosePanel` (đóng tab).
Panel chỉ cần khai báo `zoomable()` + `closable()`.

### 4.4 Implement mẫu cho TerminalPanel

```rust
pub struct TerminalPanel {
    focus_handle: FocusHandle,
    // TODO: TerminalSession handle, scrollback grid, v.v.
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
    // dump dùng default (PanelState::new) — chưa có state phụ
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

SessionPanel/SftpPanel tương tự, đổi `panel_name` thành `"session"`/`"sftp"` và
placeholder text.

---

## 5. Panel serialization registry

> Phần này là kết quả đọc `reference/.../dock/state.rs` + `panel.rs:293` +
> fixture `layout.json`. Đây là cơ chế **cốt lõi** cần hiểu đúng.

### 5.1 Luồng deserialize

```
DockAreaState (JSON)
   │  dock_area.load(state, window, cx)
   ▼
DockArea::load
   ├── center:     state.center.to_item(dock_area, window, cx)  → DockItem
   ├── left_dock:  state.left_dock.map(DockState::to_dock)        → Entity<Dock>  (bỏ)
   ├── right_dock: state.right_dock.map(DockState::to_dock)       → Entity<Dock>
   └── bottom_dock: state.bottom_dock.map(DockState::to_dock)     (bỏ)
```

`DockState::to_dock` (`state.rs:36`) gọi `self.panel.to_item(...)` rồi dựng
`Dock::from_state(placement, size, item, open, ...)`.

### 5.2 `PanelState::to_item` — dispatch theo `PanelInfo` (KHÔNG theo `panel_name`)

```rust
// state.rs:168
match info {
    PanelInfo::Stack { sizes, axis } => {
        // TÁI TẠO DockItem::split_with_sizes — KHÔNG qua registry
        // children = self.children.iter().map(|c| c.to_item(...))
        DockItem::split_with_sizes(axis, items, sizes, dock_area, window, cx)
    }
    PanelInfo::Tabs { active_index } => {
        // TÁI TẠO DockItem::tabs — KHÔNG qua registry
        // Nếu chỉ 1 child → return items[0].clone() (unwrap tab wrapper)
        DockItem::tabs(items, dock_area, window, cx).active_index(active_index, cx)
    }
    PanelInfo::Panel(value) => {
        // ⭐ CHỈ đây mới gọi PanelRegistry::build_panel(panel_name, ...)
        let view = PanelRegistry::build_panel(&self.panel_name, dock_area, self, &info, window, cx);
        DockItem::tabs(vec![view.into()], dock_area, window, cx)
    }
    PanelInfo::Tiles { metas } => DockItem::tiles(items, metas, dock_area, window, cx),
}
```

**Hệ quả quan trọng:**

- Cấu trúc **ngôi nhà** (split/tabs/tiles) được gpui-component **tự tái tạo** dựa vào
  `PanelInfo` — không cần đăng ký gì.
- `panel_name = "StackPanel"` / `"TabPanel"` **chỉ là nhãn** trong JSON (dùng debug/test);
  chúng **không** đi qua `PanelRegistry`. `to_item` bỏ qua `panel_name` ở nhánh
  Stack/Tabs/Tiles.
- **Chỉ leaf panel** (`PanelInfo::Panel`) mới cần đăng ký qua `register_panel`.

### 5.3 Cấu trúc JSON khi save

Right dock dạng `DockItem::v_split([tab(Session), tab(Sftp)])` serialize thành:

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

Center dạng `DockItem::tabs([TerminalPanel, TerminalPanel])`:

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

### 5.4 `PanelRegistry` — đăng ký 3 leaf panel

`panel.rs:293`:

```rust
pub struct PanelRegistry {
    items: HashMap<String, Arc<dyn Fn(WeakEntity<DockArea>, &PanelState, &PanelInfo, &mut Window, &mut App) -> Box<dyn PanelView>>>,
}

pub fn register_panel<F>(cx: &mut App, panel_name: &str, deserialize: F)
where F: Fn(...) -> Box<dyn PanelView> + 'static

pub fn build_panel(panel_name, dock_area, panel_state, panel_info, window, cx) -> Box<dyn PanelView> {
    // nếu có trong registry → gọi fn
    // else → InvalidPanel (hiện "The `{}` panel type is not registered")
}
```

**myTerm2 đăng ký 3** (trong `ui::init` hoặc `app::init`):

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

Các panel này ban đầu **không có state phụ** (chỉ placeholder) nên bỏ qua `PanelInfo`
— constructor trả panel mới.

> `PanelRegistry::init` đã được `gpui_component::init(cx)` tự gọi (`mod.rs:27`).
> Không cần gọi thêm.

### 5.5 `Panel::dump` — leaf panel tự serialize

Default (`panel.rs`):

```rust
fn dump(&self, cx: &App) -> PanelState {
    PanelState::new(self)   // panel_name + info = PanelInfo::Panel(Null)
}
```

Khi TerminalPanel có state (host_id, cwd, scrollback hash...) — override:

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

và parse trong registry constructor:

```rust
register_panel(cx, "terminal", |_, _, info, window, cx| {
    let value = match info { PanelInfo::Panel(v) => v.clone(), _ => json!(null) };
    Box::new(cx.new(|cx| TerminalPanel::restore(value, window, cx)))
});
```

Pattern này giống `StoryContainer::dump` + `StoryState::to_value`/`from_value`
trong `reference/.../story/src/lib.rs:603,242`.

### 5.6 Version check

`state.rs:8`:

```rust
#[serde(default)]
pub version: Option<usize>,
```

`dock.rs` `load_layout` so sánh `state.version != Some(MAIN_DOCK_AREA.version)` →
prompt "reset to default?". myTerm2 đặt `version = 1`, bump khi thay đổi cấu trúc
panel khiến JSON cũ không hợp lệ (vd. đổi `panel_name`, thêm trường bắt buộc).

---

## 6. Layout state save/load

Giữ nguyên cơ chế `dock.rs`. Đặt trong `MyTermWorkspace` (`app/src/app.rs` hoặc
`ui/src/layout/workspace.rs`).

### 6.1 Const

```rust
const MAIN_DOCK_AREA: DockAreaTab = DockAreaTab { id: "main-dock", version: 1 };

#[cfg(debug_assertions)]
const STATE_FILE: &str = "target/docks.json";
#[cfg(not(debug_assertions))]
const STATE_FILE: &str = "docks.json";
```

### 6.2 Constructor — load hoặc reset

```rust
impl MyTermWorkspace {
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

        // Save layout trước khi quit
        cx.on_app_quit({
            let dock_area = dock_area.clone();
            move |_, cx| {
                let state = dock_area.read(cx).dump(cx);
                cx.background_executor().spawn(async move {
                    Self::save_state(&state).unwrap();
                })
            }
        }).detach();

        let title_bar = cx.new(|cx| AppTitleBar::new("myTerm2", window, cx).child(...));

        Self { dock_area, title_bar, last_layout_state: None, toggle_button_visible: true, _save_layout_task: None }
    }
}
```

### 6.3 `save_layout` — debounce 10s, skip khi không đổi

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

### 6.4 `load_layout` — version check + prompt reset

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

### 6.5 `reset_default_layout` — dựng layout mặc định myTerm2

```rust
fn reset_default_layout(dock_area: WeakEntity<DockArea>, window: &mut Window, cx: &mut App) {
    let weak = dock_area.clone();

    let center = DockItem::tabs(
        vec![ Arc::new(TerminalPanel::new_entity(window, cx)) ],   // 1 terminal mặc định
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
        view.set_right_dock(right, Some(px(480.)), true, window, cx);  // 30% của 1600
        view.set_dock_collapsible(Edges { right: true, ..Default::default() }, window, cx);
        Self::save_state(&view.dump(cx)).unwrap();
    });
}
```

---

## 7. Title bar & App menu bar

### 7.1 Giữ nguyên `AppTitleBar` + `AppMenuBar`

- `AppTitleBar::new("myTerm2", window, cx)` — cấu trúc giữ nguyên (`title_bar.rs`).
- `AppMenuBar` qua `app_menus::init(title, cx)` (`app_menus.rs`):
  - Menu "myTerm2": About, Open..., Appearance (Light/Dark), **Theme submenu**
    (list từ `ThemeRegistry::global(cx).sorted_themes()`), Language, Quit.
  - Edit: Undo/Redo/Cut/Copy/Paste/Find/SelectAll.
  - Window: Toggle Search.
  - Help: Documentation, Open Website.
- `cx.observe_global::<Theme>` → refresh menu check-state khi theme đổi.
- `FontSizeSelector` (font-size, gutter toggle) — giữ; bỏ radius/scrollbar (mặc định radius=0px, scrollbar=Scrolling cố định ở `theme::init`) và list highlight (mặc định active_highlight=true, không toggle). Gutter toggle thay đổi `TerminalSettings.show_gutter` toàn cục.

### 7.2 Theme system — giữ nguyên 100%

- `Theme::global_mut(cx)` + `ThemeRegistry` + `SwitchTheme`/`SwitchThemeMode` action.
- Đăng ký theme qua `crates/ui/src/theme.rs` (pattern như `reference/.../story/src/themes.rs`).
- Không hardcode màu — đọc từ `cx.theme()`.

### 7.3 Add Panel dropdown — đơn giản hóa

Child của `AppTitleBar` (thay cho nút "add-panel" random story):

```rust
AppTitleBar::new("myTerm2", window, cx).child(move |_, cx| {
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

`on_action_add_panel` chỉ thêm TerminalPanel:

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
    .left(DateTimeClock::new(window, cx))                     // góc trái: đồng hồ
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

> `StatusBar` (`reference/.../ui/src/status_bar.rs`): `.left(...)` pin trái,
> `.right(...)` pin phải, `.child(...)` thêm center. Có cả left+right → center
> justify_center; chỉ left → center justify_end.

### 8.2 `DateTimeClock` component

File `crates/ui/src/components/datetime_clock.rs`. `Entity` + `Render` + `Focusable`,
cập nhật qua `cx.spawn` interval 1s.

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

> **Dep**: thêm `chrono` vào `crates/ui/Cargo.toml` (hoặc `time` crate).
> Đề xuất `chrono` (phổ biến, format string dễ).

---

## 9. Resizable behavior

GPUI Dock tự handle resize — **không cần code thêm**:

- **Center ↔ right_dock**: draggable divider dọc giữa 2 vùng. right_dock width
  khởi tạo `Some(px(480.))`, user kéo thay đổi runtime → được save vào
  `DockState.size`.
- **v_split trong right_dock** (Session ↔ SFTP): `DockItem::v_split` tạo
  `StackPanel` axis Vertical, divider ngang draggable. `sizes` auto (50/50) hoặc
  set qua `DockItem::split_with_sizes`. Kéo thay đổi → save vào
  `PanelInfo::Stack.sizes`.
- **Collapse/expand right_dock**: nút mũi tên ở góc right_dock (do
  `set_dock_collapsible(Edges{right:true})`) + toggle button ở status bar
  (`toggle_dock(DockPlacement::Right)`).

Tab panel cũng có toolbar zoom (ToggleZoom → fullscreen tạm thời) + close (ClosePanel).

---

## 10. Cấu trúc file

Theo [`docs/agents/structure.md`](agents/structure.md):

| File | Trách nhiệm |
|---|---|
| `crates/app/src/main.rs` | Entry point — gọi `run_app()` |
| `crates/app/src/app.rs` | `MyTermWorkspace` (title_bar, dock_area, save_layout, reset_default_layout) |
| `crates/app/src/window.rs` | `new_local`, `WindowOptions`, titlebar options |
| `crates/app/src/actions.rs` | Global actions / key bindings |
| `crates/ui/src/lib.rs` | Re-exports + `init(cx)` (gọi `register_panel`) |
| `crates/ui/src/root.rs` | Root view wrapper |
| `crates/ui/src/theme.rs` | Đăng ký theme (pattern `themes.rs`) |
| `crates/ui/src/layout/workspace.rs` | `MyTermWorkspace::render` (title_bar + dock_area + status_bar) |
| `crates/ui/src/layout/statusbar.rs` | StatusBar wiring |
| `crates/ui/src/views/terminal/terminal_panel.rs` | `TerminalPanel` `impl Panel` + placeholder |
| `crates/ui/src/views/session_tabs/tabs.rs` | `SessionPanel` `impl Panel` + placeholder |
| `crates/ui/src/views/sftp/file_browser.rs` | `SftpPanel` `impl Panel` + placeholder |
| `crates/ui/src/components/datetime_clock.rs` | `DateTimeClock` |
| `crates/ui/src/state/app_state.rs` | `AppState` |

---

## 11. Implementation checklist

### Bước 1 — Skeleton & registration

- [ ] Tạo `MyTermWorkspace` struct (sao chép field từ `StoryWorkspace`).
- [ ] `register_panel("terminal"/"session"/"sftp", ...)` trong `ui::init`.
- [ ] Const `MAIN_DOCK_AREA.version = 1`, `STATE_FILE`.

### Bước 2 — Panels (placeholder)

- [ ] `TerminalPanel`: `impl Panel + Render + Focusable + EventEmitter<PanelEvent>`,
      `panel_name = "terminal"`, placeholder "No terminal session...".
- [ ] `SessionPanel`: `panel_name = "session"`, placeholder "No active session...".
- [ ] `SftpPanel`: `panel_name = "sftp"`, placeholder "No SFTP connection...".
- [ ] Helper `new_entity(window, cx) -> Entity<Self>` cho mỗi panel.

### Bước 3 — Layout wiring

- [ ] `reset_default_layout`: center = tabs([TerminalPanel]), right_dock = v_split([Session, Sftp]).
- [ ] `set_dock_collapsible(Edges{right:true, ..})`.
- [ ] `load_layout` + version check + prompt reset.
- [ ] `save_layout` (debounce 10s) + `save_state` + `on_app_quit` save.

### Bước 4 — Title bar & menu

- [ ] `AppTitleBar::new("myTerm2", ...)` giữ nguyên cấu trúc.
- [ ] `app_menus::init` (Appearance/Theme/Language/Edit/Window/Help) — giữ.
- [ ] `FontSizeSelector` — giữ (chỉ font-size + Gutter toggle; radius=0px & scrollbar=Scrolling cố định; list.active_highlight=true cố định).
- [ ] Add Panel dropdown → "New Terminal Tab" + "Show/Hide Dock Toggle Button".
- [ ] `on_action_add_panel` → chỉ thêm TerminalPanel.

### Bước 5 — Status bar

- [ ] `DateTimeClock` component (chrono, timer 1s).
- [ ] `StatusBar::new().left(clock).right(toggle-right-dock button)`.
- [ ] Thêm `chrono` vào `crates/ui/Cargo.toml`.

### Bước 6 — Window & entry

- [ ] `new_local`: WindowOptions, titlebar options, `Root::new(MyTermWorkspace, ...)`.
- [ ] `main()`: `gpui_platform::application().with_assets(Assets).run(...)`.

### Bước 7 — Quality gate

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo build --workspace`

### Lưu ý version bump

Bump `MAIN_DOCK_AREA.version` (1 → 2 → ...) khi:
- Đổi `panel_name` của leaf panel.
- Thêm trường bắt buộc vào `Panel::dump` khiến JSON cũ không deserialize được.
- Thay đổi cấu trúc dock (vd. thêm left_dock trở lại).