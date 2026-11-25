//! The Keybindings page (specs/oneterm, "Settings"): an action's keys are recorded, unbound and
//! reset there, kept as `keys.<id>`, and the app's keymap follows; two actions on one
//! keystroke are said.

use std::cell::RefCell;
use std::rc::Rc;

use oneterm::{NewSession, OneTerm, keymap};
use quadra::{Key, Keymap, Mods, Role, Settings, Signal, provide, signal};
use quadra_term::echo;
use quadra_testing::Harness;

struct App {
    h: Harness,
    made: Rc<RefCell<usize>>,
    settings: Rc<Settings>,
    /// The app's keymap, as a window follows it.
    keys: Signal<Rc<Keymap>>,
}

fn app(before: impl FnOnce(&Settings)) -> App {
    let mut h = Harness::new();
    h.size = quadra::Size::new(1200.0, 700.0);
    let (made, settings, keys) = (Rc::new(RefCell::new(0)), Settings::memory(), signal(Rc::new(keymap())));
    before(&settings);
    let (kept, given) = (made.clone(), settings.clone());
    let sessions: NewSession = Rc::new(move |_| {
        *kept.borrow_mut() += 1;
        echo()
    });
    h.mount(move || {
        provide(given.clone());
        provide(keys);
        OneTerm::new(sessions).view()
    });
    let mut a = App { h, made, settings, keys };
    a.settle();
    a
}

impl App {
    fn settle(&mut self) {
        self.h.settle();
        self.h.tree.set_keymap(self.keys.get_untracked());
    }

    fn key(&mut self, key: Key, mods: Mods) {
        self.h.key(key, mods);
        self.settle();
    }

    fn sessions(&self) -> usize {
        *self.made.borrow()
    }

    fn setting(&self, id: &str) -> Signal<String> {
        self.settings.value(&format!("keys.{id}"), String::from("?"))
    }

    fn open_page(&mut self) {
        self.h.get_by_role(Role::Button, "Settings").click();
        self.settle();
        self.h.get_by_role(Role::TreeItem, "Keybindings").click();
        self.settle();
    }

    fn done(&mut self) {
        self.h.get_by_role(Role::Button, "Done").click();
        self.settle();
    }
}

#[test]
fn new_tab_is_rebound_unbound_and_reset() {
    let mut a = app(|_| ());
    let ctrl_shift = Mods::CTRL | Mods::SHIFT;
    a.open_page();
    // Recorded: Ctrl+Shift+T is New tab's, and while it records it opens nothing.
    a.h.get_by_role(Role::Button, "New tab keys").focus();
    a.settle();
    a.key(Key::T, ctrl_shift);
    assert_eq!(a.setting("new_tab").get_untracked(), "ctrl-shift-t");
    assert_eq!(a.sessions(), 1);
    a.done();
    a.key(Key::T, Mods::CTRL);
    assert_eq!(a.sessions(), 1, "the old keys open nothing");
    a.key(Key::T, ctrl_shift);
    assert_eq!(a.sessions(), 2, "the new ones open a tab");

    // Backspace unbinds it: no keys open a tab.
    a.open_page();
    a.h.get_by_role(Role::Button, "New tab keys").click();
    a.key(Key::Backspace, Mods::NONE);
    assert_eq!(a.setting("new_tab").get_untracked(), "");
    a.key(Key::T, ctrl_shift);
    a.key(Key::T, Mods::CTRL);
    assert_eq!(a.sessions(), 2);

    // Reset: Ctrl+T again. The kit's Reset shows on the row that differs, named by its action.
    let resets = a.h.query(|n| {
        n.semantics.role == Role::Button
            && n.semantics.name.starts_with("Reset ")
            && n.semantics.name != "Reset all"
    });
    assert_eq!(resets.len(), 1, "a Reset on the changed row only");
    a.h.get_by_role(Role::Button, "Reset New tab").click();
    a.settle();
    assert_eq!(a.setting("new_tab").get_untracked(), "ctrl-t");
    a.done();
    a.key(Key::T, Mods::CTRL);
    assert_eq!(a.sessions(), 3);
}

#[test]
fn keys_kept_in_the_settings_hold_from_the_start_and_a_clash_is_said() {
    let mut a = app(|settings| settings.value("keys.new_tab", String::new()).set("ctrl-shift-n".into()));
    a.key(Key::N, Mods::CTRL | Mods::SHIFT);
    assert_eq!(a.sessions(), 2, "the kept keys open a tab without the page");

    a.open_page();
    assert!(!a.h.has_text("Also New tab"));
    a.setting("split").set("ctrl-shift-n".into());
    a.settle();
    assert!(a.h.has_text("Also New tab") && a.h.has_text("Also Split the space"));
    // Written another way, the same keystroke clashes too.
    a.setting("split").set("alt-,".into());
    a.setting("lock").set("alt-comma".into());
    a.settle();
    assert!(a.h.has_text("Also Lock") && !a.h.has_text("Also New tab"));
}

#[test]
fn a_lock_while_a_field_records_takes_the_keys_from_it() {
    let mut a = app(|settings| settings.value("security.lock_after_minutes", 0.0).set(1.0));
    // A master password set since the start: a vault, which a start with it would open locked.
    a.settings
        .value("ssh.vault", String::new())
        .set(include_str!("../../vault/tests/fixtures/vault.json").into());
    a.open_page();
    let field = a.h.get_by_role(Role::Button, "New tab keys").focus().node();
    a.settle();
    a.h.advance(std::time::Duration::from_secs(70));
    a.settle();
    assert!(a.h.has_text("OneTerm is locked"));
    assert_ne!(a.h.tree.focused(), Some(field), "the focus went to the password");
    a.key(Key::P, Mods::NONE);
    assert_eq!(a.setting("new_tab").get_untracked(), "ctrl-t", "the password's keys are not recorded");
}

#[test]
fn the_app_s_other_actions_have_keys() {
    let mut a = app(|_| {});
    let docks = |a: &App| a.h.query(|n| n.semantics.name == "SFTP").len();
    let closed = docks(&a);
    a.key(Key::B, Mods::CTRL | Mods::SHIFT);
    assert!(docks(&a) > closed, "the dock shows");
    a.key(Key::B, Mods::CTRL | Mods::SHIFT);
    assert_eq!(docks(&a), closed);
    a.key(Key::Comma, Mods::CTRL);
    assert!(a.h.has_text("Appearance"), "the settings, in the window where no other opens");
    a.key(Key::Comma, Mods::CTRL);
    a.setting("duplicate").set("ctrl-shift-d".into());
    a.settle();
    a.key(Key::D, Mods::CTRL | Mods::SHIFT);
    assert_eq!(a.sessions(), 2, "the tab again");
    a.open_page();
    a.h.get_by_role(Role::Button, "Reset all").click();
    a.settle();
    assert_eq!(a.setting("duplicate").get_untracked(), "", "back to unbound");
}
