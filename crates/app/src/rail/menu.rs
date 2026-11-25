//! The `+` menu (specs/oneterm, "The `+` menu"): Local shell, the other shells found, Quick
//! connect, Split current space, the recent targets, the hosts by group, Add SSH host, under a
//! field: what is typed narrows it.

use quadra_design::{MenuItem, icons};

use crate::model::Model;
use crate::shells;

/// What the searched `+` menu's field says while empty.
pub(crate) const ASK: &str = "Search hosts, or type user@host…";

/// The `+` menu; with `typed`, what holds it, after a row that connects to it when it is a
/// target.
pub(crate) fn new_menu(model: &Model, typed: &str) -> Vec<MenuItem> {
    let words = typed.trim().to_lowercase();
    let holds = |what: &str| words.is_empty() || what.to_lowercase().contains(&words);
    let mut items = Vec::new();
    let connect = oneterm_ssh::Target::parse(typed.trim()).filter(|_| !words.is_empty()).map(|target| {
        let (goes, label) = (model.clone(), target.label());
        let remote = crate::shells::Remote::new(target, oneterm_ssh::Auth::Auto);
        MenuItem::new(format!("Connect to {label}")).on_select(move || goes.connect_to(remote.clone(), false))
    });
    if !words.is_empty() {
        let user = typed.contains('@');
        items.extend(connect.clone().filter(|_| user));
        items.extend(found_hosts(model, &holds));
        items.extend(connect.filter(|_| !user));
        return items;
    }
    let (local, splits) = (model.clone(), model.clone());
    let default = shells::shell_name(model.launch(None).shell.as_ref());
    let first = MenuItem::new("Local shell").icon(crate::shell_icons::of(&default)).keys("Ctrl-T");
    items.push(first.on_select(move || local.new_tab()));
    // The other shells found, in a menu of their own: the menu keeps the canvas's three rows.
    let others: Vec<MenuItem> = shells::found()
        .into_iter()
        .map(|shell| {
            let (makes, name) = (model.clone(), shell.name);
            MenuItem::new(name)
                .icon(crate::shell_icons::of(name))
                .on_select(move || makes.new_tab_with(makes.launch(Some(shell.clone()))))
        })
        .collect();
    if !others.is_empty() {
        items.push(MenuItem::new("Other shells").icon(icons::TERMINAL).submenu(others));
    }
    let connects = model.ui().connecting;
    let quick = MenuItem::new("Quick connect\u{2026}").icon(icons::ZAP).keys("Ctrl-Shift-K");
    items.push(quick.on_select(move || connects.set(true)));
    let split = MenuItem::new("Split current space").icon(icons::COLUMNS_2).keys("Ctrl-D");
    items.push(split.on_select(move || splits.split()));
    items.extend(found_hosts(model, &holds));
    let adds = model.ui().adding;
    items.push(MenuItem::separator());
    items.push(MenuItem::new("Add SSH host\u{2026}").icon(icons::PLUS).on_select(move || adds.set(true)));
    items
}

/// The recent targets (how long ago), then the hosts by group (their addresses), those whose
/// name or address `holds` takes; each with its colour tag.
fn found_hosts(model: &Model, holds: &dyn Fn(&str) -> bool) -> Vec<MenuItem> {
    let mut items = vec![MenuItem::separator()];
    let recent: Vec<_> = model
        .recent()
        .into_iter()
        .filter(|(remote, _)| holds(&model.host_name(remote)) || holds(&remote.target.label()))
        .collect();
    if !recent.is_empty() {
        items.push(MenuItem::heading("Recent"));
    }
    for (remote, at) in recent.into_iter().take(3) {
        let (goes, name) = (model.clone(), model.host_name(&remote));
        let item = MenuItem::new(name).swatch(model.target_tag(&remote.target)).keys(crate::hosts::ago(at));
        items.push(item.on_select(move || goes.connect_to(remote.clone(), false)));
    }
    let hosts = model.hosts().into_iter().filter(|it| holds(&it.name) || holds(&it.address()));
    let mut group = None;
    for host in hosts {
        if group.as_ref() != Some(&host.group) {
            if !host.group.is_empty() {
                items.push(MenuItem::heading(host.group.clone()));
            }
            group = Some(host.group.clone());
        }
        let (goes, remote) = (model.clone(), host.remote.clone());
        let address = match host.jump.is_empty() {
            true => crate::hostlist::short(&host.remote.target),
            false => format!("{} \u{B7} jump", crate::hostlist::short(&host.remote.target)),
        };
        items.push(
            MenuItem::new(host.name.clone())
                .swatch(model.host_tag(&host).1)
                .keys(address)
                .mono()
                .on_select(move || goes.connect_to(remote.clone(), false)),
        );
    }
    // No separator before nothing.
    if items.len() == 1 {
        items.clear();
    }
    items
}
