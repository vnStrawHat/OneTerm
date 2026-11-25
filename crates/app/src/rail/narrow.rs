//! The rail collapsed (specs/oneterm, "Rail"; the canvas's option I): 40 px, Expand tabs, a box of
//! 40 px a tab (its icon, its signal as a coloured line under it, the one that shows on the
//! editor colour with a short accent bar; the middle button closes it), then New terminal as
//! a `+`.

use quadra::{
    Align, AnyView, Display, MouseButton, NodeRef, PointerKind, Role, Side, Signal, View, by_text, column,
    div, dynamic, each, row, signal,
};
use quadra_design::{icon, icon_button, icons, progress_busy, searched_menu, token, tooltip};
use quadra_term::ProgramState;

use crate::marks;
use crate::model::{Model, Tab};

pub(crate) fn narrow(model: &Model, folded: Signal<bool>) -> impl View + use<> {
    let (tabs, shown) = (model.tabs(), model.clone());
    let rows =
        each(move || tabs.get(), |tab| tab.id, move |tab: Signal<Tab>| face(&shown, tab.get_untracked()));
    let expand =
        icon_button(icons::PANEL_LEFT_OPEN, "Expand tabs").muted().on_click(move || folded.set(false));
    let top = row()
        .height(by_text(28.0))
        .shrink(0.0)
        .align_items(Align::Center)
        .justify(quadra::Justify::Center)
        .child(tooltip("Expand tabs", expand));
    let (open, opener, making) = (signal(false), NodeRef::new(), model.clone());
    let new = row()
        .grow(1.0)
        .align_items(Align::Center)
        .justify(quadra::Justify::Center)
        .hover(|s| s.bg(token::PANEL_HOVER.alpha(0.5)))
        .role(Role::Button)
        .label("New terminal")
        .node_ref(&opener)
        .on_click(move |_| open.update(|open| *open = !*open))
        .child(icon(icons::PLUS).size(16.0));
    let new = tooltip(
        "New terminal  Ctrl-T",
        boxed().align_items(Align::Stretch).text_color(token::TEXT_2).child(new),
    );
    column()
        .width(40.0)
        .shrink(0.0)
        .bg(token::PANEL)
        .border_right(1.0)
        .border_color(token::LINE)
        .child(top)
        .child(
            column()
                .border_top(1.0)
                .border_color(token::LINE_STRONG)
                .child(rows.container(column().role(Role::TabList).label("Tabs")))
                .child(new),
        )
        .child(searched_menu(open, &opener, Side::Right, crate::rail::ASK, move |typed: &str| {
            crate::rail::new_menu(&making, typed)
        }))
}

/// The box of New terminal: 40 px the rail's width, a line over it, where the tabs end, and
/// one under it, what it holds in its middle.
fn boxed() -> quadra::Div {
    row()
        .height(40.0)
        .shrink(0.0)
        .align_items(Align::Center)
        .justify(quadra::Justify::Center)
        .border_top(1.0)
        .border_bottom(1.0)
        .border_color(token::LINE_STRONG)
}

/// A tab as its icon in a box of 40 px, no line between it and the next, as in the open
/// rail, with its signal as a line of 22 by 4 under it, the one
/// that shows on the pressed editor colour with a short accent bar; its title and signal are
/// its name. Its tooltip is two lines: where its shell is, and what it runs (read when it
/// shows).
fn face(model: &Model, tab: Tab) -> impl View + use<> {
    let (active, shows, closes) = (model.active(), model.clone(), model.clone());
    let on = move || active.get() == tab.id;
    let column = column()
        .height(40.0)
        .shrink(0.0)
        .gap(token::SPACE_SM)
        .align_items(Align::Center)
        .justify(quadra::Justify::Center);
    // While the shell has said no directory, the program it was started as.
    let first = move || match marks::tab_dir(tab) {
        dir if dir.is_empty() => marks::tab_program(tab),
        dir => dir,
    };
    let second = move || marks::tab_runs(tab);
    // The one that shows: the pressed colour a pixel past the rail's line, the bar floating.
    let fill = div()
        .absolute()
        .left(0.0)
        .top(0.0)
        .bottom(0.0)
        .right(-1.0)
        .bg(token::ACCENT_TINT)
        .display(move || if on() { Display::Flex } else { Display::None })
        .child(div().absolute().left(0.0).top(8.0).bottom(8.0).width(3.0).radius(1.0).bg(token::ACCENT));
    tooltip(
        first,
        column
            .hover(|s| s.bg(token::PANEL_HOVER.alpha(0.5)))
            .role(Role::Tab)
            .label(move || marks::tab_said(tab))
            .selected(on)
            .on_click(move |_| shows.show(tab.id))
            .on_pointer(PointerKind::Click, move |e| {
                if e.button == MouseButton::Middle {
                    closes.close_tab(tab.id);
                }
            })
            .child(fill)
            .child(marks::tab_icon(tab, 16.0, on, token::TEXT_2))
            .child(state_line(tab)),
    )
    .side(Side::Right)
    .meta(second)
}

/// What a folded tab's line is.
#[derive(Clone, Copy, PartialEq)]
enum Line {
    None,
    Solid(quadra::Token),
    /// The accent filled to the part, or the error colour for a progress that failed.
    Part(u8, quadra::Token),
    Busy,
    Bell,
}

fn line_of(tab: Tab) -> Line {
    let failed = matches!(tab.progress(), Some(quadra_term::vt::Progress::Error(_)));
    match (tab.status(), marks::part_of(tab)) {
        (ProgramState::Blocked, _) => Line::Solid(token::WARN),
        (ProgramState::Error, _) => Line::Solid(token::ERROR),
        (ProgramState::Done, _) => Line::Solid(token::OK),
        (_, Some(n)) => Line::Part(n, if failed { token::ERROR } else { token::ACCENT }),
        (ProgramState::Working, _) => Line::Busy,
        _ if tab.bells() > 0 => Line::Bell,
        _ => Line::None,
    }
}

/// The signal's line, 22 by 4 with round ends; none leaves the icon in the box's middle.
fn state_line(tab: Tab) -> impl View {
    let line = quadra::memo(move || line_of(tab));
    let bar = || div().width(22.0).height(4.0).shrink(0.0).radius(token::RADIUS_XS);
    // With no line its box is out of the column, or the gap before it would lift the icon.
    let there = move || if line.get() == Line::None { Display::None } else { Display::Flex };
    dynamic(move || {
        AnyView::new(match line.get() {
            Line::None => div().display(Display::None),
            Line::Solid(color) => bar().bg(color),
            Line::Part(n, color) => {
                let fill = div().height(4.0).radius(token::RADIUS_XS).width(f32::from(n) * 0.22).bg(color);
                bar().bg(token::LINE_STRONG).child(fill)
            }
            Line::Busy => div().child(progress_busy().width(22.0).height(4.0).track(token::LINE_STRONG)),
            Line::Bell => {
                let piece = || div().width(9.0).height(4.0).radius(token::RADIUS_XS).bg(token::ERROR);
                row().width(22.0).height(4.0).gap(token::SPACE_XS).child(piece()).child(piece())
            }
        })
    })
    .container(div().display(there))
}
