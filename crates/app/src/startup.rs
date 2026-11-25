//! Startup commands (specs/oneterm, "Startup commands"): what is written once a host's shell is
//! open, kept by name in the setting `ssh.startup_commands`, managed on the SSH & hosts page
//! and chosen by name in a host's dialog.

use quadra::{Align, AnyView, Signal, View, column, div, dynamic, effect, row, signal, text};
use quadra_design::{SettingsGroup, Variant, button, fonts, input, select, size, token};

use crate::model::Model;

/// What the select says for a host that runs nothing.
const NONE: &str = "None";

impl Model {
    /// The command saved under `name`.
    pub(crate) fn startup_command(&self, name: &str) -> Option<String> {
        let rows = self.prefs().startup_commands;
        rows.with_untracked(|rows| {
            rows.iter().find(|it| it.first().map(String::as_str) == Some(name))?.get(1).cloned()
        })
    }

    /// Saves `command` under `name` (under its own words where no name is given), in the
    /// place of the one of that name. An empty command saves nothing.
    pub fn save_startup_command(&self, name: &str, command: &str) {
        let command = command.trim();
        let name = if name.trim().is_empty() { command } else { name.trim() };
        if command.is_empty() {
            return;
        }
        self.prefs().startup_commands.update(|rows| {
            let row = vec![name.to_string(), command.to_string()];
            match rows.iter_mut().find(|it| it.first().map(String::as_str) == Some(name)) {
                Some(had) => *had = row,
                None => rows.push(row),
            }
        });
    }

    /// Forgets the command of `name`: the hosts that chose it run nothing.
    pub fn remove_startup_command(&self, name: &str) {
        let is = |it: &Vec<String>, at: usize| it.get(at).map(String::as_str) == Some(name);
        self.prefs().startup_commands.update(|rows| rows.retain(|it| !is(it, 0)));
        self.prefs().startup.update(|rows| rows.retain(|it| !is(it, 1)));
    }
}

/// A select of the commands by name, None first, that writes the name chosen to `name`
/// (empty for None). A name the list does not have, a command since removed, is None.
pub(crate) fn picker(model: &Model, name: Signal<String>, id: &'static str) -> impl View + use<> {
    let rows = model.prefs().startup_commands.get_untracked();
    let names: Vec<String> = rows.into_iter().filter_map(|it| it.into_iter().next()).collect();
    let had = name.get_untracked();
    let chosen = signal(names.iter().position(|it| *it == had).map_or(0, |at| at + 1));
    let options: Vec<String> = std::iter::once(NONE.to_string()).chain(names.iter().cloned()).collect();
    effect(move || {
        let now = chosen.get().checked_sub(1).and_then(|at| names.get(at).cloned()).unwrap_or_default();
        if name.get_untracked() != now {
            name.set(now);
        }
    });
    select(options, chosen).test_id(id)
}

/// The settings' group: each command with Edit and Remove, then a name and a command to save.
pub(crate) fn group(model: &Model) -> SettingsGroup {
    let (commands, name, command) =
        (model.prefs().startup_commands, signal(String::new()), signal(String::new()));
    let (saves, lists) = (model.clone(), model.clone());
    let save = button("Save command")
        .variant(Variant::Secondary)
        .disabled(move || command.with(|it| it.trim().is_empty()))
        .on_click(move || {
            saves.save_startup_command(&name.get_untracked(), &command.get_untracked());
            name.set(String::new());
            command.set(String::new());
        });
    let list = dynamic(move || {
        let all = commands.get();
        if all.is_empty() {
            return AnyView::new(div().text_color(token::TEXT_2).child(text("No commands yet.")));
        }
        AnyView::new(
            all.into_iter()
                .fold(column().grow(1.0).gap(token::SPACE_MD).align_items(Align::Stretch), |list, saved| {
                    list.child(line(&lists, saved, name, command))
                }),
        )
    })
    .container(column().grow(1.0).align_items(Align::Stretch));
    let fields = row()
        .grow(1.0)
        .gap(token::SPACE_MD)
        .align_items(Align::Center)
        .child(input(name).placeholder("Name").label("Command name"))
        .child(input(command).placeholder("tmux attach || tmux new").label("Command").fill())
        .child(save);
    SettingsGroup::new("Startup commands")
        .about(
            "Commands written for you once a host's shell is open. A host's dialog chooses one by name; saving a name again replaces its command.",
        )
        .wide(list)
        .wide(fields)
}

/// A command: its name over what is written, Edit (the fields below take it) and Remove.
fn line(model: &Model, saved: Vec<String>, name: Signal<String>, command: Signal<String>) -> quadra::Div {
    let (named, written) =
        (saved.first().cloned().unwrap_or_default(), saved.get(1).cloned().unwrap_or_default());
    let words = column()
        .grow(1.0)
        .min_width(0.0)
        .gap(token::SPACE_XXS)
        .child(div().font_weight(600).text_size(size::TEXT_MD.size).child(text(named.clone()).ellipsis()))
        .child(
            div()
                .font_family(fonts::MONO)
                .text_size(12.0)
                .text_color(token::TEXT_2)
                .child(text(written.clone()).ellipsis()),
        );
    let (edited, gone, model) = (named.clone(), named.clone(), model.clone());
    let edit = button("Edit").label(format!("Edit {named}")).variant(Variant::Ghost).on_click(move || {
        name.set(edited.clone());
        command.set(written.clone());
    });
    let remove = button("Remove")
        .label(format!("Remove {named}"))
        .variant(Variant::Ghost)
        .on_click(move || model.remove_startup_command(&gone));
    row().gap(token::SPACE_MD).align_items(Align::Center).label(named).child(words).child(edit).child(remove)
}
