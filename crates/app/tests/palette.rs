//! The command palette (specs/oneterm, "Window"): Ctrl+Shift+P shows it; what is typed finds a
//! command, a tab, a saved host or a look, and Enter runs it.

use std::cell::RefCell;
use std::rc::Rc;

use oneterm::{Model, NewSession, OneTerm, keymap};
use quadra::{Key, Mods};
use quadra_term::echo;
use quadra_testing::Harness;

struct App {
    h: Harness,
    model: Model,
}

fn app() -> App {
    let mut h = Harness::new();
    h.size = quadra::Size::new(1200.0, 700.0);
    h.tree.set_keymap(Rc::new(keymap()));
    let sessions: NewSession = Rc::new(|_| echo());
    let built = Rc::new(RefCell::new(None));
    let tell = built.clone();
    h.mount(move || {
        let app = OneTerm::new(sessions);
        *tell.borrow_mut() = Some(app.model.clone());
        app.view()
    });
    h.settle();
    let model = built.borrow_mut().take().expect("the window made its model");
    App { h, model }
}

impl App {
    /// Opens the palette, types and takes the first row found.
    fn run(&mut self, typed: &str) {
        self.h.key(Key::P, Mods::CTRL | Mods::SHIFT);
        self.h.settle();
        self.h.type_text(typed);
        self.h.settle();
        self.h.key(Key::Enter, Mods::NONE);
        self.h.settle();
    }
}

#[test]
fn the_palette_runs_commands_goes_to_tabs_and_changes_the_look() {
    let mut a = app();
    a.h.key(Key::P, Mods::CTRL | Mods::SHIFT);
    a.h.settle();
    assert!(a.h.has_text("New tab") && a.h.has_text("Theme: Dark"), "{}", a.h.semantics());
    a.h.key(Key::Escape, Mods::NONE);
    a.h.settle();

    // A command of the keys.
    assert_eq!(a.model.tabs().get_untracked().len(), 1);
    a.run("new tab");
    assert_eq!(a.model.tabs().get_untracked().len(), 2);
    assert!(!a.h.has_text("Theme: Dark"), "it closes once a row is taken");
    // A tab, by its title; the look.
    let first = a.model.tabs().get_untracked()[0].id;
    assert_ne!(a.model.active().get_untracked(), first);
    a.model.tabs().get_untracked()[0].name.set(Some("build box".into()));
    a.run("build box");
    assert_eq!(a.model.active().get_untracked(), first);
    a.run("theme: light");
    assert_eq!(a.model.prefs().theme.get_untracked(), 2);
    a.run("tabs in a strip");
    assert!(!a.model.vertical().get_untracked());
}
