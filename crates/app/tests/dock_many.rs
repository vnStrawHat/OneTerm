//! More of the SFTP dock (specs/oneterm, "M9 in detail"), on a real connection to the engine's
//! test server on 127.0.0.1: several rows chosen with Ctrl and Shift, downloaded and deleted
//! together; Open in terminal; a mode set from Properties; the two switches kept.

use quadra::{Mods, MouseButton, Role};
use quadra_tree::PointerInput::{Down, Move, Up};

#[path = "support/dock_app.rs"]
mod support;
use support::{App, app};

/// A click on an entry with keys held.
fn click(a: &mut App, name: &str, mods: Mods) {
    let node = a.h.get_by_role(Role::Button, name).node();
    let at = a.h.tree.window_rect(node).center();
    for input in [Move, Down, Up] {
        let now = a.h.now();
        a.h.tree.pointer(input, MouseButton::Left, at, mods, now);
    }
    a.h.settle();
}

#[test]
fn several_rows_are_chosen_downloaded_and_deleted_together() {
    let mut a = app("dock-many");
    a.wait(|a| a.entry("config.yaml"), "the home folder");
    for name in ["a.txt", "b.txt", "c.txt"] {
        std::fs::write(a.served.join("home").join(name), name).unwrap();
    }
    a.h.get_by_role(Role::Button, "Refresh remote").click();
    a.wait(|a| a.entry("a.txt") && a.entry("c.txt"), "listed again");

    // A click, then Shift and a click two rows on: the three between.
    click(&mut a, "a.txt", Mods::NONE);
    click(&mut a, "c.txt", Mods::SHIFT);
    a.menu("b.txt", "Download 3 items");
    let got = |a: &App, name: &str| a.downloads.join(name).is_file();
    a.wait(|a| got(a, "a.txt") && got(a, "b.txt") && got(a, "c.txt"), "the three downloaded");

    // Ctrl adds a row and takes one out; a row not among them keeps its own menu.
    click(&mut a, "config.yaml", Mods::CTRL);
    click(&mut a, "b.txt", Mods::CTRL);
    a.menu("a.txt", "Delete 3 items\u{2026}");
    assert!(a.h.has_text("Delete 3 entries from the server? It cannot be undone."), "{}", a.h.semantics());
    a.h.get_by_role(Role::Button, "Delete").click();
    let there = |a: &App, name: &str| a.served.join("home").join(name).exists();
    a.wait(
        |a| !there(a, "a.txt") && !there(a, "c.txt") && !there(a, "config.yaml") && !a.entry("a.txt"),
        "the three deleted",
    );
    assert!(there(&a, "b.txt") && a.entry("b.txt"), "the one taken out stays");
    a.menu("b.txt", "Delete\u{2026}");
    assert!(a.h.has_text("Delete the file b.txt from the server? It cannot be undone."));
}

#[test]
fn a_folder_opens_in_the_terminal_a_mode_is_set_and_the_switches_are_kept() {
    let mut a = app("dock-acts");
    a.wait(|a| a.entry("config.yaml") && a.entry("src"), "the home folder");
    // Open in terminal types `cd` and Enter into the shell (the test's server says it back).
    a.menu("src", "Open in terminal");
    let session = a.model.active_tab().unwrap().focused().unwrap().session;
    a.wait(|_| session.screen_text().join("\n").contains("cd -- '/home/src'"), "the cd typed");

    // Properties: the boxes are the entry's permissions; unticked, the line reads them, and
    // Apply sets them.
    a.menu("config.yaml", "Properties\u{2026}");
    a.wait(|a| a.h.has_text("-rw-r--r-- (0644)"), "the dialog");
    for id in ["mode-gr", "mode-or"] {
        a.h.get_by_test_id(id).click();
        a.h.settle();
    }
    assert!(a.h.has_text("-rw------- (0600)"), "read back: {}", a.h.semantics());
    a.h.get_by_role(Role::Button, "Apply").click();
    a.wait(|a| !a.h.has_text("-rw-r--r-- (0644)") && a.h.has_text("-rw-------"), "the mode in the list");

    // What the options were left as is in the settings.
    let prefs = a.model.prefs();
    assert!(!prefs.dock_hidden.get_untracked() && !prefs.dock_follow.get_untracked());
    for name in ["Show hidden files", "Sync CWD"] {
        a.h.get_by_role(Role::Button, "Remote options").click();
        a.h.settle();
        a.h.get_by_role(Role::MenuItem, name).click();
        a.h.settle();
    }
    assert!(prefs.dock_hidden.get_untracked() && prefs.dock_follow.get_untracked());
}
