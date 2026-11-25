//! Links in a space (specs/oneterm, "Spaces"), the shell faked: an https link Ctrl+clicked opens, an
//! http one asks and opens on Open, a `javascript:` one is refused with a card; the menu of
//! the other button opens and copies a link. And the card of a crash, whose Show report
//! reveals the report through the same shell.

use std::rc::Rc;
use std::time::Instant;

use oneterm::{NewSession, OneTerm, keymap};
use quadra::{Key, Mods, MouseButton, Point, Role};
use quadra_shell::FakeShell;
use quadra_term::{Session, echo};
use quadra_testing::Harness;
use quadra_tree::PointerInput;

struct App {
    h: Harness,
    s: Session,
    shell: Rc<FakeShell>,
}

fn app() -> App {
    let mut h = Harness::new();
    h.size = quadra::Size::new(1200.0, 700.0);
    h.tree.set_keymap(Rc::new(keymap()));
    let (slot, shell) = (Rc::new(std::cell::RefCell::new(None)), FakeShell::new());
    let (kept, given) = (slot.clone(), shell.clone());
    let sessions: NewSession = Rc::new(move |_| {
        let s = echo();
        *kept.borrow_mut() = Some(s.clone());
        s
    });
    h.mount(move || {
        given.provide();
        OneTerm::new(sessions).view()
    });
    h.settle();
    let s = slot.borrow_mut().take().unwrap();
    App { h, s, shell }
}

impl App {
    /// Writes `bytes` on a line of its own and presses `button` on the cell two left of where
    /// the cursor then is: inside what was written.
    fn press_on(&mut self, bytes: &[u8], button: MouseButton, mods: Mods) {
        self.s.feeder().feed(b"\r\n", Instant::now());
        self.s.feeder().feed(bytes, Instant::now());
        self.h.settle();
        let caret = self.h.tree.text_caret().expect("the terminal has the focus");
        let at = Point::new(caret.x - caret.width * 1.5, caret.y + caret.height / 2.0);
        for input in [PointerInput::Move, PointerInput::Down, PointerInput::Up] {
            self.h.tree.pointer(input, button, at, mods, self.h.now());
        }
        self.h.settle();
    }

    fn ctrl_click(&mut self, bytes: &[u8]) {
        self.press_on(bytes, MouseButton::Left, Mods::CTRL);
    }

    fn opened(&self) -> Vec<String> {
        self.shell.asked().into_iter().filter(|it| it.starts_with("open: ")).collect()
    }
}

#[test]
fn https_opens_http_asks_and_javascript_is_refused() {
    let mut a = app();
    a.ctrl_click(b"\x1b]8;;https://example.com/docs\x1b\\the docs\x1b]8;;\x1b\\");
    assert_eq!(a.opened(), ["open: https://example.com/docs"]);

    a.ctrl_click(b"see http://example.com/plain");
    assert_eq!(a.opened().len(), 1, "asked first");
    assert!(a.h.has_text("Open http://example.com/plain in the browser?"), "{}", a.h.semantics());
    a.h.get_by_role(Role::Button, "Open").click();
    a.h.settle();
    assert_eq!(a.opened()[1..], ["open: http://example.com/plain".to_string()]);

    a.ctrl_click(b"\x1b]8;;javascript:alert(1)\x1b\\run me\x1b]8;;\x1b\\");
    assert_eq!(a.opened().len(), 2, "never opened");
    assert!(a.h.has_text("javascript:alert(1) was not opened: only web links open."), "{}", a.h.semantics());
}

#[test]
fn the_menu_opens_and_copies_the_link_it_was_opened_on() {
    let mut a = app();
    a.press_on(b"go to https://example.com/x now", MouseButton::Right, Mods::NONE);
    assert!(a.h.query(|n| n.semantics.role == Role::MenuItem && n.semantics.name == "Open link").is_empty());
    a.h.key(Key::Escape, Mods::NONE);
    a.h.settle();
    a.press_on(b"https://example.com/x", MouseButton::Right, Mods::NONE);
    a.h.get_by_role(Role::MenuItem, "Copy link").click();
    a.h.settle();
    assert_eq!(a.h.clipboard.read_text().as_deref(), Some("https://example.com/x"));
    a.press_on(b"https://example.com/y", MouseButton::Right, Mods::NONE);
    a.h.get_by_role(Role::MenuItem, "Open link").click();
    a.h.settle();
    assert_eq!(a.opened(), ["open: https://example.com/y"]);
}

#[test]
fn a_crash_of_the_last_run_is_a_card_that_shows_its_report() {
    let mut h = Harness::new();
    h.size = quadra::Size::new(1200.0, 700.0);
    let shell = FakeShell::new();
    let given = shell.clone();
    let sessions: NewSession = Rc::new(|_| echo());
    h.mount(move || {
        given.provide();
        let term = OneTerm::new(sessions);
        term.crashed(std::path::Path::new("crashes/2026-11-17-10-52-00.txt"));
        term.view()
    });
    h.settle();
    assert!(
        h.has_text("OneTerm closed unexpectedly")
            && h.has_text("A report was written: 2026-11-17-10-52-00.txt")
    );
    h.get_by_role(Role::Button, "Show report").click();
    h.settle();
    assert!(
        shell.asked().iter().any(|it| it.starts_with("reveal: ") && it.ends_with("2026-11-17-10-52-00.txt"))
    );
}
