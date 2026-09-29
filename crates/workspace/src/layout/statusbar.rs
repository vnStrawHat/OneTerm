//! Status bar — left: the clock; centre: breadcrumb + git status; right: the
//! network speed, the CPU/memory indicator and the right-dock toggle.
//!
//! The clock, net-speed, breadcrumb, git-status, and resource entities are created once in
//! [`StatusBarView::new`], avoiding a fresh one each render (which would drop the
//! timer Task → updates stop).
//!
//! ## A cached view (`US-0150`)
//!
//! The workspace embeds [`StatusBarView`] through [`embed`]: `.cached(..)` at
//! [`status_bar_height`] (uncached while accessibility is active, see
//! [`crate::layout::cached_unless_a11y`]), so a frame another view asked for (a cursor blink,
//! terminal output) reuses the bar instead of measuring its five labels and
//! laying it out again. The bar re-renders when one of its indicators notifies
//! (each is a child view, so its notify dirties the bar too), when its own
//! button is hovered, and when the window is refreshed or resized.
//!
//! ## Who gives way when the window is narrow (`US-0112`)
//!
//! The bar neither wraps nor scrolls. The two indicators that can grow without
//! bound — the cwd and the git branch — sit in the **centre** region, which the
//! kit lays out as `flex-1` with a zero basis: it takes the width the pinned
//! ends leave and can never push them. So `MEM 577.0 MB` and the dock button
//! stay on screen at any width, whatever the path is.
//!
//! Inside that region the two shortening indicators still have to divide the
//! room, and [`build_status_bar`] measures every label through the window's text
//! system to do it — the only estimated quantities are the bar's own fixed
//! pieces (icons, separators, the button, padding), which do not depend on the
//! text.

use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    AnyElement, Context, Entity, IntoElement, ParentElement as _, Pixels, Render, StyleRefinement,
    Styled, Window, div, px,
};
use gpui_component::dock::{DockArea, DockEvent, DockPlacement};
use gpui_component::{
    ActiveTheme as _, IconName, Sizable,
    button::{Button, ButtonVariants as _},
    status_bar::StatusBar,
};

use crate::widgets::status_text::measure_status_text;
use crate::widgets::{StatusText, breadcrumb, datetime_clock, git_status, net_speed, resource};

/// Everything in the bar that is not a label: the bar's padding, the gaps
/// between its items, the four separators and the dock-toggle button.
const BAR_CHROME: Pixels = px(116.);
/// One indicator's leading icon and the gap after it.
const ICON_CHROME: Pixels = px(16.);
/// The path keeps at least this much before the branch starts giving way.
const MIN_PATH_WIDTH: Pixels = px(80.);
/// ...and the branch keeps at least this much.
const MIN_BRANCH_WIDTH: Pixels = px(40.);

/// How the centre region's width is divided between the cwd and the branch.
struct CentreBudgets {
    path: Pixels,
    git: Pixels,
}

/// Divide what the bar has left between its two shortening indicators.
///
/// `fixed` are the measured widths of the labels that never shorten, `git` the
/// width the git label wants. The path takes the rest; when that would leave it
/// under [`MIN_PATH_WIDTH`], the branch gives way first, down to
/// [`MIN_BRANCH_WIDTH`] — and never past what the centre actually has, or the
/// two budgets together would promise more room than exists.
fn divide_centre(window_width: Pixels, icons: usize, fixed: Pixels, git: Pixels) -> CentreBudgets {
    let chrome = BAR_CHROME + ICON_CHROME * (icons as f32);
    let shrinkable = (window_width - chrome - fixed).max(px(0.));
    if shrinkable - git >= MIN_PATH_WIDTH {
        return CentreBudgets {
            path: shrinkable - git,
            git,
        };
    }
    let git = (shrinkable - MIN_PATH_WIDTH)
        .max(MIN_BRANCH_WIDTH)
        .min(git)
        .min(shrinkable);
    CentreBudgets {
        path: shrinkable - git,
        git,
    }
}

/// The height the kit's `StatusBar` lays itself out at, for the cached embed
/// (a cached view is laid out from its style, not measured).
///
/// `py_1` above and below, a 1 px top border, and the tallest item: the
/// `xsmall` dock button (`h_5`, 1.25 rem). The labels are `text_xs` at gpui's
/// default line height (0.75 rem x 1.618 = 1.21 rem), so they never set it.
/// Everything but the border is in rems, so the bar follows the UI font size;
/// `status_bar_height_matches_the_kit_layout` pins the sum against the real
/// layout.
pub(crate) fn status_bar_height(rem: Pixels) -> Pixels {
    rem * 1.75 + px(1.)
}

