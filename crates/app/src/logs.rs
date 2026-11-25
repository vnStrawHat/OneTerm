//! A space's log (specs/oneterm, "Settings"): new local shells and new SSH spaces as the settings
//! say, or from a space's menu; a file a space in the folder set, its lines in local time.

use std::path::PathBuf;
use std::rc::Rc;

use quadra::time::{DateTime, unix_now, utc_offset};
use quadra::{AppDirs, use_context};
use quadra_design::{Notice, Tone};
use quadra_term::Session;

use crate::model::Model;
use crate::prefs::Prefs;

pub const MODES: [&str; 2] = ["Append", "Overwrite"];

/// Where logs go: the folder set, else `session-logs` in the app's data folder; none where
/// the app has no folders and none is set.
pub(crate) fn folder(p: &Prefs) -> Option<PathBuf> {
    let set = p.log_folder.get_untracked();
    if !set.trim().is_empty() {
        return Some(PathBuf::from(set.trim()));
    }
    use_context::<Rc<AppDirs>>().map(|dirs| dirs.data.join("session-logs"))
}

/// `name_2026-11-15_14-07-09.log`, the name kept to letters, digits, `-`, `_` and `.`.
fn file_name(name: &str, at: DateTime) -> String {
    let name: String =
        name.chars().map(|c| if c.is_alphanumeric() || "-_.".contains(c) { c } else { '_' }).collect();
    let (d, t) = (at.date, at.time);
    format!("{name}_{}-{:02}-{:02}_{:02}-{:02}-{:02}.log", d.year, d.month, d.day, t.hour, t.minute, t.second)
}

impl Model {
    /// Logs a space's output to a new file in `folder`; what fails is a card.
    pub(crate) fn start_log(&self, session: &Session, name: &str, folder: Option<PathBuf>) {
        let offset = utc_offset();
        let Some(folder) = folder else {
            self.notify(Notice::new("Logging").body("No folder is set for logs.").tone(Tone::Warning));
            return;
        };
        let path = folder.join(file_name(name, DateTime::from_unix(unix_now(), offset)));
        let append = self.prefs().log_mode.get_untracked() == 0;
        if let Err(why) =
            session.start_log(&path, append, move || DateTime::from_unix(unix_now(), offset).to_string())
        {
            let body = format!("{}: {why}", path.display());
            self.notify(Notice::new("Logging").body(body).tone(Tone::Warning));
        }
    }
}

#[cfg(test)]
#[test]
fn names_of_log_files() {
    let at = DateTime::from_unix(1_763_215_629, 0);
    assert_eq!(file_name("prod api", at), "prod_api_2025-11-15_14-07-09.log");
}
