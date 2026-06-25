//! [`AppTitleBar`] — title bar của myTerm2.
//!
//! Mirror `reference/.../story/src/title_bar.rs`, giữ `AppMenuBar` + child
//! (Add Terminal / Add Session / Add SFTP dropdown) + `FontSizeSelector`.
//!
//! Bỏ GitHub / Bell (không dùng cho terminal app).

use std::rc::Rc;

use gpui::{
    Anchor, AnyElement, App, AppContext as _, Context, Entity, FocusHandle,
    InteractiveElement as _, IntoElement, MouseButton, ParentElement as _, Render, Styled as _,
    Window, div, px,
};
use gpui_component::{
    ActiveTheme as _, IconName, Side, Sizable as _, Theme, TitleBar,
    button::{Button, ButtonVariants as _},
    menu::{AppMenuBar, DropdownMenu as _},
};

use crate::actions::{
    AddPanel, AddSession, AddSftpBrowser, SelectFont, ToggleGutter,
};

use crate::layout::app_menus;
use crate::state::TerminalSettings;

pub struct AppTitleBar {
    app_menu_bar: Entity<AppMenuBar>,
    font_size_selector: Entity<FontSizeSelector>,
    child: Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>,
}

impl AppTitleBar {
    /// Tạo title bar mới.
    pub fn new(
        title: impl Into<gpui::SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let app_menu_bar = app_menus::init(title, cx);
        let font_size_selector = cx.new(|cx| FontSizeSelector::new(window, cx));
        Self {
            app_menu_bar,
            font_size_selector,
            child: Rc::new(|_, _| div().into_any_element()),
        }
    }

    /// Set child element (nút Add Terminal dropdown).
    pub fn child<F, E>(mut self, f: F) -> Self
    where
        E: IntoElement,
        F: Fn(&mut Window, &mut App) -> E + 'static,
    {
        self.child = Rc::new(move |window, cx| f(window, cx).into_any_element());
        self
    }
}

impl Render for AppTitleBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        TitleBar::new()
            // Sync border bottom color với Dock border (cx.theme().border)
            .border_color(cx.theme().border)
            // left side
            .child(div().flex().items_center().child(self.app_menu_bar.clone()))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .px_2()
                    .gap_2()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child((self.child.clone())(window, cx))
                    .child(self.font_size_selector.clone()),
            )
    }
}

/// [`FontSizeSelector`] — dropdown chỉnh font size + bật/tắt Gutter
/// (timestamp + line number) của terminal (mirror reference `FontSizeSelector`,
/// đã bỏ Border Radius — mặc định 0px — Scrollbar — mặc định Scrolling to show
/// — và List Active Highlight — mặc định bật, không toggle).
struct FontSizeSelector {
    focus_handle: FocusHandle,
}

impl FontSizeSelector {
    pub fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
        }
    }

    fn on_select_font(
        &mut self,
        font_size: &SelectFont,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        Theme::global_mut(cx).font_size = px(font_size.0 as f32);
        window.refresh();
    }

    fn on_toggle_gutter(
        &mut self,
        _: &ToggleGutter,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Gutter — cột bên trái terminal hiển thị timestamp [HH:MM:SS] + line
        // number cho mỗi dòng. Bật = hiện (mặc định), tắt = ẩn, terminal dùng
        // toàn bộ chiều rộng. Lưu trong `TerminalSettings` toàn cục, ảnh hưởng
        // lên mọi terminal panel trong app.
        let settings = TerminalSettings::global(cx);
        settings.update(cx, |st, cx| {
            st.show_gutter = !st.show_gutter;
            cx.notify();
        });
        window.refresh();
    }
}

impl Render for FontSizeSelector {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focus_handle = self.focus_handle.clone();
        let font_size = cx.theme().font_size.as_f32() as i32;

        div()
            .id("font-size-selector")
            .track_focus(&focus_handle)
            .on_action(cx.listener(Self::on_select_font))
            .on_action(cx.listener(Self::on_toggle_gutter))
            .child(
                Button::new("btn")
                    .small()
                    .ghost()
                    .icon(IconName::Settings2)
                    .dropdown_menu(move |this, _, cx| {
                        this.scrollable(true)
                            .check_side(Side::Right)
                            .max_h(px(480.))
                            .label("Font Size")
                            .menu_with_check("Large", font_size == 18, Box::new(SelectFont(18)))
                            .menu_with_check(
                                "Medium (default)",
                                font_size == 16,
                                Box::new(SelectFont(16)),
                            )
                            .menu_with_check("Small", font_size == 14, Box::new(SelectFont(14)))
                            .separator()
                            // Gutter — bật/tắt cột timestamp + line number bên trái terminal.
                            .menu_with_check(
                                "Gutter",
                                TerminalSettings::global(cx).read(cx).show_gutter,
                                Box::new(ToggleGutter),
                            )
                    })
                    .anchor(Anchor::TopRight),
            )
    }
}

/// Build nút "Add Terminal Tab" dropdown dùng trong title bar.
///
/// Menu gồm:
/// - New Terminal Tab (thêm TerminalPanel vào center)
/// - Add Session (thêm SessionPanel vào right dock)
/// - Add SFTP Browser (thêm SftpPanel vào right dock)
pub fn add_terminal_button() -> impl IntoElement + 'static {
    Button::new("add-panel")
        .icon(IconName::LayoutDashboard)
        .small()
        .ghost()
        .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _, _| {
            menu.menu(
                "New Terminal Tab",
                Box::new(AddPanel(gpui_component::dock::DockPlacement::Center)),
            )
            .separator()
            .menu("Add Session", Box::new(AddSession))
            .menu("Add SFTP Browser", Box::new(AddSftpBrowser))
            .separator()
        })
}
