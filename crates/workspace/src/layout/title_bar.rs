//! [`AppTitleBar`] — OneTerm's title bar.
//!
//! Mirrors `reference/.../story/src/title_bar.rs`, keeping `AppMenuBar` + the
//! right-dock mode toggle group (SSH Client / Agent / None).
//!
//! Drops GitHub / Bell (not used in a terminal app).
//!
//! ## What is cached (`US-0150`)
//!
//! Everything OneTerm puts in the bar — the icon, the app menu, the elevation
//! suffix and the toggles — is one [`TitleBarContent`] view embedded with
//! `.cached(..)`, so a frame that some other view asked for (a cursor blink, a
//! status-bar tick, terminal output) reuses it instead of rebuilding and laying
//! out the toggle group again. The kit's `TitleBar` around it is **not**
//! cached: its drag region and min/max/close buttons register window-control
//! hitboxes, which gpui does not replay for a reused view
//! (`docs/gui-layout.md` § Frames and re-rendering).
//!
//! While accessibility is active the content is embedded uncached
//! ([`crate::layout::cached_unless_a11y`]): gpui does not replay a reused
//! view's accessibility nodes, and the menu and toggles must stay in the tree.
//!
//! The content re-renders when it, or a view inside it (the app menu), is
//! notified, and when the window is refreshed (a theme switch, a resize). The
//! toggles read `UiConfig::right_dock_mode`, so the content observes `UiConfig`:
//! anything that changes the mode must go through `UiConfig::update` + notify.

use gpui::{
    AnyElement, App, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement,
    MouseButton, ParentElement as _, Pixels, Render, StyleRefinement, Styled as _, Subscription,
    Window, div, px, svg,
};
use gpui_component::{
    ActiveTheme as _, Sizable as _, TitleBar,
    button::{Toggle, ToggleGroup, ToggleVariants as _},
    h_flex,
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
    content: Entity<TitleBarContent>,
}

impl AppTitleBar {
    /// Create a new title bar. `mode_toggles` adds the right-dock mode toggle
    /// group; an elevated window has no right dock and passes `false`.
    pub fn new(
        title: impl Into<gpui::SharedString>,
        mode_toggles: bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let app_menu_bar = app_menus::init(title, cx);
        let content = cx.new(|cx| {
            let config = oneterm_settings::UiConfig::global(cx);
            TitleBarContent {
                app_menu_bar,
                mode_toggles,
                _config: cx.observe(&config, |_, _, cx| cx.notify()),
                #[cfg(test)]
                renders: 0,
            }
        });
        Self { content }
    }
}

impl Render for AppTitleBar {
    #[cfg_attr(
        feature = "hotpath-profiling",
        hotpath::measure(impl_type = "AppTitleBar")
    )]
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        TitleBar::new()
            // Sync the bottom border color with the Dock border.
            //
            // An elevated window is marked by its **title text** and nothing
            // else (`DEC-0019` M5, amended by the owner 2026-09-21): the border
            // briefly carried the theme's warning colour and no longer does.
            // Colour was never the marker the decision relied on — "colour alone
            // is not a marker: themes are user-editable" — so what is left is
            // what was always carrying the weight.
            .border_color(cx.theme().border)
            // The kit's `bar` row is a flex row; the content fills it, so its
            // size comes from the kit's layout and never from its own children.
            .child(crate::layout::cached_unless_a11y(
                self.content.clone(),
                content_style(),
                window,
            ))
    }
}

/// The content's box: the whole of the kit's `bar` row, whatever its children.
fn content_style() -> StyleRefinement {
    StyleRefinement::default().flex_1().h_full()
}

/// What OneTerm draws inside the kit's title bar: the icon, the app menu and
/// the elevation suffix on the left, the mode toggles on the right.
struct TitleBarContent {
    app_menu_bar: Entity<AppMenuBar>,
    mode_toggles: bool,
    _config: Subscription,
    #[cfg(test)]
    renders: usize,
}

impl Render for TitleBarContent {
    #[cfg_attr(
        feature = "hotpath-profiling",
        hotpath::measure(impl_type = "TitleBarContent")
    )]
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(test)]
        {
            self.renders += 1;
        }
        // The same row the kit's `bar` is (`h_flex`, `justify_between`), so the
        // two groups land where they did as the bar's own children.
        h_flex()
            // Test builds only: `the_content_fills_the_kit_row_cached_or_not`.
            .debug_selector(|| "title-bar-content".into())
            .size_full()
            .justify_between()
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
                    .child(self.app_menu_bar.clone())
                    .children(elevation_suffix(cx)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .px_2()
                    .gap_2()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .children(self.mode_toggles.then(|| mode_toggle_group(cx))),
            )
    }
}

