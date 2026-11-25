//! OneTerm (specs/oneterm): the window, its title bar, the rail of tabs, the workspace of spaces,
//! the status bar. Where a session's program comes from is the caller's: a local shell in the
//! app ([`local_shell`]), `quadra_term::echo` in tests.

mod asking;
mod channels;
mod chrome;
mod complete;
mod connect;
mod dock;
mod editing;
mod expanded;
mod find;
mod git;
mod host_connect;
mod hostlist;
mod hosts;
mod import;
mod links;
mod local;
mod lock;
mod locked;
mod logs;
mod marks;
mod model;
mod notify;
mod page;
mod panes;
mod prefs;
mod rail;
mod remote;
mod sftp;
mod shell_icons;
mod shells;
mod signals;
mod spaces;
mod split;
mod sshkeys;
mod startup;
mod statusbar;
mod strip;
mod tab;
mod themes;
mod ui;
mod updates;
mod usage;
mod vault;

use std::rc::Rc;

use quadra::{AnyView, Keymap, Signal, View, Window, column, div, dynamic, row, signal, text, use_context};
use quadra_term::Session;

pub use hosts::Host;
pub use import::adopt;
pub use local::{integrate, local_shell};
pub use model::{Command, Model, Side, Space, Tab};
pub use notify::abilities;
pub use prefs::{KIT_THEME, Prefs, theme};
pub use remote::{remote_shell, remote_shell_with};
pub use shells::{Launch, Remote, Shell};
pub use signals::{percent, state_word};
pub use updates::{Update, Updates};
pub use usage::Clock;

/// Makes the session of a new space: it is called in the scope the space lives in.
pub type NewSession = Rc<dyn Fn(Launch) -> Session>;

/// The app's sessions: a host's shell when the launch names one, else a local one.
pub fn start(launch: Launch) -> Session {
    if launch.ssh.is_some() { remote_shell(launch) } else { local_shell(launch) }
}

/// The app's keys as they are by default: the kit's, the terminal's, and the tabs' and
/// spaces'. The settings change them while the app runs (the Keybindings page).
pub fn keymap() -> Keymap {
    page::keys::keymap(|_| None)
}

/// What the window shows.
pub struct OneTerm {
    pub model: Model,
    /// The time of the status bar, kept by a timer.
    pub clock: Signal<String>,
    /// Where that time comes from: the system's clock at the platform's offset, unless a test
    /// sets another before `view`.
    pub now: Clock,
}

impl OneTerm {
    /// The window's model, with one tab of one space: made where the window's view is built.
    pub fn new(new_session: NewSession) -> OneTerm {
        let now: Clock = Rc::new(quadra::time::local_now);
        OneTerm { model: Model::new(new_session, Prefs::load()), clock: signal(String::new()), now }
    }

    /// Says the last run crashed (specs/oneterm, "Window"): a card whose Show report shows the
    /// report in the file manager.
    pub fn crashed(&self, report: &std::path::Path) {
        let opener = use_context::<Rc<dyn quadra_shell::Opener>>();
        let (shown, name) = (report.to_path_buf(), report.file_name().unwrap_or_default().to_string_lossy());
        let notice = quadra_design::Notice::new("OneTerm closed unexpectedly")
            .body(format!("A report was written: {name}"))
            .tone(quadra_design::Tone::Warning)
            .action("Show report", move || {
                if let Some(opener) = &opener {
                    opener.reveal(&shown);
                }
            });
        self.model.notify(notice);
    }

