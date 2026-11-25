//! The settings page (specs/oneterm, "Settings"): it takes the window's body while it is open, in
//! the kit's layout of cards: the pages with their icons and a search in the column, each
//! page's title and what it is about, its groups. Each control writes a setting, which saves it.

use quadra::{Align, Signal, column, div, effect, row, signal, text};
use quadra_design::{
    SettingsGroup, SettingsPage, Variant, button, icon, icons, input, number_input, select, settings, switch,
    token,
};

use crate::model::Model;

mod complete;
mod files;
pub(crate) mod keys;
mod look;
use crate::prefs::{self, Prefs};

/// Each page: its name, its icon and what it is about.
const PAGES_ABOUT: [(&str, &str, &str); 11] = [
    ("Appearance", icons::PALETTE, "The theme, and the terminal's text and colours."),
    (
        "Terminal",
        icons::SQUARE_TERMINAL,
        "The cursor, the history and the shell. Changes apply to new spaces.",
    ),
    ("Tabs & spaces", icons::LAYERS, "Where the tabs are and how they are named."),
    (
        "SSH & hosts",
        icons::APP_WINDOW,
        "Saved hosts, recent connections, key files, startup commands and keepalive.",
    ),
    ("SFTP", icons::FOLDER, "Downloads, and how remote files are edited."),
    (
        "Notifications & signals",
        icons::BELL,
        "How OneTerm shows what programs report: state, progress, bells and notifications.",
    ),
    ("Status bar", icons::GAUGE, "What the status bar shows."),
    ("Security & lock", icons::LOCK, "The master password and the lock when idle."),
    ("Keybindings", icons::COMMAND, "Keyboard shortcuts."),
    ("Advanced", icons::CODE, "Where your settings are kept, and how OneTerm reaches the network."),
    ("About", icons::INFO, "Which OneTerm this is."),
];

/// Opens the settings in a window of their own, or brings it to the front: its own title bar
/// and the page. It closes when OneTerm locks, and with the main window.
pub(crate) fn open_window(windows: &quadra::Windows, model: &Model) {
    let model = model.clone();
    let options =
        quadra::WindowOptions::new("OneTerm Settings").size(980.0, 700.0).custom_frame().remember("settings");
    windows.open(options, move || {
        let (locked, me) = (model.lock().locked, quadra::window());
        effect(move || {
            if locked.get() {
                me.close_now();
            }
        });
        let mark = quadra_design::hstack()
            .padding_left(token::SPACE_LG)
            .child(icon(icons::SETTINGS).size(16.0).color(token::ACCENT))
            .child(div().font_weight(600).child(text("OneTerm Settings")));
        // Using the settings is using OneTerm: idle starts again.
        crate::locked::watches(quadra_design::root(), model.lock())
            .child(quadra_design::titlebar("").control(mark).control(div().grow(1.0)))
            .child(page(&model))
    });
}

/// A row that goes back to the default of its setting: `default` is the same setting where
/// nothing was written (`Prefs::defaults`).
pub(crate) trait Back {
    fn back<T: PartialEq + Clone + 'static>(self, value: Signal<T>, default: Signal<T>) -> Self;
}

impl Back for SettingsGroup {
    fn back<T: PartialEq + Clone + 'static>(self, value: Signal<T>, default: Signal<T>) -> Self {
        self.resets(value, default.get_untracked())
    }
}

pub(crate) fn page(model: &Model) -> quadra::Div {
    // The title bar says the page and has Done.
    let (p, current, query) = (model.prefs(), model.ui().page, signal(String::new()));
    let mut view = settings(current).search(query).cards();
    for ((name, body, about), groups) in PAGES_ABOUT.into_iter().zip(groups(p, Prefs::defaults(), model)) {
        let page = SettingsPage::new(name).icon(body).about(about);
        view = view.page(groups.into_iter().fold(page, SettingsPage::group));
    }
    column().grow(1.0).min_height(0.0).child(view)
}

