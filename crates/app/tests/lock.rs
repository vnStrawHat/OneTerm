//! The lock (specs/oneterm, "Lock"): the password set on the settings page, locking at once and
//! after a time idle, what a locked window hides and refuses, the back-off, unlocking.

use std::rc::Rc;
use std::time::{Duration, Instant};

use oneterm::{NewSession, OneTerm, keymap};
use quadra::{Key, Mods, MouseButton, Role, Settings, provide};
use quadra_term::{Session, echo};
use quadra_testing::Harness;
use quadra_tree::PointerInput;

struct App {
    h: Harness,
    first: Rc<std::cell::RefCell<Option<Session>>>,
    settings: Rc<Settings>,
}

/// A vault as OneTerm 0.6 writes it (its own fixture, at the least cost it reads), and the master
/// password that opens it.
const VAULT: &str = include_str!("../../vault/tests/fixtures/vault.json");
const PW: &str = "correct horse battery staple";

/// A window, with a master password when one is given: opened by it, as a start is locked.
fn app(password: Option<&str>) -> App {
    let mut a = app_with(password.map(|_| VAULT.to_string()));
    if let Some(password) = password {
        a.unlock_with(password);
        assert!(!a.locked());
    }
    a
}

/// A window whose settings hold a vault, when given.
fn app_with(vault: Option<String>) -> App {
    let mut h = Harness::new();
    h.size = quadra::Size::new(1200.0, 700.0);
    h.tree.set_keymap(Rc::new(keymap()));
    let settings = Settings::memory();
    if let Some(vault) = vault {
        settings.value("ssh.vault", String::new()).set(vault);
    }
    let (made, given) = (Rc::new(std::cell::RefCell::new(None::<Session>)), settings.clone());
    let keeps = made.clone();
    let sessions: NewSession = Rc::new(move |_| {
        let s = echo();
        keeps.borrow_mut().get_or_insert_with(|| s.clone());
        s
    });
    h.mount(move || {
        provide(given.clone());
        OneTerm::new(sessions).view()
    });
    h.settle();
    App { h, first: made, settings }
}

impl App {
    fn key(&mut self, key: Key, mods: Mods) {
        self.h.key(key, mods);
        self.h.settle();
    }

    fn says(&self, words: &str) -> bool {
        self.h.has_text(words)
    }

    fn locked(&self) -> bool {
        self.says("OneTerm is locked")
    }

    fn tabs(&self) -> usize {
        self.h.query(|n| n.semantics.role == Role::Tab).len()
    }

    fn unlock_with(&mut self, password: &str) {
        // The field is as wide as the card's body.
        let field = self.h.get_by_test_id("unlock").node();
        let field = self.h.tree.window_rect(self.h.tree.parent(field).expect("its frame"));
        assert_eq!(field.width, 414.0);
        self.h.get_by_test_id("unlock").set_value(password);
        self.h.settle();
        self.h.get_by_role(Role::Button, "Unlock").click();
        self.h.settle();
    }
}

#[test]
fn without_a_password_nothing_locks() {
    let mut a = app(None);
    assert!(a.h.query(|n| n.semantics.role == Role::Button && n.semantics.name == "Lock").is_empty());
    a.key(Key::L, Mods::CTRL | Mods::SHIFT);
    a.h.advance(Duration::from_secs(3600));
    a.h.settle();
    assert!(!a.locked());
}

