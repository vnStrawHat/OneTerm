//! Uploads from the dock (specs/oneterm, "M9 in detail"): local files and folders' trees to the
//! folder shown, and the question before one goes over a name the folder already has.

use std::path::{Path, PathBuf};

use quadra::{View, text};
use quadra_design::{Variant, dialog};

use super::{Dock, join, store};
use crate::model::Model;

fn name_of(path: &Path) -> String {
    path.file_name().map(|it| it.to_string_lossy().into_owned()).unwrap_or_default()
}

impl Dock {
    /// Uploads local files and folders' trees to the folder shown. Those whose names the
    /// folder already has wait for the answer of `replace`; the others go at once.
    pub fn upload(&self, model: &Model, paths: Vec<PathBuf>) {
        let there: Vec<String> =
            self.entries.with_untracked(|all| all.iter().map(|it| it.name.clone()).collect());
        let paths = paths.into_iter().filter(|it| it.is_file() || it.is_dir());
        let (over, free): (Vec<_>, Vec<_>) = paths.partition(|it| there.contains(&name_of(it)));
        self.send(model, free);
        if !over.is_empty() {
            // A question closed without an answer left its files: they are not asked again.
            let open = self.ask.replace.get_untracked();
            self.ask.over.update(|all| *all = all.drain(..).filter(|_| open).chain(over).collect());
            self.ask.replace.set(true);
        }
    }

    fn send(&self, model: &Model, paths: Vec<PathBuf>) {
        for path in paths {
            let name = name_of(&path);
            let remote = join(&self.dir.get_untracked(), &name);
            let transfer = self.start(&name, true, self.dir.get_untracked());
            let (done, whole, cancel) =
                (transfer.done.clone(), transfer.whole.clone(), transfer.cancel.clone());
            let (model, ran) = (model.clone(), transfer.clone());
            self.queue(&transfer, move |dock| {
                let again = model.clone();
                dock.with_files(
                    &model,
                    move |files| {
                        let tell = |d, w| store(&done, &whole, d, w);
                        match path.is_dir() {
                            true => files.upload_folder(&path, &remote, tell, &cancel),
                            false => files.upload(&path, &remote, tell, &cancel),
                        }
                    },
                    move |dock, got| {
                        if got.is_ok() {
                            dock.refresh(&again);
                        }
                        dock.finish(&ran, got);
                    },
                )
            });
        }
    }
}

/// The question of an upload onto names the folder has: Skip sends none of them.
pub(crate) fn replace(model: &Model, d: Dock) -> impl View + use<> {
    let asked = move || {
        let (over, dir) = (d.ask.over.get(), d.dir.get());
        let names: Vec<String> = over.iter().map(|it| name_of(it)).collect();
        let keeps =
            if over.iter().any(|it| it.is_dir()) { " A folder there keeps its other files." } else { "" };
        match names.as_slice() {
            [one] => {
                format!("{one} is already in {dir}. Replace it on the server?{keeps} It cannot be undone.")
            }
            all => format!(
                "{} names are already in {dir}: {}. Replace them on the server?{keeps} It cannot be undone.",
                all.len(),
                all.join(", ")
            ),
        }
    };
    let model = model.clone();
    dialog(d.ask.replace, "Replace", move || text(asked))
        .width(360.0)
        .action("Skip", Variant::Secondary, move || d.ask.over.set(Vec::new()))
        .action("Replace", Variant::Primary, move || {
            let over = d.ask.over.get_untracked();
            d.ask.over.set(Vec::new());
            d.send(&model, over);
        })
        .danger()
}
