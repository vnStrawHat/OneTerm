//! [`MyTermWorkspace`] — view chính của myTerm2.
//!
//! Module gốc `workspace.rs` đã được tách thành `workspace/`.

use std::collections::HashSet;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

use gpui::{
    App, AppContext, Context, Entity, EntityId, InteractiveElement as _, IntoElement,
    KeyBinding, ParentElement, Render, Styled, Task, Window, div,
};
use gpui_component::{
    Root,
    dock::{
        ClosePanel, DockArea, DockEvent, PanelEvent, ToggleZoom,
    },
};

use crate::{
    components::DateTimeClock,
    layout::{statusbar, title_bar::AppTitleBar},
    state::AppState,
};

pub(crate) mod actions;
pub(crate) mod layout;
pub(crate) mod persistence;
pub(crate) mod zoom;

pub const MAIN_DOCK_VERSION: usize = 2;
pub const MAIN_DOCK_ID: &str = "main-dock";
pub const TOGGLE_BUTTON_VISIBLE_FIELD: &str = "toggle_button_visible";

#[cfg(debug_assertions)]
pub const STATE_FILE: &str = "target/docks.json";
#[cfg(not(debug_assertions))]
pub const STATE_FILE: &str = "docks.json";

/// Workspace chính: title bar + dock area + status bar.
pub struct MyTermWorkspace {
    pub title_bar: Entity<AppTitleBar>,
    pub dock_area: Entity<DockArea>,
    /// Đồng hồ datetime — tạo 1 lần để timer 1s fire ổn định.
    pub clock: Entity<DateTimeClock>,
    last_layout_state: Option<gpui_component::dock::DockAreaState>,
    toggle_button_visible: Arc<AtomicBool>,
    _save_layout_task: Option<Task<()>>,

    /// Mirror trạng thái zoom: tên panel đang zoom (fullscreen).
    /// `gpui-component` giữ `TabPanel.zoomed` (private) + `DockArea.zoom_view`
    /// (private) nên từ ngoài crate không đọc được → tự track qua subscription
    /// `PanelEvent::ZoomIn`/`ZoomOut`. Dùng `Arc<Mutex<..>>` chia sẻ với closure
    /// `on_app_quit` để đọc an toàn ngay cả khi entity workspace đã bị drop
    /// trong quá trình shutdown.
    zoomed_panel: Arc<Mutex<Option<String>>>,
    /// Các `TabPanel` đã subscribe — tránh subscribe trùng (LayoutChanged fire
    /// nhiều lần, và TabPanel có thể tái tạo).
    subscribed_tabs: HashSet<EntityId>,
}

impl MyTermWorkspace {
    /// Tạo workspace mới: load layout cũ (giữ right dock + settings),
    /// nhưng reset center (terminal tabs) về 1 tab mặc định.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        AppState::init(cx);

        let dock_area = cx.new(|cx| {
            use gpui_component::dock::PanelStyle;
            DockArea::new(MAIN_DOCK_ID, Some(MAIN_DOCK_VERSION), window, cx)
                .panel_style(PanelStyle::TabBar)
        });
        let weak_dock_area = dock_area.downgrade();

        // Đọc tên panel đang zoom TRƯỚC khi layout bị reset (center luôn reset,
        // và reset_* ghi lại docks.json không kèm zoom → phải lưu trước).
        let saved_zoom = persistence::read_zoomed_panel();
        // Đọc toggle_button_visible TRƯỚC khi reset_* ghi lại docks.json.
        let saved_toggle_button_visible = persistence::read_toggle_button_visible().unwrap_or(true);

        match Self::load_layout(dock_area.clone(), window, cx) {
            Ok(()) => {
                layout::reset_center_only(weak_dock_area, saved_toggle_button_visible, window, cx);
            }
            Err(_) => {
                layout::reset_default_layout(weak_dock_area, window, cx);
            }
        }

        // Mirror toggle_button_visible — `Arc<AtomicBool>` chia sẻ giữa
        // subscription callbacks và closure `on_app_quit`.
        let toggle_button_visible = Arc::new(AtomicBool::new(saved_toggle_button_visible));

