//! The bar after the dock's path (specs/oneterm, "M9 in detail"), on a real connection to the
//! engine's test server on 127.0.0.1: Go, Refresh and the options, whose Show hidden files
//! and Sync CWD the list follows.

use quadra::Role;

#[path = "support/dock_app.rs"]
mod support;
use support::{App, app};

#[test]
fn after_the_path_stand_refresh_and_the_options_hidden_files_and_sync_cwd() {
    let mut a = app("dock-bar");
    a.wait(|a| a.entry("config.yaml"), "the home folder");
    // What came to the folder since shows once it is listed again; a name with a dot does not.
    std::fs::write(a.served.join("home/late.txt"), b"x").unwrap();
    std::fs::write(a.served.join("home/.env"), b"x").unwrap();
    a.h.get_by_role(Role::Button, "Refresh remote").click();
    a.wait(|a| a.entry("late.txt") && !a.entry(".env"), "listed again");
    // Go, Refresh and the options, in that order after the path.
    let x = |a: &mut App, name: &str| {
        let node = a.h.get_by_role(Role::Button, name).node();
        a.h.tree.window_rect(node).x
    };
    let (go, again, more) =
        (x(&mut a, "Go to the typed path"), x(&mut a, "Refresh remote"), x(&mut a, "Remote options"));
    assert!(go < again && again < more, "{go} {again} {more}");
    let option = |a: &mut App, name: &str| {
        a.h.get_by_role(Role::Button, "Remote options").click();
        a.h.settle();
        a.h.get_by_role(Role::MenuItem, name).click();
        a.h.settle();
    };
    option(&mut a, "Show hidden files");
    a.wait(|a| a.entry(".env"), "hidden files");
    // Sync CWD: the folder goes where the focused space's shell says it is (the test's server
    // says back the OSC 7 it is typed).
    option(&mut a, "Sync CWD");
    let session = a.model.active_tab().unwrap().focused().unwrap().session;
    session.write(b"\x1b]7;file://srv/home/src\x07");
    a.wait(|a| a.entry("Parent folder") && !a.entry("config.yaml"), "the shell's folder");
}
