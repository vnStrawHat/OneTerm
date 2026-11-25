//! What the dock does to the entries chosen and to the shell (specs/oneterm, "M9 in detail"): the
//! entries chosen, deleting them, a mode set, and Open in terminal.

use oneterm_ssh::{Entry, join};
use quadra::untrack;

use super::Dock;
use crate::model::Model;

impl Dock {
    /// The entries chosen that the folder shown has.
    pub fn chosen(&self) -> Vec<Entry> {
        let names = self.picked.names();
        self.entries.with_untracked(|all| all.iter().filter(|it| names.contains(&it.name)).cloned().collect())
    }

    /// Removes entries of the folder shown, each in turn; those that could not be are said
    /// in one card, and the folder is listed again either way.
    pub fn delete(&self, model: &Model, entries: Vec<Entry>) {
        let dir = self.dir.get_untracked();
        let paths: Vec<(String, bool)> = entries.iter().map(|it| (join(&dir, &it.name), it.dir)).collect();
        let again = model.clone();
        self.with_files(
            model,
            move |files| {
                let failed: Vec<String> = paths
                    .iter()
                    .filter_map(|(path, dir)| {
                        files.remove(path, *dir).err().map(|why| format!("{path}: {why}"))
                    })
                    .collect();
                if failed.is_empty() { Ok(()) } else { Err(failed.join("\n")) }
            },
            move |dock, got| {
                if got.is_err() {
                    dock.refresh(&again);
                }
                dock.after(&again, got);
            },
        );
    }

    /// Sets the permission bits of a path, then lists the folder again.
    pub fn chmod(&self, model: &Model, path: String, mode: u32) {
        let again = model.clone();
        self.with_files(
            model,
            move |files| files.chmod(&path, mode),
            move |dock, got| dock.after(&again, got),
        );
    }

    /// Open in terminal: types `cd` to a folder of the server, and Enter, into the focused
    /// space's shell.
    pub fn cd(&self, model: &Model, path: &str) {
        let Some(space) = untrack(|| model.active_tab().and_then(|tab| tab.focused())) else { return };
        space.session.write(format!("cd -- {}\r", quoted(path)).as_bytes());
    }
}

/// A path in single quotes for a POSIX shell: a quote in it is `'\''`.
fn quoted(path: &str) -> String {
    format!("'{}'", path.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_is_quoted_for_the_shell() {
        assert_eq!(quoted("/srv/my files"), "'/srv/my files'");
        assert_eq!(quoted("/srv/it's"), r"'/srv/it'\''s'");
    }
}
