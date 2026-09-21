//! [`AppTitleBar`] — OneTerm's title bar.
//!
//! Mirrors `reference/.../story/src/title_bar.rs`, keeping `AppMenuBar` + child
//! (right-dock mode toggle group: SSH Client / Agent).
//!
//! Drops GitHub / Bell (not used in a terminal app).

use std::rc::Rc;

use gpui::{
    AnyElement, App, Context, Entity, InteractiveElement as _, IntoElement, MouseButton,
    ParentElement as _, Pixels, Render, Styled as _, Window, div, px, svg,
};
use gpui_component::{
    ActiveTheme as _, Sizable as _, TitleBar,
    button::{Toggle, ToggleGroup, ToggleVariants as _},
    menu::AppMenuBar,
};

use oneterm_actions::{RightDockMode, SetRightDockMode};
use oneterm_theme::brand_accent;

use crate::layout::app_menus;

/// Width of the "SSH Client" / "Agent" segments of the mode toggle group.
const MODE_TOGGLE_WIDE: Pixels = px(70.);
/// Width of the shorter "None" segment.
const MODE_TOGGLE_NARROW: Pixels = px(50.);

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

    /// Set the child element (the right-dock mode toggle group).
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
            // Sync the bottom border color with the Dock border (cx.theme().border),
            // except in an elevated window, which wears the theme's warning colour
            // so it is recognisable at a glance (`DEC-0019` M5). A border rather
            // than a background: it introduces no new text surface, so the
            // contrast gate's `SURFACES` table is untouched.
            .border_color(if oneterm_core::elevation::is_elevated() {
                cx.theme().warning
            } else {
                cx.theme().border
            })
            // left side
            .child(
                div()
                    .flex()
                    .items_center()
                    .child(
                        svg()
                            .text_color(brand_accent())
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

/// Build the right-dock mode toggle group used in the title bar.
///
/// Three segmented toggles — "SSH Client", "Agent", and "None" — that switch
/// the right dock content (see [`RightDockMode`]). "None" hides the right dock
/// entirely; "SSH Client"/"Agent" rebuild and force-open the dock. The active
/// one mirrors the mode persisted in `ui_config.json`; clicking dispatches
/// [`SetRightDockMode`], which the workspace turns into a live right-dock swap.
///
/// This replaces the former `add_terminal_button` dropdown. The "New Terminal
/// Tab" action it used to host is still reachable via its key binding
/// (`Ctrl-T`) and the terminal context menu.
///
/// The three toggles act as a single-select segmented control: clicking any of
/// them — including the one already selected — dispatches `SetRightDockMode`
/// for that mode, which is what reopens a dock the tab bar's dock button
/// collapsed (`BUG-0067`). (`ToggleGroup` is multi-select by nature, so the
/// clicked segment is recovered from the check vector by
/// [`clicked_mode_index`].)
pub fn mode_toggle_group(cx: &App) -> AnyElement {
    let current = oneterm_settings::UiConfig::global(cx)
        .read(cx)
        .right_dock_mode;
    // Index 0 = SshClient, 1 = Agent, 2 = None — kept in sync with the `child`
    // order below.
    let modes = [
        RightDockMode::SshClient,
        RightDockMode::Agent,
        RightDockMode::None,
    ];
    let current_ix = modes.iter().position(|m| *m == current).unwrap_or(0);
    ToggleGroup::new("right-dock-mode")
        .xsmall()
        .outline()
        .segmented()
        .child(
            Toggle::new("ssh-client-mode")
                .label("SSH Client")
                .w(MODE_TOGGLE_WIDE)
                .checked(current_ix == 0),
        )
        .child(
            Toggle::new("agent-mode")
                .label("Agent")
                .w(MODE_TOGGLE_WIDE)
                .checked(current_ix == 1),
        )
        .child(
            Toggle::new("none-mode")
                .label("None")
                .w(MODE_TOGGLE_NARROW)
                .checked(current_ix == 2),
        )
        .on_click(move |checks, window, cx| {
            let Some(mode) = clicked_mode_index(checks, current_ix).and_then(|ix| modes.get(ix))
            else {
                return;
            };
            window.dispatch_action(Box::new(SetRightDockMode(*mode)), cx);
        })
        .into_any_element()
}

/// Which segment the user clicked, from the group's post-click check vector.
///
/// `ToggleGroup` is multi-select: it inverts exactly the clicked toggle and
/// hands the result over. The group was rendered single-select — only
/// `current_ix` checked — so the clicked segment is the one whose post-click
/// state differs from that. Reading it this way (rather than looking for a
/// newly checked toggle) is what lets a click on the already-selected segment
/// dispatch at all: that click arrives as *every* toggle unchecked.
fn clicked_mode_index(checks: &[bool], current_ix: usize) -> Option<usize> {
    (0..checks.len()).find(|&ix| checks[ix] != (ix == current_ix))
}

#[cfg(test)]
mod tests {
    use super::clicked_mode_index;

    #[test]
    fn clicking_another_segment_reports_that_segment() {
        // "Agent" clicked while "SSH Client" was selected.
        assert_eq!(clicked_mode_index(&[true, true, false], 0), Some(1));
    }

    #[test]
    fn clicking_the_selected_segment_reports_it_too() {
        // The bug: the group unchecks the selected segment, so nothing is
        // checked — the click must still resolve to that segment.
        assert_eq!(clicked_mode_index(&[false, false, false], 0), Some(0));
        assert_eq!(clicked_mode_index(&[false, false, false], 2), Some(2));
    }

    #[test]
    fn an_unchanged_vector_reports_no_click() {
        assert_eq!(clicked_mode_index(&[true, false, false], 0), None);
        assert_eq!(clicked_mode_index(&[], 0), None);
    }
}
