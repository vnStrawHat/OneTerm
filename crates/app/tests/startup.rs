//! Startup commands (specs/oneterm, "Startup commands"): kept by name on the SSH & hosts page, and
//! what a host's own command of before becomes.

use std::rc::Rc;

use oneterm::{NewSession, OneTerm, keymap};
use quadra::{Role, Settings, provide};
use quadra_term::echo;
use quadra_testing::Harness;

type Rows = Vec<Vec<String>>;

/// A window over `settings`, its Settings open at SSH & hosts.
fn app(settings: &Rc<Settings>) -> Harness {
    let mut h = Harness::new();
    h.size = quadra::Size::new(1200.0, 900.0);
    h.tree.set_keymap(Rc::new(keymap()));
    let (given, sessions): (_, NewSession) = (settings.clone(), Rc::new(|_| echo()));
    h.mount(move || {
        provide(given.clone());
        OneTerm::new(sessions).view()
    });
    h.settle();
    h.get_by_role(Role::Button, "Settings").click();
    h.settle();
    h.get_by_role(Role::TreeItem, "SSH & hosts").click();
    h.settle();
    h
}

fn rows(of: &[[&str; 2]]) -> Rows {
    of.iter().map(|it| it.iter().map(|it| it.to_string()).collect()).collect()
}

#[test]
fn commands_are_saved_by_name_edited_and_removed_on_the_ssh_page() {
    let settings = Settings::memory();
    let mut h = app(&settings);
    let saved = settings.value("ssh.startup_commands", Rows::new());
    let chosen = settings.value("ssh.startup", Rows::new());
    assert!(h.has_text("Startup commands") && h.has_text("No commands yet."), "{}", h.semantics());
    let save = |h: &mut Harness, name: &str, command: &str| {
        h.get_by_role(Role::TextInput, "Command name").set_value(name);
        h.get_by_role(Role::TextInput, "Command").set_value(command);
        h.settle();
        h.get_by_role(Role::Button, "Save command").click();
        h.settle();
    };
    save(&mut h, "Attach", "tmux attach");
    // With no name, a command is named by its own words.
    save(&mut h, "", "htop");
    assert_eq!(saved.get(), rows(&[["Attach", "tmux attach"], ["htop", "htop"]]));
    assert!(h.has_text("tmux attach") && !h.has_text("No commands yet."));
    // Edit puts it in the fields; saved under its name again, the command is replaced.
    h.get_by_role(Role::Button, "Edit Attach").click();
    h.settle();
    let holds = |h: &Harness, field: &str, words: &str| {
        let is = |n: &quadra_tree::SemanticNode| {
            n.semantics.role == Role::TextInput
                && n.semantics.name == field
                && n.semantics.value.as_deref() == Some(words)
        };
        h.query(is).len() == 1
    };
    assert!(holds(&h, "Command name", "Attach") && holds(&h, "Command", "tmux attach"), "{}", h.semantics());
    save(&mut h, "Attach", "tmux attach || tmux new");
    assert_eq!(saved.get(), rows(&[["Attach", "tmux attach || tmux new"], ["htop", "htop"]]));
    // Removed, the host that chose it runs nothing.
    chosen.set(rows(&[["prod", "Attach"], ["lab", "htop"]]));
    h.get_by_role(Role::Button, "Remove Attach").click();
    h.settle();
    assert_eq!((saved.get(), chosen.get()), (rows(&[["htop", "htop"]]), rows(&[["lab", "htop"]])));
}

#[test]
fn a_host_s_own_command_of_before_becomes_a_command_of_the_list() {
    let dir = std::path::absolute(".tmp/startup-migrate").unwrap();
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("settings.json");
    let old =
        r#"{"version":2,"ssh":{"startup":[["prod","tmux attach"],["lab","tmux attach"],["db","psql"]]}}"#;
    std::fs::write(&file, old).unwrap();
    let settings = Settings::open(&file);
    let h = app(&settings);
    // Each is named by its words, once; the hosts keep their choice.
    let saved = settings.value("ssh.startup_commands", Rows::new()).get();
    assert_eq!(saved, rows(&[["tmux attach", "tmux attach"], ["psql", "psql"]]));
    assert_eq!(settings.value("ssh.startup", Rows::new()).get().len(), 3);
    assert!(h.has_text("psql"), "{}", h.semantics());
}
