//! OSC 99 notifications as cards (specs/oneterm, "Signals"): buttons, opening, a card that takes
//! the place of one with its id, and what the program asked to hear, written back to it; when
//! one wants to show, its icon, its sound, and what OneTerm says it does.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use oneterm::{NewSession, OneTerm, keymap};
use quadra::{Key, Mods, Role};
use quadra_shell::FakeShell;
use quadra_term::vt::ResizePolicy;
use quadra_term::{Link, Session};
use quadra_testing::Harness;

struct App {
    h: Harness,
    made: Rc<RefCell<Vec<Session>>>,
    wrote: Rc<RefCell<Vec<u8>>>,
    shell: Rc<FakeShell>,
}

fn app() -> App {
    let mut h = Harness::new();
    h.size = quadra::Size::new(1200.0, 700.0);
    h.tree.set_keymap(Rc::new(keymap()));
    let (made, wrote) = (Rc::new(RefCell::new(Vec::new())), Rc::new(RefCell::new(Vec::new())));
    let (kept, into) = (made.clone(), wrote.clone());
    let sessions: NewSession = Rc::new(move |_| {
        // As OneTerm's shells declare.
        let config = quadra_term::vt::Config { notifications: oneterm::abilities(), ..Default::default() };
        let s = Session::configured(ResizePolicy::BottomAnchor, config);
        let into = into.clone();
        s.connect(Link::new(move |bytes| into.borrow_mut().extend_from_slice(bytes)));
        kept.borrow_mut().push(s.clone());
        s
    });
    let shell = FakeShell::new();
    let given = shell.clone();
    h.mount(move || {
        given.provide();
        OneTerm::new(sessions).view()
    });
    h.settle();
    App { h, made, wrote, shell }
}

impl App {
    /// Feeds a space's terminal; what it answers.
    fn feed(&mut self, n: usize, bytes: &[u8]) -> Vec<u8> {
        let s = self.made.borrow()[n].clone();
        let replies = s.feeder().feed(bytes, Instant::now());
        self.h.settle();
        replies
    }

    /// What was written to the programs since the last time, as text.
    fn told(&self) -> String {
        String::from_utf8_lossy(&std::mem::take(&mut *self.wrote.borrow_mut())).into_owned()
    }

    fn cards(&self) -> usize {
        self.h.query(|n| n.semantics.role == Role::Button && n.semantics.name == "Dismiss notification").len()
    }

    fn pictures(&self) -> usize {
        self.h.query(|n| n.semantics.role == Role::Image).len()
    }
}

#[test]
fn a_card_has_the_programs_buttons_takes_the_place_of_its_id_and_reports_what_was_asked() {
    let mut a = app();
    a.feed(
        0,
        "\x1b]99;i=b:d=0:a=report:c=1;Build failed\x1b\\\x1b]99;i=b:p=buttons;Log\u{2028}Retry\x1b\\"
            .as_bytes(),
    );
    assert_eq!(a.cards(), 1);
    a.h.get_by_role(Role::Button, "Log");
    // The same id again: the card is replaced, and the program hears no close.
    a.feed(0, b"\x1b]99;i=b:a=report:c=1;Build still failing\x1b\\");
    assert_eq!(a.told(), "");
    assert_eq!(a.cards(), 1);
    a.h.get_by_role(Role::Group, "Build still failing");
    a.feed(
        0,
        "\x1b]99;i=b:d=0:a=report:c=1;Build failed\x1b\\\x1b]99;i=b:p=buttons;Log\u{2028}Retry\x1b\\"
            .as_bytes(),
    );
    a.h.get_by_role(Role::Button, "Retry").click();
    a.h.settle();
    assert_eq!(a.told(), "\x1b]99;i=b;2\x1b\\\x1b]99;i=b:p=close;\x1b\\", "the button, then the close");
    assert_eq!(a.cards(), 0);
}