/// The elevation suffix, highlighted, or nothing at all in an ordinary window.
///
/// The marker is the **title text** (`DEC-0019` M5, amended by the owner
/// 2026-09-21 — a warning-coloured border came first and was removed), and this
/// is the half of it that is coloured. The OS title bar carries the same words
/// plain, because a window title has no spans; `window_title_parts` is a view of
/// `window_title` rather than a second copy, so the two cannot drift.
///
/// It is a sibling of the app menu bar rather than part of it: the kit renders a
/// menu name as one string and gives it no place for a second colour, and
/// patching `gpui-component` is not allowed (`docs/PROJECT.md`).
///
/// `warning` is now set explicitly by every theme in `crates/theme/themes/` and
/// is checked against `title_bar.background` by
/// `scripts/check-theme-contrast.py`, so the marker is legible in all 39
/// variants rather than merely coloured.
fn elevation_suffix(cx: &App) -> Option<AnyElement> {
    let (_, suffix) =
        oneterm_core::elevation::window_title_parts(oneterm_core::elevation::elevation());
    Some(
        div()
            .flex_none()
            .font_weight(gpui::FontWeight::BOLD)
            .text_color(cx.theme().warning)
            .child(suffix?)
            .into_any_element(),
    )
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

    /// `US-0150`: the content is a cached view — a frame another view asked for
    /// reuses it, and a right-dock mode change re-renders it.
    mod caching {
        use gpui::{
            AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Styled as _,
            TestAppContext, VisualTestContext, Window, div, px, size,
        };
        use gpui_component::TitleBar;
        use oneterm_actions::RightDockMode;
        use oneterm_settings::terminal_settings::{TerminalSettings, TerminalSettingsGlobal};
        use oneterm_settings::ui_config::{UiConfig, UiConfigGlobal};

        use super::super::{AppTitleBar, content_style};
        use crate::layout::embed_view;

        /// Stands in for every other view of the window (a terminal that blinks).
        struct Ticker {
            renders: usize,
        }

        impl Render for Ticker {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                self.renders += 1;
                div()
            }
        }

        struct Host {
            title_bar: Entity<AppTitleBar>,
            ticker: Entity<Ticker>,
        }

        impl Render for Host {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .child(self.title_bar.clone())
                    .child(self.ticker.clone())
            }
        }

        fn counts(host: &Entity<Host>, cx: &mut VisualTestContext) -> (usize, usize) {
            host.read_with(cx, |host, cx| {
                (
                    host.title_bar.read(cx).content.read(cx).renders,
                    host.ticker.read(cx).renders,
                )
            })
        }

        #[gpui::test]
        fn another_views_frame_reuses_the_content_and_a_mode_change_does_not(
            cx: &mut TestAppContext,
        ) {
            cx.update(|cx| {
                gpui_component::init(cx);
                let config = cx.new(|_| UiConfig::default());
                cx.set_global(UiConfigGlobal(config));
                let settings = cx.new(|_| TerminalSettings::default());
                cx.set_global(TerminalSettingsGlobal(settings));
            });
            let (host, cx) = cx.add_window_view(|window, cx| Host {
                title_bar: cx.new(|cx| AppTitleBar::new("OneTerm", true, window, cx)),
                ticker: cx.new(|_| Ticker { renders: 0 }),
            });
            cx.run_until_parked();
            let (content, ticker) = counts(&host, cx);
            assert!(content >= 1 && ticker >= 1, "first frame drew both");

            // Three frames the ticker asked for: the window re-renders from the
            // root, the title bar's content is reused.
            for _ in 0..3 {
                host.update(cx, |host, cx| host.ticker.update(cx, |_, cx| cx.notify()));
                cx.run_until_parked();
            }
            assert_eq!(counts(&host, cx), (content, ticker + 3));

            // The toggles read the right-dock mode: changing it re-renders them.
            cx.update(|_, cx| {
                UiConfig::global(cx).update(cx, |config, cx| {
                    config.right_dock_mode = RightDockMode::Agent;
                    cx.notify();
                });
            });
            cx.run_until_parked();
            assert_eq!(counts(&host, cx).0, content + 1);
        }

        /// A kit title bar around the real content, embedded cached or not.
        struct Frame {
            content: Entity<super::super::TitleBarContent>,
            cached: bool,
        }

        impl Render for Frame {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                div().size_full().child(TitleBar::new().child(embed_view(
                    self.content.clone(),
                    content_style(),
                    self.cached,
                )))
            }
        }

        /// The content fills the kit's row in both branches of the
        /// accessibility switch: it grows with the window and is as tall as
        /// the row, cached or not (`US-0150` F1/F2).
        #[gpui::test]
        fn the_content_fills_the_kit_row_cached_or_not(cx: &mut TestAppContext) {
            cx.update(|cx| {
                gpui_component::init(cx);
                let config = cx.new(|_| UiConfig::default());
                cx.set_global(UiConfigGlobal(config));
                let settings = cx.new(|_| TerminalSettings::default());
                cx.set_global(TerminalSettingsGlobal(settings));
            });
            let (frame, cx) = cx.add_window_view(|window, cx| {
                let title_bar = cx.new(|cx| AppTitleBar::new("OneTerm", true, window, cx));
                Frame {
                    content: title_bar.read(cx).content.clone(),
                    cached: true,
                }
            });
            let mut seen = Vec::new();
            for cached in [true, false] {
                frame.update(cx, |frame, cx| {
                    frame.cached = cached;
                    cx.notify();
                });
                for width in [900., 1100.] {
                    cx.simulate_resize(size(px(width), px(600.)));
                    cx.run_until_parked();
                    let bounds = cx.debug_bounds("title-bar-content").expect("content drawn");
                    seen.push((cached, width, bounds.size));
                }
            }
            let [(_, _, a), (_, _, b), (_, _, c), (_, _, d)] = seen[..] else {
                unreachable!()
            };
            assert_eq!(
                b.width - a.width,
                px(200.),
                "cached content follows the row"
            );
            assert_eq!((c, d), (a, b), "the uncached branch lays out the same");
        }
    }
}
