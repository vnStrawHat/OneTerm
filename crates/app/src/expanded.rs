//! The expanded SFTP view (specs/oneterm, "M9 in detail"): in the workspace's place, Local at the
//! left and Remote at the right, the transfer buttons between, the transfers under both. The
//! canvas gives the layout; the kit's tokens and parts draw it.

use std::path::PathBuf;
use std::rc::Rc;

use oneterm_ssh::Entry;
use quadra::{Align, AnyView, Length, Role, Signal, View, by_text, column, div, dynamic, row, signal, text};
use quadra_design::{Variant, context_menu, icon_button, icons, token, tooltip};

use crate::model::Model;
use crate::panes::{Sort, compared, sorted, visible};
use crate::sftp::Dock;

mod pane;
mod rows;
pub(crate) use pane::heading;
use pane::{LOCAL, REMOTE, pane};
pub(crate) use rows::cell;
use rows::{line, transfers};

const SIZE: f32 = 80.0;
const MODIFIED: f32 = 124.0;
/// The permissions, which the Remote pane has.
const MODE: f32 = 92.0;

/// The parent of a folder as a row, `..`, when it has one.
fn parent() -> Entry {
    Entry { name: "..".into(), dir: true, ..Entry::default() }
}

/// What stands for a list that cannot show: why.
fn said_row(said: String) -> AnyView {
    AnyView::new(
        column()
            .padding(token::SPACE_XS)
            .child(div().padding(token::SPACE_MD).text_color(token::TEXT_2).child(text(said))),
    )
}