#[test]
fn a_program_closes_its_card_and_asks_which_show_and_opening_shows_the_space() {
    let mut a = app();
    a.feed(0, b"\x1b]99;i=one:a=report;Deploy done\x1b\\\x1b]99;i=two;Tests passed\x1b\\");
    a.told();
    a.feed(0, b"\x1b]99;i=two:p=close;\x1b\\\x1b]99;i=q:p=alive;\x1b\\");
    assert_eq!(a.cards(), 1);
    assert_eq!(a.told(), "\x1b]99;i=q:p=alive;one\x1b\\");
    // From another tab, opening it shows its tab again and tells the program.
    a.h.key(Key::T, Mods::CTRL);
    a.h.settle();
    let chosen = |a: &App| {
        let tabs = a.h.query(|n| n.semantics.role == Role::Tab);
        let picked = a.h.query(|n| n.semantics.role == Role::Tab && n.semantics.selected);
        tabs.iter().position(|id| picked.contains(id))
    };
    assert_eq!(chosen(&a), Some(1));
    a.h.get_by_role(Role::Button, "Deploy done").click();
    a.h.settle();
    assert_eq!(chosen(&a), Some(0));
    assert_eq!(a.told(), "\x1b]99;i=one;\x1b\\");
}

#[test]
fn every_ability_is_declared_and_a_card_waits_for_nothing_while_its_space_is_seen() {
    let mut a = app();
    let answer = a.feed(0, b"\x1b]99;i=q:p=?;\x1b\\");
    let want = "a=focus,report:c=1:o=always,unfocused,invisible:p=title,body,?,close,icon,alive,buttons:\
                s=system,silent,info,warning,warn,error,question:u=0,1,2:w=1";
    assert_eq!(String::from_utf8_lossy(&answer), format!("\x1b]99;i=q:p=?;{want}\x1b\\"));
    // The space has the keyboard: neither shows.
    a.feed(0, b"\x1b]99;o=unfocused;Built\x1b\\\x1b]99;o=invisible;Deployed\x1b\\");
    assert_eq!(a.cards(), 0);
    // The window lost the keyboard: unfocused shows, invisible not while the tab shows.
    a.h.tree.set_active(false);
    a.feed(0, b"\x1b]99;o=unfocused;Built\x1b\\\x1b]99;o=invisible;Deployed\x1b\\");
    assert_eq!(a.cards(), 1);
    a.h.tree.set_minimized(true);
    a.feed(0, b"\x1b]99;o=invisible;Deployed\x1b\\");
    assert_eq!(a.cards(), 2, "minimized, it is not seen");
    // Active again, another tab shown: the space lacks the keyboard but is not unseen.
    a.h.tree.set_minimized(false);
    a.h.tree.set_active(true);
    a.h.key(Key::T, Mods::CTRL);
    a.h.settle();
    a.feed(0, b"\x1b]99;o=unfocused;Tested\x1b\\\x1b]99;o=invisible;Linted\x1b\\");
    assert_eq!(a.cards(), 3);
}

#[test]
fn an_icon_is_a_name_first_else_its_data_kept_by_its_id_and_a_named_sound_plays() {
    let mut a = app();
    // "flat 16 16 ff0000ff": a picture the harness decodes.
    a.feed(0, b"\x1b]99;i=a:d=0:g=red;Built\x1b\\\x1b]99;i=a:p=icon:e=1;ZmxhdCAxNiAxNiBmZjAwMDBmZg==\x1b\\");
    assert_eq!((a.cards(), a.pictures()), (1, 1));
    a.feed(0, b"\x1b]99;g=red;Built again\x1b\\");
    assert_eq!(a.pictures(), 2, "the data kept under its id");
    a.feed(0, b"\x1b]99;g=red:n=ZXJyb3I=;Failed\x1b\\\x1b]99;g=none;Odd\x1b\\");
    assert_eq!((a.cards(), a.pictures()), (4, 2), "a name first; an id of nothing kept, the tone's");
    assert!(a.shell.asked().is_empty(), "no sound named, none plays");
    // Names are base64: "error", "silent", "bloop".
    a.feed(0, b"\x1b]99;s=ZXJyb3I=;Failed\x1b\\\x1b]99;s=c2lsZW50;Quiet\x1b\\\x1b]99;s=Ymxvb3A=;Odd\x1b\\");
    assert_eq!(a.shell.asked(), ["sound: Error", "sound: Default"]);
}

#[test]
fn a_card_names_the_sequence_it_came_from() {
    let mut a = app();
    a.feed(0, b"\x1b]9;plain\x07\x1b]777;notify;Shell;done\x07\x1b]99;;kitty\x1b\\");
    assert_eq!(a.cards(), 3);
    for source in ["OSC 9", "OSC 777", "OSC 99"] {
        let said = format!("\u{B7} {source}");
        assert_eq!(a.h.query(|n| n.semantics.name.ends_with(&said)).len(), 1, "{source}");
    }
}
