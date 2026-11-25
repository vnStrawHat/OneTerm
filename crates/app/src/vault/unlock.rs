//! Unlock saved passwords (specs/oneterm, "Saved passwords"): connecting to hosts with saved
//! secrets while the data key is not in memory asks the master password first; Cancel
//! connects without them.

use std::rc::Rc;

use quadra::{View, effect, signal, text, untrack};
use quadra_design::{Variant, field, input};

use crate::model::Model;

pub(crate) fn dialog(model: &Model) -> impl View + use<> {
    let (pending, open) = (model.ui().unlocking, signal(false));
    let (typed, said) = (signal(String::new()), signal(String::new()));
    effect(move || {
        let want = pending.with(Option::is_some);
        if open.get_untracked() != want {
            open.set(want);
        }
    });
    // Given up with connections waiting: they go on without their secrets.
    let gives_up = model.clone();
    effect(move || {
        if open.get() {
            return;
        }
        let Some((remotes, here)) = untrack(|| pending.get()) else { return };
        pending.set(None);
        crate::vault::wipe(typed);
        said.set(String::new());
        for remote in remotes {
            gives_up.connect_with(remote, here, None);
        }
    });
    // Tried while the dialog is closed; wrong, the connections wait again and it says so.
    let tries = model.clone();
    let unlock = Rc::new(move || {
        let Some(waiting) = pending.get_untracked() else { return };
        pending.set(None);
        let model = tries.clone();
        tries.lock().open(typed.get_untracked(), move |right| {
            crate::vault::wipe(typed);
            if right {
                said.set(String::new());
                model.connect_all(waiting.0, waiting.1);
            } else {
                said.set("Wrong master password".into());
                pending.set(Some(waiting));
            }
        });
    });
    let (submits, clicks) = (unlock.clone(), unlock);
    let body = move || {
        let submits = submits.clone();
        let entry = input(typed)
            .label("Master password")
            .password()
            .test_id("vault-password")
            .on_submit(move |_| submits());
        quadra_design::form()
            .child(text("Saved passwords are sealed with the master password."))
            .child(field("Master password", entry).error(move || said.get()))
    };
    quadra_design::dialog(open, "Unlock saved passwords", body)
        .action("Cancel", Variant::Secondary, || {})
        .action("Unlock", Variant::Primary, move || clicks())
        .shortcut("Enter")
}
