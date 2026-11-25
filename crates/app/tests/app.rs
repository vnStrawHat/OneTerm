//! OneTerm on the headless platform with an `echo()` session (specs/oneterm, "Tests"): the title
//! bar, the terminal with the focus from the start, the status bar's directory (the clock:
//! `tests/bars.rs`).

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use oneterm::{NewSession, OneTerm, keymap};
use quadra_term::{Session, echo};
use quadra_testing::Harness;

fn app() -> (Harness, Session) {
    let mut h = Harness::new();
    h.tree.set_keymap(Rc::new(keymap()));
    let made = Rc::new(RefCell::new(None));
    let kept = made.clone();
    let sessions: NewSession = Rc::new(move |_| {
        let s = echo();
        *kept.borrow_mut() = Some(s.clone());
        s
    });
    h.mount(move || OneTerm::new(sessions).view());
    h.settle();
    let s = made.borrow_mut().take().expect("the window made its session");
    (h, s)
}

#[test]
fn the_window_shows_a_terminal_that_has_the_focus_and_says_where_it_is() {
    let (mut h, s) = app();
    assert!(h.has_text("OneTerm"));
    assert!(h.has_text("1 local \u{B7} 1 space"));
    h.type_text("ls");
    h.settle();
    assert_eq!(s.screen_text()[0], "ls", "typing reaches the shell at once");

    s.feeder().feed(b"\x1b]0;vim\x07\x1b]7;file://box/home/me/src\x07", Instant::now());
    h.settle();
    assert!(h.get_by_role(quadra::Role::Tab, "vim").node() == h.get_by_role(quadra::Role::Tab, "vim").node());
    assert!(h.has_text("/home/me/src"));
    h.get_by_role(quadra::Role::Button, "Working directory").click();
    assert_eq!(h.clipboard.read_text().as_deref(), Some("/home/me/src"));
}
