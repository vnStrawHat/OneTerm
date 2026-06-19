//! [`AppTitleBar`] — title bar của myTerm2.
//!
//! Mirror `reference/.../story/src/title_bar.rs`, giữ `AppMenuBar` + child
//! (Add Terminal / Add Session / Add SFTP dropdown) + `FontSizeSelector`.
//!
//! Bỏ GitHub / Bell (không dùng cho terminal app).

use std::rc::Rc;

use gpui::{
    Anchor, AnyElement, App, AppContext as _, Context, Entity, FocusHandle, IntoElement,
    InteractiveElement as _, MouseButton, ParentElement as _, Render, Styled as _, Window, div,
    px,
};
use gpui_component::{
    ActiveTheme as _, IconName, Side, Sizable as _, Theme, TitleBar,
    button::{Button, ButtonVariants as _},
    menu::{AppMenuBar, DropdownMenu as _},
    scroll::ScrollbarShow,
};

use crate::actions::{
    AddPanel, AddSftpBrowser, AddSession, SelectFont, SelectRadius, SelectScrollbarShow,
    ToggleListActiveHighlight,
};

use crate::layout::app_menus;

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

/// [`FontSizeSelector`] — dropdown chỉnh font size / radius / scrollbar / list
/// highlight của theme UI (mirror reference `FontSizeSelector`).
struct FontSizeSelector {
    focus_handle: FocusHandle,
}

impl FontSizeSelector {
    pub fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
        }
    }

    fn on_select_font(&mut self, font_size: &SelectFont, window: &mut Window, cx: &mut Context<Self>) {
        Theme::global_mut(cx).font_size = px(font_size.0 as f32);
        window.refresh();
    }

    fn on_select_radius(&mut self, radius: &SelectRadius, window: &mut Window, cx: &mut Context<Self>) {
        Theme::global_mut(cx).radius = px(radius.0 as f32);
        Theme::global_mut(cx).radius_lg = if cx.theme().radius > px(0.) {
            cx.theme().radius + px(2.)
        } else {
            px(0.)
        };
        window.refresh();
    }

    fn on_select_scrollbar_show(
        &mut self,
        show: &SelectScrollbarShow,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        Theme::global_mut(cx).scrollbar_show = show.0;
        window.refresh();
    }

    fn on_toggle_list_active_highlight(
        &mut self,
        _: &ToggleListActiveHighlight,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let theme = Theme::global_mut(cx);
        theme.list.active_highlight = !theme.list.active_highlight;
        window.refresh();
    }
}

impl Render for FontSizeSelector {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focus_handle = self.focus_handle.clone();
        let font_size = cx.theme().font_size.as_f32() as i32;
        let radius = cx.theme().radius.as_f32() as i32;
        let scroll_show = cx.theme().scrollbar_show;

        div()
            .id("font-size-selector")
            .track_focus(&focus_handle)
            .on_action(cx.listener(Self::on_select_font))
            .on_action(cx.listener(Self::on_select_radius))
            .on_action(cx.listener(Self::on_select_scrollbar_show))
            .on_action(cx.listener(Self::on_toggle_list_active_highlight))
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
                            .label("Border Radius")
                            .menu_with_check("8px", radius == 8, Box::new(SelectRadius(8)))
                            .menu_with_check(
                                "6px (default)",
                                radius == 6,
                                Box::new(SelectRadius(6)),
                            )
                            .menu_with_check("4px", radius == 4, Box::new(SelectRadius(4)))
                            .menu_with_check("0px", radius == 0, Box::new(SelectRadius(0)))
                            .separator()
                            .label("Scrollbar")
                            .menu_with_check(
                                "Scrolling to show",
                                scroll_show == ScrollbarShow::Scrolling,
                                Box::new(SelectScrollbarShow(ScrollbarShow::Scrolling)),
                            )
                            .menu_with_check(
                                "Hover to show",
                                scroll_show == ScrollbarShow::Hover,
                                Box::new(SelectScrollbarShow(ScrollbarShow::Hover)),
                            )
                            .menu_with_check(
                                "Always show",
                                scroll_show == ScrollbarShow::Always,
                                Box::new(SelectScrollbarShow(ScrollbarShow::Always)),
                            )
                            .separator()
                            .menu_with_check(
                                "List Active Highlight",
                                cx.theme().list.active_highlight,
                                Box::new(ToggleListActiveHighlight),
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
            menu.menu("New Terminal Tab", Box::new(AddPanel(gpui_component::dock::DockPlacement::Center)))
                .separator()
                .menu("Add Session", Box::new(AddSession))
                .menu("Add SFTP Browser", Box::new(AddSftpBrowser))
                .separator()
        })
}