//! An upload onto a name the folder shown already has asks first (specs/oneterm, "M9 in
//! detail"), on a real connection to the engine's test server on 127.0.0.1: Skip leaves what
//! is there, Replace sends over it, and what was not there goes without a question.

use std::path::Path;
use std::rc::Rc;

use quadra::{DroppedFiles, Role};

#[path = "support/dock_app.rs"]
mod support;
use support::{App, app};

fn drop_on_dock(a: &mut App, paths: &[&Path]) {
    let dock = a.h.query(|n| n.semantics.name == "SFTP" && n.semantics.role != Role::Button)[0];
    let at = a.h.tree.window_rect(dock).center();
    let files = paths.iter().map(|it| it.to_path_buf()).collect();
    a.h.tree.os_drag_enter(Rc::new(DroppedFiles(files)), at);
    a.h.tree.os_drop(at);
    a.h.settle();
}

#[test]
fn an_upload_onto_a_name_that_is_there_asks_first() {
    let mut a = app("dock-replace");
    a.wait(|a| a.entry("config.yaml"), "the home folder");
    let (mine, new) = (a.downloads.join("config.yaml"), a.downloads.join("fresh.txt"));
    std::fs::write(&mine, b"mine").unwrap();
    std::fs::write(&new, b"fresh").unwrap();
    // A name that does not show while hidden files are not shown is there all the same.
    let (dot, kept) = (a.downloads.join(".env"), a.served.join("home/.env"));
    std::fs::write(&dot, b"mine").unwrap();
    std::fs::write(&kept, b"the server's").unwrap();
    a.h.get_by_test_id("sftp-path").set_value("/home/src");
    a.h.settle();
    a.h.get_by_role(Role::Button, "Go to the typed path").click();
    a.wait(|a| !a.entry("config.yaml"), "another folder");
    a.h.get_by_test_id("sftp-path").set_value("/home");
    a.h.settle();
    a.h.get_by_role(Role::Button, "Go to the typed path").click();
    a.wait(|a| a.entry("config.yaml"), "the folder listed again");
    assert!(!a.entry(".env"));
    drop_on_dock(&mut a, &[&dot]);
    assert!(a.h.has_text(".env is already in /home. Replace it on the server? It cannot be undone."));
    a.h.get_by_role(Role::Button, "Skip").click();
    a.h.settle();
    assert_eq!(std::fs::read(&kept).unwrap(), b"the server's");
    let (there, sent) = (a.served.join("home/config.yaml"), a.served.join("home/fresh.txt"));

    // Both dropped: the one not there goes at once, the other waits for the answer.
    drop_on_dock(&mut a, &[&mine, &new]);
    assert!(a.h.has_text("config.yaml is already in /home. Replace it on the server? It cannot be undone."));
    a.wait(|_| sent.is_file(), "the file that was not there uploaded");
    assert_eq!(std::fs::read(&there).unwrap().len(), 2458, "nothing went over it yet");
    a.h.get_by_role(Role::Button, "Skip").click();
    a.wait(|a| a.h.has_text("Uploading fresh.txt") && a.h.has_text("done"), "the one transfer done");
    assert!(!a.h.has_text("Uploading config.yaml"));
    assert_eq!(std::fs::read(&there).unwrap().len(), 2458, "Skip sent nothing over it");

    // Asked again, and Replace sends it.
    drop_on_dock(&mut a, &[&mine]);
    a.h.get_by_role(Role::Button, "Replace").click();
    a.wait(|_| std::fs::read(&there).is_ok_and(|it| it == b"mine"), "the file replaced");

    // The expanded view's Upload selected asks the same.
    std::fs::write(&mine, b"again").unwrap();
    a.h.get_by_role(Role::Button, "Zoom SFTP").click();
    a.h.settle();
    a.h.get_by_role(Role::Button, "Refresh local").click();
    let both = |a: &App| {
        a.h.query(|n| n.semantics.role == Role::Button && n.semantics.name == "config.yaml").len() == 2
    };
    a.wait(both, "the file on both sides");
    let local = a.h.query(|n| n.semantics.role == Role::Button && n.semantics.name == "config.yaml");
    let left =
        local.into_iter().min_by(|x, y| a.h.tree.window_rect(*x).x.total_cmp(&a.h.tree.window_rect(*y).x));
    let at = a.h.tree.window_rect(left.unwrap()).center();
    a.h.click(at.x, at.y);
    a.h.settle();
    a.h.get_by_role(Role::Button, "Upload selected").click();
    a.h.settle();
    assert!(a.h.has_text("config.yaml is already in /home. Replace it on the server? It cannot be undone."));
    a.h.get_by_role(Role::Button, "Skip").click();
    a.h.settle();
    assert_eq!(std::fs::read(&there).unwrap(), b"mine");
}
