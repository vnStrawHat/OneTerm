//! [`AppTitleBar`] — title bar của myTerm2.
//!
//! Mirror `reference/.../story/src/title_bar.rs`, giữ `AppMenuBar` + child
//! (Add Terminal dropdown). Skeleton bỏ `FontSizeSelector` / GitHub / Bell.

use std::rc::Rc;

use gpui::{
    Anchor, AnyElement, App, Context, Entity, IntoElement, ParentElement, Render, Styled, Window,
    div,
};
use gpui_component::{
    IconName, Sizable, TitleBar,
    button::{Button, ButtonVariants},
    dock::DockPlacement,
    menu::{AppMenuBar, DropdownMenu as _},
};

use crate::actions::AddPanel;

use crate::layout::app_menus;

pub struct AppTitleBar {
    app_menu_bar: Entity<AppMenuBar>,
    child: Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>,
}

impl AppTitleBar {
    /// Tạo title bar mới.
    pub fn new(
        title: impl Into<gpui::SharedString>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let app_menu_bar = app_menus::init(title, cx);
        Self {
            app_menu_bar,
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
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        TitleBar::new()
            .child(div().flex().items_center().child(self.app_menu_bar.clone()))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .px_2()
                    .gap_2()
                    .child((self.child.clone())(_window, _cx)),
            )
    }
}

/// Build nút "Add Terminal Tab" dropdown dùng trong title bar.
pub fn add_terminal_button() -> impl IntoElement + 'static {
    Button::new("add-panel")
        .icon(IconName::LayoutDashboard)
        .small()
        .ghost()
        .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _, _| {
            menu.menu(
                "New Terminal Tab",
                Box::new(AddPanel(DockPlacement::Center)),
            )
            .separator()
        })
}