        // Áp dụng toggle_button_visible đã lưu lên DockArea (mặc định = true).
        if !saved_toggle_button_visible {
            dock_area.update(cx, |dock_area, cx| {
                dock_area.set_toggle_button_visible(false, cx);
            });
        }

        // Mirror trạng thái zoom (tên panel đang zoom) — `Arc<Mutex<..>>` chia sẻ
        // giữa subscription callbacks và closure `on_app_quit` để đọc an toàn ngay
        // cả khi entity workspace đã bị drop khi shutdown.
        let zoomed_panel = Arc::new(Mutex::new(None::<String>));

        cx.subscribe_in(
            &dock_area,
            window,
            move |this, dock_area, ev: &DockEvent, window, cx| match ev {
                DockEvent::LayoutChanged => {
                    // Subscribe mọi TabPanel mới (tab/panel thêm động).
                    this.sync_tab_subscriptions(window, cx);
                    this.save_layout(dock_area, window, cx);
                }
                _ => {}
            },
        )
        .detach();

        cx.on_app_quit({
            let dock_area = dock_area.clone();
            let zoomed_panel = zoomed_panel.clone();
            let toggle_button_visible = toggle_button_visible.clone();
            move |_, cx| {
                let state = dock_area.read(cx).dump(cx);
                // Đọc tên panel đang zoom từ `Arc<Mutex<..>>` — không phụ thuộc
                // lifetime entity workspace (có thể đã bị drop khi shutdown).
                let zoomed_name = zoomed_panel
                    .lock()
                    .ok()
                    .and_then(|g| g.clone());
                tracing::info!("on_app_quit → zoomed_name={zoomed_name:?}");
                eprintln!("[zoom] on_app_quit → zoomed_name={zoomed_name:?}");
                let tbv = toggle_button_visible.load(Ordering::Relaxed);
                async move {
                    _ = persistence::save_state(&state, zoomed_name.as_deref(), tbv, "on_app_quit");
                }
            }
        })
        .detach();

        let title_bar = cx.new(|cx| {
            AppTitleBar::new("myTerm2", window, cx)
                .child(|_window, _cx| crate::layout::title_bar::add_terminal_button())
        });

        let clock = DateTimeClock::new_entity(window, cx);

        let mut me = Self {
            title_bar,
            dock_area,
            clock,
            last_layout_state: None,
            toggle_button_visible,
            _save_layout_task: None,
            zoomed_panel,
            subscribed_tabs: HashSet::new(),
        };

        // Subscribe mọi TabPanel đang có (load từ docks.json / tạo ở reset_*).
        me.sync_tab_subscriptions(window, cx);

        // Khôi phục zoom (fullscreen) cho panel trùng tên đã lưu.
        if let Some(name) = saved_zoom {
            me.restore_zoom(&name, window, cx);
        }

