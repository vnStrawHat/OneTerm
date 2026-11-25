//! Command completion in a space (specs/oneterm, "Completion"): sessions that echo what they are
//! written as a shell would, prompts marked by OSC 133, the overlay that opens on typing, the
//! keys that choose and accept, what is written to the program, Escape, the gating.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use oneterm::{Model, NewSession, OneTerm, keymap};
use oneterm_completion::Family;
use quadra::{Key, Mods, Role, Settings, provide};
use quadra_term::vt::ResizePolicy;
use quadra_term::{Link, Session};
use quadra_testing::Harness;

/// A prompt marked as a shell with OSC 133 marks it.
const PROMPT: &[u8] = b"\x1b]133;A\x07me$ \x1b]133;B\x07";

struct App {
    h: Harness,
    s: Session,
    model: Model,
    wrote: Rc<RefCell<Vec<u8>>>,
    settings: Rc<Settings>,
}

/// The app with one space, its settings first set by `set`.
fn app_with(set: impl FnOnce(&Settings)) -> App {
    let mut h = Harness::new();
    h.size = quadra::Size::new(1200.0, 700.0);
    h.tree.set_keymap(Rc::new(keymap()));
    let settings = Settings::memory();
    // Off at first; these tests are of completion on.
    settings.value("completion.enabled", false).set(true);
    set(&settings);
    let (made, wrote) = (Rc::new(RefCell::new(None)), Rc::new(RefCell::new(Vec::new())));
    let (kept, into, given) = (made.clone(), wrote.clone(), settings.clone());
    let sessions: NewSession = Rc::new(move |_| {
        let s = Session::new(ResizePolicy::BottomAnchor);
        let (feeder, into) = (s.feeder(), into.clone());
        s.connect(Link::new(move |bytes: &[u8]| {
            into.borrow_mut().extend_from_slice(bytes);
            feeder.feed(&echoed(bytes), Instant::now());
        }));
        *kept.borrow_mut() = Some(s.clone());
        s
    });
    let model = Rc::new(RefCell::new(None));
    let shows = model.clone();
    h.mount(move || {
        provide(given.clone());
        let app = OneTerm::new(sessions);
        *shows.borrow_mut() = Some(app.model.clone());
        app.view()
    });
    h.settle();
    let (s, model) = (made.borrow_mut().take().unwrap(), model.borrow_mut().take().unwrap());
    App { h, s, model, wrote, settings }
}

/// What a line editor shows of what it is written: text, Enter as a new line, DEL as a
/// character erased; Tab and the keys of escape sequences show nothing.
fn echoed(bytes: &[u8]) -> Vec<u8> {
    let (mut shown, mut in_key) = (Vec::new(), false);
    for &b in bytes {
        match b {
            0x1b => in_key = true,
            _ if in_key => in_key = !(b.is_ascii_alphabetic() || b == b'~'),
            b'\r' => shown.extend_from_slice(b"\r\n"),
            b'\t' => {} // its own completion found nothing
            0x7f => shown.extend_from_slice(b"\x08 \x08"),
            _ => shown.push(b),
        }
    }
    shown
}

/// Only the history is asked, so what is suggested is what the test ran.
fn history_only(s: &Settings) {
    s.value("completion.sources.manual", true).set(false);
    s.value("completion.sources.external", true).set(false);
}

impl App {
    fn feed(&mut self, bytes: &[u8]) {
        self.s.feeder().feed(bytes, Instant::now());
        self.h.settle();
    }

    fn typed(&mut self, text: &str) {
        self.h.type_text(text);
        self.h.settle();
    }

    fn key(&mut self, key: Key, mods: Mods) {
        self.h.key(key, mods);
        self.h.settle();
    }

    fn taken(&mut self) -> Vec<u8> {
        std::mem::take(&mut *self.wrote.borrow_mut())
    }

    /// The rows of the overlay, each with whether it is chosen.
    fn rows(&self) -> Vec<(String, bool)> {
        let rows = RefCell::new(Vec::new());
        self.h.query(|n| {
            if n.semantics.role == Role::ListItem {
                rows.borrow_mut().push((n.semantics.name.clone(), n.semantics.selected));
            }
            false
        });
        rows.into_inner()
    }

    /// Runs a line at a new prompt: typed, then Enter.
    fn run(&mut self, line: &str) {
        self.feed(PROMPT);
        self.typed(line);
        self.key(Key::Enter, Mods::NONE);
    }
}

#[test]
fn typing_opens_the_overlay_and_tab_chooses_then_accepts() {
    let mut a = app_with(|_| {});
    a.feed(PROMPT);
    a.typed("di");
    let rows = a.rows();
    assert!(rows.iter().any(|(text, _)| text == "dir"), "{rows:?}");
    assert!(rows.len() <= 8 && rows.iter().all(|(_, chosen)| !chosen), "nothing is chosen at first");
    // Under the caret, at the start of the token.
    let caret = a.s.command_line().get_untracked().caret.expect("the terminal has the focus");
    let list = a.h.get_by_role(Role::List, "Suggestions").node();
    let term = a.h.get_by_role(Role::TextInput, "Terminal").node();
    let (list, term) = (a.h.tree.window_rect(list), a.h.tree.window_rect(term));
    assert!(list.y >= term.y + caret.bottom() && list.y <= term.y + caret.bottom() + 4.0, "{list:?}");
    assert_eq!(list.x, (term.x + caret.x - 2.0 * caret.width).round());

    a.taken();
    a.key(Key::Tab, Mods::NONE);
    let first = a.rows()[0].clone();
    assert!(first.1, "Tab chose the first row");
    assert!(a.taken().is_empty(), "Tab went to no program");
    a.key(Key::Tab, Mods::NONE);
    assert_eq!(a.taken(), first.0.as_bytes()[2..].to_vec(), "the rest of the chosen row is typed");
    assert_eq!(a.s.command_line().get_untracked().text, first.0);
    assert!(a.rows().iter().all(|(_, chosen)| !chosen), "the list is the new line's, nothing chosen");
}

