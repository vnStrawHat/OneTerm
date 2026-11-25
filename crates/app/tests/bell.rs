//! A bell's sound and flash (specs/oneterm, "Settings", Signals): each bell plays the system's
//! default sound, or tints its space for a moment, as the setting says; counted either way.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use oneterm::{Model, NewSession, OneTerm, keymap};
use quadra_shell::FakeShell;
use quadra_term::{Session, echo};
use quadra_testing::Harness;

struct App {
    h: Harness,
    made: Rc<RefCell<Vec<Session>>>,
    model: Model,
    shell: Rc<FakeShell>,
}

fn app() -> App {
    let mut h = Harness::new();
    h.size = quadra::Size::new(1200.0, 700.0);
    h.tree.set_keymap(Rc::new(keymap()));
    let (made, built) = (Rc::new(RefCell::new(Vec::new())), Rc::new(RefCell::new(None)));
    let (kept, tell) = (made.clone(), built.clone());
    let sessions: NewSession = Rc::new(move |_| {
        let s = echo();
        kept.borrow_mut().push(s.clone());
        s
    });
    let shell = FakeShell::new();
    let given = shell.clone();
    h.mount(move || {
        given.provide();
        let term = OneTerm::new(sessions);
        *tell.borrow_mut() = Some(term.model.clone());
        term.view()
    });
    h.settle();
    let model = built.borrow_mut().take().unwrap();
    App { h, made, model, shell }
}

impl App {
    fn ring(&mut self) {
        let s = self.made.borrow()[0].clone();
        s.feeder().feed(b"\x07", Instant::now());
        self.h.settle();
    }

    fn flashes(&self) -> bool {
        self.model.tabs().get_untracked()[0].spaces.get_untracked()[0].flashing()
    }
}

#[test]
fn a_bell_sounds_or_flashes_as_the_setting_says_and_nothing_by_default() {
    let mut a = app();
    a.ring();
    assert!(a.shell.asked().is_empty() && !a.flashes(), "a dot and a count: no sound, no flash");
    a.model.prefs().bell.set(1);
    a.ring();
    a.ring();
    assert_eq!(
        a.shell.asked(),
        ["sound: Default", "sound: Default"],
        "a sound a bell, in the looked-at space too"
    );
    a.model.prefs().bell.set(2);
    a.ring();
    assert!(a.flashes(), "the tint over the space");
    assert_eq!(a.shell.asked().len(), 2, "a flash plays nothing");
    a.h.advance(Duration::from_millis(150));
    a.h.settle();
    assert!(!a.flashes(), "the flash is over");
    a.model.prefs().bell.set(3);
    a.ring();
    assert!(!a.flashes() && a.shell.asked().len() == 2, "ignored");
}
