//! The signals (specs/oneterm, "Signals") from bytes fed to `echo()` sessions: OSC 7501 on the
//! tab and the header, OSC 9;4 on the tab and the status bar, a bell in a space not looked
//! at, OSC 9 cards and the count of what came unseen.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use oneterm::{NewSession, OneTerm, keymap};
use quadra::{Key, Mods, Role};
use quadra_term::{Session, echo};
use quadra_testing::Harness;

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
    fn feed(&mut self, n: usize, bytes: &[u8]) {
        let s = self.made.borrow()[n].clone();
        s.feeder().feed(bytes, Instant::now());
        self.h.settle();
    }

    fn key(&mut self, key: Key, mods: Mods) {
        self.h.key(key, mods);
        self.h.settle();
    }

    fn says(&self, words: &str) -> bool {
        self.h.has_text(words)
    }
}

#[test]
fn a_status_shows_on_the_tab_and_the_header_and_done_is_seen_when_the_tab_shows() {
    let mut a = app();
    a.key(Key::D, Mods::CTRL);
    a.feed(0, b"\x1b]7501;state=working:id=build\x07");
    let turning = |n: &quadra_tree::SemanticNode| {
        n.semantics.role == Role::ProgressBar && n.semantics.name == "Working"
    };
    assert_eq!(a.h.query(turning).len(), 2, "the tab's spinner and the header's, the kit's that turns");
    assert!(a.says("build working"), "the header's label: the record's id and its state");
    a.feed(0, b"\x1b]7501;state=done:id=build\x07");
    assert!(a.says("Done, result not yet seen"));
    a.key(Key::T, Mods::CTRL);
    a.feed(2, b"\x1b]0;other\x07");
    assert!(a.says("Done, result not yet seen"), "a tab that does not show keeps its done");
    a.h.get_by_role(Role::Tab, "Terminal").click();
    a.h.settle();
    assert!(!a.says("Done, result not yet seen"), "seen once the tab shows");
}

#[test]
fn progress_shows_on_the_tab_and_in_the_status_bar() {
    let mut a = app();
    a.key(Key::D, Mods::CTRL);
    a.feed(0, b"\x1b]9;4;1;40\x07");
    assert_eq!(a.h.query(|n| n.semantics.name == "40%").len(), 3, "the tab, the header, the meter");
    a.feed(0, b"\x1b]9;4;0\x07");
    assert!(!a.says("40%"));
}

#[test]
fn a_bell_counts_where_it_is_not_seen_and_is_seen_with_the_focus() {
    let mut a = app();
    a.feed(0, b"\x07");
    assert!(a.says("0 bells and notices"), "the space looked at hears its own bell");
    a.key(Key::D, Mods::CTRL);
    a.feed(0, b"\x07\x07");
    assert!(a.says("2 bells and notices"));
    a.key(Key::Left, Mods::CTRL | Mods::ALT);
    assert!(a.says("0 bells and notices"));
}

#[test]
fn a_notification_is_a_card_and_counts_until_looked_at() {
    let mut a = app();
    a.feed(0, b"\x1b]9;tests passed\x07");
    assert!(a.says("tests passed"), "the card says it");
    assert!(a.says("1 bells and notices"));
    // The status bar's count marks them seen (no button of notifications in the title bar).
    a.h.get_by_role(Role::Button, "1 bells and notices").click();
    a.h.settle();
    assert!(a.says("0 bells and notices"));
}

#[test]
fn a_header_names_the_shell_until_the_program_gives_a_title() {
    let mut a = app();
    assert!(!a.says("Command Prompt"), "a tab of one space has no header");
    a.key(Key::D, Mods::CTRL);
    // A newer console host says no title at the start: the shell's name stands for it.
    let names = |a: &App| a.h.query(|n| n.semantics.name == "Command Prompt").len();
    assert_eq!(names(&a), 2, "{}", a.h.semantics());
    a.feed(0, b"\x1b]0;C:\\WINDOWS\\System32\\cmd.exe - ls\x07");
    assert!(a.says("cmd.exe - ls") && names(&a) == 1, "the title, its path shortened");
}

