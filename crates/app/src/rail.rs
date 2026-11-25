//! The rail (specs/oneterm, "Rail"): 270 px of tabs on the panel colour, a heading with the count,
//! a row of 36 px a tab (the kit's tab rules on their side), "New terminal", the SSH hosts. A
//! click shows a tab, the middle button closes it, two clicks rename it (a dialog), the right button
//! opens its menu, and rows drag to reorder.

use quadra::{
    Align, AnyView, Display, Length, MouseButton, NodeRef, PointerKind, Role, Side, Signal, State, View,
    by_text, column, div, dynamic, each, row, signal, text,
};
use quadra_design::{
    MenuItem, context_menu, fonts, icon, icon_button, icons, input, searched_menu, token, tooltip,
};

mod menu;
mod narrow;
mod rename;

pub(crate) use self::menu::{ASK, new_menu};
pub(crate) use self::rename::dialog as rename_dialog;
use crate::marks;
use crate::model::{Model, Tab};

/// What a tab's row carries while it is dragged.
pub(crate) struct Dragged(pub u64);

pub(crate) fn rail(model: &Model) -> impl View + use<> {
    let (folded, shown) = (model.prefs().rail_folded, model.clone());
    dynamic(move || match folded.get() {
        true => AnyView::new(narrow::narrow(&shown, folded)),
        false => AnyView::new(full(&shown, folded)),
    })
}

/// The rail at its width: the tabs, New terminal, the hosts.
fn full(model: &Model, folded: Signal<bool>) -> impl View + use<> {
    let (tabs, expanded, wide) = (model.tabs(), model.ui().expanded, model.prefs().rail_width);
    let count = div()
        .font_family(fonts::MONO)
        .font_weight(400)
        .text_color(token::TEXT_3)
        .child(text(move || tabs.with(Vec::len).to_string()));
    let fold = icon_button(icons::PANEL_LEFT_CLOSE, "Collapse tabs to icons")
        .muted()
        .on_click(move || folded.set(true));
    let heading = heading("Tabs").child(count).child(tooltip("Collapse tabs to icons", fold));
    let shown = model.clone();
    let rows =
        each(move || tabs.get(), |tab| tab.id, move |tab: Signal<Tab>| tab_row(&shown, tab.get_untracked()));
    let (open, opener, making) = (signal(false), NodeRef::new(), model.clone());
    // A box of a dashed line in the row, 4 px from its seams and 6 from its sides; the `+`
    // stays 14 px from the rail's edge.
    let dashed = row()
        .grow(1.0)
        .align_items(Align::Center)
        .gap(token::SPACE_MD)
        .padding_left(token::SPACE_SM)
        .padding_right(token::SPACE_XXS)
        .radius(token::RADIUS_MD)
        .border(1.0)
        .border_style(quadra::BorderStyle::Dashed)
        .border_color(token::LINE_STRONG)
        .hover(|s| s.bg(token::PANEL_HOVER))
        .child(icon(icons::PLUS).size(14.0))
        .child(div().grow(1.0).child(text("New terminal")))
        .child(div().text_size(12.0).text_color(token::TEXT_3).child(text("Ctrl-T")));
    let new = row()
        .height(by_text(40.0))
        .padding_x(token::SPACE_SM)
        .padding_y(token::SPACE_SM)
        .border_bottom(1.0)
        .border_color(token::LINE_STRONG)
        .text_color(token::TEXT_2)
        .role(Role::Button)
        .label("New terminal")
        .node_ref(&opener)
        .on_click(move |_| open.update(|open| *open = !*open))
        .child(dashed);
    // The menu stands at the rail's right, over its edge as a popup of the row.
    let menu = searched_menu(open, &opener, Side::Right, ASK, move |typed: &str| new_menu(&making, typed));
    let strip = column()
        .border_top(1.0)
        .border_color(token::LINE_STRONG)
        .child(rows.container(column().role(Role::TabList).label("Tabs")))
        .child(new);
    column()
        .width(move || wide.get().clamp(140.0, 400.0) as f32)
        .shrink(0.0)
        .bg(token::PANEL)
        .border_right(1.0)
        .border_color(token::LINE)
        .child(heading)
        .child(strip)
        .child(menu)
        // While SFTP is expanded the rail holds the tabs alone.
        .child(
            div()
                .grow(1.0)
                .min_height(0.0)
                .display(crate::locked::shown(move || !expanded.get()))
                .child(crate::hostlist::hosts(model)),
        )
}

/// A heading of the rail, 28 px: 12 px semibold in the second text colour; what is added sits
/// at its right.
pub(crate) fn heading(words: &'static str) -> quadra::Div {
    row()
        .height(by_text(28.0))
        .shrink(0.0)
        .align_items(Align::Center)
        .gap(token::SPACE_SM)
        .padding_left(token::SPACE_LG)
        .padding_right(token::SPACE_SM)
        .text_size(12.0)
        .font_weight(600)
        .text_color(token::TEXT_2)
        .child(div().grow(1.0).child(text(words)))
}

