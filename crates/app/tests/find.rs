//! Finding in a space (specs/oneterm, "Find"): Ctrl+F opens the bar with the focus in its query,
//! the count follows what is typed, Enter, Shift+Enter and F3 step, the bar survives its view
//! being built again, Escape closes it and gives the focus back to the terminal.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use oneterm::{NewSession, OneTerm};
use quadra::{Key, Mods, Role, Settings, provide};
use quadra_term::{Session, echo};
use quadra_testing::Harness;

struct App {
    h: Harness,
    sessions: Rc<RefCell<Vec<Session>>>,
}

fn app() -> App {
    let mut h = Harness::new();
    h.size = quadra::Size::new(1200.0, 700.0);
    let sessions = Rc::new(RefCell::new(Vec::new()));
    let kept = sessions.clone();
    let make: NewSession = Rc::new(move |_| {
        let s = echo();
        kept.borrow_mut().push(s.clone());
        s
    });
    h.mount(move || {
        provide(Settings::memory());
        OneTerm::new(make).view()
    });
    h.tree.set_keymap(Rc::new(oneterm::keymap()));
    let mut a = App { h, sessions };
    a.h.settle();
    a
}

impl App {
    fn key(&mut self, key: Key, mods: Mods) {
        self.h.key(key, mods);
        self.h.settle();
    }

    fn focused(&self, role: Role, name: &str) -> bool {
        let found = self.h.query(|n| n.semantics.role == role && n.semantics.name == name);
        found.len() == 1 && self.h.tree.focused() == Some(found[0])
    }
}

#[test]
fn ctrl_f_finds_steps_and_escape_gives_the_focus_back() {
    let mut a = app();
    // 100 lines, `foo` on lines 10, 50 and 90: the screen shows the last of them.
    let s = a.sessions.borrow()[0].clone();
    let lines: String =
        (0..100).map(|i| format!("line {i}{}\r\n", if i % 40 == 10 { " foo" } else { "" })).collect();
    s.feeder().feed(lines.as_bytes(), Instant::now());
    a.h.settle();
    assert!(a.focused(Role::TextInput, "Terminal"));

    a.key(Key::F, Mods::CTRL);
    assert!(a.focused(Role::TextInput, "Find"), "the focus is in the query");
    a.h.get_by_role(Role::TextInput, "Find").type_text("foo");
    a.h.settle();
    assert!(a.h.has_text("3 of 3"), "the one in view first");
    a.key(Key::Enter, Mods::NONE);
    assert!(a.h.has_text("1 of 3"), "Enter wraps to the first");
    assert!(s.screen_text().iter().any(|row| row == "line 10 foo"), "scrolled to it");
    a.key(Key::Enter, Mods::SHIFT);
    assert!(a.h.has_text("3 of 3"));
    a.key(Key::F3, Mods::NONE);
    a.key(Key::F3, Mods::NONE);
    assert!(a.h.has_text("2 of 3"));
    a.key(Key::F3, Mods::SHIFT);
    assert!(a.h.has_text("1 of 3"));
    a.h.get_by_role(Role::TextInput, "Find").type_text("d");
    a.h.settle();
    assert!(a.h.has_text("No results"));
    a.h.get_by_role(Role::Button, "Match case").click();
    a.h.settle();
    assert!(a.h.has_text("No results"));
    // Another tab shown and closed: the space's view is built again, its bar with it.
    a.key(Key::T, Mods::CTRL);
    assert!(a.h.query(|n| n.semantics.name == "Find").is_empty());
    a.key(Key::W, Mods::CTRL);
    assert!(a.h.has_text("No results"), "the bar is open again with its query");
    a.h.get_by_role(Role::TextInput, "Find").focus();
    a.h.settle();

    a.key(Key::Escape, Mods::NONE);
    assert!(a.h.query(|n| n.semantics.name == "Find").is_empty(), "the bar is closed");
    assert!(a.focused(Role::TextInput, "Terminal"), "the focus is back in the terminal");
    assert_eq!(s.found().get_untracked(), (0, 0), "nothing is looked for");
    // Opened again, the query is kept and found again.
    a.key(Key::F, Mods::CTRL);
    assert!(a.focused(Role::TextInput, "Find"));
    assert!(a.h.has_text("No results"));
}
