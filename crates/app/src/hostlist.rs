//! The rail's SSH hosts (specs/oneterm, "M8 in detail"): a filter, the hosts by group, each
//! connecting on a click, and the dialog that adds one.

mod form;
mod layout;
mod secret;

pub(crate) use form::add_dialog;

use oneterm_ssh::Target;
use quadra::{Align, AnyView, Length, Role, View, by_text, column, div, dynamic, row, signal, text};
use quadra_design::{MenuItem, context_menu, fonts, icon, icon_button, icons, input, token, tooltip};

use crate::hosts::Host;
use crate::model::Model;

const AUTHS: [&str; 4] = ["Auto", "SSH agent", "Key file", "Password"];

/// The heading with its Add button, the filter, the groups (a chevron, the name, the count;
/// folded by a click) and their hosts under a line.
pub(crate) fn hosts(model: &Model) -> impl View + use<> {
    let (filter, folded) = (signal(String::new()), signal(Vec::<String>::new()));
    let (shown, adds) = (model.clone(), model.ui().adding);
    let add = tooltip(
        "Add SSH host",
        icon_button(icons::PLUS, "Add SSH host").muted().on_click(move || adds.set(true)),
    );
    let heading = crate::rail::heading("SSH hosts").child(add);
    let field = input(filter).placeholder("Filter hosts").fill().test_id("host-filter");
    let list = dynamic(move || {
        let words = filter.get().to_lowercase();
        let hosts: Vec<Host> = (shown.hosts().into_iter())
            .filter(|it| {
                (it.name.to_lowercase().contains(&words)) || it.address().to_lowercase().contains(&words)
            })
            .collect();
        let mut groups: Vec<String> = Vec::new();
        for host in &hosts {
            if !groups.contains(&host.group) {
                groups.push(host.group.clone());
            }
        }
        let mut list = column().gap(1.0).width(Length::Fill);
        for group in groups {
            let closed = folded.with(|it| it.contains(&group));
            let count = hosts.iter().filter(|it| it.group == group).count();
            if !group.is_empty() {
                let name = group.clone();
                let fold = move |_: &mut quadra::PointerEvent| {
                    folded.update(|it| match it.iter().position(|g| *g == name) {
                        Some(at) => _ = it.remove(at),
                        None => it.push(name.clone()),
                    })
                };
                let chevron = if closed { icons::CHEVRON_RIGHT } else { icons::CHEVRON_DOWN };
                // Its menu connects to each of its hosts, a tab each.
                let (connects, all) = (shown.clone(), hosts.iter().filter(|it| it.group == group));
                let remotes: Vec<_> = all.map(|it| it.remote.clone()).collect();
                let menu = move || {
                    let (connects, remotes) = (connects.clone(), remotes.clone());
                    let said = format!("Connect all ({})", remotes.len());
                    vec![MenuItem::new(said).on_select(move || connects.connect_all(remotes.clone(), false))]
                };
                list = list.child(context_menu(
                    line()
                        .role(Role::Button)
                        .label(group.clone())
                        .expanded(!closed)
                        .on_click(fold)
                        .child(icon(chevron).size(14.0).color(token::TEXT_2))
                        .child(div().grow(1.0).min_width(0.0).child(text(group.clone()).ellipsis()))
                        .child(address(count.to_string())),
                    menu,
                ));
            }
            // A group's hosts stand under a line from its chevron; those of no group do not.
            let mut under = column().gap(1.0).width(Length::Fill);
            if !group.is_empty() {
                under = under.padding_left(token::SPACE_XS).border_left(1.0).border_color(token::LINE);
            }
            for host in hosts.iter().filter(|it| it.group == group && !closed) {
                under = under.child(host_row(&shown, host.clone()));
            }
            let inset = if group.is_empty() { 0.0 } else { 14.0 };
            list = list.child(div().width(Length::Fill).padding_left(inset).child(under));
        }
        AnyView::new(list)
    });
    column()
        .grow(1.0)
        .min_height(0.0)
        .child(heading)
        .child(row().padding_x(token::SPACE_MD).padding_bottom(token::SPACE_SM).child(field))
        .child(quadra::scroll(signal(0.0), list).container(
            column().grow(1.0).min_height(0.0).padding_x(token::SPACE_SM).padding_bottom(token::SPACE_MD),
        ))
}

/// A row of the list: a data row (24), radius 6, 8 px in, its parts 8 px apart.
fn line() -> quadra::Div {
    row()
        .width(Length::Fill)
        .height(by_text(24.0))
        .align_items(Align::Center)
        .gap(token::SPACE_MD)
        .padding_x(token::SPACE_MD)
        .radius(token::RADIUS_MD)
        .text_color(token::TEXT)
        .hover(|s| s.bg(token::PANEL_HOVER))
}

/// The words at a row's right: in mono, 12 px, the third text colour.
fn address(words: String) -> quadra::Div {
    div().font_family(fonts::MONO).text_size(12.0).text_color(token::TEXT_3).child(text(words))
}

/// A host: the square of its colour tag, its name, its address.
fn host_row(model: &Model, host: Host) -> impl View + use<> {
    let (opens, menu) = (model.clone(), model.clone());
    let (tag, color) = model.host_tag(&host);
    let square = div()
        .size(10.0, 10.0)
        .radius(token::RADIUS_XS)
        .shrink(0.0)
        .bg(color)
        .role(Role::Image)
        .label(format!("Colour tag: {tag}"));
    let named = host.name.clone();
    let face = line()
        .role(Role::Button)
        .label(host.name.clone())
        // Its Connect dialog; a double click's second press, outside it, opens it again.
        .on_click(move |_| opens.ui().opening.host.set(Some((named.clone(), false))))
        .child(square)
        .child(div().grow(1.0).min_width(Length::Px(0.0)).child(text(host.name.clone()).ellipsis()))
        .child(address(short(&host.remote.target)));
    context_menu(face, move || host_menu(&menu, &host))
}

/// The host and its port when it is not 22, as the rail shows it.
pub(crate) fn short(target: &Target) -> String {
    match target.port {
        22 => target.host.clone(),
        port => format!("{}:{port}", target.host),
    }
}

fn host_menu(model: &Model, host: &Host) -> Vec<MenuItem> {
    let (tab, space, gone, forgets) = (model.clone(), model.clone(), model.clone(), model.clone());
    let (a, b, name) = (host.name.clone(), host.name.clone(), host.name.clone());
    let (editing, edited) = (model.ui().editing, host.name.clone());
    let (tab, space) = (tab.ui().opening.host, space.ui().opening.host);
    let mut items = vec![
        MenuItem::new("Connect").on_select(move || tab.set(Some((a.clone(), false)))),
        MenuItem::new("Connect in a new space").on_select(move || space.set(Some((b.clone(), true)))),
        MenuItem::new("Edit…").on_select(move || editing.set(Some(edited.clone()))),
    ];
    if model.sealed(&host.name).is_some() {
        let name = host.name.clone();
        items.push(MenuItem::new("Forget password").on_select(move || forgets.set_sealed(&name, None)));
    }
    items.push(MenuItem::separator());
    items.push(MenuItem::new("Remove").on_select(move || gone.remove_host(&name)));
    items
}
