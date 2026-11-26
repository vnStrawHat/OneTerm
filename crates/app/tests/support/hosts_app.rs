//! The app the saved hosts' tests drive: sessions that are `echo()` (no host is reached),
//! the settings in memory, and what each launch was.
#![allow(dead_code)] // each test file uses some of it

use std::cell::RefCell;
use std::rc::Rc;

use oneterm::{Launch, NewSession, OneTerm, keymap};
use quadra::{Mods, Role, Settings, provide};
use quadra_term::echo;
use quadra_testing::Harness;

pub struct App {
    pub h: Harness,
    pub made: Rc<RefCell<Vec<Launch>>>,
    pub settings: Rc<Settings>,
}

pub fn app() -> App {
    let mut h = Harness::new();
    h.size = quadra::Size::new(1200.0, 800.0);
    h.tree.set_keymap(Rc::new(keymap()));
    let (made, settings) = (Rc::new(RefCell::new(Vec::new())), Settings::memory());
    let (kept, given) = (made.clone(), settings.clone());
    let sessions: NewSession = Rc::new(move |launch| {
        kept.borrow_mut().push(launch);
        echo()
    });
    h.mount(move || {
        provide(given.clone());
        let app = OneTerm::new(sessions);
        // No key is known: the Connect dialog asks to trust it (never the user's own file).
        app.model.ui().opening.known_hosts.set(".tmp/hosts-tests/known_hosts".into());
        app.view()
    });
    h.settle();
    App { h, made, settings }
}

impl App {
    /// Clicks a host's row with the pointer, `times` in a row.
    pub fn click_host(&mut self, name: &str, times: usize) {
        let row = self.h.get_by_role(Role::Button, name).node();
        let at = self.h.tree.window_rect(row).center();
        for _ in 0..times {
            self.h.click(at.x, at.y);
        }
        self.h.settle();
    }

    /// The Connect dialog's Connect.
    pub fn connect(&mut self) {
        self.h.get_by_role(Role::Button, "Connect").click();
        self.h.settle();
    }

    pub fn add(&mut self, name: &str, group: &str, target: &str) {
        self.h.get_by_role(Role::Button, "Add SSH host").click();
        self.h.settle();
        for (id, value) in [("host-name", name), ("host-group", group), ("host-target", target)] {
            self.h.get_by_test_id(id).set_value(value);
        }
        self.h.settle();
        self.h.get_by_role(Role::Button, "Add host").click();
        self.h.settle();
    }

    /// Opens the editor of a host from its row's menu.
    pub fn edit(&mut self, name: &str) {
        let node = self.h.get_by_role(Role::Button, name).node();
        let at = self.h.tree.window_rect(node).center();
        for input in [quadra_tree::PointerInput::Down, quadra_tree::PointerInput::Up] {
            self.h.tree.pointer(input, quadra::MouseButton::Right, at, Mods::NONE, self.h.now());
        }
        self.h.settle();
        self.h.get_by_role(Role::MenuItem, "Edit\u{2026}").click();
        self.h.settle();
    }

    pub fn rows(&self) -> Vec<Vec<String>> {
        self.settings.value("ssh.hosts", Vec::<Vec<String>>::new()).get()
    }
}
