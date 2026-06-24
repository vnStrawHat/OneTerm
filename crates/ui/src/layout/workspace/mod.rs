//! [`MyTermWorkspace`] — view chính của myTerm2.
//!
//! Module gốc `workspace.rs` đã được tách thành `workspace/`.

use std::time::Duration;

use gpui::{
    App, AppContext, Context, Entity, InteractiveElement as _, IntoElement, KeyBinding,
    ParentElement, Render, Styled, Task, Window, div,
};
use gpui_component::{
    Root,
    dock::{ClosePanel, DockArea, DockEvent, ToggleZoom},
};

use crate::{
    components::DateTimeClock,
    layout::{statusbar, title_bar::AppTitleBar},
    state::AppState,
};

pub(crate) mod actions;
pub(crate) mod layout;
pub(crate) mod persistence;

pub const MAIN_DOCK_VERSION: usize = 2;
pub const MAIN_DOCK_ID: &str = "main-dock";

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
    toggle_button_visible: bool,
    _save_layout_task: Option<Task<()>>,
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

        match Self::load_layout(dock_area.clone(), window, cx) {
            Ok(()) => {
                layout::reset_center_only(weak_dock_area, window, cx);
            }
            Err(_) => {
                layout::reset_default_layout(weak_dock_area, window, cx);
            }
        }

        cx.subscribe_in(
            &dock_area,
            window,
            |this, dock_area, ev: &DockEvent, window, cx| match ev {
                DockEvent::LayoutChanged => this.save_layout(dock_area, window, cx),
                _ => {}
            },
        )
        .detach();

        cx.on_app_quit({
            let dock_area = dock_area.clone();
            move |_, cx| {
                let state = dock_area.read(cx).dump(cx);
                cx.background_executor().spawn(async move {
                    _ = persistence::save_state(&state);
                })
            }
        })
        .detach();

        let title_bar = cx.new(|cx| {
            AppTitleBar::new("myTerm2", window, cx)
                .child(|_window, _cx| crate::layout::title_bar::add_terminal_button())
        });

        let clock = DateTimeClock::new_entity(window, cx);

        Self {
            title_bar,
            dock_area,
            clock,
            last_layout_state: None,
            toggle_button_visible: true,
            _save_layout_task: None,
        }
    }

    /// Debounce save 10s, skip khi state không đổi.
    fn save_layout(
        &mut self,
        dock_area: &Entity<DockArea>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let dock_area = dock_area.clone();
        self._save_layout_task = Some(cx.spawn_in(window, async move |story, window| {
            window
                .background_executor()
                .timer(Duration::from_secs(10))
                .await;

            _ = story.update_in(window, move |this, _, cx| {
                let state = dock_area.read(cx).dump(cx);
                if Some(&state) == this.last_layout_state.as_ref() {
                    return;
                }
                _ = persistence::save_state(&state);
                this.last_layout_state = Some(state);
            });
        }));
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
