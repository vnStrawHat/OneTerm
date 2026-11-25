//! The dialogs the SFTP menus open (specs/oneterm, "M9 in detail"): a new folder's name, a new
//! name, a delete asked (of one entry, or of those chosen), a large file's edit, an entry's
//! properties, an upload over a name.

use std::rc::Rc;

use quadra::{View, div, text};
use quadra_design::{Variant, dialog, field, input};

use crate::model::Model;
use crate::sftp::{Dock, Editing};

pub(crate) fn dialogs(model: &Model, d: Dock) -> impl View + use<> {
    let ask = d.ask;
    let named = move || {
        let name = ask.name.get_untracked();
        let name = name.trim();
        (!name.is_empty() && name != "." && name != ".." && !name.contains('/')).then(|| name.to_string())
    };
    let (makes, renames, deletes) = (model.clone(), model.clone(), model.clone());
    let create: Rc<dyn Fn()> = Rc::new(move || {
        if let Some(name) = named() {
            d.mkdir(&makes, &name);
        }
    });
    let rename: Rc<dyn Fn()> = Rc::new(move || {
        if let (Some(name), Some(entry)) = (named(), ask.entry.get_untracked())
            && name != entry.name
        {
            d.rename(&renames, &entry.name, &name);
        }
    });
    // Enter does what the dialog that shows is for, and closes it.
    let (creates, renames) = (create.clone(), rename.clone());
    let body = move || {
        let (creates, renames) = (creates.clone(), renames.clone());
        let submit = move |_: &str| {
            for (open, does) in [(ask.folder, &creates), (ask.rename, &renames)] {
                if open.get_untracked() {
                    does();
                    open.set(false);
                }
            }
        };
        field("Name", input(ask.name).test_id("sftp-name").on_submit(submit))
    };
    // A question, or one field: the dialog of 360 (Quadra's specs/design/surfaces.md).
    let folder = dialog(ask.folder, "New folder", body.clone())
        .width(360.0)
        .action("Cancel", Variant::Secondary, || {})
        .action("Create", Variant::Primary, move || create());
    let rename = dialog(ask.rename, "Rename", body)
        .width(360.0)
        .action("Cancel", Variant::Secondary, || {})
        .action("Rename", Variant::Primary, move || rename());
    let asked = move || {
        let many = ask.many.get();
        match many.as_slice() {
            [one] => {
                let what = if one.dir { "the empty folder" } else { "the file" };
                format!("Delete {what} {} from the server? It cannot be undone.", one.name)
            }
            _ => format!("Delete {} entries from the server? It cannot be undone.", many.len()),
        }
    };
    let delete = dialog(ask.delete, "Delete", move || text(asked))
        .width(360.0)
        .action("Cancel", Variant::Secondary, || {})
        .action("Delete", Variant::Primary, move || {
            d.delete(&deletes, ask.many.get_untracked());
        })
        .danger();
    let (edits, editing) = (model.clone(), Editing::here());
    let large = move || {
        let entry = ask.entry.get();
        let (name, size) = entry.as_ref().map(|it| (it.name.clone(), it.size)).unwrap_or_default();
        format!("{name} is {}. Download it to edit anyway?", crate::panes::size(size))
    };
    let large = dialog(ask.large, "A large file", move || text(large))
        .width(360.0)
        .action("Cancel", Variant::Secondary, || {})
        .action("Edit", Variant::Primary, move || {
            if let Some(entry) = ask.entry.get_untracked() {
                d.edit(&edits, &entry, &editing);
            }
        });
    let properties = crate::sftp::properties::properties(model, d);
    let replace = crate::sftp::upload::replace(model, d);
    div().child(folder).child(rename).child(delete).child(large).child(properties).child(replace)
}