fn groups(p: Prefs, d: Prefs, model: &Model) -> Vec<Vec<SettingsGroup>> {
    let tabs = SettingsGroup::new("Tabs & spaces")
        .row(
            "Vertical tabs",
            "Show tabs in a rail on the left. Off, they are a strip along the top.",
            switch(p.vertical),
        )
        .back(p.vertical, d.vertical)
        .row(
            "Rail width",
            "Width of the tab rail, 140 to 400 px.",
            number_input(p.rail_width).stepper().range(140.0, 400.0).step(10.0).label("Rail width"),
        )
        .back(p.rail_width, d.rail_width)
        .row("Fold the rail", "Show only the tabs' icons.", switch(p.rail_folded))
        .back(p.rail_folded, d.rail_folded)
        .row(
            "Tab title",
            "The name a tab has until you rename it.",
            select(prefs::TITLES, p.tab_title).width(240.0),
        )
        .back(p.tab_title, d.tab_title);
    let signals = SettingsGroup::new("Signals")
        .row(
            "Program states",
            "Where a program's state (working, blocked, done) shows. Sent with OSC 7501.",
            select(prefs::STATUS_SHOWN, p.status_shown).width(240.0),
        )
        .back(p.status_shown, d.status_shown)
        .row(
            "Progress on the tab",
            "A line of progress under the tab. Sent with OSC 9;4.",
            switch(p.progress_tab),
        )
        .back(p.progress_tab, d.progress_tab)
        .row(
            "Progress in the status bar",
            "The same progress as a meter in the status bar.",
            switch(p.progress_bar),
        )
        .back(p.progress_bar, d.progress_bar)
        .row(
            "Bell",
            "What a bell does in a space you are not looking at.",
            select(prefs::BELLS, p.bell).width(240.0),
        )
        .back(p.bell, d.bell)
        .row(
            "Notifications",
            "How a program's notification shows. Sent with OSC 9.",
            select(prefs::NOTICES, p.notices).width(240.0),
        )
        .back(p.notices, d.notices);
    let status = SettingsGroup::new("Status bar")
        .row("Git", "The branch and how many files changed.", switch(p.show_git))
        .back(p.show_git, d.show_git)
        .row("CPU and memory", "How much OneTerm itself uses.", switch(p.show_usage))
        .back(p.show_usage, d.show_usage)
        .row("Clock", "The local time.", switch(p.show_clock))
        .back(p.show_clock, d.show_clock)
        .row("Frames a second", "How many frames the window drew in the last second.", switch(p.show_fps))
        .back(p.show_fps, d.show_fps);
    let lock = crate::locked::security(model.lock(), p.lock_after, d.lock_after);
    let keys = keys::group();
    let advanced = files::files();
    vec![
        look::appearance(p, d),
        look::terminal(p, d).into_iter().chain([complete::completion(model), files::logging(p, d)]).collect(),
        vec![tabs],
        vec![
            hosts(p, model),
            crate::sshkeys::group(model),
            crate::startup::group(model),
            SettingsGroup::new("Connections")
                .row("Keepalive", "Ping an idle server so the connection is not dropped.", switch(p.keepalive_on))
                .back(p.keepalive_on, d.keepalive_on)
                .row(
                    "Every",
                    "Seconds between pings, 5 to 3600.",
                    number_input(p.keepalive_secs).stepper().range(5.0, 3600.0).step(5.0).label("Every"),
                )
                .back(p.keepalive_secs, d.keepalive_secs)
                .row(
                    "Unanswered",
                    "Pings that may go unanswered before the connection is closed.",
                    number_input(p.keepalive_max).stepper().range(1.0, 20.0).label("Unanswered"),
                )
                .back(p.keepalive_max, d.keepalive_max),
        ],
        vec![
            SettingsGroup::new("Downloads")
                .row(
                    "Downloads folder",
                    "Where downloaded files are saved. Leave empty for your Downloads folder.",
                    input(p.downloads).placeholder("Downloads").label("Downloads folder"),
                )
                .back(p.downloads, d.downloads),
            SettingsGroup::new("Editing")
                .row(
                    "Editor",
                    "What opens a remote file when you choose Edit.",
                    select(crate::sftp::EDITORS, p.editor).width(240.0),
                )
                .back(p.editor, d.editor)
                .row(
                    "Program",
                    "The program used when Editor is Custom program.",
                    input(p.editor_program).placeholder("notepad.exe").label("Program"),
                )
                .back(p.editor_program, d.editor_program)
                .row(
                    "Arguments",
                    "Placed before the file's name, separated by spaces.",
                    input(p.editor_args).label("Arguments"),
                )
                .back(p.editor_args, d.editor_args)
                .row("Wrap long lines", "In OneTerm's editor, instead of scrolling sideways.", switch(p.editor_wrap))
                .back(p.editor_wrap, d.editor_wrap)
                .row(
                    "Ask before editing",
                    "Ask first when a file is larger than this many KB. 0 never asks.",
                    number_input(p.edit_limit_kb).stepper().range(0.0, 1_048_576.0).step(256.0).label("Ask before editing"),
                )
                .back(p.edit_limit_kb, d.edit_limit_kb),
        ],
        vec![signals],
        vec![status],
        vec![
            lock,
            SettingsGroup::new("Clipboard")
                .row(
                    "Programs read the clipboard",
                    "Let local and remote programs read your clipboard (OSC 52). They can always write to it.",
                    switch(p.clipboard_read),
                )
                .back(p.clipboard_read, d.clipboard_read),
        ],
        vec![keys],
        vec![advanced, files::network(p, d)],
        vec![
            SettingsGroup::new("OneTerm")
                .row("Version", "", text(env!("CARGO_PKG_VERSION")))
                .row("Terminal engine", "", text(concat!("oneterm-vt ", env!("ONETERM_VT_VERSION"))))
                .row("Built with", "", text("Quadra, a native UI framework on Direct3D 11 and DirectWrite"))
                .row("License", "", text(env!("CARGO_PKG_LICENSE"))),
            crate::updates::group(p, d, model),
        ],
    ]
}

/// The saved hosts and the recent targets, with what adds one and what forgets them.
fn hosts(p: Prefs, model: &Model) -> SettingsGroup {
    let (hosts, recent, adds, closes) = (p.hosts, p.recent, model.ui().adding, model.ui().settings);
    let count = move |n: usize, one: &str| format!("{n} {one}{}", if n == 1 { "" } else { "s" });
    let saved = row()
        .align_items(Align::Center)
        .gap(token::SPACE_LG)
        .child(text(move || count(hosts.with(Vec::len), "host")))
        .child(button("Add SSH host\u{2026}").variant(Variant::Secondary).on_click(move || {
            closes.set(false);
            adds.set(true);
        }));
    let lately = row()
        .align_items(Align::Center)
        .gap(token::SPACE_LG)
        .child(text(move || count(recent.with(Vec::len), "target")))
        .child(button("Forget").variant(Variant::Secondary).on_click(move || recent.set(Vec::new())));
    SettingsGroup::new("Hosts")
        .row("Saved hosts", "Edit or remove a host from its menu in the rail.", saved)
        .row("Recent", "Recent connections, listed in the New terminal menu.", lately)
}