#[test]
fn a_password_set_on_the_page_locks_and_opens_the_window() {
    let mut a = app(None);
    a.h.get_by_role(Role::Button, "Settings").click();
    a.h.settle();
    a.h.get_by_role(Role::TreeItem, "Security & lock").click();
    a.h.settle();
    a.h.get_by_test_id("password-new").set_value("open sesame");
    a.h.get_by_test_id("password-again").set_value("open sesame");
    a.h.settle();
    a.h.get_by_role(Role::Button, "Set password").click();
    a.h.settle();
    assert!(a.says("Password set"));
    // What is kept is a vault the password opens, never the password or a hash of it.
    let stored = a.settings.value("ssh.vault", String::new()).get();
    assert!(stored.contains("\"algorithm\":\"argon2id\"") && !stored.contains("open sesame"), "{stored}");
    a.h.get_by_role(Role::Button, "Done").click();
    a.h.settle();

    a.h.get_by_role(Role::Button, "Lock").click();
    a.h.settle();
    assert!(a.locked());
    assert!(a.says("Locked. Your sessions are still running."));
    assert!(a.says("Locked, details hidden"));
    assert!(a.says("1 sessions connected \u{B7} 0 notices"));
    assert_eq!(a.tabs(), 0, "the rail is not displayed, so readers do not get it");
    assert!(a.h.query(|n| n.semantics.name == "Terminal").is_empty());
    assert!(a.h.query(|n| n.semantics.name == "Settings").is_empty());

    a.unlock_with("open sesam");
    assert!(a.locked() && a.says("Wrong password"));
    a.unlock_with("open sesame");
    assert!(!a.locked());
    assert_eq!(a.tabs(), 1);
    let typing = a.h.query(|n| n.semantics.name == "Terminal" && n.focused);
    assert_eq!(typing.len(), 1, "the focus is back in the space");
}

#[test]
fn a_locked_window_refuses_the_app_keys_and_hides_cards() {
    let mut a = app(Some(PW));
    a.key(Key::L, Mods::CTRL | Mods::SHIFT);
    assert!(a.locked());
    a.key(Key::T, Mods::CTRL);
    let s = a.first.borrow().clone().expect("the first space");
    s.feeder().feed(b"\x1b]9;secret result\x07", Instant::now());
    a.h.settle();
    assert!(!a.says("secret result"), "a card would show what a program said");
    // The meter goes on, with its percentage and without the name of what reports it.
    s.feeder().feed(b"\x1b]9;4;1;64\x07\x1b]7501;state=working:id=payroll\x07", Instant::now());
    a.h.settle();
    assert!(a.says("64%") && !a.says("payroll"), "{}", a.h.semantics());
    assert!(a.says("1 sessions connected \u{B7} 1 notices"));
    a.unlock_with(PW);
    assert!(a.says("payroll"), "{}", a.h.semantics());
    assert_eq!(a.tabs(), 1, "Ctrl+T did nothing while locked");
}

#[test]
fn it_locks_after_the_time_idle_and_activity_starts_it_again() {
    let mut a = app(Some(PW));
    a.settings.value("security.lock_after_minutes", 0.0).set(1.0);
    a.h.advance(Duration::from_secs(50));
    a.h.settle();
    a.h.pointer(quadra_tree::PointerInput::Move, 600.0, 300.0);
    a.h.advance(Duration::from_secs(50));
    a.h.settle();
    assert!(!a.locked(), "the pointer moved 50 s ago");
    a.h.advance(Duration::from_secs(20));
    a.h.settle();
    assert!(a.locked());
    assert!(a.says("Locked after 1 minute idle. Your sessions are still running."));
}

#[test]
fn five_wrong_tries_start_a_back_off() {
    let mut a = app(Some(PW));
    a.key(Key::L, Mods::CTRL | Mods::SHIFT);
    for _ in 0..5 {
        a.unlock_with("no");
    }
    assert!(a.says("Wrong password \u{B7} try again in 2 s"));
    a.unlock_with(PW);
    assert!(a.locked(), "the right password waits too");
    a.h.advance(Duration::from_secs(2));
    a.h.settle();
    a.unlock_with(PW);
    assert!(!a.locked());
}

