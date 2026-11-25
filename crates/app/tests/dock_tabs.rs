//! The SFTP dock (specs/oneterm, "M9 in detail"): each space keeps its own, and its left edge is
//! dragged to make it wider.

use quadra::Role;
use quadra_tree::PointerInput;

#[path = "support/dock_app.rs"]
mod support;
use support::{App, app};

#[test]
fn each_tab_keeps_its_dock() {
    let mut a = app("dock-tabs");
    a.wait(|a| a.entry("config.yaml"), "the home folder");
    a.clicks("src", 2);
    a.wait(|a| a.entry("Parent folder") && !a.entry("config.yaml"), "the folder src");
    a.clicks("Parent folder", 1);
    // The local tab, its dock opened: it says it is local.
    let tab = |a: &mut App, local: bool| {
        let found =
            a.h.query(|n| n.semantics.role == Role::Tab && n.semantics.name.starts_with("tester@") != local);
        let at = a.h.tree.window_rect(found[0]).center();
        a.h.click(at.x, at.y);
        a.h.settle();
    };
    tab(&mut a, true);
    a.h.get_by_role(Role::Button, "Toggle SFTP panel").click();
    a.h.settle();
    let local = a.h.has_text("local space \u{B7} n/a") && !a.entry("Parent folder");
    assert!(local, "{}", a.h.semantics());
    // The SSH tab again: its folder and its choice as they were, at once.
    tab(&mut a, false);
    assert!(a.entry("Parent folder") && !a.entry("config.yaml"), "src again\n{}", a.h.semantics());
    assert!(a.h.semantics().contains("Button \"Parent folder\" [selected]"), "{}", a.h.semantics());
}

#[test]
fn the_dock_is_as_wide_as_its_left_edge_is_dragged() {
    let mut a = app("dock-width");
    let dock = a.h.get_by_test_id("sftp-dock").node();
    let was = a.h.tree.window_rect(dock);
    assert_eq!(was.width, 460.0);
    // The strip lies over the edge: a press 2 px outside the dock takes it.
    let (x, y) = (was.x - 2.0, was.center().y);
    a.h.pointer(PointerInput::Down, x, y);
    a.h.pointer(PointerInput::Move, x - 100.0, y);
    a.h.pointer(PointerInput::Up, x - 100.0, y);
    a.h.settle();
    let now = a.h.tree.window_rect(dock);
    assert_eq!((now.width, now.right()), (560.0, was.right()), "wider to the left");
    a.h.pointer(PointerInput::Down, now.x, y);
    a.h.pointer(PointerInput::Move, now.x + 1000.0, y);
    a.h.pointer(PointerInput::Up, now.x + 1000.0, y);
    a.h.settle();
    assert_eq!(a.h.tree.window_rect(dock).width, 220.0, "never narrower than 220");
}
