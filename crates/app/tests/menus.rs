//! A space's menu for the other button (specs/oneterm, "Spaces"): what it does to the terminal and
//! the spaces, and the setting that turns it off.

use std::cell::RefCell;
use std::rc::Rc;

use oneterm::{NewSession, OneTerm, keymap};
use quadra::{Mods, MouseButton, Point, Role};
use quadra_term::{Session, echo};
use quadra_testing::Harness;
use quadra_tree::PointerInput;

struct App {
    h: Harness,
    made: Rc<RefCell<Vec<Session>>>,
}

fn app() -> App {
    let mut h = Harness::new();
    h.size = quadra::Size::new(1200.0, 700.0);
    h.tree.set_keymap(Rc::new(keymap()));
    let made = Rc::new(RefCell::new(Vec::new()));
    let kept = made.clone();
    let sessions: NewSession = Rc::new(move |_| {
        let s = echo();
        kept.borrow_mut().push(s.clone());
        s
    });
    h.mount(move || OneTerm::new(sessions).view());
    h.settle();
    App { h, made }
}

impl App {
    /// A click of the other button on the first terminal.
    fn other_click(&mut self) {
        let id = self.h.query(|n| n.semantics.role == Role::TextInput && n.semantics.name == "Terminal")[0];
        let at: Point = self.h.tree.window_rect(id).center();
        for input in [PointerInput::Move, PointerInput::Down, PointerInput::Up] {
            self.h.tree.pointer(input, MouseButton::Right, at, Mods::NONE, self.h.now());
        }
        self.h.settle();
    }

    fn choose(&mut self, name: &str) {
        self.h.get_by_role(Role::MenuItem, name).click();
        self.h.settle();
    }
}

#[test]
fn the_menu_pastes_splits_and_closes() {
    let mut a = app();
    a.other_click();
    for name in [
        "Copy",
        "Paste",
        "Select all",
        "Clear scrollback",
        "Start logging",
        "Split right",
        "Split down",
        "Input channel",
        "Close space",
    ] {
        assert!(
            a.h.query(|n| n.semantics.role == Role::MenuItem && n.semantics.name == name).len() == 1,
            "{name}"
        );
    }
    assert!(a.h.query(|n| n.semantics.name == "Split space").is_empty(), "Ctrl+D is enough");
    a.h.clipboard.write_text("ls");
    a.choose("Paste");
    assert_eq!(a.made.borrow()[0].screen_text()[0], "ls", "the echo shows what was pasted");

    a.other_click();
    a.choose("Select all");
    assert!(a.made.borrow()[0].selection_text().is_some());
    a.other_click();
    a.choose("Copy");
    assert!(a.h.clipboard.read_text().unwrap_or_default().starts_with("ls"));

    a.other_click();
    a.choose("Split right");
    assert!(a.h.has_text("2 local \u{B7} 2 spaces"));
    a.other_click();
    a.choose("Close space");
    assert!(a.h.has_text("1 local \u{B7} 1 space"));
}

#[test]
fn the_setting_turns_the_menu_off() {
    let mut a = app();
    a.h.get_by_role(Role::Button, "Settings").click();
    a.h.settle();
    a.h.get_by_role(Role::TreeItem, "Terminal").click();
    a.h.settle();
    // The page's switches: the cursor's blink, then the context menu.
    let switch = a.h.query(|n| n.semantics.role == Role::Switch)[1];
    a.h.tree.act(switch, &quadra::SemanticAction::Click);
    a.h.get_by_role(Role::Button, "Done").click();
    a.h.settle();
    a.other_click();
    assert!(a.h.query(|n| n.semantics.role == Role::MenuItem).is_empty());
}

#[test]
fn a_program_that_reports_the_mouse_takes_the_press_and_a_click_copies_nothing() {
    let mut a = app();
    a.h.clipboard.write_text("kept");
    let id = a.h.query(|n| n.semantics.role == Role::TextInput && n.semantics.name == "Terminal")[0];
    let at: Point = a.h.tree.window_rect(id).center();
    for input in [PointerInput::Down, PointerInput::Up] {
        a.h.tree.pointer(input, MouseButton::Left, at, Mods::NONE, a.h.now());
    }
    a.h.settle();
    assert_eq!(a.h.clipboard.read_text().as_deref(), Some("kept"), "a click selects nothing to copy");

    a.made.borrow()[0].feeder().feed(b"\x1b[?1000h", std::time::Instant::now());
    a.h.settle();
    a.other_click();
    assert!(a.h.query(|n| n.semantics.role == Role::MenuItem).is_empty(), "no menu over the program");
}

#[test]
fn a_space_that_logs_has_a_dot_in_the_status_bar() {
    let mut a = app();
    let dot = |a: &App| a.h.query(|n| n.semantics.name == "Logging").len();
    assert_eq!(dot(&a), 0, "nothing logs");
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.tmp/oneterm-rec-test/a.log");
    let s = a.made.borrow()[0].clone();
    s.start_log(&path, false, String::new).unwrap();
    a.h.advance(std::time::Duration::from_millis(500));
    a.h.settle();
    assert_eq!(dot(&a), 1, "{}", a.h.semantics());
    a.other_click();
    a.choose("Stop logging");
    a.h.advance(std::time::Duration::from_millis(500));
    a.h.settle();
    assert_eq!(dot(&a), 0, "the log closed");
}
