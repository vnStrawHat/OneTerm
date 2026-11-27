//! A target whose user is a token (specs/oneterm, "Rail": a long target): the `+` menu's Recent
//! and the folded tab's tooltip say it by its end, each on one line; a remote space's tooltip
//! is who it is over where it is.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use oneterm::{NewSession, OneTerm, keymap};
use quadra::{Role, Settings, provide};
use quadra_term::{Session, echo};
use quadra_testing::Harness;

const USER: &str = "0123456789abcdef0123456789abcdef01234567";

struct App {
    h: Harness,
    made: Rc<RefCell<Vec<Session>>>,
}

fn app() -> App {
    let mut h = Harness::new();
    h.size = quadra::Size::new(1200.0, 700.0);
    h.tree.set_keymap(Rc::new(keymap()));
    let (made, settings) = (Rc::new(RefCell::new(Vec::new())), Settings::memory());
    // Yesterday: the longest of what Recent says of when.
    let at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() - 90_000;
    let recent = vec![vec![format!("{USER}@pam.example.com:4422"), at.to_string()]];
    settings.value("ssh.recent", Vec::<Vec<String>>::new()).set(recent);
    settings.value("tabs.rail_folded", false).set(true);
    let kept = made.clone();
    let sessions: NewSession = Rc::new(move |_| {
        let s = echo();
        kept.borrow_mut().push(s.clone());
        s
    });
    h.mount(move || {
        provide(settings.clone());
        OneTerm::new(sessions).view()
    });
    h.settle();
    App { h, made }
}

impl App {
    /// Where the last text that says `words` is: a menu's or a tooltip's, over the window's.
    fn words(&self, words: &str) -> quadra::Rect {
        let found = self.h.query(|n| n.semantics.role == Role::Text && n.semantics.name == words);
        self.h.tree.window_rect(*found.last().unwrap_or_else(|| panic!("{words}: {}", self.h.semantics())))
    }
}

#[test]
fn a_long_target_is_one_line_in_the_menu_and_a_remote_tabs_tooltip_is_who_over_where() {
    let mut a = app();
    let cut = "\u{2026}4567@pam.example.com:4422";
    assert_eq!(cut.chars().count(), 26);
    a.h.get_by_role(Role::Button, "New terminal").click();
    a.h.settle();
    let row = a.h.get_by_role(Role::MenuItem, cut).node();
    let (row, words) = (a.h.tree.window_rect(row), a.words(cut));
    assert!(words.height < row.height, "the row's words are one line: {words:?} in {row:?}");
    assert!(words.right() < row.right(), "and end before the row does: {words:?} in {row:?}");

    // No saved host: it connects at once. Its shell says where it is.
    a.h.get_by_role(Role::MenuItem, cut).click();
    a.h.settle();
    let s = a.made.borrow()[1].clone();
    s.feeder().feed(b"\x1b]9;9;/root\x07", Instant::now());
    a.h.settle();
    let tabs = a.h.query(|n| n.semantics.role == Role::Tab);
    let at = a.h.tree.window_rect(tabs[1]).center();
    a.h.pointer(quadra_tree::PointerInput::Move, at.x, at.y);
    a.h.advance(Duration::from_millis(600));
    a.h.settle();
    let tip = a.h.query(|n| n.semantics.role == Role::Tooltip);
    assert_eq!(tip.len(), 1, "{}", a.h.semantics());
    let (who, place) = (a.words(cut), a.words("/root"));
    assert!(who.bottom() <= place.y, "who it is over where it is: {who:?}, {place:?}");
    assert!(who.height < 24.0, "on one line: {who:?}");
    assert!(!a.h.has_text("ssh "), "{}", a.h.semantics());
}