/// The bar's box in the workspace's column: full width, [`status_bar_height`].
fn bar_style(window: &Window) -> StyleRefinement {
    StyleRefinement::default()
        .w_full()
        .flex_none()
        .h(status_bar_height(window.rem_size()))
}

/// How the workspace embeds the bar (`US-0150`): cached at [`bar_style`], or
/// uncached in the same box while accessibility is active.
pub(crate) fn embed(bar: &Entity<StatusBarView>, window: &Window) -> AnyElement {
    crate::layout::cached_unless_a11y(bar.clone(), bar_style(window), window)
}

/// The status bar: owns its five indicators and lays them out.
pub struct StatusBarView {
    dock_area: Entity<DockArea>,
    /// Datetime clock — created once so the 1s timer fires reliably.
    clock: Entity<StatusText>,
    /// Network speed indicator — created once so the 1s timer fires reliably.
    net_speed: Entity<StatusText>,
    /// Breadcrumb (cwd + foreground process) indicator — created once so the
    /// 500ms timer fires reliably.
    breadcrumb: Entity<StatusText>,
    /// Git status of the active local terminal's cwd — created once so the
    /// 500ms timer fires reliably.
    git_status: Entity<StatusText>,
    /// CPU/memory resource indicator — created once so the 2s timer fires reliably.
    resource: Entity<StatusText>,
    #[cfg(test)]
    renders: usize,
}

impl StatusBarView {
    pub fn new(dock_area: Entity<DockArea>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        // The bar refreshes both budgets every time it renders; they start wide
        // enough that the first frame shows the labels whole.
        let budget = || Rc::new(Cell::new(px(f32::MAX)));
        Self {
            clock: datetime_clock(window, cx),
            net_speed: net_speed(dock_area.downgrade(), window, cx),
            breadcrumb: breadcrumb(dock_area.downgrade(), budget(), window, cx),
            git_status: git_status(dock_area.downgrade(), budget(), window, cx),
            resource: resource(window, cx),
            dock_area,
            #[cfg(test)]
            renders: 0,
        }
    }

    /// A bar over the given indicators, for tests that need labels they control.
    #[cfg(test)]
    pub(crate) fn with_items(dock_area: Entity<DockArea>, items: [Entity<StatusText>; 5]) -> Self {
        let [clock, net_speed, breadcrumb, git_status, resource] = items;
        Self {
            dock_area,
            clock,
            net_speed,
            breadcrumb,
            git_status,
            resource,
            renders: 0,
        }
    }
}

impl Render for StatusBarView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(test)]
        {
            self.renders += 1;
        }
        // The cached embed fixes the bar's size; the bar fills it.
        build_status_bar(self, window, cx).size_full()
    }
}

/// Build the kit `StatusBar` from the indicators.
///
/// This is also where the two shortening indicators are told how much width
/// they have, because the bar is the only place that sees every label at once.
#[cfg_attr(feature = "hotpath-profiling", hotpath::measure)]
fn build_status_bar(
    bar: &StatusBarView,
    window: &mut Window,
    cx: &mut Context<StatusBarView>,
) -> StatusBar {
    let dock_area = bar.dock_area.clone();
    let clock = bar.clock.clone();
    let net_speed = bar.net_speed.clone();
    let breadcrumb = bar.breadcrumb.clone();
    let git_status = bar.git_status.clone();
    let resource = bar.resource.clone();

    let width_of = |entity: &gpui::Entity<crate::widgets::StatusText>| {
        entity
            .read(cx)
            .text()
            .map(|text| measure_status_text(window, &text))
    };
    let visible = [
        width_of(&clock),
        width_of(&net_speed),
        width_of(&resource),
        width_of(&breadcrumb),
        width_of(&git_status),
    ];
    let icons = visible.iter().filter(|width| width.is_some()).count();
    let fixed = [visible[0], visible[1], visible[2]]
        .into_iter()
        .flatten()
        .fold(px(0.), |total, width| total + width);
    let budgets = divide_centre(
        window.viewport_size().width,
        icons,
        fixed,
        visible[4].unwrap_or(px(0.)),
    );
    breadcrumb.read(cx).set_budget(budgets.path);
    git_status.read(cx).set_budget(budgets.git);

    let separator = || div().w(px(1.)).h(px(12.)).bg(cx.theme().border);

    StatusBar::new()
        // Sync the top border color with the Dock border (cx.theme().border)
        .border_color(cx.theme().border)
        .left(clock)
        // The centre region is `flex-1` with a zero basis, so these two are the
        // only indicators the window can squeeze (`US-0112`).
        .child(
            div()
                .flex()
                .w_full()
                .items_center()
                .gap_2()
                .overflow_hidden()
                .child(separator())
                .child(breadcrumb)
                .child(separator())
                .child(git_status),
        )
        .right(net_speed)
        .right(separator())
        .right(resource)
        .right(separator())
        .right(
            Button::new("toggle-right-dock")
                .ghost()
                .xsmall()
                .icon(IconName::PanelRight)
                .tooltip("Toggle Right Dock")
                .on_click({
                    let dock_area = dock_area.clone();
                    move |_, window, cx| {
                        dock_area.update(cx, |area, cx| {
                            area.toggle_dock(DockPlacement::Right, window, cx);
                            // Trigger a save — toggle_dock does not emit LayoutChanged.
                            cx.emit(DockEvent::LayoutChanged);
                        });
                    }
                }),
        )
}

