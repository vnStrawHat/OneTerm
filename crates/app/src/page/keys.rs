//! The Keybindings page (specs/oneterm, "Settings"): a row an action, with its keys, Reset, and a
//! warning when another action has the same keys. Each action's keys are the setting
//! `keys.<id>`: absent, its default; empty, unbound. The keymap is built again from them
//! whenever one changes, and put in the app's (`app_keymap()`).

use std::rc::Rc;

use quadra::{Keymap, KeymapError, Keystroke, Role, Settings, Signal, effect, text, use_context};
use quadra_design::{SettingsGroup, key_input, token};
use quadra_term::{Scroll, TERMINAL_CONTEXT, TermCommand};

use crate::model::{Command, Side};

/// Binds keys to an action, in its context.
type Bind = fn(&mut Keymap, &str) -> Result<(), KeymapError>;

/// The keys that go through a terminal's history.
fn scrolls(keys: &mut Keymap, stroke: &str, to: Scroll) -> Result<(), KeymapError> {
    keys.bind_in(TERMINAL_CONTEXT, stroke, TermCommand::Scroll(to))
}

/// An action the user can bind: its id in the settings, its name, its default keys.
pub(crate) struct Binding {
    pub id: &'static str,
    pub name: &'static str,
    pub keys: &'static str,
    bind: Bind,
}

const fn binding(id: &'static str, name: &'static str, keys: &'static str, bind: Bind) -> Binding {
    Binding { id, name, keys, bind }
}

/// The actions, in the order of the page.
pub(crate) const BINDINGS: [Binding; 27] = [
    binding("new_tab", "New tab", "ctrl-t", |k, s| k.bind(s, Command::NewTab)),
    binding("split", "Split the space", "ctrl-d", |k, s| k.bind(s, Command::Split)),
    binding("split_right", "Split right", "ctrl-shift-right", |k, s| {
        k.bind(s, Command::SplitTo(Side::Right))
    }),
    binding("split_left", "Split left", "ctrl-shift-left", |k, s| k.bind(s, Command::SplitTo(Side::Left))),
    binding("split_up", "Split up", "ctrl-shift-up", |k, s| k.bind(s, Command::SplitTo(Side::Up))),
    binding("split_down", "Split down", "ctrl-shift-down", |k, s| k.bind(s, Command::SplitTo(Side::Down))),
    binding("close_space", "Close the space", "ctrl-w", |k, s| k.bind(s, Command::CloseSpace)),
    binding("focus_next", "Next space", "ctrl-alt-right", |k, s| k.bind(s, Command::FocusNext(true))),
    binding("focus_previous", "Previous space", "ctrl-alt-left", |k, s| k.bind(s, Command::FocusNext(false))),
    binding("lock", "Lock", "ctrl-shift-l", |k, s| k.bind(s, Command::Lock)),
    binding("quick_connect", "Quick connect", "ctrl-shift-k", |k, s| k.bind(s, Command::QuickConnect)),
    binding("settings", "Settings", "ctrl-,", |k, s| k.bind(s, Command::Settings)),
    binding("dock", "SFTP dock", "ctrl-shift-b", |k, s| k.bind(s, Command::Dock)),
    binding("select_all", "Select all", "ctrl-shift-a", |k, s| k.bind(s, Command::SelectAll)),
    binding("find", "Find", "ctrl-f", |k, s| k.bind(s, crate::find::Find)),
    binding("clear", "Clear the scrollback", "", |k, s| k.bind(s, Command::Clear)),
    binding("duplicate", "Duplicate the tab", "", |k, s| k.bind(s, Command::Duplicate)),
    binding("zoom_in", "Larger text", "ctrl-=", |k, s| k.bind(s, Command::Zoom(1))),
    binding("zoom_out", "Smaller text", "ctrl-minus", |k, s| k.bind(s, Command::Zoom(-1))),
    binding("zoom_reset", "Text size back", "ctrl-0", |k, s| k.bind(s, Command::Zoom(0))),
    binding("quit", "Quit", "ctrl-shift-q", |k, s| k.bind(s, Command::Quit)),
    binding("copy", "Copy", "ctrl-shift-c", |k, s| k.bind_in(TERMINAL_CONTEXT, s, TermCommand::Copy)),
    binding("paste", "Paste", "ctrl-shift-v", |k, s| k.bind_in(TERMINAL_CONTEXT, s, TermCommand::Paste)),
    binding("scroll_page_up", "Scroll a page up", "shift-pageup", |k, s| scrolls(k, s, Scroll::PageUp)),
    binding("scroll_page_down", "Scroll a page down", "shift-pagedown", |k, s| {
        scrolls(k, s, Scroll::PageDown)
    }),
    binding("scroll_top", "Scroll to the top", "ctrl-shift-home", |k, s| scrolls(k, s, Scroll::Top)),
    binding("scroll_bottom", "Scroll to the bottom", "ctrl-shift-end", |k, s| scrolls(k, s, Scroll::Bottom)),
];

