//! Command completion in a space (specs/oneterm, "Completion"): the engine of specs/completion, one for
//! the app, and an overlay under the caret that follows the terminal's command line and takes
//! its keys before the shell while it is open.

mod overlay;
mod prefs;

use std::cell::RefCell;
use std::rc::Rc;

use oneterm_completion::{Config, Engine, Family, Suggestion};
use quadra::{
    Along, Key, Length, Mods, NodeRef, Place, Side, Signal, View, batch, column, div, effect, overlay,
    signal, untrack,
};
use quadra_term::{CommandLine, Session, Terminal, terminal};

pub use prefs::{CompletePrefs, FAMILIES};

use crate::NewSession;
use crate::model::{Model, Space};

/// The app's completion: its settings and its engine, made when first asked. A copy is the
/// same one.
#[derive(Clone, Copy)]
pub struct Completion {
    pub prefs: CompletePrefs,
    engine: Signal<Rc<RefCell<Option<Engine>>>>,
}

impl Completion {
    /// The settings from the app's; the engine is made when first asked.
    pub(crate) fn new() -> Completion {
        Completion { prefs: CompletePrefs::load(), engine: signal(Rc::new(RefCell::new(None))) }
    }

    /// Runs `f` with the engine, on the settings as they are now.
    fn with<R>(&self, f: impl FnOnce(&mut Engine) -> R) -> R {
        let (engine, config) = (self.engine.get_untracked(), untrack(|| self.prefs.config()));
        let mut engine = engine.borrow_mut();
        let engine = match engine.as_mut() {
            Some(engine) if *engine.config() != config => {
                engine.set_config(config);
                engine
            }
            Some(engine) => engine,
            None => engine.insert(Engine::new(config)),
        };
        f(engine)
    }
}

/// What a session was started as, for its family: a local shell's program, or `ssh`.
struct Started(String);

/// How a program's lines are read by its file's name: Command Prompt's, PowerShell's, else a
/// Unix shell's (SSH reaches one, WSL and Git Bash are).
fn profile(started: &str) -> quadra_term::ShellProfile {
    let name = std::path::Path::new(started).file_stem().map(|it| it.to_string_lossy().to_lowercase());
    match name.as_deref() {
        Some("cmd") => quadra_term::ShellProfile::Cmd,
        Some("powershell" | "pwsh") => quadra_term::ShellProfile::PowerShell,
        _ => quadra_term::ShellProfile::Unix,
    }
}

/// The file name of the program a local session was started as: `cmd.exe`, `pwsh.exe`.
pub(crate) fn program(session: &quadra_term::Session) -> Option<String> {
    let started = session.kept::<Started>()?;
    let name = std::path::Path::new(&started.0).file_name()?;
    Some(name.to_string_lossy().into_owned())
}

/// The process of a local session's shell, kept on the session once it started.
pub(crate) struct ShellPid(pub(crate) u32);

/// What a local session's shell runs, read from the OS now: the program's file name and the
/// command line of the newest process it started (`pwsh.exe · npm run dev`), or with none its
/// full path. Without its process (a test's session), the program it was started as.
pub(crate) fn running(session: &quadra_term::Session) -> Option<String> {
    let found = session.kept::<ShellPid>().and_then(|pid| read_process(pid.0));
    let Some(found) = found else { return session.kept::<Started>().map(|it| it.0.clone()) };
    let name = std::path::Path::new(&found.path).file_name().map(|it| it.to_string_lossy().into_owned());
    match (found.running, name) {
        (Some(command), Some(name)) => Some(format!("{name} \u{B7} {command}")),
        _ => Some(found.path),
    }
}

#[cfg(windows)]
fn read_process(pid: u32) -> Option<quadra_shell::ShellProcess> {
    quadra_shell::shell_process(pid)
}

#[cfg(not(windows))]
fn read_process(_: u32) -> Option<quadra_shell::ShellProcess> {
    None
}

/// Makes sessions as `make` does, each kept with what it was started as and read by its shell.
pub(crate) fn tagged(make: NewSession) -> NewSession {
    Rc::new(move |launch: crate::Launch| {
        let started = match (&launch.ssh, &launch.shell) {
            (Some(_), _) => "ssh".to_string(),
            (None, Some(shell)) => shell.program.display().to_string(),
            (None, None) => "cmd.exe".to_string(), // the OS's shell
        };
        let session = make(launch);
        session.set_shell_profile(profile(&started));
        session.keep(Rc::new(Started(started)));
        session
    })
}

/// What a space's overlay shows: the suggestions for a line, the row chosen, and the line
/// Escape closed it at.
#[derive(Clone, Copy)]
struct Shown {
    list: Signal<Vec<Suggestion>>,
    chosen: Signal<Option<usize>>,
    line: Signal<String>,
    closed: Signal<Option<String>>,
}