#[test]
fn up_and_down_move_the_choice_and_enter_accepts_it() {
    let mut a = app_with(history_only);
    a.run("git status");
    a.run("git stash");
    a.feed(PROMPT);
    a.typed("git st");
    assert_eq!(a.rows().len(), 2);
    a.taken();
    a.key(Key::Down, Mods::NONE);
    assert_eq!(a.taken(), b"\x1b[B", "before a row is chosen, Down is the shell's");
    a.key(Key::Tab, Mods::NONE);
    a.key(Key::Down, Mods::NONE);
    let rows = a.rows();
    assert!(rows[1].1 && !rows[0].1, "{rows:?}");
    a.key(Key::Up, Mods::NONE);
    a.key(Key::Down, Mods::NONE);
    a.key(Key::Enter, Mods::NONE);
    let rest = rows[1].0.strip_prefix("git st").unwrap();
    assert_eq!(a.taken(), rest.as_bytes(), "Enter accepted, and ran nothing");
}

#[test]
fn without_accept_tab_tab_is_the_shells_and_down_chooses() {
    let mut a = app_with(|s| {
        history_only(s);
        s.value("completion.accept_tab", true).set(false);
    });
    a.run("git status");
    a.feed(PROMPT);
    a.typed("git s");
    a.taken();
    a.key(Key::Tab, Mods::NONE);
    assert_eq!(a.taken(), b"\t");
    a.key(Key::Down, Mods::NONE);
    assert!(a.taken().is_empty() && a.rows()[0].1);
    a.key(Key::Enter, Mods::NONE);
    assert_eq!(a.taken(), b"tatus");
}

#[test]
fn enter_with_nothing_chosen_runs_the_line_and_records_it_redacted() {
    let mut a = app_with(history_only);
    a.run("mysql --password hunter2 -h db");
    assert_eq!(a.taken().last(), Some(&b'\r'));
    let kept = a.model.completion_history(Family::Cmd);
    assert_eq!(kept.len(), 1);
    assert!(kept[0].starts_with("mysql --password") && !kept[0].contains("hunter2"), "{kept:?}");

    // A case that differs is erased and typed again, as Cmd matches without case.
    a.run("cd Project");
    a.feed(PROMPT);
    a.typed("cd p");
    a.taken();
    a.key(Key::Tab, Mods::NONE);
    a.key(Key::Tab, Mods::NONE);
    assert_eq!(a.taken(), b"\x7fProject");
}

#[test]
fn escape_closes_the_overlay_until_the_line_changes() {
    let mut a = app_with(history_only);
    a.run("git status");
    a.feed(PROMPT);
    a.typed("git s");
    assert_eq!(a.rows().len(), 1);
    a.taken();
    a.key(Key::Escape, Mods::NONE);
    assert!(a.rows().is_empty());
    assert!(a.taken().is_empty(), "Escape went to no program");
    a.key(Key::Escape, Mods::NONE);
    assert_eq!(a.taken(), b"\x1b", "closed, Escape is the shell's");
    a.typed("t");
    assert_eq!(a.rows().len(), 1, "a new line opens it again");
}

#[test]
fn nothing_shows_on_the_alternate_screen_or_away_from_a_prompt() {
    let mut a = app_with(history_only);
    a.run("git status");
    a.feed(PROMPT);
    a.feed(b"\x1b[?1049h");
    a.typed("git s");
    assert!(a.rows().is_empty(), "a full-screen program has the keys");
    a.feed(b"\x1b[?1049l\r\n\x1b]133;C\x07");
    a.typed("git s");
    assert!(a.rows().is_empty(), "the output of a command is no prompt");

    a.settings.value("completion.require_prompt_region", true).set(false);
    a.h.settle();
    assert_eq!(a.rows().len(), 1, "with the setting off, any line is completed");
    a.settings.value("completion.require_prompt_region", true).set(true);

    a.feed(b"\r\n");
    a.feed(PROMPT);
    a.settings.value("completion.enabled", true).set(false);
    a.typed("git s");
    assert!(a.rows().is_empty(), "completion is off");
    a.settings.value("completion.enabled", true).set(true);
    a.h.settle();
    assert_eq!(a.rows().len(), 1, "on again, the line is completed");
}

#[test]
fn the_settings_are_on_the_terminal_page() {
    let mut a = app_with(|_| {});
    a.h.get_by_role(Role::Button, "Settings").click();
    a.h.settle();
    a.h.get_by_role(Role::TreeItem, "Terminal").click();
    a.h.settle();
    assert!(a.h.has_text("Accept with Tab") && a.h.has_text("Redact secrets"));
}

#[test]
fn nothing_shows_over_the_settings_page() {
    let mut a = app_with(history_only);
    a.run("git status");
    a.feed(PROMPT);
    a.typed("git s");
    assert_eq!(a.rows().len(), 1);
    a.h.get_by_role(Role::Button, "Settings").click();
    a.h.settle();
    assert!(a.rows().is_empty());
}