#[cfg(test)]
mod tests {
    use super::{BAR_CHROME, ICON_CHROME, MIN_BRANCH_WIDTH, MIN_PATH_WIDTH, divide_centre};
    use gpui::px;

    #[test]
    fn a_wide_window_gives_the_path_what_the_branch_does_not_want() {
        // 1900 px, four visible indicators, 300 px of fixed labels, a 200 px
        // branch: the branch keeps all it wants and the path takes the rest.
        let budgets = divide_centre(px(1900.), 4, px(300.), px(200.));
        assert_eq!(budgets.git, px(200.));
        assert_eq!(
            budgets.path,
            px(1900.) - BAR_CHROME - ICON_CHROME * 4. - px(300.) - px(200.)
        );
    }

    #[test]
    fn a_narrow_window_takes_it_out_of_the_branch_first() {
        // 700 px with the same content: the path would be left under its floor,
        // so the branch gives way instead — and neither goes below its floor.
        let budgets = divide_centre(px(700.), 4, px(300.), px(200.));
        assert!(budgets.path >= MIN_PATH_WIDTH, "{:?}", budgets.path);
        assert!(budgets.git >= MIN_BRANCH_WIDTH, "{:?}", budgets.git);
        assert!(budgets.git < px(200.));
    }

    #[test]
    fn no_branch_leaves_the_whole_centre_to_the_path() {
        let budgets = divide_centre(px(900.), 3, px(300.), px(0.));
        assert_eq!(budgets.git, px(0.));
        assert_eq!(
            budgets.path,
            px(900.) - BAR_CHROME - ICON_CHROME * 3. - px(300.)
        );
    }

    #[test]
    fn a_window_too_narrow_for_anything_never_returns_a_negative_budget() {
        let budgets = divide_centre(px(200.), 4, px(300.), px(200.));
        assert!(budgets.path >= px(0.), "{:?}", budgets.path);
        assert!(budgets.git >= px(0.), "{:?}", budgets.git);
    }

    #[test]
    fn the_two_budgets_never_promise_more_room_than_the_centre_has() {
        // The branch floor is not a floor the centre can pay for once the
        // centre is smaller than it: below 40 px the two budgets used to add
        // up to 40 px of room that does not exist, and both labels were then
        // fitted against a width the region could not give them.
        let chrome = BAR_CHROME + ICON_CHROME * 4.;
        for shrinkable in [0., 10., 39., 40., 79., 80., 119., 400.] {
            let budgets = divide_centre(chrome + px(shrinkable), 4, px(0.), px(200.));
            assert!(
                budgets.path + budgets.git <= px(shrinkable),
                "{shrinkable} px of centre handed out {:?} + {:?}",
                budgets.path,
                budgets.git
            );
            assert!(budgets.path >= px(0.) && budgets.git >= px(0.));
        }
    }

    /// `US-0150`: the bar is a cached view laid out at [`status_bar_height`].
    mod caching {
        use std::cell::Cell;
        use std::rc::Rc;
        use std::time::Duration;

        use gpui::{
            AppContext as _, Context, Entity, InteractiveElement as _, IntoElement,
            ParentElement as _, Render, Styled as _, TestAppContext, VisualTestContext, Window,
            div, px,
        };
        use gpui_component::dock::DockArea;
        use gpui_component::{Icon, IconName};

        use super::super::{StatusBarView, bar_style, embed, status_bar_height};
        use crate::layout::embed_view;
        use crate::widgets::StatusText;
        use crate::widgets::status_text::{Label, Presentation, Shorten};

        /// Stands in for every other view of the window (a terminal that blinks).
        struct Ticker;

        impl Render for Ticker {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                div().flex_1()
            }
        }