#[test]
fn a_folded_rail_says_each_tab_s_signal_as_a_coloured_line_under_its_icon() {
    let mut a = app();
    a.key(Key::T, Mods::CTRL);
    a.key(Key::T, Mods::CTRL);
    let fold =
        a.h.query(|n| n.semantics.role == Role::Button && n.semantics.name == "Collapse tabs to icons");
    a.h.tree.act(fold[0], &quadra::SemanticAction::Click);
    a.h.settle();
    a.feed(0, b"\x1b]7501;state=blocked:id=deploy\x07");
    a.feed(1, b"\x1b]9;4;1;64\x07");
    // Tabs of 40 px in a rail of 40 (the touch step); each says its signal in its name.
    let tabs = a.h.query(|n| n.semantics.role == Role::Tab);
    let first = a.h.tree.window_rect(tabs[0]);
    assert_eq!((first.width, first.height), (39.0, 40.0), "40 with the line at its right");
    // A tab with no signal has no line under its icon: the icon is in the box's middle.
    let (quiet, icon) = (a.h.tree.window_rect(tabs[2]), a.h.tree.window_rect(a.h.tree.children(tabs[2])[1]));
    assert!((icon.center().y - (quiet.y + 19.5)).abs() <= 0.5, "{icon:?} in {quiet:?}");
    assert!(a.says("Terminal, blocked, needs permission"));
    assert!(a.says("Terminal, working, 64 percent"));
    assert_eq!(a.h.query(|n| n.semantics.role == Role::Tab && n.semantics.name == "Terminal").len(), 1);
    // Working with no progress: the line is the kit's busy bar, 22 by 4 under the icon.
    let busy = |a: &App| a.h.query(|n| n.semantics.role == Role::ProgressBar && n.semantics.name == "Busy");
    assert!(busy(&a).is_empty());
    a.feed(2, b"\x1b]7501;state=working:id=build\x07");
    let line = busy(&a);
    assert_eq!(line.len(), 1);
    let (bar, tab) = (a.h.tree.window_rect(line[0]), a.h.tree.window_rect(tabs[2]));
    assert_eq!((bar.width, bar.height), (22.0, 4.0));
    assert!(tab.contains(bar.center()) && bar.y > tab.y + 22.0, "under the icon");
    a.feed(2, b"\x1b]9;4;1;30\x07");
    assert!(busy(&a).is_empty() && a.says("Terminal, working, 30 percent"));
}

#[test]
fn a_folded_tab_s_tooltip_at_its_right_says_where_its_shell_is_and_what_it_runs() {
    let mut a = app();
    let fold =
        a.h.query(|n| n.semantics.role == Role::Button && n.semantics.name == "Collapse tabs to icons");
    a.h.tree.act(fold[0], &quadra::SemanticAction::Click);
    a.h.settle();
    a.feed(0, b"\x1b]7;file:///D:/work/api\x07");
    let cwd = a.made.borrow()[0].cwd().get_untracked();
    assert!(cwd.contains("work"), "{cwd}");
    let tab = a.h.tree.window_rect(a.h.query(|n| n.semantics.role == Role::Tab)[0]);
    let at = tab.center();
    a.h.pointer(quadra_tree::PointerInput::Move, at.x, at.y);
    a.h.advance(std::time::Duration::from_millis(500));
    a.h.settle();
    // Its words, then where the shell is and the program it started (the OS's shell).
    assert!(
        a.says("Terminal") && !a.says("idle") && a.says(&cwd) && a.says("cmd.exe"),
        "{}",
        a.h.semantics()
    );
    let tip = a.h.tree.window_rect(a.h.tree.overlays().next().expect("the tooltip"));
    assert!(tip.x >= tab.right(), "at the tab's right: {tip:?} {tab:?}");
}
