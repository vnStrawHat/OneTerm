//! The SFTP dock's columns (specs/oneterm, "M9 in detail"): a header that sorts, each entry's
//! permissions, and the columns a narrow dock has no room for.

use quadra::Role;
use quadra_tree::PointerInput;

#[path = "support/dock_app.rs"]
mod support;
use support::{App, app};

/// Whether the row `first` is over the row `then`.
fn above(a: &App, first: &str, then: &str) -> bool {
    let y = |name: &str| {
        let all = a.h.query(|n| n.semantics.role == Role::Button && n.semantics.name == name);
        a.h.tree.window_rect(*all.last().unwrap()).y
    };
    y(first) < y(then)
}

#[test]
fn the_dock_has_a_header_that_sorts_and_says_each_entry_s_permissions() {
    let mut a = app("dock-columns");
    let src = a.served.join("home/src");
    std::fs::write(src.join("a.bin"), vec![0u8; 5000]).unwrap();
    std::fs::write(src.join("b.bin"), vec![0u8; 10]).unwrap();
    std::fs::create_dir_all(src.join("zz")).unwrap();
    a.wait(|a| a.entry("config.yaml"), "the home folder");
    a.clicks("src", 2);
    a.wait(|a| a.entry("a.bin") && a.entry("zz"), "the folder src");

    // A header of the columns, each a button that sorts by it.
    for column in ["Name", "Size", "Modified", "Permissions"] {
        assert!(a.entry(&format!("Sort by {column}")), "{column}\n{}", a.h.semantics());
    }
    // What the test's server says of a folder and of a file.
    assert!(a.h.has_text("drwxr-xr-x") && a.h.has_text("-rw-r--r--"), "{}", a.h.semantics());
    assert!(above(&a, "zz", "a.bin") && above(&a, "a.bin", "b.bin"), "by name, folders first");
    a.h.get_by_role(Role::Button, "Sort by Size").click();
    a.wait(|a| above(a, "b.bin", "a.bin") && a.h.has_text("Size \u{2191}"), "by size");
    a.h.get_by_role(Role::Button, "Sort by Size").click();
    a.wait(|a| above(a, "a.bin", "b.bin") && a.h.has_text("Size \u{2193}"), "the other way");
    assert!(above(&a, "zz", "a.bin"), "folders first still");
    // A folder has the wider permissions: last by them, the other way.
    a.h.get_by_role(Role::Button, "Sort by Permissions").click();
    a.h.get_by_role(Role::Button, "Sort by Permissions").click();
    a.wait(|a| a.h.has_text("Permissions \u{2193}") && above(a, "zz", "b.bin"), "by permissions");

    // The expanded view's Remote pane is in the dock's order and says the permissions too.
    a.h.get_by_role(Role::Button, "Zoom SFTP").click();
    a.h.settle();
    assert!(a.h.has_text("Permissions \u{2193}") && a.h.has_text("drwxr-xr-x"), "{}", a.h.semantics());
    a.h.get_by_role(Role::Button, "Collapse SFTP").click();
    a.h.settle();

    // Made narrow, the dock keeps the name and the size: the other columns have no room.
    let dock = a.h.get_by_test_id("sftp-dock").node();
    let was = a.h.tree.window_rect(dock);
    let (x, y) = (was.x - 2.0, was.center().y);
    a.h.pointer(PointerInput::Down, x, y);
    a.h.pointer(PointerInput::Move, x + 60.0, y);
    a.h.pointer(PointerInput::Up, x + 60.0, y);
    a.h.settle();
    assert!(a.entry("Sort by Modified") && !a.entry("Sort by Permissions"), "{}", a.h.semantics());
    assert!(!a.h.has_text("drwxr-xr-x"));
    let now = a.h.tree.window_rect(dock);
    a.h.pointer(PointerInput::Down, now.x, y);
    a.h.pointer(PointerInput::Move, now.x + 100.0, y);
    a.h.pointer(PointerInput::Up, now.x + 100.0, y);
    a.h.settle();
    assert!(a.entry("Sort by Size") && !a.entry("Sort by Modified"), "{}", a.h.semantics());
}