        me
    }

    /// Debounce save 5s, skip khi state không đổi.
    fn save_layout(
        &mut self,
        dock_area: &Entity<DockArea>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let dock_area = dock_area.clone();
        // Snapshot tên panel đang zoom tại thời điểm schedule (mirror state).
        let zoomed_name = self
            .zoomed_panel
            .lock()
            .ok()
            .and_then(|g| g.clone());
        self._save_layout_task = Some(cx.spawn_in(window, async move |story, window| {
            window
                .background_executor()
                .timer(Duration::from_secs(2))
                .await;

            _ = story.update_in(window, move |this, _, cx| {
                let state = dock_area.read(cx).dump(cx);
                if Some(&state) == this.last_layout_state.as_ref() {
                    return;
                }
                _ = persistence::save_state(&state, zoomed_name.as_deref(), this.toggle_button_visible.load(Ordering::Relaxed), "debounce");
                this.last_layout_state = Some(state);
            });
        }));
    }

    /// Subscribe `PanelEvent` trên mọi `TabPanel` chưa subscribe — cập nhật
    /// mirror `zoomed_panel`. Gọi sau mỗi `DockEvent::LayoutChanged` và lúc init.
    fn sync_tab_subscriptions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tabs = zoom::collect_tab_panels(&self.dock_area.read(cx), cx);
        tracing::info!("sync_tab_subscriptions → found {} tab panel(s)", tabs.len());
        eprintln!("[zoom] sync_tab_subscriptions → found {} tab panel(s)", tabs.len());
        for tp in tabs {
            let id = tp.entity_id();
            if self.subscribed_tabs.insert(id) {
                let zoomed_panel = self.zoomed_panel.clone();
                let dock_area = self.dock_area.clone();
                let toggle_button_visible = self.toggle_button_visible.clone();
                cx.subscribe_in(
                    &tp,
                    window,
                    move |_this, tp, ev: &PanelEvent, _window, cx| match ev {
                        PanelEvent::ZoomIn => {
                            // Giải tên panel active tại lúc zoom.
                            let name = tp
                                .read(cx)
                                .active_panel(cx)
                                .map(|p| p.panel_name(cx).to_string());
                            tracing::info!("PanelEvent::ZoomIn → name={name:?}");
                            eprintln!("[zoom] PanelEvent::ZoomIn → name={name:?}");
                            if let Ok(mut g) = zoomed_panel.lock() {
                                *g = name.clone();
                            }
                            // Lưu NGAY vào docks.json — không phụ thuộc quit/debounce.
                            let state = dock_area.read(cx).dump(cx);
                            _ = persistence::save_state(&state, name.as_deref(), toggle_button_visible.load(Ordering::Relaxed), "zoom_in");
                            cx.notify();
                        }
                        PanelEvent::ZoomOut => {
                            tracing::info!("PanelEvent::ZoomOut");
                            eprintln!("[zoom] PanelEvent::ZoomOut");
                            if let Ok(mut g) = zoomed_panel.lock() {
                                *g = None;
                            }
                            let state = dock_area.read(cx).dump(cx);
                            _ = persistence::save_state(&state, None, toggle_button_visible.load(Ordering::Relaxed), "zoom_out");
                            cx.notify();
                        }
                        _ => {}
                    },
                )
                .detach();
            }
        }
    }

    /// Khôi phục zoom: tìm `TabPanel` có active panel trùng `name` → focus +
    /// dispatch `ToggleZoom` (qua đúng code path của gpui-component → toolbar
    /// state nhất quán, `TabPanel.zoomed` được set đúng).
    fn restore_zoom(&self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        let target = zoom::find_tab_by_panel_name(&self.dock_area.read(cx), name, cx);
        if let Some(tp) = target {
            if let Some(panel) = tp.read(cx).active_panel(cx) {
                panel.focus_handle(cx).focus(window, cx);
                window.dispatch_action(Box::new(ToggleZoom), cx);
                tracing::info!("Restored zoom for panel \"{name}\"");
            }
        }
    }

    /// Bind key bindings toàn cục cho workspace.
    pub fn bind_keys(cx: &mut App) {
        cx.bind_keys(vec![
            KeyBinding::new("shift-escape", ToggleZoom, None),
            KeyBinding::new("ctrl-w", ClosePanel, None),
        ]);
    }
}

impl Render for MyTermWorkspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let sheet_layer = Root::render_sheet_layer(window, cx);
        let dialog_layer = Root::render_dialog_layer(window, cx);
        let notification_layer = Root::render_notification_layer(window, cx);

        div()
            .id("myterm-workspace")
            .on_action(cx.listener(Self::on_action_add_panel))
            .on_action(cx.listener(Self::on_action_add_session))
            .on_action(cx.listener(Self::on_action_add_sftp_browser))
            .on_action(cx.listener(Self::on_action_toggle_dock_toggle_button))
            .on_action(cx.listener(Self::on_action_quit))
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .child(self.title_bar.clone())
            .child(div().flex_1().min_h_0().child(self.dock_area.clone()))
            .child(statusbar::build_status_bar(
                &self.dock_area,
                self.clock.clone(),
                window,
                cx,
            ))
            .children(sheet_layer)
            .children(dialog_layer)
            .children(notification_layer)
    }
}