//! [`AppTitleBar`] — OneTerm's title bar.
//!
//! Mirrors `reference/.../story/src/title_bar.rs`, keeping `AppMenuBar`. The
//! right-dock mode toggles it used to carry are gone: the right dock follows
//! the active tab's kind (`IN-0054`).
//!
//! Drops GitHub / Bell (not used in a terminal app).
//!
//! ## What is cached (`US-0150`)
//!
//! Everything OneTerm puts in the bar — the icon, the app menu and the elevation
//! suffix — is one [`TitleBarContent`] view embedded with `.cached(..)`, so a
//! frame that some other view asked for (a cursor blink, a status-bar tick,
//! terminal output) reuses it instead of rebuilding it. The kit's `TitleBar` around it is **not**
//! cached: its drag region and min/max/close buttons register window-control
//! hitboxes, which gpui does not replay for a reused view
//! (`docs/gui-layout.md` § Frames and re-rendering).
//!
//! While accessibility is active the content is embedded uncached
//! ([`crate::layout::cached_unless_a11y`]): gpui does not replay a reused
//! view's accessibility nodes, and the menu must stay in the tree.
//!
//! The content re-renders when it, or a view inside it (the app menu), is
//! notified, and when the window is refreshed (a theme switch, a resize).

use gpui::{
    AnyElement, App, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, StyleRefinement, Styled as _, Window, div, svg,
};
use gpui_component::{ActiveTheme as _, TitleBar, h_flex, menu::AppMenuBar};

use oneterm_theme::brand_accent;

use crate::layout::app_menus;

pub struct AppTitleBar {
    content: Entity<TitleBarContent>,
}

impl AppTitleBar {
    /// Create a new title bar.
    pub fn new(
        title: impl Into<gpui::SharedString>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let app_menu_bar = app_menus::init(title, cx);
        let content = cx.new(|_| TitleBarContent {
            app_menu_bar,
            #[cfg(test)]
            renders: 0,
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
/// the elevation suffix.
struct TitleBarContent {
    app_menu_bar: Entity<AppMenuBar>,
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

#[cfg(test)]
mod tests {
    /// `US-0150`: the content is a cached view — a frame another view asked for
    /// reuses it.
    mod caching {
        use gpui::{
            AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Styled as _,
            TestAppContext, VisualTestContext, Window, div, px, size,
        };
        use gpui_component::TitleBar;
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
        fn another_views_frame_reuses_the_content(cx: &mut TestAppContext) {
            cx.update(|cx| {
                gpui_component::init(cx);
                let config = cx.new(|_| UiConfig::default());
                cx.set_global(UiConfigGlobal(config));
                let settings = cx.new(|_| TerminalSettings::default());
                cx.set_global(TerminalSettingsGlobal(settings));
            });
            let (host, cx) = cx.add_window_view(|window, cx| Host {
                title_bar: cx.new(|cx| AppTitleBar::new("OneTerm", window, cx)),
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
                let title_bar = cx.new(|cx| AppTitleBar::new("OneTerm", window, cx));
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
