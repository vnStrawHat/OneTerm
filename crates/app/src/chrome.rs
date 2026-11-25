//! The window's title bar (specs/oneterm, "Window"): 38 px with the app's mark ("Locked" after it
//! while locked), icon buttons of 22 px (SSH hosts with the tabs in a strip, the bells
//! and notices with their count, Lock with the minutes before it locks, Settings), then the
//! window's buttons; in the settings, Done.

use quadra::{Align, Role, View, div, row, text};
use quadra_design::{button, fonts, icon, icon_button, icons, titlebar, token, tooltip};

use crate::locked::shown;
use crate::model::Model;

/// The colour of the mark: the brand's, the same in every theme.
const BRAND: quadra::Color = quadra::Color::rgb(0x58C4DC);

pub(crate) fn title_bar(model: &Model) -> impl View + use<> {
    let (seen, lock) = (model.clone(), model.lock());
    let (expanded, settings) = (model.ui().expanded, model.ui().settings);
    let locked =
        div().text_color(token::TEXT_2).display(shown(move || lock.locked.get())).child(text("Locked"));
    let mark = row()
        .align_items(Align::Center)
        .gap(token::SPACE_XS)
        .padding_left(token::SPACE_LG)
        .child(icon(icons::TERMINAL).size(20.0).color(BRAND))
        .child(div().padding_x(token::SPACE_XS).text_size(14.0).font_weight(600).child(text("OneTerm")))
        .child(locked);
    let (open, hosts, vertical) = (seen.ui().settings, seen.ui().hosts, seen.vertical());
    // With the tabs in a strip, the hosts are a panel this opens.
    let panel = div().display(shown(move || !vertical.get())).child(tooltip(
        "SSH hosts",
        icon_button(icons::NETWORK, "SSH hosts").muted().on_click(move || hosts.update(|on| *on = !*on)),
    ));
    // The settings open in a window of their own; where there are no windows to open (the
    // harness), in this one.
    let (windows, opens) = (quadra::use_context::<quadra::Windows>(), seen.clone());
    let gear = tooltip(
        "Open settings",
        icon_button(icons::SETTINGS, "Settings").muted().on_click(move || match &windows {
            Some(windows) => crate::page::open_window(windows, &opens),
            None => open.update(|on| *on = !*on),
        }),
    );
    let collapse = div()
        .display(shown(move || expanded.get()))
        .child(button("Collapse SFTP").icon(icons::WINDOW_RESTORE).on_click(move || expanded.set(false)));
    let locks =
        div().display(shown(move || lock.can_lock())).child(tooltip("Lock OneTerm now", lock_button(&seen)));
    // Locked, what would open something is not displayed; in the settings, only Done.
    let buttons = row()
        .align_items(Align::Center)
        .gap(token::SPACE_XS)
        .padding_right(token::SPACE_MD)
        .display(shown(move || !lock.locked.get() && !settings.get()))
        .child(collapse)
        .child(panel)
        .child(bells(&seen))
        .child(locks)
        .child(gear);
    let done = div()
        .padding_right(token::SPACE_MD)
        .display(shown(move || settings.get()))
        .child(button("Done").on_click(move || open.set(false)));
    titlebar("").control(mark).control(div().grow(1.0)).control(buttons).control(done)
}

/// Lock: its icon and the minutes before it locks (`9m`), muted; a click locks now.
fn lock_button(model: &Model) -> impl View + use<> {
    let lock = model.lock();
    row()
        .align_items(Align::Center)
        .justify(quadra::Justify::Center)
        .gap(token::SPACE_XS)
        .padding_x(token::SPACE_XS)
        .min_width(22.0)
        .height(22.0)
        .radius(token::RADIUS_SM)
        .text_color(token::TEXT_2)
        .hover(|s| s.bg(token::PANEL_HOVER))
        .role(Role::Button)
        .label("Lock")
        .on_click(move |_| lock.lock_now())
        .child(icon(icons::LOCK).size(14.0))
        .child(
            div()
                .font_family(fonts::MONO)
                .text_size(12.0)
                .display(shown(move || lock.left().is_some()))
                .child(text(move || lock.left().map(|n| format!("{n}m")).unwrap_or_default())),
        )
}

/// The bells not heard and the notices not seen, one count after a bell, in the error colour
/// while there are any; a click marks the notices seen.
fn bells(model: &Model) -> impl View + use<> {
    let (tabs, unread) = (model.tabs(), model.unread());
    let count = move || crate::signals::all_bells(tabs) + unread.get();
    let tone = move || if count() > 0 { token::ERROR } else { token::TEXT_2 };
    let face = row()
        .align_items(Align::Center)
        .gap(token::SPACE_XS)
        .padding_x(token::SPACE_XS)
        .height(22.0)
        .radius(token::RADIUS_SM)
        .hover(|s| s.bg(token::PANEL_HOVER))
        .role(Role::Button)
        .label(move || format!("{} bells and notices", count()))
        .on_click(move |_| unread.set(0))
        .child(div().text_color(tone).child(icon(icons::BELL).size(14.0)))
        .child(
            div()
                .font_family(fonts::MONO)
                .text_size(12.0)
                .text_color(tone)
                .child(text(move || count().to_string())),
        );
    tooltip("Bells and notices", face)
}