/// A space's terminal, as `inner` wraps it, with the overlay of suggestions under its caret.
pub(crate) fn around<V: View + 'static>(
    model: &Model,
    space: &Space,
    inner: impl FnOnce(Terminal) -> V,
) -> quadra::Div {
    let (c, s) = (model.ui().complete, space.session.clone());
    let started = s.kept::<Started>().map_or_else(|| "cmd.exe".to_string(), |it| it.0.clone());
    let family = move |config: &Config| config.family(&started);
    let line = s.command_line();
    let shown = Shown {
        list: signal(Vec::new()),
        chosen: signal(None),
        line: signal(String::new()),
        closed: signal(None),
    };
    let asks = family.clone();
    effect(move || {
        let now = line.get();
        let config = c.prefs.config();
        if untrack(|| shown.closed.get()).is_some_and(|at| at != now.text) {
            shown.closed.set(None);
        }
        let quiet = !config.enabled
            || (config.disable_in_alt_screen && now.alt_screen)
            || (config.require_prompt_region && !now.at_prompt)
            || now.caret.is_none()
            || untrack(|| shown.closed.get()).is_some();
        let list = if quiet {
            Vec::new()
        } else {
            c.with(|engine| engine.suggest(&now.text, now.text.len(), asks(&config)))
        };
        batch(|| {
            shown.line.set(now.text);
            shown.list.set(list);
            shown.chosen.set(None);
        });
    });
    let typed = s.clone();
    let view = terminal(&s).on_key_first(move |key, mods| {
        took(c, shown, &typed, family(&untrack(|| c.prefs.config())), key, mods)
    });
    let anchor = NodeRef::new();
    let place = Place::Beside(anchor.clone(), Side::Below, Along::Start, 2.0);
    let most = move || untrack(|| c.prefs.config()).max_visible_items;
    // Nothing over the lock's card, the settings page or the expanded SFTP view.
    let (ui, locked) = (model.ui(), model.lock().locked);
    let covered = move || locked.get() || ui.settings.get() || ui.expanded.get();
    let open =
        move || shown.list.with(|list| !list.is_empty()) && line.with(|it| it.caret.is_some()) && !covered();
    column()
        .grow(1.0)
        .min_width(0.0)
        .min_height(0.0)
        .child(inner(view))
        .child(marker(line, shown).node_ref(&anchor))
        // Mounted while it has rows: keys that only change the rows build nothing else.
        .child(quadra::show(open, move || overlay(place.clone(), overlay::list(shown, most))))
}

/// A box over the caret, moved back to where the token completed starts: what the overlay
/// stands under.
fn marker(line: Signal<CommandLine>, shown: Shown) -> quadra::Div {
    let caret = move || line.with(|it| it.caret.unwrap_or_default());
    let back = move || {
        let typed = shown.line.get();
        shown.list.with(|list| {
            list.first().and_then(|it| typed.get(it.range.start..)).map_or(0, |it| it.chars().count())
        })
    };
    div()
        .absolute()
        .left(move || Length::Px((caret().x - back() as f32 * caret().width).max(0.0)))
        .top(move || Length::Px(caret().y))
        .width(1.0)
        .height(move || Length::Px(caret().height))
}

/// A key the terminal would send: taken by the open overlay (true), or let through. Enter that
/// goes to the shell records the line first.
fn took(c: Completion, shown: Shown, s: &Session, family: Family, key: Key, mods: Mods) -> bool {
    let (count, chosen) = (shown.list.with_untracked(Vec::len), shown.chosen.get_untracked());
    let close = || batch(|| (shown.list.set(Vec::new()), shown.chosen.set(None)));
    let (plain, ctrl) = (mods == Mods::NONE, mods == Mods::CTRL);
    let to = |at: usize| shown.chosen.set(Some(at));
    let tab = untrack(|| c.prefs.accept_tab.get());
    match (key, chosen) {
        (Key::Enter, Some(at)) if plain && count > 0 => accept(shown, s, at),
        (Key::Enter, _) => {
            record(c, s.command_line().get_untracked(), family);
            close();
            return false;
        }
        _ if count == 0 => return false,
        (Key::Escape, _) if plain => {
            shown.closed.set(Some(shown.line.get_untracked()));
            close();
        }
        (Key::Tab, None) if plain && tab => to(0),
        (Key::Tab, Some(at)) if plain && tab => accept(shown, s, at),
        // Without Tab, Down is what starts choosing.
        (Key::Down, None) if plain && !tab => to(0),
        (Key::Down, Some(at)) if plain => to((at + 1).min(count - 1)),
        (Key::N, Some(at)) if ctrl => to((at + 1).min(count - 1)),
        (Key::Up, Some(at)) if plain => to(at.saturating_sub(1)),
        (Key::P, Some(at)) if ctrl => to(at.saturating_sub(1)),
        _ => return false,
    }
    true
}

/// Types what the chosen suggestion makes of the line: a DEL for each character it erases,
/// then its text. One that does not extend what is typed only closes the overlay.
fn accept(shown: Shown, s: &Session, at: usize) {
    let item = shown.list.with_untracked(|list| list.get(at).cloned());
    batch(|| (shown.list.set(Vec::new()), shown.chosen.set(None)));
    let Some(item) = item.filter(|it| it.prefix) else { return };
    let typed = shown.line.get_untracked();
    let (erase, text) = item.edit(&typed);
    let mut bytes = vec![0x7f; erase];
    bytes.extend_from_slice(text.as_bytes());
    s.send(&quadra_term::Input::Keys(&bytes)); // typed for the user: its channel gets it
}

/// Keeps a line run at a prompt (the engine redacts it), never one of the alternate screen.
fn record(c: Completion, line: CommandLine, family: Family) {
    let at_prompt = line.at_prompt || !untrack(|| c.prefs.only_at_prompt.get());
    let on = untrack(|| c.prefs.enabled.get());
    if on && !line.alt_screen && at_prompt && !line.text.trim().is_empty() {
        c.with(|engine| engine.record(&line.text, family));
    }
}

impl Model {
    /// The history of completion for a family, the lines in the order they were first run.
    pub fn completion_history(&self, family: Family) -> Vec<String> {
        self.ui().complete.with(|engine| engine.history(family).map(str::to_string).collect())
    }
}
