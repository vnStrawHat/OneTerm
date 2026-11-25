//! Splits (specs/oneterm, "Spaces"): a tab that arranges itself beside the rail and in the strip
//! until its layout is the user's; a space split to each side, nested; the spaces in the
//! tree's reading order; a run along one axis shared evenly; a seam dragged; a close.

use std::cell::RefCell;
use std::rc::Rc;

use oneterm::{NewSession, OneTerm, keymap};
use quadra::{Key, Mods, MouseButton, Point, Rect, Role};
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
    fn key(&mut self, key: Key, mods: Mods) {
        self.h.key(key, mods);
        self.h.settle();
    }

    fn split(&mut self, key: Key) {
        self.key(key, Mods::CTRL | Mods::SHIFT);
    }

    /// The terminals' boxes, in the order of the tree.
    fn rects(&self) -> Vec<Rect> {
        let ids = self.h.query(|n| n.semantics.role == Role::TextInput && n.semantics.name == "Terminal");
        ids.into_iter().map(|id| self.h.tree.window_rect(id)).collect()
    }

    /// Types a word, and says which session got it.
    fn typed_in(&mut self, word: &str) -> usize {
        self.h.type_text(word);
        self.h.settle();
        let made = self.made.borrow();
        made.iter().position(|s| s.screen_text()[0].ends_with(word)).expect("a space got it")
    }

    fn pointer(&mut self, input: PointerInput, at: Point) {
        self.h.tree.pointer(input, MouseButton::Left, at, Mods::NONE, self.h.now());
        self.h.settle();
    }

    /// The seam right of the first space, dragged by `by` px.
    fn drag_first_seam(&mut self, by: f32) {
        let r = self.rects();
        let at = Point::new((r[0].right() + r[1].x) / 2.0, r[0].center().y);
        self.pointer(PointerInput::Down, at);
        self.pointer(PointerInput::Move, Point::new(at.x + by / 2.0, at.y));
        self.pointer(PointerInput::Move, Point::new(at.x + by, at.y));
        self.pointer(PointerInput::Up, Point::new(at.x + by, at.y));
    }

    /// The tabs from the rail into the strip (the settings' switch), or back.
    fn rail(&mut self, rail: bool) {
        if rail {
            self.h.get_by_role(Role::Button, "Switch to vertical tabs").click();
            self.h.settle();
            return;
        }
        self.h.get_by_role(Role::Button, "Settings").click();
        self.h.settle();
        self.h.get_by_role(Role::TreeItem, "Tabs & spaces").click();
        self.h.settle();
        let switch = self.h.query(|n| n.semantics.role == Role::Switch)[0];
        self.h.tree.act(switch, &quadra::SemanticAction::Click);
        self.h.get_by_role(Role::Button, "Done").click();
        self.h.settle();
    }
}

/// The first space on the left, the others stacked at its right.
fn stacked(r: &[Rect]) -> bool {
    near(r[1].x, r[2].x)
        && r[1].x > r[0].right()
        && r[2].y > r[1].bottom()
        && r[0].height > r[1].height + 100.0
}

/// Side by side in equal columns.
fn in_a_row(r: &[Rect]) -> bool {
    r.windows(2).all(|p| p[1].x > p[0].right() && near(p[0].y, p[1].y) && near(p[0].width, p[1].width))
}

#[test]
fn a_tab_arranges_itself_until_its_layout_is_the_users() {
    let mut a = app();
    a.key(Key::D, Mods::CTRL);
    assert!(in_a_row(&a.rects()), "two side by side");
    a.key(Key::D, Mods::CTRL);
    assert_eq!(a.typed_in("c"), 2, "the new space has the focus");
    let r = a.rects();
    assert!(stacked(&r) && near(r[0].width, r[1].width), "beside the rail: {r:?}");
    a.rail(false);
    assert!(in_a_row(&a.rects()), "in the strip: {:?}", a.rects());
    a.rail(true);
    assert!(stacked(&a.rects()));

    // A seam dragged: the layout is the user's, and Split space splits to the right.
    let before = a.rects();
    a.drag_first_seam(100.0);
    a.key(Key::D, Mods::CTRL);
    assert_eq!(a.typed_in("d"), 3);
    let r = a.rects();
    assert!(near(r[0].width, before[0].width + 100.0) && near(r[1].x, r[2].x), "{r:?}");
    assert!(r[3].x > r[2].right() && near(r[3].y, r[2].y) && near(r[3].height, r[2].height), "{r:?}");
    a.rail(false);
    assert!(near(a.rects()[1].x, a.rects()[2].x), "the strip leaves it as it is");
    a.rail(true);

    // Back at one space, the tab arranges itself again.
    for _ in 0..3 {
        a.key(Key::W, Mods::CTRL);
    }
    a.key(Key::D, Mods::CTRL);
    a.key(Key::D, Mods::CTRL);
    assert!(stacked(&a.rects()), "{:?}", a.rects());
}

