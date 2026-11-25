//! OneTerm's settings of an older shape brought to today's (Quadra's specs/15, "Settings").

use quadra::Settings;

/// - Version 1: settings of version 0 kept a hash of the master password, and a vault and
///   secrets of OneTerm's first format, which nothing opens any more (specs/oneterm, "Saved
///   passwords"): they are forgotten, the hash above all. What is of today's format stays.
/// - Version 2: the same settings written as objects in objects (Quadra's specs/15); the file is
///   written so at once.
/// - Version 3: a host's startup command was its own words, and is the name of a command of
///   the list (specs/oneterm, "Startup commands"): each becomes one, named by its words.
pub(super) fn migrate(s: &Settings) {
    s.migrate(3, |from, all| {
        if from < 1 {
            let security = all.get_mut("security").and_then(|it| it.as_object_mut());
            if security.is_some_and(|it| it.remove("password").is_some() && it.is_empty()) {
                all.remove("security");
            }
        }
        let Some(ssh) = all.get_mut("ssh").and_then(|it| it.as_object_mut()) else { return };
        if from < 1 {
            let vault = ssh.get("vault").and_then(|it| it.as_str());
            if vault.is_some_and(|it| crate::vault::Header::parse(it).is_none()) {
                ssh.remove("vault");
            }
            if let Some(rows) = ssh.get_mut("secrets").and_then(|it| it.as_array_mut()) {
                rows.retain(|row| {
                    row.get(1).and_then(|it| it.as_str()).is_some_and(|it| it.starts_with("otv1:"))
                });
            }
        }
        let chosen = ssh.get("startup").and_then(|it| it.as_array()).into_iter().flatten();
        let mut commands = Vec::new();
        for words in chosen.filter_map(|row| row.get(1).cloned()) {
            let command = vec![words.clone(), words];
            if !commands.contains(&command) {
                commands.push(command);
            }
        }
        if !commands.is_empty() && !ssh.contains_key("startup_commands") {
            ssh.insert("startup_commands".into(), commands.into());
        }
    });
}