    pub fn view(&self) -> impl View + use<> {
        let (model, window) = (self.model.clone(), use_context::<Window>());
        page::keys::follow(); // the keys the settings say
        // The size of the UI's text, the terminals' with it, as the settings say.
        if let Some(size) = use_context::<quadra::Signal<quadra::TextSize>>() {
            let percent = model.prefs().ui_text;
            quadra::effect(move || {
                let text = (percent.get().clamp(75.0, 200.0) / 100.0) as f32;
                if quadra::untrack(|| size.get()).text != text {
                    size.update(|it| it.text = text);
                }
            });
        }
        // The tabs in the rail at the left, or in a strip above the spaces.
        let (vertical, shown) = (model.vertical(), model.clone());
        let body = dynamic(move || match vertical.get() {
            true => AnyView::new(
                row()
                    .grow(1.0)
                    .child(rail::rail(&shown))
                    .child(spaces::workspace(&shown))
                    .child(dock::dock(&shown)),
            ),
            false => AnyView::new(
                column().grow(1.0).child(strip::strip(&shown)).child(
                    row()
                        .grow(1.0)
                        .child(hosts_panel(&shown))
                        .child(spaces::workspace(&shown))
                        .child(dock::dock(&shown)),
                ),
            ),
        });
        // Locked, the workspace shows blurred behind the lock's card and is out of reach: no
        // reader, press or key gets to it. The settings are not displayed.
        let (open, lock) = (model.ui().settings, model.lock());
        let shows = move |on: bool| move || !lock.locked.get() && open.get() == on;
        let behind = column().grow(1.0).display(locked::shown(move || !open.get())).inert(lock.locked);
        let body = column()
            .grow(1.0)
            .min_height(0.0)
            .child(body.container(behind))
            // Mounted only while open: its fields are built when they are seen.
            .child(quadra::show(shows(true), {
                let model = model.clone();
                move || page::page(&model)
            }))
            .child(locked::Card(model.clone()));
        // The cards stand at the bottom right, 12 px over the status bar, over the dock.
        let cards = div()
            .display(locked::shown(move || !lock.locked.get()))
            .child(self.model.notices().above(28.0).host());
        let windows = use_context::<quadra::Windows>();
        let closing = (self.model.clone(), window.clone());
        updates::closes(self.model.ui().updates, window.clone());
        locked::watches(quadra_design::root(), lock)
            .on_action::<Command>(move |&command| {
                if !run(&model, command, windows.as_ref())
                    && let Some(window) = &window
                {
                    window.close(); // Quit: as the close button does
                }
            })
            .on_close({
                let (model, window) = closing;
                move || {
                    let window = window.clone();
                    !model.asks_all(Rc::new(move || window.iter().for_each(Window::close)))
                }
            })
            .child(chrome::title_bar(&self.model))
            .child(body)
            .child(statusbar::status_bar(&self.model, self.clock, self.now.clone()))
            .child(cards)
            .child(connect::dialog(&self.model))
            .child(host_connect::dialog(&self.model))
            .child(asking::dialogs(&self.model))
            .child(hostlist::add_dialog(&self.model))
            .child(vault::dialog(&self.model))
            .child(rail::rename_dialog(&self.model))
    }
}

/// The hosts of the rail as a panel at the left, while the tabs are in a strip and the title
/// bar's Hosts says so.
fn hosts_panel(model: &Model) -> impl View + use<> {
    let on = model.ui().hosts;
    column()
        .width(248.0)
        .shrink(0.0)
        .bg(quadra_design::token::PANEL)
        .border_right(1.0)
        .border_color(quadra_design::token::LINE)
        .display(locked::shown(move || on.get()))
        .child(hostlist::hosts(model))
}

/// The workspace with no tab: the window stays, and says how to open one.
pub(crate) fn no_tab() -> impl View + use<> {
    let hint = div().text_color(quadra_design::token::TEXT_3).child(text("Ctrl+T opens a terminal"));
    row().grow(1.0).align_items(quadra::Align::Center).justify(quadra::Justify::Center).child(hint)
}

/// Does a command; false for Quit. Locked, only locking does anything.
fn run(model: &Model, command: Command, windows: Option<&quadra::Windows>) -> bool {
    model.lock().active();
    if model.lock().locked.get_untracked() {
        return true;
    }
    match command {
        Command::NewTab => model.new_tab(),
        Command::Split => model.split(),
        Command::SplitTo(side) => model.split_to(side),
        Command::CloseSpace => _ = model.close_focused(), // the last: the window stays
        Command::FocusNext(forward) => model.focus_next(forward),
        Command::Lock => model.lock().lock_now(),
        Command::QuickConnect => model.ui().connecting.set(true),
        Command::Settings => match windows {
            Some(windows) => page::open_window(windows, model),
            None => model.ui().settings.update(|on| *on = !*on),
        },
        Command::Dock => model.ui().dock.update(|on| *on = !*on),
        Command::SelectAll | Command::Clear => {
            let space = quadra::untrack(|| model.active_tab().and_then(|tab| tab.focused()));
            match (space, command) {
                (Some(space), Command::SelectAll) => space.session.select_all(),
                (Some(space), _) => space.session.clear_history(),
                (None, _) => {}
            }
        }
        Command::Duplicate => model.duplicate(model.active().get_untracked()),
        Command::Zoom(step) => {
            model.prefs().look.font_size.update(|size| {
                *size = if step == 0 { 15.0 } else { (*size + f64::from(step)).clamp(6.0, 48.0) }
            })
        }
        Command::Quit => return false,
    }
    true
}
