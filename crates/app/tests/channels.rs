//! Input channels (specs/oneterm, "Spaces"): spaces that joined one get what is typed in any of
//! them, a paste bracketed as each one's program asked; a chip names the channel; leave,
//! close, the tab's all, and an ended program leaves.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use oneterm::{NewSession, OneTerm, keymap};
use quadra::{Key, Mods, MouseButton, Point, Role};
use quadra_term::vt::ResizePolicy;
use quadra_term::{Link, Session};
use quadra_testing::Harness;
use quadra_tree::PointerInput;

/// A session, with what was written to it.
type Made = (Session, Rc<RefCell<Vec<u8>>>);

struct App {
    h: Harness,
    made: Rc<RefCell<Vec<Made>>>,
}

fn app() -> App {
    let mut h = Harness::new();
    h.size = quadra::Size::new(1200.0, 700.0);
    h.tree.set_keymap(Rc::new(keymap()));
    let made = Rc::new(RefCell::new(Vec::new()));
    let kept = made.clone();
    let sessions: NewSession = Rc::new(move |_| {
        let (s, wrote) = (Session::new(ResizePolicy::BottomAnchor), Rc::new(RefCell::new(Vec::new())));
        let into = wrote.clone();
        s.connect(Link::new(move |bytes| into.borrow_mut().extend_from_slice(bytes)));
        kept.borrow_mut().push((s.clone(), wrote));
        s
    });
    h.mount(move || OneTerm::new(sessions).view());
    h.settle();
    App { h, made }
}

impl App {
    fn key(&mut self, key: Key, mods: Mods) {
        self.h.key(key, mods);
        self.h.settle();
    }

    /// What session `n` was written since the last look.
    fn taken(&self, n: usize) -> String {
        String::from_utf8_lossy(&std::mem::take(&mut *self.made.borrow()[n].1.borrow_mut())).into_owned()
    }

    fn written(&self) -> Vec<String> {
        (0..self.made.borrow().len()).map(|n| self.taken(n)).collect()
    }

    /// The menu of the `n`th space (in reading order), and a row of its Input channel.
    fn channel(&mut self, n: usize, row: &str) {
        let id = self.h.query(|n| n.semantics.role == Role::TextInput && n.semantics.name == "Terminal")[n];
        let at: Point = self.h.tree.window_rect(id).center();
        for input in [PointerInput::Move, PointerInput::Down, PointerInput::Up] {
            self.h.tree.pointer(input, MouseButton::Right, at, Mods::NONE, self.h.now());
        }
        self.h.settle();
        self.h.get_by_role(Role::MenuItem, "Input channel").click();
        self.h.settle();
        self.h.get_by_role(Role::MenuItem, row).click();
        self.h.settle();
    }

    fn chips(&self, name: &str) -> usize {
        self.h.query(|n| n.semantics.role == Role::Image && n.semantics.name == name).len()
    }
}

#[test]
fn members_get_what_is_typed_in_any_of_them_and_a_paste_as_their_own() {
    let mut a = app();
    a.key(Key::D, Mods::CTRL);
    a.key(Key::D, Mods::CTRL);
    a.channel(0, "Channel A");
    a.channel(1, "Channel A");
    assert_eq!(a.chips("Channel A"), 2);
    a.written();

    // Typed in the second (its menu gave it the focus): both members. In the third, a
    // non-member: only it.
    a.h.type_text("ls");
    a.key(Key::Enter, Mods::NONE);
    assert_eq!(a.written(), ["ls\r", "ls\r", ""]);
    a.key(Key::Right, Mods::CTRL | Mods::ALT);
    a.h.type_text("x");
    a.h.settle();
    assert_eq!(a.written(), ["", "", "x"]);
    a.key(Key::Left, Mods::CTRL | Mods::ALT);

    // A paste: bracketed for the one whose program asked.
    a.made.borrow()[0].0.feeder().feed(b"\x1b[?2004h", Instant::now());
    a.h.settle();
    a.h.clipboard.write_text("pwd");
    a.key(Key::V, Mods::CTRL | Mods::SHIFT);
    assert_eq!(a.written(), ["\x1b[200~pwd\x1b[201~", "pwd", ""]);

    // Another channel for the first: the two are apart.
    a.channel(0, "Channel B");
    assert_eq!((a.chips("Channel A"), a.chips("Channel B")), (1, 1));
    a.h.type_text("y");
    a.h.settle();
    assert_eq!(a.written(), ["y", "", ""]);
}

#[test]
fn leave_close_the_tabs_all_and_an_ended_program() {
    let mut a = app();
    a.key(Key::D, Mods::CTRL);
    a.key(Key::D, Mods::CTRL);
    a.channel(2, "Channel C");
    a.channel(2, "Join all spaces in tab to C");
    assert_eq!(a.chips("Channel C"), 3);
    a.channel(1, "Leave channel");
    assert_eq!(a.chips("Channel C"), 2);
    a.channel(0, "Close channel C");
    assert_eq!(a.chips("Channel C"), 0);

    a.channel(0, "Channel D");
    a.channel(1, "Channel E");
    a.channel(2, "Leave with all spaces in tab");
    assert_eq!(a.chips("Channel D") + a.chips("Channel E"), 0);

    // A space whose program ended leaves its channel: what the others type does not close it.
    a.channel(0, "Channel A");
    a.channel(1, "Channel A");
    a.made.borrow()[0].0.feeder().exited(1);
    a.h.settle();
    assert_eq!(a.chips("Channel A"), 1);
    a.h.type_text("q"); // in the second, which the menu gave the focus
    a.h.settle();
    assert!(a.h.has_text("3 local \u{B7} 3 spaces"), "the ended space is still there");
    assert_eq!(a.written()[..2], ["", "q"]);
}

#[test]
fn a_split_is_in_no_channel_and_a_closed_member_gets_nothing() {
    let mut a = app();
    a.channel(0, "Channel A");
    a.key(Key::D, Mods::CTRL);
    assert_eq!(a.chips("Channel A"), 1);
    a.channel(1, "Channel A");
    a.key(Key::W, Mods::CTRL); // the second goes
    a.written();
    a.h.type_text("z");
    a.h.settle();
    assert_eq!(a.written(), ["z", ""]);
}