/// The menu of a tab: Rename, Duplicate, Close, Close others, Close to the right.
pub(crate) fn tab_menu(model: &Model, id: u64) -> Vec<MenuItem> {
    let (renames, copies, closes, keeps) = (model.renaming(), model.clone(), model.clone(), model.clone());
    let rights = model.clone();
    vec![
        MenuItem::new("Rename").on_select(move || renames.set(Some(id))),
        MenuItem::new("Duplicate").on_select(move || copies.duplicate(id)),
        MenuItem::separator(),
        MenuItem::new("Close").on_select(move || _ = closes.close_tab(id)),
        MenuItem::new("Close others").on_select(move || keeps.close_others(id)),
        MenuItem::new("Close to the right").on_select(move || rights.close_right(id)),
    ]
}

/// The title of a tab, or the field that renames it in the strip: Enter keeps the name (an empty one
/// gives the program's title back), Escape leaves it as it was.
pub(crate) fn title(model: &Model, tab: Tab) -> impl View + use<> {
    let renaming = model.renaming();
    dynamic(move || {
        if renaming.get() != Some(tab.id) {
            return AnyView::new(text(move || tab.title()).ellipsis());
        }
        let value = signal(tab.name.get_untracked().unwrap_or_else(|| tab.title()));
        let field = input(value).on_submit(move |name: &str| {
            let name = name.trim();
            tab.name.set((!name.is_empty()).then(|| name.to_string()));
            renaming.set(None);
        });
        let field = div()
            .on_key_down(move |e| {
                if e.key == quadra::Key::Escape {
                    renaming.set(None);
                }
            })
            .on_mount(|tree, id| _ = tree.focus_first(id))
            .child(field);
        AnyView::new(field)
    })
}

/// The fill of the tab that shows: the editor's colour pressed, `wider` px past the box's
/// right edge (over the rail's line), under an accent bar of 3 px on the side `bar` names.
pub(crate) fn active_fill(
    on: impl Fn() -> bool + Copy + 'static,
    wider: f32,
    bar_on_top: bool,
) -> quadra::Div {
    let bar = div().absolute().left(0.0).top(0.0).bg(token::ACCENT);
    let bar = if bar_on_top { bar.right(0.0).height(3.0) } else { bar.bottom(0.0).width(3.0) };
    div()
        .absolute()
        .left(0.0)
        .top(0.0)
        .bottom(0.0)
        .right(-wider)
        .bg(token::CANVAS)
        .display(move || if on() { Display::Flex } else { Display::None })
        .child(div().grow(1.0).bg(token::ACCENT_TINT))
        .child(bar)
}

/// Close tab: an × of 14 px in a box of 18, on the tab that shows.
pub(crate) fn close_button(model: &Model, tab: Tab, on: impl Fn() -> bool + 'static) -> quadra::Div {
    let closes = model.clone();
    row()
        .size(18.0, 18.0)
        .shrink(0.0)
        .radius(token::RADIUS_SM)
        .align_items(Align::Center)
        .justify(quadra::Justify::Center)
        .text_color(token::TEXT)
        .hover(|s| s.bg(token::PANEL_HOVER))
        .role(Role::Button)
        .label("Close tab")
        .display(move || if on() { Display::Flex } else { Display::None })
        .on_click(move |e| {
            e.stop();
            closes.close_tab(tab.id);
        })
        .child(icon(icons::X).size(14.0))
}

/// A tab's row: its icon, its title (and while it shows a second line and Close tab), its
/// signal; the pressed editor colour and the accent bar while it shows.
fn tab_row(model: &Model, tab: Tab) -> impl View + use<> {
    let active = model.active();
    let on = move || active.get() == tab.id;
    let (shows, closes, moves, menu) = (model.clone(), model.clone(), model.clone(), model.clone());
    // The second line, only while the tab shows.
    let line = div()
        .text_size(12.0)
        .line_height(17.0 / 12.0)
        .font_weight(400)
        .text_color(token::TEXT_2)
        .display(move || if on() { Display::Flex } else { Display::None })
        .child(text(move || marks::tab_line(tab)).ellipsis());
    let words = column()
        .grow(1.0)
        .min_width(0.0)
        .justify(quadra::Justify::Center)
        .child(div().line_height(20.0 / 14.0).child(text(move || tab.title()).ellipsis()))
        .child(line);
    // An item row; the touch step while it shows, for its second line.
    let (short, tall) = (by_text(28.0), by_text(40.0));
    let face = row()
        .width(Length::Fill)
        .height(move || if on() { tall() } else { short() })
        .align_items(Align::Center)
        .gap(token::SPACE_MD)
        .padding_left(token::SPACE_LG)
        .padding_right(token::SPACE_MD)
        .hover(|s| s.bg(token::PANEL_HOVER.alpha(0.5)))
        .text_color(move || if on() { token::TEXT } else { token::TEXT_2 })
        .when(State::SELECTED, |s| s.font_weight(600))
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
        .child(active_fill(on, 1.0, false))
        .child(marks::tab_icon(tab, 14.0, on, token::TEXT_3))
        .child(words)
        .child(marks::tab_mark(tab))
        .child(close_button(model, tab, on))
        .child(marks::progress_bar(tab, 0.0));
    context_menu(face, move || tab_menu(&menu, tab.id))
}
