//! The dock's list (specs/oneterm, "M9 in detail"): a header of the columns, a click on one
//! sorting by it, then `..` and the entries as rows. A column the dock is too narrow for is
//! not shown.

use oneterm_ssh::Entry;
use quadra::{
    AnyView, Length, Paint, Role, Signal, State, View, by_text, column, div, dynamic, signal, text,
};
use quadra_design::{context_menu, icon, icons, token};

use super::{SHOWN, local, menus};
use crate::expanded::{cell, heading};
use crate::locked::shown;
use crate::model::Model;
use crate::panes::{By, day, size, sorted, visible};
use crate::sftp::Dock;
use crate::sftp::properties::letters;

/// The columns after the name: their widths, and the width of the dock each shows from.
const SIZE: f32 = 64.0; // the widest size, `1023.9 MB`, in the mono face
const MODIFIED: (f32, f64) = (92.0, 340.0); // `Nov 16 10:52` in the mono face
const MODE: (f32, f64) = (84.0, 440.0); // `drwxr-xr-x` in the mono face

/// Whether the dock is wide enough for a column shown from `least`.
fn fits(width: Signal<f64>, least: f64) -> impl Fn() -> quadra::Display + 'static {
    shown(move || width.get() >= least)
}

/// The columns' names: a click sorts by one, again the other way.
pub(super) fn header(model: &Model, d: Dock) -> impl View + use<> {
    let width = model.prefs().dock_width;
    quadra_design::hstack()
        .padding_left(token::SPACE_LG)
        .padding_right(token::SPACE_LG)
        .height(by_text(24.0))
        .shrink(0.0)
        .border_left(2.0)
        .border_color(token::ACCENT.alpha(0.0))
        .text_size(12.0)
        .text_color(token::TEXT_2)
        .display(shown(move || !d.dir.with(String::is_empty) && d.said.with(String::is_empty)))
        .role(Role::Group)
        .label("Columns")
        .child(heading(div().grow(1.0), "Name", By::Name, d.sort))
        .child(heading(cell(String::new(), SIZE), "Size", By::Size, d.sort))
        .child(
            heading(cell(String::new(), MODIFIED.0), "Modified", By::Modified, d.sort)
                .display(fits(width, MODIFIED.1)),
        )
        .child(
            heading(cell(String::new(), MODE.0), "Permissions", By::Mode, d.sort)
                .display(fits(width, MODE.1)),
        )
}

pub(super) fn entries(model: &Model, d: Dock) -> impl View + use<> {
    let (model, model_of) = (model.clone(), model.clone());
    let list = dynamic(move || {
        let said = d.said.get();
        if said == crate::sftp::LOCAL {
            return AnyView::new(local(&model));
        }
        if !said.is_empty() {
            return AnyView::new(div().padding(token::SPACE_LG).text_color(token::TEXT_2).child(text(said)));
        }
        let all = sorted(visible(&d.entries.get(), d.hidden.get()), d.sort.get());
        let width = model.prefs().dock_width;
        let mut list = column().width(Length::Fill);
        let dir = d.dir.get_untracked();
        if dir != "/" && !dir.is_empty() {
            let (up, goes) = (dir.rsplit_once('/').map_or("/", |it| it.0).to_string(), model.clone());
            let up = if up.is_empty() { "/".to_string() } else { up };
            let entry = Entry { name: "..".into(), dir: true, ..Entry::default() };
            let line = line(&entry, d, width).label("Parent folder");
            list = list.child(line.on_click(move |e| match e.clicks {
                2 => d.load(&goes, Some(up.clone())),
                _ => d.selected.set(Some("..".into())),
            }));
        }
        for entry in all.iter().take(SHOWN) {
            list = list.child(entry_row(&model, d, entry.clone(), width));
        }
        if all.len() > SHOWN {
            list = list.child(
                div()
                    .padding(token::SPACE_MD)
                    .text_color(token::TEXT_3)
                    .child(text(format!("{} more", all.len() - SHOWN))),
            );
        }
        AnyView::new(list)
    });
    let area = column().grow(1.0).min_height(0.0);
    context_menu(quadra::scroll(signal(0.0), list).container(area), menus::remote_area(&model_of, d))
}

/// A row of 26 px: its icon, its name, then its size, when it changed and its permissions,
/// each under its heading; a row chosen has the press fill and a bar of the accent at its
/// left, a file's icon in the accent.
fn line(entry: &Entry, d: Dock, width: Signal<f64>) -> quadra::Div {
    let (file, named) = (!entry.dir, entry.name.clone());
    let picked = move || d.selected.with(|it| it.as_deref() == Some(named.as_str()));
    let (tints, quiets) = (picked.clone(), picked.clone());
    let tint = move || -> Paint {
        match (file, tints()) {
            (false, _) => token::HUE_BLUE.into(),
            (true, true) => token::ACCENT.into(),
            (true, false) => token::TEXT_2.into(),
        }
    };
    let quiet = move || -> Paint { if quiets() { token::TEXT_2.into() } else { token::TEXT_3.into() } };
    let shape = if entry.dir { icons::FOLDER } else { icons::FILE };
    let size = if file { size(entry.size) } else { String::new() };
    let said = |words: String, width: f32| cell(words, width).text_color(quiet.clone());
    quadra_design::hstack()
        .padding_left(token::SPACE_LG)
        .padding_right(token::SPACE_LG)
        .height(by_text(24.0))
        .shrink(0.0)
        .border_left(2.0)
        .border_color(token::ACCENT.alpha(0.0))
        .hover(|s| s.bg(token::PANEL_HOVER))
        .selected(picked)
        .when(State::SELECTED, |s| s.bg(token::ACCENT_TINT).border_color(token::ACCENT))
        .role(Role::Button)
        .label(entry.name.clone())
        .child(div().text_color(tint).child(icon(shape).size(14.0)))
        .child(div().grow(1.0).min_width(Length::Px(0.0)).child(text(entry.name.clone()).ellipsis()))
        .child(said(size, SIZE))
        .child(said(day(entry.modified), MODIFIED.0).display(fits(width, MODIFIED.1)))
        .child(said(letters(entry.mode), MODE.0).display(fits(width, MODE.1)))
}

/// A click chooses a row; two open a folder or download a file (a reader: the row's menu).
fn entry_row(model: &Model, d: Dock, entry: Entry, width: Signal<f64>) -> impl View + use<> {
    let (opens, kept) = (model.clone(), entry.clone());
    let face = line(&entry, d, width).on_click(move |e| match (kept.dir, e.clicks) {
        (true, 2) => d.load(&opens, Some(oneterm_ssh::join(&d.dir.get_untracked(), &kept.name))),
        (false, 2) => d.download(&opens, &kept),
        _ => d.selected.set(Some(kept.name.clone())),
    });
    context_menu(face, menus::remote(model, d, &entry))
}
