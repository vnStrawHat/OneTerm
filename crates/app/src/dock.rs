//! The SFTP dock (specs/oneterm, "M9 in detail"): 460 px at the workspace's right, the folder of
//! the focused space's connection in a field, its entries under a header of columns
//! (`list.rs`), the transfers; files dropped on it upload.

use quadra::{
    Align, AnyView, DroppedFiles, Length, Role, View, by_text, column, div, dynamic, effect, memo, row,
    signal, text,
};
use quadra_design::{Variant, button, icon, icon_button, icons, input, token, tooltip};

use crate::locked::shown;
use crate::model::Model;
use crate::sftp::Dock;

mod grip;
mod list;
mod menus;
mod transfers;
use list::{entries, header};
pub(crate) use menus::{Ask, local as local_menu, remote as remote_menu, remote_area};
use transfers::transfers;

/// How many entries the list shows; a longer folder says how many more.
const SHOWN: usize = 2000;

/// The dock is open or closed as the user left it for the kind of tab that shows: a local
/// shell's, closed at first, or an SSH space's, open.
fn follow_kind(model: &Model, open: quadra::Signal<bool>) {
    let (p, kinds) = (model.prefs(), model.clone());
    let remote = memo(move || {
        kinds.active_tab().is_some_and(|tab| tab.spaces.with(|all| all.iter().any(|it| it.remote.is_some())))
    });
    let slot = move |remote: bool| if remote { p.dock_ssh } else { p.dock_local };
    effect(move || {
        let wanted = slot(remote.get()).get();
        if quadra::untrack(|| open.get()) != wanted {
            open.set(wanted);
        }
    });
    effect(move || {
        let (now, slot) = (open.get(), slot(quadra::untrack(|| remote.get())));
        if quadra::untrack(|| slot.get()) != now {
            slot.set(now);
        }
    });
}

/// The dock, or in the workspace's place the expanded view.
pub(crate) fn dock(model: &Model) -> impl View + use<> {
    let (open, expanded) = (model.ui().dock, model.ui().expanded);
    let d = Dock::new(model, open);
    follow_kind(model, open);
    // Each space's dock, again whenever another space has the focus (the expanded view too).
    let follows = model.clone();
    let focused = memo(move || follows.active_tab().and_then(|tab| tab.focused()).map(|it| it.files_of()));
    let loads = model.clone();
    effect(move || {
        let space = focused.get();
        if open.get() {
            quadra::untrack(|| d.show_space(&loads, space));
        }
    });
    let views = model.clone();
    let shows = dynamic(move || match expanded.get() {
        true => AnyView::new(crate::expanded::view(&views, d)),
        false => AnyView::new(panel(&views, d)),
    });
    // Expanded, it takes the workspace's room.
    let grows = move || if expanded.get() { 1.0 } else { 0.0 };
    row()
        .min_width(quadra::Length::Px(0.0))
        .grow(grows)
        .display(shown(move || open.get()))
        .child(shows.container(row().min_width(quadra::Length::Px(0.0)).grow(grows)))
        .child(menus::dialogs(model, d))
}

fn panel(model: &Model, d: Dock) -> impl View + use<> {
    let host_of = model.clone();
    let host = move || {
        let space = host_of.active_tab().and_then(|tab| tab.focused());
        let remote = space.and_then(|it| it.remote);
        remote.map(|it| host_of.host_name(&it)).unwrap_or_else(|| "local space \u{B7} n/a".into())
    };
    let zoom = icon_button(icons::MAXIMIZE_2, "Zoom SFTP").on_click(move || d.expanded.set(true));
    let head = row()
        .align_items(Align::Center)
        .gap(token::SPACE_SM)
        .padding_left(token::SPACE_LG)
        .padding_right(token::SPACE_SM)
        .height(by_text(28.0))
        .shrink(0.0)
        .text_size(12.0)
        .font_weight(600)
        .text_color(token::TEXT_2)
        .child(text("SFTP"))
        .child(
            div()
                .grow(1.0)
                .min_width(Length::Px(0.0))
                .font_weight(400)
                .text_color(token::TEXT_3)
                .child(text(host).ellipsis()),
        )
        .child(tooltip("Zoom SFTP: local and remote side by side", zoom));
    let (drops, width) = (model.clone(), model.prefs().dock_width);
    let px = move || Length::Px(width.get().clamp(grip::LEAST, grip::MOST) as f32);
    column()
        .width(px)
        .shrink(0.0)
        .bg(token::PANEL)
        .border_left(1.0)
        .border_color(token::LINE)
        .role(Role::Group)
        .label("SFTP")
        .test_id("sftp-dock")
        .on_drop(move |files: &DroppedFiles, _| d.upload(&drops, files.0.clone()))
        .drop_target(|s| s.bg(token::ACCENT_TINT))
        .child(head)
        .child(path(model, d))
        .child(header(model, d))
        .child(entries(model, d))
        .child(transfers(d))
        .child(grip::grip(width))
}

/// The folder in a field: Enter or the arrow goes to what is typed.
fn path(model: &Model, d: Dock) -> impl View + use<> {
    let typed = signal(String::new());
    effect(move || {
        let dir = d.dir.get();
        if quadra::untrack(|| typed.get()) != dir {
            typed.set(dir);
        }
    });
    let go = move |model: &Model| {
        let to = typed.get_untracked().trim().to_string();
        if !to.is_empty() {
            d.load(model, Some(to));
        }
    };
    let (submits, goes) = (model.clone(), model.clone());
    let field = input(typed).mono().small().fill().label("Remote path").test_id("sftp-path");
    row()
        .align_items(Align::Center)
        .gap(token::SPACE_XS)
        .padding_left(token::SPACE_MD)
        .padding_right(token::SPACE_SM)
        .padding_bottom(token::SPACE_SM)
        .display(shown(move || !d.dir.with(String::is_empty)))
        .child(field.on_submit(move |_| go(&submits)))
        .child(icon_button(icons::ARROW_RIGHT, "Go to the typed path").on_click(move || go(&goes)))
}

/// What the dock shows for a local space: why it is empty, and a button that shows the
/// first tab with a remote space.
fn local(model: &Model) -> impl View + use<> {
    let found = model.tabs().get_untracked().into_iter().find_map(|tab| {
        tab.spaces
            .with_untracked(|spaces| spaces.iter().find_map(|it| it.remote.clone()))
            .map(|it| (tab.id, it))
    });
    let mut view = column()
        .grow(1.0)
        .align_items(Align::Center)
        .justify(quadra::Justify::Center)
        .gap(token::SPACE_MD)
        .padding(token::SPACE_XXL)
        .text_size(12.0)
        .text_color(token::TEXT_2)
        .child(icon(icons::FOLDER).size(24.0).color(token::TEXT_3))
        .child(text(crate::sftp::LOCAL));
    if let Some((id, remote)) = found {
        let shows = model.clone();
        let label = format!("Show {}", model.host_name(&remote));
        view = view.child(button(label).variant(Variant::Secondary).on_click(move || shows.show(id)));
    }
    view
}
