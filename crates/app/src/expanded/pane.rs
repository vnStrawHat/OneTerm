//! A pane of the expanded SFTP view: a bar with its label, its folder in a field, Go and
//! Refresh; a header row; its rows.

use quadra::{Align, Length, Role, Signal, View, by_text, column, div, effect, row, signal, text};
use quadra_design::{icon_button, icons, input, token};

use super::rows::cell;
use super::{MODIFIED, SIZE};
use crate::panes::{By, Sort};

/// What a pane is told: its label, its folder, where to go, how to list it again.
pub(super) struct Side {
    /// Whether its entries have permissions to show: a server's.
    pub modes: bool,
    pub label: &'static str,
    pub go_name: &'static str,
    pub again_name: &'static str,
}

pub(super) const LOCAL: Side = Side {
    modes: false,
    label: "Local",
    go_name: "Local: go to the typed path",
    again_name: "Refresh local",
};
pub(super) const REMOTE: Side = Side {
    modes: true,
    label: "Remote",
    go_name: "Remote: go to the typed path",
    again_name: "Refresh remote",
};

/// A pane; `go` goes to what is typed in its field.
pub(super) fn pane(
    side: Side,
    path: impl Fn() -> String + 'static,
    go: impl Fn(String) + Clone + 'static,
    again: impl FnMut() + 'static,
    sort: Signal<Sort>,
    rows: impl View + 'static,
) -> impl View {
    let typed: Signal<String> = signal(String::new());
    effect(move || {
        let now = path();
        if quadra::untrack(|| typed.get()) != now {
            typed.set(now);
        }
    });
    let goes = go.clone();
    let field = input(typed)
        .mono()
        .small()
        .height(22.0)
        .fill()
        .label(format!("{} path", side.label))
        .on_submit(move |words| go(words.trim().to_string()));
    let bar = quadra_design::hstack()
        .padding_left(token::SPACE_LG)
        .padding_right(token::SPACE_SM)
        .height(by_text(28.0))
        .shrink(0.0)
        .bg(token::PANEL)
        .text_size(12.0)
        .text_color(token::TEXT_2)
        .child(div().font_weight(600).child(text(side.label)))
        .child(field)
        .child(
            icon_button(icons::ARROW_RIGHT, side.go_name)
                .on_click(move || goes(typed.get_untracked().trim().to_string())),
        )
        .child(icon_button(icons::REFRESH_CW, side.again_name).on_click(again));
    let header = row()
        .gap(token::SPACE_LG)
        .padding_x(token::SPACE_LG)
        .height(by_text(28.0))
        .shrink(0.0)
        .align_items(Align::Center)
        .bg(token::PANEL)
        .text_size(12.0)
        .text_color(token::TEXT_2)
        .border_bottom(1.0)
        .border_color(token::LINE)
        .child(heading(div().grow(1.0), "Name", By::Name, sort))
        .child(heading(cell(String::new(), SIZE), "Size", By::Size, sort))
        .child(heading(cell(String::new(), MODIFIED), "Modified", By::Modified, sort));
    let header = match side.modes {
        true => header.child(heading(cell(String::new(), super::MODE), "Permissions", By::Mode, sort)),
        false => header,
    };
    let rows = column().width(Length::Fill).child(rows);
    let list = quadra::scroll(signal(0.0), rows).container(column().grow(1.0).min_height(0.0));
    column()
        .grow(1.0)
        .width(0.0)
        .min_width(Length::Px(0.0))
        .role(Role::Group)
        .label(side.label)
        .child(bar)
        .child(header)
        .child(list)
}

/// A column's label: a click sorts by it, again the other way; the column sorted by is marked.
pub(crate) fn heading(cell: quadra::Div, label: &'static str, by: By, sort: Signal<Sort>) -> quadra::Div {
    cell.role(Role::Button)
        .label(format!("Sort by {label}"))
        .child(text(move || match sort.get() {
            (it, false) if it == by => format!("{label} \u{2191}"),
            (it, true) if it == by => format!("{label} \u{2193}"),
            _ => label.to_string(),
        }))
        .on_click(move |_| sort.update(|(it, back)| (*it, *back) = (by, *it == by && !*back)))
}