/// The same to a few pixels: the seams' strips of 6 px come off the sides after them.
fn near(a: f32, b: f32) -> bool {
    (a - b).abs() <= 8.0
}

#[test]
fn a_space_splits_to_each_side_and_the_order_is_the_trees() {
    let mut a = app();
    a.split(Key::Right); // 1 at the right of 0
    assert_eq!(a.typed_in("b"), 1, "the new space has the focus");
    let r = a.rects();
    assert!(r[1].x > r[0].right() && near(r[1].y, r[0].y) && near(r[1].width, r[0].width));
    a.split(Key::Down); // 2 under 1
    assert_eq!(a.typed_in("c"), 2);
    let r = a.rects();
    assert!(near(r[2].x, r[1].x) && r[2].y > r[1].bottom() && r[0].height > r[1].height + 100.0);
    a.split(Key::Left); // 3 at the left of 2
    assert_eq!(a.typed_in("d"), 3);
    a.split(Key::Up); // 4 over 3
    assert_eq!(a.typed_in("e"), 4);
    // 0 | (1 over ((4 over 3) | 2)): in reading order 0, 1, 4, 3, 2.
    let r = a.rects();
    assert!(r[1].bottom() < r[2].y && near(r[2].x, r[3].x) && r[2].bottom() < r[3].y, "{r:?}");
    assert!(r[4].x > r[3].right() && near(r[4].y, r[2].y) && r[4].x > r[0].right(), "{r:?}");
    // The focus keys go in that order: from 4 back to 1, then on past 4 to 3.
    a.key(Key::Left, Mods::CTRL | Mods::ALT);
    assert_eq!(a.typed_in("f"), 1);
    a.key(Key::Right, Mods::CTRL | Mods::ALT);
    a.key(Key::Right, Mods::CTRL | Mods::ALT);
    assert_eq!(a.typed_in("g"), 3);
}

#[test]
fn a_run_of_splits_is_shared_evenly_a_seam_drags_and_a_close_evens_again() {
    let mut a = app();
    a.split(Key::Right);
    a.split(Key::Right);
    let r = a.rects();
    assert!(near(r[0].width, r[1].width) && near(r[1].width, r[2].width), "thirds: {r:?}");

    // The seam between the first two, dragged 100 px to the right.
    a.drag_first_seam(100.0);
    let dragged = a.rects();
    assert!(near(dragged[0].width, r[0].width + 100.0), "{dragged:?}");

    // The middle one closed: the two left share the room.
    a.h.click(dragged[1].center().x, dragged[1].center().y);
    a.h.settle();
    a.key(Key::W, Mods::CTRL);
    assert!(a.h.has_text("2 local \u{B7} 2 spaces"));
    let r = a.rects();
    assert!(near(r[0].width, r[1].width), "halves: {r:?}");
    assert_eq!(a.typed_in("x"), 2, "the focus goes to the one after");
}

#[test]
fn the_menu_splits_down_and_the_keys_are_on_the_page() {
    let mut a = app();
    let r = a.rects()[0];
    for input in [PointerInput::Move, PointerInput::Down, PointerInput::Up] {
        a.h.tree.pointer(input, MouseButton::Right, r.center(), Mods::NONE, a.h.now());
    }
    a.h.settle();
    a.h.get_by_role(Role::MenuItem, "Split down").click();
    a.h.settle();
    let r = a.rects();
    assert!(near(r[0].x, r[1].x) && r[1].y > r[0].bottom());

    a.h.get_by_role(Role::Button, "Settings").click();
    a.h.settle();
    a.h.get_by_role(Role::TreeItem, "Keybindings").click();
    a.h.settle();
    for name in ["Split right", "Split left", "Split up", "Split down"] {
        a.h.get_by_role(Role::Button, &format!("{name} keys"));
    }
}

#[test]
fn a_press_in_another_space_focuses_what_was_pressed() {
    let mut a = app();
    a.key(Key::D, Mods::CTRL);
    a.key(Key::Left, Mods::CTRL | Mods::ALT);
    a.key(Key::F, Mods::CTRL); // the find bar of the first
    a.key(Key::Right, Mods::CTRL | Mods::ALT);
    // The first's query, then its terminal, pressed from the second.
    let query = a.h.get_by_role(Role::TextInput, "Find").node();
    let at = a.h.tree.window_rect(query).center();
    a.h.click(at.x, at.y);
    a.h.settle();
    assert_eq!(a.h.tree.focused(), Some(query), "the query keeps the press's focus");
    a.key(Key::Right, Mods::CTRL | Mods::ALT);
    let at = a.rects()[0].center();
    a.h.click(at.x, at.y);
    a.h.settle();
    assert_eq!(a.typed_in("t"), 0, "the terminal pressed has the keys");
}
