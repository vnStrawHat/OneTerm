//! The SFTP menus for the other button (specs/oneterm, "M9 in detail"), shared by the dock and the
//! expanded view, and what their dialogs (`dialogs.rs`) ask about. A row among several
//! chosen has the choice's menu.

use std::rc::Rc;

use oneterm_ssh::{Entry, Stat, join};
use quadra::{Clipboard, Signal, signal, use_context};
use quadra_design::{MenuItem, icons};

use crate::model::Model;
use crate::panes::Local;
use crate::sftp::{Dock, Editing};

/// What the dialogs of a dock ask about.
#[derive(Clone, Copy)]
pub(crate) struct Ask {
    pub folder: Signal<bool>,
    pub rename: Signal<bool>,
    pub delete: Signal<bool>,
    /// A file larger than editing takes without asking.
    pub large: Signal<bool>,
    /// The entry renamed or edited, and the name typed.
    pub entry: Signal<Option<Entry>>,
    pub name: Signal<String>,
    /// The entries Delete asks about.
    pub many: Signal<Vec<Entry>>,
    /// The permissions ticked in Properties: read, write and execute for the owner, the
    /// group and the others, in that order.
    pub bits: [Signal<bool>; 9],
    /// The properties shown, and the path they are of.
    pub properties: Signal<bool>,
    pub stat: Signal<Option<(String, Stat)>>,
    /// The uploads that wait for Replace: their names are in the folder shown.
    pub replace: Signal<bool>,
    pub over: Signal<Vec<std::path::PathBuf>>,
}

impl Ask {
    pub fn new() -> Ask {
        let no = || signal(false);
        let name = signal(String::new());
        let (properties, stat) = (no(), signal(None));
        Ask {
            folder: no(),
            rename: no(),
            delete: no(),
            large: no(),
            entry: signal(None),
            name,
            many: signal(Vec::new()),
            bits: std::array::from_fn(|_| no()),
            properties,
            stat,
            replace: no(),
            over: signal(Vec::new()),
        }
    }

    pub fn folder(&self) {
        self.name.set(String::new());
        self.folder.set(true);
    }
}

fn clipboard() -> Option<Rc<dyn Clipboard>> {
    use_context::<Rc<dyn Clipboard>>()
}

fn copies(path: String) -> impl Fn() + 'static {
    let clipboard = clipboard();
    move || {
        if let Some(clipboard) = &clipboard {
            clipboard.write_text(&path);
        }
    }
}

/// A remote entry's menu: open or download it, rename, copy its path, its properties, delete.
pub(crate) fn remote(model: &Model, d: Dock, entry: &Entry) -> impl Fn() -> Vec<MenuItem> + use<> {
    let (model, entry, copy, editing) = (model.clone(), entry.clone(), clipboard(), Editing::here());
    move || {
        // Among several chosen, the choice's menu: download them, delete them.
        let many = d.chosen();
        if many.len() > 1 && many.iter().any(|it| it.name == entry.name) {
            let (gets, all, n) = (model.clone(), many.clone(), many.len());
            return vec![
                MenuItem::new(format!("Download {n} items"))
                    .icon(icons::ARROW_DOWN)
                    .on_select(move || all.iter().for_each(|it| d.download(&gets, it))),
                MenuItem::separator(),
                MenuItem::new(format!("Delete {n} items\u{2026}")).danger().on_select(move || {
                    d.ask.many.set(many.clone());
                    d.ask.delete.set(true);
                }),
            ];
        }
        let path = join(&d.dir.get_untracked(), &entry.name);
        let (opens, (a, b, c)) = (model.clone(), (entry.clone(), entry.clone(), entry.clone()));
        let (stats, shown) = (model.clone(), entry.clone());
        let first = match entry.dir {
            true => MenuItem::new("Open").icon(icons::FOLDER).on_select({
                let path = path.clone();
                move || d.load(&opens, Some(path.clone()))
            }),
            false => {
                MenuItem::new("Download").icon(icons::ARROW_DOWN).on_select(move || d.download(&opens, &a))
            }
        };
        let (copy, edits, (asks, e), editing) =
            (copy.clone(), model.clone(), (entry.clone(), entry.clone()), editing.clone());
        let edit = MenuItem::new("Edit\u{2026}").icon(icons::PENCIL).on_select(move || {
            if d.too_large(&edits, &e) {
                d.ask.entry.set(Some(asks.clone()));
                d.ask.large.set(true);
            } else {
                d.edit(&edits, &e, &editing);
            }
        });
        let mut items = vec![first];
        let (gets, whole) = (model.clone(), entry.clone());
        let (shell, there) = (model.clone(), path.clone());
        match entry.dir {
            true => items.extend([
                MenuItem::new("Open in terminal")
                    .icon(icons::TERMINAL)
                    .on_select(move || d.cd(&shell, &there)),
                MenuItem::new("Download")
                    .icon(icons::ARROW_DOWN)
                    .on_select(move || d.download(&gets, &whole)),
            ]),
            false => items.push(edit),
        }
        items.extend([
            MenuItem::new("Rename\u{2026}").on_select(move || {
                d.ask.name.set(b.name.clone());
                d.ask.entry.set(Some(b.clone()));
                d.ask.rename.set(true);
            }),
            MenuItem::new("Copy path").icon(icons::COPY).on_select(move || {
                if let Some(clipboard) = &copy {
                    clipboard.write_text(&path);
                }
            }),
            MenuItem::new("Properties\u{2026}")
                .icon(icons::INFO)
                .on_select(move || d.properties(&stats, &shown)),
            MenuItem::separator(),
            MenuItem::new("Delete\u{2026}").danger().on_select(move || {
                d.ask.many.set(vec![c.clone()]);
                d.ask.delete.set(true);
            }),
        ]);
        items
    }
}

