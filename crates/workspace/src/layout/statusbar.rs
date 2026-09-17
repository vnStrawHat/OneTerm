//! Status bar — left: the clock; centre: breadcrumb + git status; right: the
//! network speed, the CPU/memory indicator and the right-dock toggle.
//!
//! The clock, net-speed, breadcrumb, git-status, and resource entities are created once in
//! `OneTermWorkspace::new` and passed in here, avoiding a fresh one each render
//! (which would drop the timer Task → updates stop).
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

use gpui::{Context, ParentElement as _, Pixels, Styled, Window, div, px};
use gpui_component::dock::{DockEvent, DockPlacement};
use gpui_component::{
    ActiveTheme as _, IconName, Sizable,
    button::{Button, ButtonVariants as _},
    status_bar::StatusBar,
};

use crate::layout::OneTermWorkspace;
use crate::widgets::status_text::measure_status_text;

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

/// Build the `StatusBar` for `OneTermWorkspace`.
///
/// The indicator entities are read from the workspace, which created them once
/// so their timers fire reliably — not recreated each render. This is also where
/// the two shortening indicators are told how much width they have, because the
/// bar is the only place that sees every label at once.
pub fn build_status_bar(
    workspace: &OneTermWorkspace,
    window: &mut Window,
    cx: &mut Context<OneTermWorkspace>,
) -> StatusBar {
    let dock_area = workspace.dock_area.clone();
    let clock = workspace.clock.clone();
    let net_speed = workspace.net_speed.clone();
    let breadcrumb = workspace.breadcrumb.clone();
    let git_status = workspace.git_status.clone();
    let resource = workspace.resource.clone();

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
}
