//! How the lock looks (specs/oneterm, "Lock"): the card where dialogs hang, what the
//! window counts as being used, and the Security & lock page of the settings.

use quadra::{
    Align, Display, Div, Justify, MountCx, NodeId, PointerKind, Signal, View, column, div, effect, row,
    signal, text,
};
use quadra_design::{SettingsGroup, Variant, button, icon, icons, input, number_input, token};

use crate::lock::Lock;
use crate::model::Model;
use crate::page::Back;

/// How far the workspace behind the lock is blurred: no text can be made out.
const BLUR: f32 = 24.0;

/// Shown while `on` says so, else not displayed (and so not in the semantic tree).
pub(crate) fn shown(on: impl Fn() -> bool + 'static) -> impl Fn() -> Display + 'static {
    move || if on() { Display::Flex } else { Display::None }
}

/// Keys, presses and moves anywhere in `root` start the idle time again; they go on.
pub(crate) fn watches(root: Div, lock: Lock) -> Div {
    root.on_mount(move |tree, id| {
        tree.on_key_capture(id, true, move |_| lock.active());
        for kind in [PointerKind::Down, PointerKind::Move, PointerKind::Wheel] {
            tree.on_pointer_capture(id, kind, move |_| lock.active());
        }
    })
}

/// The card of the locked window, the focus in its field whenever it locks.
pub(crate) struct Card(pub Model);

impl View for Card {
    fn mount(self, cx: &mut MountCx) -> NodeId {
        let lock = self.0.lock();
        let id = card(&self.0).mount(cx);
        let handle = cx.handle.clone();
        effect(move || {
            if lock.locked.get() {
                // After the writes of this pass, one of which displays the card.
                let later = handle.clone();
                // What was open over the window (a menu) closes: it is no part of the workspace.
                handle.with(move |tree| {
                    tree.dismiss_overlays();
                    later.with(move |tree| _ = tree.focus_first(id))
                });
            }
        });
        id
    }
}

fn card(model: &Model) -> Div {
    let (lock, tabs, unread) = (model.lock(), model.tabs(), model.unread());
    let why = move || match lock.after.get() {
        0 => "Locked. Your sessions are still running.".to_string(),
        1 => "Locked after 1 minute idle. Your sessions are still running.".to_string(),
        n => format!("Locked after {n} minutes idle. Your sessions are still running."),
    };
    let running = move || {
        let n = tabs.with(|tabs| {
            tabs.iter()
                .map(|tab| {
                    tab.spaces.with(|s| {
                        s.iter().filter(|it| it.edit.is_none() && it.session.exited().get().is_none()).count()
                    })
                })
                .sum::<usize>()
        });
        format!("{n} sessions connected")
    };
    let field = input(lock.typed)
        .password()
        .reveal()
        .placeholder("Master password")
        .test_id("unlock")
        .height(32.0) // the lg step: the one field of a sign-in card
        .on_submit(move |_| lock.unlock());
    let head = quadra_design::hstack()
        .padding_x(token::SPACE_LG)
        .padding_top(token::SPACE_LG)
        .padding_bottom(token::SPACE_XS)
        .child(icon(icons::LOCK).size(16.0).color(token::ACCENT))
        .child(div().font_weight(600).text_size(16.0).child(text("OneTerm is locked")));
    // What runs behind, and what came while locked.
    let foot = row()
        .align_items(Align::Center)
        .gap(token::SPACE_LG)
        .text_size(12.0)
        .label(move || format!("{} \u{B7} {} notices", running(), unread.get()))
        .role(quadra::Role::Group)
        .child(
            row()
                .align_items(Align::Center)
                .gap(token::SPACE_SM)
                .child(div().size(8.0, 8.0).radius(token::RADIUS_SM).bg(token::OK))
                .child(text(running)),
        )
        .child(
            row()
                .align_items(Align::Center)
                .gap(token::SPACE_SM)
                .child(icon(icons::BELL).size(14.0).color(token::ERROR))
                .child(text(move || format!("{} new while locked", unread.get()))),
        );
    // The card's body: a form, so that the field is as wide as the card; what is wrong with
    // the password is said under it.
    let said = quadra_design::field("Password", field).error(move || lock.said.get());
    let body = column()
        .padding_x(token::SPACE_LG)
        .padding_bottom(token::SPACE_LG)
        .text_color(token::TEXT_2)
        .child(quadra_design::form().child(text(why)).child(said).child(foot));
    let unlock = button("Unlock")
        .variant(Variant::Primary)
        .shortcut("Enter")
        .disabled(move || lock.wait.get() > 0)
        .on_click(move || lock.unlock());
    // Where dialogs hang, with their look; neither Escape nor a press outside closes it.
    let card = column()
        .width(440.0)
        .radius(token::RADIUS_LG)
        .bg(token::OVERLAY_BG)
        .border(1.0)
        .border_color(token::LINE_STRONG)
        .elevation(2)
        .focus_scope()
        .child(head)
        .child(body)
        .child(
            row()
                .justify(Justify::End)
                .padding_y(token::SPACE_MD)
                .padding_x(token::SPACE_LG)
                .border_top(1.0)
                .border_color(token::LINE)
                .child(unlock),
        );
    // Over the workspace, which it blurs and dims.
    column()
        .absolute()
        .top(0.0)
        .left(0.0)
        .right(0.0)
        .bottom(0.0)
        .align_items(Align::Center)
        .padding_top(48.0) // design: the room over the lock card's mark
        .padding_x(token::SPACE_XL)
        .backdrop_blur(BLUR)
        .bg(token::SCRIM)
        .display(shown(move || lock.locked.get()))
        .child(card)
}

/// Security & lock: the master password set, changed or removed, and the idle time.
pub(crate) fn security(lock: Lock, after: Signal<f64>, default: Signal<f64>) -> SettingsGroup {
    let (current, new, again, said) =
        (signal(String::new()), signal(String::new()), signal(String::new()), signal(String::new()));
    let field = |value: Signal<String>, hint: &str, name: &str, id: &'static str| {
        input(value).password().placeholder(hint).label(name).test_id(id)
    };
    let set = move || {
        if new.get_untracked() != again.get_untracked() {
            return said.set("The two new passwords differ".into());
        }
        if new.with_untracked(String::is_empty) {
            return said.set("The new password is empty".into());
        }
        lock.set_password(current.get_untracked(), new.get_untracked(), move |words| said.set(words.into()));
        for field in [current, new, again] {
            crate::vault::wipe(field);
        }
    };
    // Removing asks first: the saved passwords go with it.
    let asking = signal(false);
    let remove = move || {
        lock.set_password(current.get_untracked(), String::new(), move |words| said.set(words.into()));
        crate::vault::wipe(current);
    };
    let asked = move || {
        let saved = match lock.saved_secrets() {
            0 => String::new(),
            1 => ", and the saved password is forgotten".into(),
            n => format!(", and the {n} saved passwords are forgotten"),
        };
        text(format!("The window will no longer lock{saved}. It cannot be undone."))
    };
    let confirm = quadra_design::dialog(asking, "Remove the master password?", asked)
        .action("Cancel", quadra_design::Variant::Secondary, || {})
        .action("Remove", quadra_design::Variant::Danger, remove);
    let buttons = quadra::row().gap(token::SPACE_MD).child(button("Set password").on_click(set)).child(
        div()
            .display(shown(move || lock.can_lock()))
            .child(button("Remove password").on_click(move || asking.set(true)))
            .child(confirm),
    );
    SettingsGroup::new("Master password")
        .about("Kept as a salted hash, never the password. Without one the window does not lock.")
        .row(
            "Current password",
            "Needed to change or remove the password.",
            field(current, "Current", "Current password", "password-current"),
        )
        .row("New password", "", field(new, "New", "New password", "password-new"))
        .row("Repeat it", "", field(again, "Again", "Repeat it", "password-again"))
        .row("", "", buttons)
        .row("", "", div().text_color(token::TEXT_2).child(text(move || said.get())))
        .row(
            "Lock after",
            "Minutes without a key or the mouse, 1 to 480.",
            number_input(after).stepper().range(1.0, 480.0).label("Lock after"),
        )
        .back(after, default)
        .row(
            "Lock now",
            "The same as Ctrl+Shift+L.",
            div()
                .display(shown(move || lock.can_lock()))
                .child(button("Lock now").on_click(move || lock.lock_now())),
        )
}
