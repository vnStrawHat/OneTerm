//! Selecting in a space with the pointer (specs/oneterm, "Spaces"; the view is Quadra's,
//! specs/18): two clicks select a word, three the line, and Shift and a click reaches from
//! the selection there is to the cell clicked.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use oneterm::{NewSession, OneTerm, keymap};
use quadra::{Mods, MouseButton, Point};
use quadra_term::{Session, echo};
use quadra_testing::Harness;
use quadra_tree::PointerInput::{Down, Move, Up};

/// A press and a release at `x` in the space's first row.
fn click(h: &mut Harness, x: f32, mods: Mods) {
    for input in [Move, Down, Up] {
        let now = h.now();
        h.tree.pointer(input, MouseButton::Left, Point::new(x, 32.0 + 12.0), mods, now);
    }
    h.settle();
}

#[test]
fn two_clicks_select_a_word_three_the_line_and_shift_reaches_on() {
    let mut h = Harness::new();
    h.size = quadra::Size::new(1200.0, 700.0);
    h.tree.set_keymap(Rc::new(keymap()));
    let made = Rc::new(RefCell::new(Vec::new()));
    let kept = made.clone();
    let sessions: NewSession = Rc::new(move |_| {
        let s: Session = echo();
        kept.borrow_mut().push(s.clone());
        s
    });
    h.mount(move || OneTerm::new(sessions).view());
    h.settle();
    let s = made.borrow()[0].clone();
    s.feeder().feed(b"hello world\r\n", Instant::now());
    h.settle();
    // Over `hello`, after the rail of 270.
    let x = 270.0 + 30.0;
    click(&mut h, x, Mods::NONE);
    assert_eq!(s.selection_text(), None, "one click selects nothing");
    click(&mut h, x, Mods::NONE);
    assert_eq!(s.selection_text().as_deref(), Some("hello"), "two clicks: the word");
    click(&mut h, x, Mods::NONE);
    let line = s.selection_text().unwrap_or_default();
    assert_eq!(line.trim_end(), "hello world", "three clicks: the line");

    // Later, a click and then Shift and a click: the text between; further on, more of it
    // from the same start.
    h.advance(Duration::from_secs(1));
    click(&mut h, x, Mods::NONE);
    assert_eq!(s.selection_text(), None);
    click(&mut h, x + 40.0, Mods::SHIFT);
    let near = s.selection_text().expect("Shift and a click select");
    assert!(near.len() >= 2 && "hello world".contains(&near), "{near:?}");
    click(&mut h, x + 70.0, Mods::SHIFT);
    let far = s.selection_text().expect("Shift and a click go on");
    assert!(far.len() > near.len() && far.starts_with(&near), "{near:?} then {far:?}");
}
