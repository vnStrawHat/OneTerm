//! The horizontal variant of the tabs (specs/oneterm, "Rail"): a strip of 32 px and its line under
//! the title bar, a tab a button split by lines (the one that shows on the pressed editor
//! colour, semibold, an accent line along its top and Close tab; a progress is a bar along
//! its bottom), New terminal after the last, and "Vertical tabs" at the end.

use quadra::{Align, MouseButton, PointerKind, Role, Signal, State, View, by_text, div, each, row, text};
use quadra_design::{context_menu, icon, icons, searched_icon_button, token, tooltip};

use crate::model::{Model, Tab};
use crate::rail::{Dragged, active_fill, close_button, new_menu, tab_menu, title};

pub(crate) fn strip(model: &Model) -> impl View + use<> {
    let (tabs, vertical, making) = (model.tabs(), model.vertical(), model.clone());
    let shown = model.clone();
    let buttons = each(
        move || tabs.get(),
        |tab| tab.id,
        move |tab: Signal<Tab>| tab_button(&shown, tab.get_untracked()),
    );
    let new = searched_icon_button(icons::PLUS, "New terminal", crate::rail::ASK, move |typed: &str| {
        new_menu(&making, typed)
    })
    .muted();
    row()
        .align_items(Align::Stretch)
        .height(by_text(32.0))
        .shrink(0.0)
        .bg(token::PANEL)
        .border_bottom(1.0)
        .border_color(token::LINE_STRONG)
        .role(Role::TabList)
        .label("Tabs")
        .child(buttons.container(row().align_items(Align::Stretch).min_width(0.0)))
        .child(
            row().align_items(Align::Center).padding_x(token::SPACE_XS).child(tooltip("New terminal", new)),
        )
        .child(div().grow(1.0))
        .child(
            row()
                .align_items(Align::Center)
                .gap(token::SPACE_SM)
                .padding_x(token::SPACE_LG)
                .border_left(1.0)
                .border_color(token::LINE_STRONG)
                .text_size(12.0)
                .text_color(token::TEXT_2)
                .hover(|s| s.bg(token::PANEL_HOVER).text_color(token::TEXT))
                .role(Role::Button)
                .label("Switch to vertical tabs")
                .on_click(move |_| vertical.set(true))
                .child(icon(icons::PANEL_LEFT).size(14.0))
                .child(text("Vertical tabs")),
        )
}

fn tab_button(model: &Model, tab: Tab) -> impl View + use<> {
    let active = model.active();
    let on = move || active.get() == tab.id;
    let (shows, closes, moves, menu) = (model.clone(), model.clone(), model.clone(), model.clone());
    let face = quadra_design::hstack()
        .padding_x(token::SPACE_LG)
        .max_width(240.0)
        .min_width(0.0)
        .text_color(move || if on() { token::TEXT } else { token::TEXT_2 })
        .when(State::SELECTED, |s| s.font_weight(600))
        .border_right(1.0)
        .border_color(token::LINE_STRONG)
        .drop_target(|s| s.bg(token::ACCENT_TINT))
        .role(Role::Tab)
        .label(move || tab.title())
        .selected(on)
        .on_click(move |e| {
            shows.show(tab.id);
            if e.clicks == 2 {
                shows.renaming().set(Some(tab.id));
            }
        })
        .on_pointer(PointerKind::Click, move |e| {
            if e.button == MouseButton::Middle {
                closes.close_tab(tab.id);
            }
        })
        .draggable(move || Dragged(tab.id))
        .on_drop::<Dragged>(move |dragged, _| {
            let to = moves.tabs().with_untracked(|tabs| tabs.iter().position(|it| it.id == tab.id));
            moves.move_tab(dragged.0, to.unwrap_or(usize::MAX));
        })
        .child(active_fill(on, 0.0, true))
        .child(crate::marks::tab_icon(tab, 14.0, on, quadra_design::token::TEXT_3))
        .child(div().min_width(0.0).child(title(model, tab)))
        .child(crate::marks::tab_mark(tab))
        .child(close_button(model, tab, on))
        .child(crate::marks::progress_bar(tab, 0.0));
    context_menu(face, move || tab_menu(&menu, tab.id))
}
