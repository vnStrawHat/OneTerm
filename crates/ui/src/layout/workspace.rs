//! [`MyTermWorkspace`] — view chính của myTerm2.
//!
//! Mirror `reference/.../story/examples/dock.rs` `StoryWorkspace`, thay:
//! - center = `DockItem::v_split([tabs([TerminalPanel, ...])])` (wrap trong
//!   StackPanel để DockArea subscribe zoom event của center TabPanel).
//! - right_dock = `DockItem::v_split([SessionPanel, SftpPanel])`
//! - bỏ left_dock + bottom_dock
//! - status bar chỉ còn Toggle Right Dock + DateTimeClock
//! - Add Panel menu: New Terminal Tab / Add Session / Add SFTP Browser

use std::{sync::Arc, time::Duration};

use anyhow::{Context as _, Result};
use gpui::{
    App, AppContext, Context, Edges, Entity, InteractiveElement as _, IntoElement, KeyBinding,
    ParentElement, PromptLevel, Render, Styled, Task, Window, div, px,
};
use gpui_component::{
    Root,
    dock::{ClosePanel, DockArea, DockAreaState, DockEvent, DockItem, PanelStyle, ToggleZoom},
};

use crate::{
    actions::{AddPanel, AddSession, AddSftpBrowser, Quit, ToggleDockToggleButton},
    components::DateTimeClock,
    layout::{statusbar, title_bar::AppTitleBar},
    state::AppState,
    views::{SessionPanel, SftpPanel, TerminalPanel},
};

const MAIN_DOCK_VERSION: usize = 2;
const MAIN_DOCK_ID: &str = "main-dock";

#[cfg(debug_assertions)]
const STATE_FILE: &str = "target/docks.json";
#[cfg(not(debug_assertions))]
const STATE_FILE: &str = "docks.json";

/// Workspace chính: title bar + dock area + status bar.
pub struct MyTermWorkspace {
    pub title_bar: Entity<AppTitleBar>,
    pub dock_area: Entity<DockArea>,
    /// Đồng hồ datetime — tạo 1 lần để timer 1s fire ổn định, không bị drop
    /// khi workspace re-render (nếu tạo mới mỗi render, timer Task bị drop).
    pub clock: Entity<DateTimeClock>,
    last_layout_state: Option<DockAreaState>,
    toggle_button_visible: bool,
    _save_layout_task: Option<Task<()>>,
}

impl MyTermWorkspace {
    /// Tạo workspace mới: load layout cũ (giữ right dock + settings),
    /// nhưng reset center (terminal tabs) về 1 tab mặc định.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        AppState::init(cx);
        // Luôn render tab bar style (kể cả khi chỉ 1 tab) thay vì simple title,
        // để highlight/look của 1 tab giống 2+ tab.
        let dock_area = cx.new(|cx| {
            DockArea::new(MAIN_DOCK_ID, Some(MAIN_DOCK_VERSION), window, cx)
                .panel_style(PanelStyle::TabBar)
        });
        let weak_dock_area = dock_area.downgrade();

        // Load layout cũ (giữ right dock, version check, etc.) nhưng luôn
        // reset center về 1 terminal tab — không restore số tab trước đó.
        match Self::load_layout(dock_area.clone(), window, cx) {
            Ok(()) => {
                // Layout loaded — reset center về 1 terminal tab.
                Self::reset_center_only(weak_dock_area, window, cx);
            }
            Err(_) => {
                // Không có layout cũ hoặc lỗi → full reset default.
                Self::reset_default_layout(weak_dock_area, window, cx);
            }
        }

        // Save layout khi DockEvent::LayoutChanged (debounce 10s).
        // Chỉ save để version check hoạt động — nhưng KHÔNG load lại khi mở.
        cx.subscribe_in(
            &dock_area,
            window,
            |this, dock_area, ev: &DockEvent, window, cx| match ev {
                DockEvent::LayoutChanged => this.save_layout(dock_area, window, cx),
                _ => {}
            },
        )
        .detach();

        // Save layout trước khi quit.
        cx.on_app_quit({
            let dock_area = dock_area.clone();
            move |_, cx| {
                let state = dock_area.read(cx).dump(cx);
                cx.background_executor().spawn(async move {
                    _ = Self::save_state(&state);
                })
            }
        })
        .detach();

        let title_bar = cx.new(|cx| {
            AppTitleBar::new("myTerm2", window, cx)
                .child(|_window, _cx| crate::layout::title_bar::add_terminal_button())
        });

