//! What a new local shell is told (specs/oneterm, "M4 in detail"): Force links, on until its switch
//! in the settings turns it off.

use std::cell::RefCell;
use std::rc::Rc;

use oneterm::{NewSession, OneTerm, keymap};
use quadra::{Key, Mods, Role, Settings, provide};
use quadra_term::echo;
use quadra_testing::Harness;

#[test]
fn force_links_is_on_for_a_new_local_shell_until_its_switch_turns_it_off() {
    let mut h = Harness::new();
    h.size = quadra::Size::new(1200.0, 700.0);
    h.tree.set_keymap(Rc::new(keymap()));
    let settings = Settings::memory();
    let (given, made) = (settings.clone(), Rc::new(RefCell::new(Vec::new())));
    let told = made.clone();
    let sessions: NewSession = Rc::new(move |launch: oneterm::Launch| {
        told.borrow_mut().push(launch.force_links);
        echo()
    });
    h.mount(move || {
        provide(given.clone());
        OneTerm::new(sessions).view()
    });
    h.settle();
    let new_tab = |h: &mut Harness| {
        h.key(Key::T, Mods::CTRL);
        h.settle();
        made.borrow().last().copied()
    };
    assert_eq!(new_tab(&mut h), Some(true));
    h.get_by_role(Role::Button, "Settings").click();
    h.settle();
    h.get_by_role(Role::TreeItem, "Terminal").click();
    h.settle();
    h.get_by_role(Role::Switch, "Force links").click();
    h.get_by_role(Role::Button, "Done").click();
    h.settle();
    assert!(!settings.value("terminal.force_hyperlink", true).get());
    assert_eq!(new_tab(&mut h), Some(false));
}