/// The app's keymap: the kit's, the terminal's keys that do not change, then each action's
/// keys as `keys(id)` says (`None`: the default). Empty is unbound; what is no keystroke (a
/// file edited by hand) is the default.
pub(crate) fn keymap(keys: impl Fn(&str) -> Option<String>) -> Keymap {
    let mut map = quadra_design::keymap();
    let fixed = [
        ("tab", TermCommand::Tab(false)),
        ("shift-tab", TermCommand::Tab(true)),
        ("ctrl-insert", TermCommand::Copy),
        ("shift-insert", TermCommand::Paste),
    ];
    for (stroke, command) in fixed {
        map.bind_in(TERMINAL_CONTEXT, stroke, command).expect("the terminal's bindings are well formed");
    }
    // The editor of a file edited in OneTerm, and its Ctrl+S.
    quadra_code::bind_keys(&mut map);
    map.bind_in(crate::editing::CONTEXT, "ctrl-s", crate::editing::Save).expect("ctrl-s is well formed");
    for b in &BINDINGS {
        let chosen = keys(b.id).unwrap_or_else(|| b.keys.to_string());
        if !chosen.is_empty() && (b.bind)(&mut map, &chosen).is_err() {
            (b.bind)(&mut map, b.keys).expect("the app's bindings are well formed");
        }
    }
    map
}

/// The setting of each action's keys, in the order of [`BINDINGS`].
fn settings() -> Vec<Signal<String>> {
    let settings = use_context::<Rc<Settings>>().unwrap_or_else(Settings::memory);
    BINDINGS.iter().map(|b| settings.value(&format!("keys.{}", b.id), b.keys.to_string())).collect()
}

/// Puts the keymap of the settings in the app's whenever one changes: called once, with the
/// main window. Without an app (a bare tree) it does nothing.
pub(crate) fn follow() {
    let Some(app) = use_context::<Signal<Rc<Keymap>>>() else { return };
    let keys = settings();
    effect(move || {
        let map = keymap(|id| BINDINGS.iter().position(|b| b.id == id).map(|at| keys[at].get()));
        app.set_always(Rc::new(map));
    });
}

/// Keys as the keymap reads them, written one way (`ctrl-,` is `ctrl-comma`); `None` for
/// none or what is no keystroke.
fn normal(keys: &str) -> Option<String> {
    let strokes: Result<Vec<_>, _> = keys.split_whitespace().map(Keystroke::parse).collect();
    let strokes = strokes.ok().filter(|s| !s.is_empty())?;
    Some(strokes.iter().map(Keystroke::to_string).collect::<Vec<_>>().join(" "))
}

/// The page's group: a row an action.
pub(crate) fn group() -> SettingsGroup {
    let keys = Rc::new(settings());
    BINDINGS.iter().enumerate().fold(SettingsGroup::new("Keys"), |group, (at, b)| {
        let (mine, all, default) = (keys[at], keys.clone(), b.keys);
        // The others with the same keys, named.
        let clash = move || {
            let Some(me) = normal(&mine.get()) else { return String::new() };
            let same =
                (0..BINDINGS.len()).filter(|&i| i != at && normal(&all[i].get()).as_ref() == Some(&me));
            let names: Vec<&str> = same.map(|i| BINDINGS[i].name).collect();
            if names.is_empty() { String::new() } else { format!("Also {}", names.join(", ")) }
        };
        let control = quadra_design::hstack()
            .role(Role::Group)
            .label(b.name.to_string())
            .child(text(clash).color(token::WARN))
            .child(key_input(mine).label(format!("{} keys", b.name)));
        group.row(b.name, "", control).resets(mine, default.to_string())
    })
}