pub(crate) fn view(model: &Model, d: Dock) -> impl View + use<> {
    let compare = signal(false);
    // The Remote pane is in the dock's order, and the dock in its.
    let (local_sort, remote_sort): (Signal<Sort>, Signal<Sort>) = (signal(Sort::default()), d.sort);
    let (host_of, refreshes, goes, gets, collapse) =
        (model.clone(), model.clone(), model.clone(), model.clone(), d.expanded);
    let host = move || {
        let space = host_of.active_tab().and_then(|tab| tab.focused());
        space.and_then(|it| it.remote).map(|it| host_of.host_name(&it)).unwrap_or_default()
    };
    let out = icon_button(icons::MINIMIZE_2, "Zoom out")
        .variant(Variant::Link)
        .on_click(move || collapse.set(false));
    let head = quadra_design::hstack()
        .padding_left(token::SPACE_LG)
        .padding_right(token::SPACE_MD)
        .height(by_text(28.0))
        .shrink(0.0)
        .bg(token::PANEL)
        .border_bottom(1.0)
        .border_color(token::LINE)
        .text_size(12.0)
        .text_color(token::TEXT_2)
        .child(div().font_weight(600).child(text("SFTP")))
        .child(text(host))
        .child(
            div()
                .grow(1.0)
                .min_width(Length::Px(0.0))
                .text_color(token::TEXT_3)
                .child(text("ssh session keeps running").ellipsis()),
        )
        .child(tooltip("Zoom out: back to the SFTP side panel", out));
    let (local, locals_of) = (d.local, model.clone());
    let local_rows = dynamic(move || {
        let (all, said) = (visible(&local.entries.get(), d.hidden.get()), local.said.get());
        let all = sorted(all, local_sort.get());
        if !said.is_empty() {
            return said_row(said);
        }
        let mut list = column().width(Length::Fill);
        if local.dir.with(|it| it.parent().is_some()) {
            list = list.child(line(&parent(), None, false, || false).on_click(move |e| {
                if e.clicks == 2 {
                    local.up();
                }
            }));
        }
        let order: Rc<Vec<String>> = Rc::new(all.iter().map(|it| it.name.clone()).collect());
        for entry in all {
            let (name, picks, opens) = (entry.name.clone(), entry.name.clone(), entry.clone());
            let (chosen, order) = (move || local.picked.has(&name), order.clone());
            let line = line(&entry, None, false, chosen);
            let line = line.on_click(move |e| match (opens.dir, e.clicks) {
                (true, 2) => local.load(local.dir.get_untracked().join(&opens.name)),
                _ => local.picked.click(&picks, e.mods, &order),
            });
            list = list.child(context_menu(line, crate::dock::local_menu(&locals_of, d, &entry)));
        }
        AnyView::new(list)
    });
    let remotes = model.clone();
    let remote_rows = dynamic(move || {
        let (all, said) = (visible(&d.entries.get(), d.hidden.get()), d.said.get());
        let all = sorted(all, remote_sort.get());
        if !said.is_empty() {
            return said_row(said);
        }
        let mut list = column().width(Length::Fill);
        let dir = d.dir.get_untracked();
        if dir != "/" && !dir.is_empty() {
            let (ups, up) = (remotes.clone(), dir.rsplit_once('/').map_or("/", |it| it.0).to_string());
            let up = if up.is_empty() { "/".to_string() } else { up };
            list = list.child(line(&parent(), None, true, || false).on_click(move |e| {
                if e.clicks == 2 {
                    d.load(&ups, Some(up.clone()));
                }
            }));
        }
        let locals = if compare.get() { local.entries.get() } else { Vec::new() };
        let order: Rc<Vec<String>> = Rc::new(all.iter().map(|it| it.name.clone()).collect());
        for entry in all {
            let (name, picks, opens, goes) =
                (entry.name.clone(), entry.name.clone(), entry.clone(), remotes.clone());
            let (chosen, order) = (move || d.picked.has(&name), order.clone());
            let line = line(&entry, compared(&entry, &locals), true, chosen);
            let line = line.on_click(move |e| match (opens.dir, e.clicks) {
                (true, 2) => d.load(&goes, Some(oneterm_ssh::join(&d.dir.get_untracked(), &opens.name))),
                _ => d.picked.click(&picks, e.mods, &order),
            });
            list = list.child(context_menu(line, crate::dock::remote_menu(&remotes, d, &entry)));
        }
        AnyView::new(list)
    });
    let local_pane = pane(
        LOCAL,
        move || local.dir.get().display().to_string(),
        move |to: String| {
            if !to.is_empty() {
                local.load(PathBuf::from(to));
            }
        },
        move || local.refresh(),
        crate::dock::more("Local options", d, false),
        local_sort,
        local_rows,
    );
    let remote_pane = pane(
        REMOTE,
        move || d.dir.get(),
        move |to: String| {
            if !to.is_empty() {
                d.load(&goes, Some(to));
            }
        },
        move || d.refresh(&refreshes),
        crate::dock::more("Remote options", d, true),
        remote_sort,
        context_menu(remote_rows, crate::dock::remote_area(model, d)),
    );
    let sends = model.clone();
    let square = |shape, name: &'static str| {
        icon_button(shape, name).variant(Variant::Secondary).size(quadra_design::Size::Md)
    };
    let upload = square(icons::ARROW_RIGHT, "Upload selected").on_click(move || {
        let dir = local.dir.get_untracked();
        let paths: Vec<PathBuf> = local.picked.names().iter().map(|it| dir.join(it)).collect();
        if !paths.is_empty() {
            d.upload(&sends, paths);
        }
    });
    let download = square(icons::ARROW_LEFT, "Download selected").on_click(move || {
        d.chosen().iter().for_each(|it| d.download(&gets, it));
    });
    let compares = square(icons::ARROW_RIGHT_LEFT, "Compare")
        .toggled(move || compare.get())
        .on_click(move || compare.update(|on| *on = !*on));
    let middle = column()
        .width(49.0)
        .shrink(0.0)
        .align_items(Align::Center)
        .justify(quadra::Justify::Center)
        .gap(token::SPACE_MD)
        .bg(token::PANEL)
        .border_left(1.0)
        .border_right(1.0)
        .border_color(token::LINE)
        .child(tooltip("Upload selected to remote", upload))
        .child(tooltip("Download selected to local", download))
        .child(tooltip("Compare local and remote", compares));
    column()
        .grow(1.0)
        .min_width(Length::Px(0.0))
        .bg(token::CANVAS)
        .border(1.0)
        .border_color(token::ACCENT)
        .role(Role::Group)
        .label("SFTP")
        .child(head)
        .child(row().grow(1.0).min_height(0.0).child(local_pane).child(middle).child(remote_pane))
        .child(transfers(d))
}
