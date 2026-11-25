//! Where a new tab's shell starts (specs/oneterm, "Settings", Terminal): the last directory a local
//! shell said, kept in the settings, and home when it is gone.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Instant;

use oneterm::{Model, NewSession, OneTerm, keymap};
use quadra_term::{Session, echo};
use quadra_testing::Harness;

#[test]
fn the_last_directory_used_is_where_a_new_tab_starts_and_home_once_it_is_gone() {
    let mut h = Harness::new();
    h.tree.set_keymap(Rc::new(keymap()));
    let (made, starts) = (Rc::new(RefCell::new(Vec::new())), Rc::new(RefCell::new(Vec::new())));
    let (kept, told, built) = (made.clone(), starts.clone(), Rc::new(RefCell::new(None)));
    let sessions: NewSession = Rc::new(move |launch: oneterm::Launch| {
        told.borrow_mut().push(launch.cwd.clone());
        let s: Session = echo();
        kept.borrow_mut().push(s.clone());
        s
    });
    let tell = built.clone();
    h.mount(move || {
        let term = OneTerm::new(sessions);
        *tell.borrow_mut() = Some(term.model.clone());
        term.view()
    });
    h.settle();
    let model: Model = built.borrow_mut().take().unwrap();
    model.prefs().start.set(2);
    let here = std::path::absolute(".").unwrap();
    let said = format!("\x1b]9;9;{}\x07", here.display());
    made.borrow()[0].feeder().feed(said.as_bytes(), Instant::now());
    h.settle();
    assert_eq!(model.prefs().last_dir.get_untracked(), here.display().to_string(), "kept");
    // Another space somewhere else: a later tab starts where it was, not in the first's.
    model.new_tab();
    h.settle();
    assert_eq!(starts.borrow().last().cloned().flatten(), Some(here.clone()));
    model.prefs().last_dir.set(here.join("no such folder").display().to_string());
    model.new_tab();
    h.settle();
    let home = std::env::var_os("USERPROFILE").map(PathBuf::from);
    assert_eq!(starts.borrow().last().cloned().flatten(), home, "gone: home");
    // A bash of Windows says a folder its own way (OSC 7, `/d/work`): it is that folder.
    let shown = here.display().to_string();
    let (drive, under) = shown.split_once(":\\").expect("a drive");
    let said = format!("\x1b]7;file://pc/{}/{}\x07", drive.to_lowercase(), under.replace('\\', "/"));
    made.borrow().last().unwrap().feeder().feed(said.as_bytes(), Instant::now());
    h.settle();
    assert_eq!(model.prefs().last_dir.get_untracked(), shown);
    assert_eq!(made.borrow().last().unwrap().cwd().get_untracked(), shown);
}