/// The menu of the remote list where no entry is: a new folder, the list again.
pub(crate) fn remote_area(model: &Model, d: Dock) -> impl Fn() -> Vec<MenuItem> + use<> {
    let model = model.clone();
    move || {
        let (again, shell) = (model.clone(), model.clone());
        vec![
            MenuItem::new("New folder\u{2026}").icon(icons::FOLDER).on_select(move || d.ask.folder()),
            MenuItem::new("Open in terminal")
                .icon(icons::TERMINAL)
                .on_select(move || d.cd(&shell, &d.dir.get_untracked())),
            MenuItem::new("Refresh").on_select(move || d.refresh(&again)),
        ]
    }
}

/// A local entry's menu in the expanded view: open or upload it, copy its path.
pub(crate) fn local(model: &Model, d: Dock, entry: &Entry) -> impl Fn() -> Vec<MenuItem> + use<> {
    let (model, entry, local): (Model, Entry, Local) = (model.clone(), entry.clone(), d.local);
    move || {
        // Among several chosen, the choice's menu: upload them.
        let (dir, names) = (local.dir.get_untracked(), local.picked.names());
        if names.len() > 1 && names.contains(&entry.name) {
            let (sends, n) = (model.clone(), names.len());
            let paths: Vec<_> = names.iter().map(|it| dir.join(it)).collect();
            return vec![
                MenuItem::new(format!("Upload {n} items"))
                    .icon(icons::ARROW_UP)
                    .on_select(move || d.upload(&sends, paths.clone())),
            ];
        }
        let path = dir.join(&entry.name);
        let (sends, at) = (model.clone(), path.clone());
        let first = match entry.dir {
            true => MenuItem::new("Open").icon(icons::FOLDER).on_select(move || local.load(at.clone())),
            false => MenuItem::new("Upload")
                .icon(icons::ARROW_UP)
                .on_select(move || d.upload(&sends, vec![at.clone()])),
        };
        let copy = copies(path.display().to_string());
        let (sends, at) = (model.clone(), path.clone());
        let folder = entry.dir.then(|| {
            MenuItem::new("Upload")
                .icon(icons::ARROW_UP)
                .on_select(move || d.upload(&sends, vec![at.clone()]))
        });
        vec![first]
            .into_iter()
            .chain(folder)
            .chain([
                MenuItem::new("Copy path").icon(icons::COPY).on_select(copy),
                MenuItem::new("Refresh").on_select(move || local.refresh()),
            ])
            .collect()
    }
}
