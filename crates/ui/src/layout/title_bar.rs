//! [`AppTitleBar`] — OneTerm's title bar.
//!
//! Mirrors `reference/.../story/src/title_bar.rs`, keeping `AppMenuBar` + child
//! (Add Terminal / Add Session / Add SFTP dropdown). Font Size + Gutter moved to the
//! AppMenuBar "View" menu (see `app_menus.rs`).
//!
//! Drops GitHub / Bell (not used in a terminal app).

use std::rc::Rc;

use gpui::{
    Anchor, AnyElement, App, Context, Entity, InteractiveElement as _, IntoElement, MouseButton,
    ParentElement as _, Render, Styled as _, Window, div, rgb, svg,
};
use gpui_component::{
    ActiveTheme as _, IconName, Sizable as _, TitleBar,
    button::{Button, ButtonVariants as _},
    menu::{AppMenuBar, DropdownMenu as _},
};

use crate::actions::{AddPanel, AddSession, AddSftpBrowser};

use crate::layout::app_menus;

pub struct AppTitleBar {
    app_menu_bar: Entity<AppMenuBar>,
    child: Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>,
}

impl AppTitleBar {
    /// Create a new title bar.
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

    /// Set the child element (the Add Terminal dropdown button).
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
            // Sync the bottom border color with the Dock border (cx.theme().border)
            .border_color(cx.theme().border)
            // left side
            .child(
                div()
                    .flex()
                    .items_center()
                    .child(
                        svg()
                            .text_color(rgb(0x58c4dc))
                            .size_5()
                            .flex_none()
                            .path("icons/terminal.svg"),
                    )
                    .child(self.app_menu_bar.clone()),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .px_2()
                    .gap_2()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child((self.child.clone())(window, cx)),
            )
    }
}

/// Build the "Add Terminal Tab" dropdown button used in the title bar.
///
/// The menu contains:
/// - New Terminal Tab (adds a TerminalPanel to the center)
/// - Add Session (adds a SessionPanel to the right dock)
/// - Add SFTP Browser (adds an SftpPanel to the right dock)
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