        /// How the host embeds the bar.
        #[derive(Clone, Copy, PartialEq, Debug)]
        enum Embed {
            /// `statusbar::embed`, what the workspace renders.
            Workspace,
            /// The accessibility branch of the same embed (uncached box).
            Uncached,
            /// A plain child: the kit's own layout of the bar, to measure.
            Natural,
        }

        struct Host {
            bar: Entity<StatusBarView>,
            ticker: Entity<Ticker>,
            embed: Embed,
            rem: gpui::Pixels,
        }

        impl Render for Host {
            fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                window.set_rem_size(self.rem);
                let bar = match self.embed {
                    Embed::Workspace => embed(&self.bar, window),
                    Embed::Uncached => embed_view(self.bar.clone(), bar_style(window), false),
                    Embed::Natural => self.bar.clone().into_any_element(),
                };
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .child(self.ticker.clone())
                    // Sized by the bar alone, so its bounds are the bar's.
                    .child(div().debug_selector(|| "bar".into()).child(bar))
            }
        }

        /// An indicator with a fixed label and an icon, like the real ones.
        fn item(
            id: &'static str,
            text: &'static str,
            shorten: Shorten,
            window: &mut Window,
            cx: &mut gpui::App,
        ) -> Entity<StatusText> {
            StatusText::new_entity(
                id,
                Duration::from_secs(3600),
                Presentation {
                    icon: Some(Icon::new(IconName::Info)),
                    copyable: false,
                    shorten,
                },
                Box::new(move |_| Some(Label::from(text.to_string()))),
                window,
                cx,
            )
        }

        fn host(embed: Embed, cx: &mut TestAppContext) -> (Entity<Host>, &mut VisualTestContext) {
            cx.update(gpui_component::init);
            cx.add_window_view(|window, cx| {
                let budget = || Rc::new(Cell::new(px(f32::MAX)));
                let dock_area = cx.new(|cx| DockArea::new("status-test", None, window, cx));
                let items = [
                    item("clock", "2026-09-29 10:00:00", Shorten::Never, window, cx),
                    item("net", "0 B/s", Shorten::Never, window, cx),
                    item(
                        "crumb",
                        r"C:\Users\me",
                        Shorten::PathTail(budget()),
                        window,
                        cx,
                    ),
                    item("git", "main", Shorten::HeadFirst(budget()), window, cx),
                    item("res", "CPU 0.2%  MEM 57.0 MB", Shorten::Never, window, cx),
                ];
                Host {
                    bar: cx.new(|_| StatusBarView::with_items(dock_area, items)),
                    ticker: cx.new(|_| Ticker),
                    embed,
                    rem: px(16.),
                }
            })
        }

        /// The workspace's embed, and its accessibility branch, lay the bar out
        /// exactly where the kit would: same size at every UI font size.
        #[gpui::test]
        fn status_bar_height_matches_the_kit_layout(cx: &mut TestAppContext) {
            let (host, cx) = host(Embed::Natural, cx);
            for rem in [12., 14., 16., 20.] {
                let mut sizes = Vec::new();
                for embed in [Embed::Natural, Embed::Workspace, Embed::Uncached] {
                    host.update(cx, |host, cx| {
                        host.rem = px(rem);
                        host.embed = embed;
                        cx.notify();
                    });
                    cx.run_until_parked();
                    sizes.push(cx.debug_bounds("bar").expect("bar drawn").size);
                }
                assert_eq!(sizes[0].height, status_bar_height(px(rem)), "rem {rem}");
                assert_eq!(sizes[1], sizes[0], "workspace embed, rem {rem}");
                assert_eq!(sizes[2], sizes[0], "accessibility embed, rem {rem}");
            }
        }

        #[gpui::test]
        fn another_views_frame_reuses_the_bar_and_an_item_notify_does_not(cx: &mut TestAppContext) {
            let (host, cx) = host(Embed::Workspace, cx);
            cx.run_until_parked();
            let renders = |cx: &mut VisualTestContext| {
                host.read_with(cx, |host, cx| host.bar.read(cx).renders)
            };
            let first = renders(cx);
            assert!(first >= 1, "first frame drew the bar");

            // Three frames the ticker asked for: the bar is reused.
            for _ in 0..3 {
                host.update(cx, |host, cx| host.ticker.update(cx, |_, cx| cx.notify()));
                cx.run_until_parked();
            }
            assert_eq!(renders(cx), first);

            // An indicator that changed (the clock ticking) re-renders the bar,
            // which is where the labels are measured.
            host.update(cx, |host, cx| {
                host.bar
                    .update(cx, |bar, cx| bar.clock.update(cx, |_, cx| cx.notify()))
            });
            cx.run_until_parked();
            assert_eq!(renders(cx), first + 1);
        }
    }
}
