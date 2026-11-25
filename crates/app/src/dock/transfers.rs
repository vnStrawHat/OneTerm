//! The dock's transfers (specs/oneterm, "M9 in detail"): each with its arrow, `Uploading name` or
//! `Downloading name`, its state, a cross and a bar of 2 px.

use std::rc::Rc;

use quadra::{Align, AnyView, Length, View, column, div, dynamic, row, text};
use quadra_design::{fonts, icon, icon_button, icons, token};

use crate::locked::shown;
use crate::sftp::{Dock, Transfer};

/// The transfers, newest last, each with a cross that cancels it or clears it.
pub(super) fn transfers(d: Dock) -> impl View + use<> {
    let list = dynamic(move || {
        let list = column().width(Length::Fill).gap(token::SPACE_MD).padding_x(token::SPACE_LG);
        let mut list = list.padding_top(token::SPACE_SM).padding_bottom(token::SPACE_LG).text_size(12.0);
        for transfer in d.transfers.get() {
            list = list.child(transfer_row(d, transfer));
        }
        AnyView::new(list)
    });
    list.container(
        div()
            .shrink(0.0)
            .border_top(1.0)
            .border_color(token::LINE)
            .display(shown(move || d.transfers.with(|it| !it.is_empty()))),
    )
}

/// What a transfer does: `Uploading name`, or why it failed.
fn what(transfer: &Transfer) -> String {
    match transfer.end.get() {
        Some(Err(_)) => transfer.said(),
        _ => format!("{} {}", if transfer.up { "Uploading" } else { "Downloading" }, transfer.name),
    }
}

fn transfer_row(d: Dock, transfer: Rc<Transfer>) -> impl View {
    let (said, gone) = (transfer.clone(), transfer.clone());
    let cross = icon_button(icons::X, "Cancel transfer").size(quadra_design::Size::Xs).on_click(move || {
        if gone.cross() {
            d.transfers.update(|all| all.retain(|it| !Rc::ptr_eq(it, &gone)));
        }
    });
    let (state, part) = (transfer.clone(), transfer.clone());
    let arrow = if transfer.up { icons::ARROW_UP } else { icons::ARROW_DOWN };
    let line = row()
        .align_items(Align::Center)
        .gap(token::SPACE_SM)
        .text_color(token::TEXT_2)
        .child(icon(arrow).size(12.0))
        .child(
            div()
                .grow(1.0)
                .min_width(Length::Px(0.0))
                .text_color(token::TEXT)
                .child(text(move || what(&said)).ellipsis()),
        )
        .child(div().font_family(fonts::MONO).child(text(move || {
            d.tick.get();
            state.state()
        })))
        .child(cross);
    let fill = div().height(2.0).bg(token::ACCENT).width(move || {
        d.tick.get();
        Length::Pct(part.part() * 100.0)
    });
    column().gap(token::SPACE_SM).child(line).child(div().height(2.0).bg(token::LINE_STRONG).child(fill))
}