#[test]
fn removing_the_password_asks_first_and_forgets_the_saved_ones() {
    let mut a = app(Some(PW));
    let secrets = a.settings.value("ssh.secrets", Vec::<Vec<String>>::new());
    secrets.set(vec![vec!["a".into(), "sv1:00".into()], vec!["b".into(), "sv1:00".into()]]);
    a.h.get_by_role(Role::Button, "Settings").click();
    a.h.settle();
    a.h.get_by_role(Role::TreeItem, "Security & lock").click();
    a.h.settle();
    a.h.get_by_test_id("password-current").set_value(PW);
    a.h.settle();
    a.h.get_by_role(Role::Button, "Remove password").click();
    a.h.settle();
    let said =
        "The window will no longer lock, and the 2 saved passwords are forgotten. It cannot be undone.";
    assert!(a.says("Remove the master password?") && a.says(said), "{}", a.h.semantics());
    a.h.get_by_role(Role::Button, "Cancel").click();
    a.h.settle();
    assert_eq!(a.settings.value("ssh.vault", String::new()).get(), VAULT);
    assert_eq!(secrets.get().len(), 2, "Cancel keeps all");

    a.h.get_by_role(Role::Button, "Remove password").click();
    a.h.settle();
    a.h.get_by_role(Role::Button, "Remove").click();
    let end = Instant::now() + Duration::from_secs(20);
    while !a.says("Password removed") {
        assert!(Instant::now() < end, "never removed");
        a.h.advance(Duration::from_millis(10));
        a.h.settle();
    }
    assert!(a.settings.value("ssh.vault", String::new()).get().is_empty());
    assert!(secrets.get().is_empty());
}

#[test]
fn with_a_vault_the_window_opens_locked_and_its_password_opens_the_vault() {
    let mut a = app_with(Some(VAULT.to_string()));
    assert!(a.locked() && a.tabs() == 0, "locked at start, nothing shown");
    a.unlock_with("master");
    assert!(a.locked() && a.says("Wrong password"));
    a.unlock_with(PW);
    a.h.settle();
    assert!(!a.locked() && a.tabs() == 1);
    let kept = a.settings.value("ssh.vault", String::new()).get();
    assert_eq!(kept, VAULT, "the vault opened, not made again");
}

#[test]
fn locked_nothing_but_the_card_the_title_bar_and_the_status_bar_is_in_reach() {
    let mut a = app(Some(PW));
    // A space's menu open when the lock falls: an overlay, no part of the workspace.
    let id = a.h.query(|n| n.semantics.role == Role::TextInput && n.semantics.name == "Terminal")[0];
    let at = a.h.tree.window_rect(id).center();
    for input in [PointerInput::Move, PointerInput::Down, PointerInput::Up] {
        a.h.tree.pointer(input, MouseButton::Right, at, Mods::NONE, a.h.now());
    }
    a.h.settle();
    assert!(a.h.query(|n| n.semantics.role == Role::MenuItem).len() > 1, "the menu is open");
    a.h.advance(Duration::from_secs(11 * 60));
    a.h.settle();
    assert!(a.locked());
    let reach = |n: &quadra_tree::SemanticNode| {
        matches!(
            n.semantics.role,
            Role::Button | Role::MenuItem | Role::TextInput | Role::Tab | Role::TreeItem
        )
    };
    let in_reach = |a: &App| {
        let nodes = a.h.tree.semantic_tree().nodes;
        let mut names: Vec<String> =
            nodes.iter().filter(|n| reach(n)).map(|n| n.semantics.name.clone()).collect();
        names.sort();
        names
    };
    let card = ["Close", "Master password", "Maximize", "Minimize", "Show password", "Unlock"];
    assert_eq!(in_reach(&a), card, "the menu closed as the lock fell");
    // A dialog open when it locks (Quick connect) is not in reach either.
    a.unlock_with(PW);
    a.key(Key::K, Mods::CTRL | Mods::SHIFT);
    assert_eq!(a.h.query(|n| n.semantics.role == Role::Dialog).len(), 1, "Quick connect is open");
    a.h.advance(Duration::from_secs(11 * 60));
    a.h.settle();
    assert_eq!(in_reach(&a), card);
}
