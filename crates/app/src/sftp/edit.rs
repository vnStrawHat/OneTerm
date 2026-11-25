//! Editing a remote file (specs/oneterm, "M9 in detail"): it is downloaded to the app's data
//! folder, then opened in a space of OneTerm (`crate::editing`), or in the editor the settings
//! name and sent back each time it is saved.

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, SystemTime};

use oneterm_ssh::{Entry, join, safe_name};
use quadra::{AppDirs, interval, use_context};
use quadra_design::{Notice, Tone};
use quadra_shell::Opener;

use super::{Dock, store};
use crate::editing::Content;
use crate::model::Model;

pub const EDITORS: [&str; 3] = ["OneTerm's editor", "System default app", "Custom program"];

/// What editing needs from where the view was built: the folder and the opener.
#[derive(Clone)]
pub(crate) struct Editing {
    folder: Option<PathBuf>,
    opener: Option<Rc<dyn Opener>>,
}

impl Editing {
    pub fn here() -> Editing {
        let folder = use_context::<Rc<AppDirs>>().map(|dirs| dirs.data.join("sftp-edit"));
        Editing { folder, opener: use_context::<Rc<dyn Opener>>() }
    }
}

fn card(model: &Model, body: String, tone: Tone) {
    model.notify(Notice::new("SFTP").body(body).tone(tone));
}

impl Dock {
    /// Whether a file is larger than editing takes without asking (0: no limit).
    pub fn too_large(&self, model: &Model, entry: &Entry) -> bool {
        let limit = model.prefs().edit_limit_kb.get_untracked().max(0.0) as u64 * 1024;
        limit > 0 && entry.size > limit
    }

    /// Downloads a file of the folder shown and opens it: in a space of the tab, or in the
    /// editor the settings name, sending it back when it is saved.
    pub fn edit(&self, model: &Model, entry: &Entry, editing: &Editing) {
        // In OneTerm: the space the dock shows is where it opens, and whose connection saves it.
        let inside = match model.prefs().editor.get_untracked() {
            0 => Some(crate::editing::source(model)),
            _ => None,
        };
        let Some(folder) = editing.folder.clone().filter(|_| safe_name(&entry.name)) else {
            return card(model, format!("{} cannot be edited here.", entry.name), Tone::Warning);
        };
        let remote = join(&self.dir.get_untracked(), &entry.name);
        let local = folder.join(format!("{:x}", hash(&remote))).join(&entry.name);
        let transfer = self.start(&entry.name, false, folder.display().to_string());
        let (done, whole, cancel) = (transfer.done.clone(), transfer.whole.clone(), transfer.cancel.clone());
        let (into, editing, opens, reads) = (local.clone(), editing.clone(), model.clone(), inside.is_some());
        self.with_files(
            model,
            move |files| {
                std::fs::create_dir_all(into.parent().unwrap_or(Path::new(".")))
                    .map_err(|e| e.to_string())?;
                files.download(&remote, &into, |d, w| store(&done, &whole, d, w), &cancel)?;
                // Read here, on the pool: whether it is text is known before it shows.
                let read = reads.then(|| std::fs::read(&into).map(Content::of).map_err(|e| e.to_string()));
                Ok((remote, read.transpose()?))
            },
            move |dock, got| {
                transfer.end.set(Some(got.as_ref().map(|_| ()).map_err(Clone::clone)));
                match (got, inside) {
                    (Ok((remote, Some(content))), Some(Some(from))) => {
                        crate::editing::open(&opens, from, local, remote, content)
                    }
                    (Ok((remote, _)), None) => dock.open_and_watch(&opens, local, remote, &editing),
                    _ => {}
                }
            },
        );
    }

    fn open_and_watch(&self, model: &Model, local: PathBuf, remote: String, editing: &Editing) {
        let p = model.prefs();
        let program = p.editor_program.get_untracked();
        let opened = match (&editing.opener, p.editor.get_untracked(), program.trim()) {
            (None, ..) => false,
            (Some(opener), 2, program) if !program.is_empty() => {
                let args: Vec<String> =
                    p.editor_args.get_untracked().split_whitespace().map(Into::into).collect();
                opener.open_with(Path::new(program), &args, &local)
            }
            (Some(opener), ..) => opener.open(&local.display().to_string()),
        };
        if !opened {
            return card(model, format!("{} could not be opened.", local.display()), Tone::Warning);
        }
        // Saved again: sent back. ponytail: a poll of the time it was written every second,
        // through the focused space's connection; a watch of the folder if many are edited.
        let (dock, model, seen) = (*self, model.clone(), std::cell::Cell::new(written(&local)));
        self.owner.run(|| {
            let _ = interval(Duration::from_secs(1), move || {
                let now = written(&local);
                if now.is_none() || now == seen.get() {
                    return;
                }
                seen.set(now);
                dock.send_back(&model, local.clone(), remote.clone());
            });
        });
    }

    fn send_back(&self, model: &Model, local: PathBuf, remote: String) {
        let name = local.file_name().map(|it| it.to_string_lossy().into_owned()).unwrap_or_default();
        let transfer = self.start(&name, true, remote.clone());
        let (done, whole, cancel) = (transfer.done.clone(), transfer.whole.clone(), transfer.cancel.clone());
        let (tells, again) = (model.clone(), model.clone());
        self.with_files(
            model,
            move |files| files.upload(&local, &remote, |d, w| store(&done, &whole, d, w), &cancel),
            move |dock, got| {
                match &got {
                    Ok(()) => card(&tells, format!("{name} saved to the server."), Tone::Success),
                    Err(why) => card(&tells, format!("{name} was not saved: {why}"), Tone::Warning),
                }
                transfer.end.set(Some(got));
                dock.refresh(&again);
            },
        );
    }
}

fn written(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|it| it.modified()).ok()
}

/// A folder a remote path: two files of one name stay apart.
fn hash(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3))
}
