//! A space's lines are read by its shell (specs/oneterm, "Spaces"; Quadra's specs/18, "Semantic
//! highlighting"): Command Prompt's, PowerShell's, else a Unix shell's.

use std::cell::RefCell;
use std::rc::Rc;

use oneterm::{Launch, Model, NewSession, OneTerm, Shell, keymap};
use quadra_term::{Session, ShellProfile, echo};
use quadra_testing::Harness;

#[test]
fn a_space_is_read_by_the_shell_it_started() {
    let mut h = Harness::new();
    h.tree.set_keymap(Rc::new(keymap()));
    let (made, built) = (Rc::new(RefCell::new(Vec::<Session>::new())), Rc::new(RefCell::new(None)));
    let (kept, tell) = (made.clone(), built.clone());
    let sessions: NewSession = Rc::new(move |_| {
        let s = echo();
        kept.borrow_mut().push(s.clone());
        s
    });
    h.mount(move || {
        let term = OneTerm::new(sessions);
        *tell.borrow_mut() = Some(term.model.clone());
        term.view()
    });
    h.settle();
    let model: Model = built.borrow_mut().take().unwrap();
    for program in [r"C:\Program Files\PowerShell\7\pwsh.exe", r"C:\Program Files\Git\bin\bash.exe"] {
        let shell = Shell { name: "test", program: program.into(), args: Vec::new() };
        model.new_tab_with(Launch { shell: Some(shell), ..model.launch(None) });
        h.settle();
    }
    let read: Vec<ShellProfile> = made.borrow().iter().map(Session::shell_profile).collect();
    assert_eq!(
        read,
        [ShellProfile::Cmd, ShellProfile::PowerShell, ShellProfile::Unix],
        "the OS's shell first"
    );
}
