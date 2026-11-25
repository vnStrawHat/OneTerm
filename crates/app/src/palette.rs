//! The command palette (specs/oneterm, "Window"): `Ctrl+Shift+P` shows the kit's palette over the
//! window: the app's commands with the keys they are bound to, the tabs to go to, the saved
//! hosts to connect to, the theme and where the tabs stand.

use quadra::{View, use_context};
use quadra_design::{Command as Row, command_palette};

use crate::model::{Command, Model, Side};
use crate::page::keys::bound;

/// The commands the palette runs, by the id their keys are kept under.
const COMMANDS: [(&str, Command); 19] = [
    ("new_tab", Command::NewTab),
    ("split", Command::Split),
    ("split_right", Command::SplitTo(Side::Right)),
    ("split_left", Command::SplitTo(Side::Left)),
    ("split_up", Command::SplitTo(Side::Up)),
    ("split_down", Command::SplitTo(Side::Down)),
    ("close_space", Command::CloseSpace),
    ("focus_next", Command::FocusNext(true)),
    ("focus_previous", Command::FocusNext(false)),
    ("duplicate", Command::Duplicate),
    ("quick_connect", Command::QuickConnect),
    ("dock", Command::Dock),
    ("select_all", Command::SelectAll),
    ("clear", Command::Clear),
    ("zoom_in", Command::Zoom(1)),
    ("zoom_out", Command::Zoom(-1)),
    ("zoom_reset", Command::Zoom(0)),
    ("lock", Command::Lock),
    ("settings", Command::Settings),
];

pub(crate) fn palette(model: &Model) -> impl View + use<> {
    let (open, model, windows) = (model.ui().palette, model.clone(), use_context::<quadra::Windows>());
    // Each command's name and the setting of its keys, read as the palette opens.
    let commands: Vec<_> = COMMANDS
        .into_iter()
        .filter_map(|(id, command)| bound(id).map(|(name, keys)| (name, keys, command)))
        .collect();
    command_palette(open, move || {
        let mut rows = Vec::new();
        for &(name, keys, command) in &commands {
            let keys = keys.get_untracked();
            let (runs, windows) = (model.clone(), windows.clone());
            let row = Row::new(name, move || _ = crate::run(&runs, command, windows.as_ref()));
            rows.push(if keys.is_empty() { row } else { row.keys(keys) }.group("Commands"));
        }
        for tab in model.tabs().get_untracked() {
            let shows = model.clone();
            rows.push(Row::new(tab.title(), move || shows.show(tab.id)).group("Go to tab"));
        }
        for host in model.hosts() {
            let (goes, remote) = (model.clone(), host.remote.clone());
            let row = Row::new(host.name.clone(), move || goes.connect_to(remote.clone(), false));
            rows.push(row.detail(host.address()).group("Connect to"));
        }
        let (theme, vertical) = (model.prefs().theme, model.vertical());
        for (at, name) in crate::prefs::THEMES.into_iter().enumerate() {
            rows.push(Row::new(format!("Theme: {name}"), move || theme.set(at)).group("Look"));
        }
        let tabs = if vertical.get_untracked() {
            "Tabs in a strip at the top"
        } else {
            "Tabs in a rail at the left"
        };
        rows.push(Row::new(tabs, move || vertical.update(|on| *on = !*on)).group("Look"));
        rows
    })
    .placeholder("Type a command, a tab or a host\u{2026}")
}
