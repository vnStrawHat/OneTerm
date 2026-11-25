//! Quick connect (specs/oneterm, "M7 in detail"): the dialog makes the launch of a tab or a space,
//! and a target that does not read is said. No host is ever reached: the sessions are
//! `echo()` (the Connect dialogs on real connections are `tests/asking.rs`).

use std::cell::RefCell;
use std::rc::Rc;

use oneterm::{Launch, NewSession, OneTerm, Remote, keymap};
use oneterm_ssh::{Auth, Target};
use quadra::{Key, Mods, Role};
use quadra_term::echo;
use quadra_testing::Harness;

fn app() -> (Harness, Rc<RefCell<Vec<Launch>>>) {
    let mut h = Harness::new();
    h.size = quadra::Size::new(1200.0, 700.0);
    h.tree.set_keymap(Rc::new(keymap()));
    let made = Rc::new(RefCell::new(Vec::new()));
    let kept = made.clone();
    let sessions: NewSession = Rc::new(move |launch| {
        kept.borrow_mut().push(launch);
        echo()
    });
    h.mount(move || OneTerm::new(sessions).view());
    h.settle();
    (h, made)
}

fn connect(h: &mut Harness, target: &str) {
    h.key(Key::K, Mods::CTRL | Mods::SHIFT);
    h.settle();
    h.get_by_test_id("connect-target").set_value(target);
    h.settle();
    h.get_by_role(Role::Button, "Connect").click();
    h.settle();
}

fn tabs(h: &Harness) -> usize {
    h.query(|n| n.semantics.role == Role::Tab).len()
}

#[test]
fn a_target_opens_a_tab_that_connects_to_it() {
    let (mut h, made) = app();
    connect(&mut h, "ssh trung@10.20.3.41 -p 2222");
    let want = Remote::new(Target::parse("trung@10.20.3.41:2222").unwrap(), Auth::Auto);
    assert_eq!(made.borrow().last().and_then(|it| it.ssh.clone()), Some(want.clone()));
    assert_eq!(tabs(&h), 2);
}

#[test]
fn a_target_that_does_not_read_is_said_and_opens_nothing() {
    let (mut h, made) = app();
    connect(&mut h, "-x nowhere");
    assert_eq!(made.borrow().len(), 1, "the first tab's shell only");
    assert!(h.has_text("Not a target: -x nowhere"));
}

#[test]
fn in_the_current_tab_it_is_a_new_space() {
    let (mut h, made) = app();
    h.key(Key::K, Mods::CTRL | Mods::SHIFT);
    h.settle();
    h.get_by_role(Role::Checkbox, "Open in the current tab as a new space").click();
    h.get_by_test_id("connect-target").set_value("a@h");
    h.settle();
    h.get_by_role(Role::Button, "Connect").click();
    h.settle();
    assert_eq!(tabs(&h), 1);
    assert_eq!(h.query(|n| n.semantics.name == "Terminal" && n.semantics.role == Role::TextInput).len(), 2);
    assert!(made.borrow()[1].ssh.is_some());
}