        // Tạo clock entity 1 lần duy nhất — timer spawn_in sẽ fire đều mỗi 1s
        // vì entity được giữ bởi workspace, không bị drop khi re-render.
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
                _ = Self::save_state(&state);
                this.last_layout_state = Some(state);
            });
        }));
    }

    fn save_state(state: &DockAreaState) -> Result<()> {
        tracing::info!("Save layout...");
        let json = serde_json::to_string_pretty(state)?;
        std::fs::write(STATE_FILE, json)?;
        Ok(())
    }

    /// Load layout từ file — dùng để giữ right dock + settings.
    fn load_layout(
        dock_area: Entity<DockArea>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let json = std::fs::read_to_string(STATE_FILE)?;
        let state = serde_json::from_str::<DockAreaState>(&json)?;

        // Version check — prompt reset nếu layout cũ khác version.
        if state.version != Some(MAIN_DOCK_VERSION) {
            let answer = window.prompt(
                PromptLevel::Info,
                "The default main layout has been updated.\n\
                Do you want to reset the layout to default?",
                None,
                &["Yes", "No"],
                cx,
            );

            let weak_dock_area = dock_area.downgrade();
            cx.spawn_in(window, async move |this, window| {
                if answer.await == Ok(0) {
                    _ = this.update_in(window, |_, window, cx| {
                        Self::reset_default_layout(weak_dock_area, window, cx);
                    });
                }
            })
            .detach();
        }

        dock_area.update(cx, |dock_area, cx| {
            dock_area.load(state, window, cx).context("load layout")?;
            dock_area.set_dock_collapsible(
                Edges {
                    right: true,
                    ..Default::default()
                },
                window,
                cx,
            );
            Ok::<(), anyhow::Error>(())
        })
    }

    /// Reset chỉ center (terminal tabs) về 1 tab — giữ right dock + settings.
    fn reset_center_only(dock_area: gpui::WeakEntity<DockArea>, window: &mut Window, cx: &mut App) {
        let weak = dock_area.clone();
        let center = DockItem::v_split(
            vec![DockItem::tabs(
                vec![Arc::new(TerminalPanel::new_entity(window, cx))],
                &weak,
                window,
                cx,
            )],
            &weak,
            window,
            cx,
        );
        _ = dock_area.update(cx, |view, cx| {
            view.set_center(center, window, cx);
            _ = Self::save_state(&view.dump(cx));
        });
    }

    /// Dựng layout mặc định myTerm2: center = terminals, right_dock = Session/SFTP.
    fn reset_default_layout(
        dock_area: gpui::WeakEntity<DockArea>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let weak = dock_area.clone();

        // Center = v_split(tabs([TerminalPanel]))
        //
        // Wrap tabs trong v_split (StackPanel) để DockArea subscribe được
        // PanelEvent::ZoomIn của center TabPanel — nếu dùng bare
        // DockItem::tabs, `set_center` không subscribe zoom event cho center,
        // dẫn tới nút Zoom In/Out không expand full (vẫn thấy right dock).
        let center = DockItem::v_split(
            vec![DockItem::tabs(
                vec![Arc::new(TerminalPanel::new_entity(window, cx))],
                &weak,
                window,
                cx,
            )],
            &weak,
            window,
            cx,
        );

        let right = DockItem::v_split(
            vec![
                DockItem::tab(SessionPanel::new_entity(window, cx), &weak, window, cx),
                DockItem::tab(SftpPanel::new_entity(window, cx), &weak, window, cx),
            ],
            &weak,
            window,
            cx,
        );

        _ = dock_area.update(cx, |view, cx| {
            view.set_version(MAIN_DOCK_VERSION, window, cx);
            view.set_center(center, window, cx);
            view.set_right_dock(right, Some(px(480.)), true, window, cx);
            view.set_dock_collapsible(
                Edges {
                    right: true,
                    ..Default::default()
                },
                window,
                cx,
            );
            _ = Self::save_state(&view.dump(cx));
        });
    }

    /// Action handler: thêm TerminalPanel mới.
    fn on_action_add_panel(
        &mut self,
        action: &AddPanel,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let panel: Arc<dyn gpui_component::dock::PanelView> =
            Arc::new(TerminalPanel::new_entity(window, cx));
        self.dock_area.update(cx, |dock_area, cx| {
            dock_area.add_panel(panel, action.0, None, window, cx);
        });
    }

    /// Action handler: thêm SessionPanel mới vào right dock.
    fn on_action_add_session(
        &mut self,
        _: &AddSession,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let panel: Arc<dyn gpui_component::dock::PanelView> =
            Arc::new(SessionPanel::new_entity(window, cx));
        self.dock_area.update(cx, |dock_area, cx| {
            dock_area.add_panel(
                panel,
                gpui_component::dock::DockPlacement::Right,
                None,
                window,
                cx,
            );
        });
    }

    /// Action handler: thêm SftpPanel mới vào right dock.
    fn on_action_add_sftp_browser(
        &mut self,
        _: &AddSftpBrowser,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let panel: Arc<dyn gpui_component::dock::PanelView> =
            Arc::new(SftpPanel::new_entity(window, cx));
        self.dock_area.update(cx, |dock_area, cx| {
            dock_area.add_panel(
                panel,
                gpui_component::dock::DockPlacement::Right,
                None,
                window,
                cx,
            );
        });
    }

    /// Action handler: toggle nút dock toggle button.
    fn on_action_toggle_dock_toggle_button(
        &mut self,
        _: &ToggleDockToggleButton,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_button_visible = !self.toggle_button_visible;
        self.dock_area.update(cx, |dock_area, cx| {
            dock_area.set_toggle_button_visible(self.toggle_button_visible, cx);
        });
    }

    /// Action handler: Quit.
    fn on_action_quit(&mut self, _: &Quit, _: &mut Window, cx: &mut Context<Self>) {
        cx.quit();
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
