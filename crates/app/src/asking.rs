//! The Connect dialogs (specs/oneterm, "M7 in detail"): the focused space's connection while it is
//! made, one dialog for each phase (connecting, an unknown host, signing in, a failure),
//! swapped as the phase moves. Giving one up is its Cancel or Close.

use std::rc::Rc;

use oneterm_ssh::Ask;
use quadra::{Signal, View, column, div, effect, signal, text, untrack};
use quadra_design::{Variant, field, fonts, input, token};

use crate::model::Model;
use crate::remote::{Conn, Phase};

const CONNECTING: usize = 0;
const HOST_KEY: usize = 1;
const SIGN_IN: usize = 2;
const FAILED: usize = 3;

/// The dialog of a phase; a saved host's Connect dialog shows each phase itself.
fn which(phase: &Phase, opening: bool) -> Option<usize> {
    match phase {
        _ if opening => None,
        Phase::Done => None,
        Phase::Connecting => Some(CONNECTING),
        Phase::Ask(Ask::HostKey { .. }) => Some(HOST_KEY),
        Phase::Ask(Ask::Failed { .. }) => Some(FAILED),
        Phase::Ask(_) => Some(SIGN_IN),
    }
}

pub(crate) fn dialogs(model: &Model) -> impl View + use<> {
    let model = model.clone();
    // The connection a Connect dialog makes, else the focused space's.
    let opening = model.ui().opening.space;
    let opening_now = move || opening.with(Option::is_some);
    let shown = Rc::new(move || match opening.get() {
        Some(space) => space.session.kept::<Conn>(),
        None => model.active_tab()?.focused()?.session.kept::<Conn>(),
    });
    let opens: [Signal<bool>; 4] = std::array::from_fn(|_| signal(false));
    // What the Sign in dialog's field holds: nothing kept after it is answered.
    let typed = signal(String::new());
    let following = shown.clone();
    effect(move || {
        let phase = following().map(|conn| conn.phase.get());
        let want = phase.as_ref().and_then(|it| which(it, opening_now()));
        if want == Some(SIGN_IN) && !opens[SIGN_IN].get_untracked() {
            let user = match &phase {
                Some(Phase::Ask(Ask::User { user, .. })) => user.clone(),
                _ => String::new(),
            };
            typed.set(user);
        } else if want != Some(SIGN_IN) && !typed.with_untracked(String::is_empty) {
            crate::vault::wipe(typed);
        }
        for (at, open) in opens.iter().enumerate() {
            if open.get_untracked() != (want == Some(at)) {
                open.set(want == Some(at));
            }
        }
    });
    // A dialog closed while its phase stands was given up.
    for (at, open) in opens.into_iter().enumerate() {
        let gives_up = shown.clone();
        effect(move || {
            if open.get() {
                return;
            }
            let Some(conn) = untrack(|| gives_up()) else { return };
            let phase = untrack(|| conn.phase.get());
            match phase {
                _ if which(&phase, untrack(opening_now)) != Some(at) => {}
                Phase::Connecting => conn.cancel(),
                _ => conn.answer(None),
            }
        });
    }
    let asked = {
        let shown = shown.clone();
        move || untrack(|| shown().map(|conn| (conn.phase.get(), conn.label.get())))
    };
    let (connecting_to, failing) = (shown.clone(), shown.clone());
    let answers = move |answer: Rc<dyn Fn() -> Option<String>>| {
        let shown = shown.clone();
        move || {
            if let Some(conn) = untrack(|| shown()) {
                conn.answer(answer());
            }
        }
    };
    let (a, b, c) = (asked.clone(), asked.clone(), connecting_to);
    // It follows the hop being reached, and the user answered.
    let connecting = quadra_design::dialog(opens[CONNECTING], "Connecting", move || {
        let shown = c.clone();
        text(move || {
            format!("Connecting to {}\u{2026}", shown().map_or_else(String::new, |it| it.label.get()))
        })
    })
    .action("Cancel", Variant::Secondary, || {});
    let host_key = quadra_design::dialog(opens[HOST_KEY], "Unknown host", move || host_key(a()))
        .action("Cancel", Variant::Secondary, || {})
        .action("Trust and connect", Variant::Primary, answers(Rc::new(|| Some("yes".into()))));
    let submit = answers(Rc::new(move || Some(typed.get_untracked())));
    let sign_in = quadra_design::dialog(opens[SIGN_IN], "Sign in", {
        let submit = submit.clone();
        move || sign_in(b(), typed, submit.clone())
    })
    .action("Cancel", Variant::Secondary, || {})
    .action("Connect", Variant::Primary, submit)
    .shortcut("Enter");
    // A changed host key fails again until the known hosts are put right: Close is what it
    // offers first, and Retry is for after.
    let changed = move || {
        let phase = failing().map(|conn| conn.phase.get());
        matches!(phase, Some(Phase::Ask(Ask::Failed { reason })) if oneterm_ssh::key_changed(&reason))
    };
    let (other, retry) = (changed.clone(), answers(Rc::new(|| Some(String::new()))));
    let (again, wrong, still) = (retry.clone(), changed.clone(), changed.clone());
    let failed = quadra_design::dialog(opens[FAILED], "Could not connect", move || failure(asked()))
        .action("Close", Variant::Secondary, || {})
        .shown(move || !other())
        .action("Retry", Variant::Primary, retry)
        .shown(move || !wrong())
        .action("Retry", Variant::Secondary, again)
        .shown(still)
        .action("Close", Variant::Primary, || {})
        .shown(changed);
    div().child(connecting).child(host_key).child(sign_in).child(failed)
}

/// A fingerprint: mono, 12 px, on one line.
fn mono(words: String) -> quadra::Div {
    // design: a fingerprint is compared as one line: at 12 px its 50 characters fit the dialog
    div().font_family(fonts::MONO).text_size(12.0).text_color(token::TEXT).child(text(words))
}

fn host_key(asked: Option<(Phase, String)>) -> quadra::Div {
    let Some((Phase::Ask(Ask::HostKey { host, kind, fingerprint }), _)) = asked else { return column() };
    quadra_design::vstack()
        .child(text(format!("The authenticity of host '{host}' can't be established.")))
        .child(
            column()
                .gap(token::SPACE_XXS)
                .child(text(format!("{kind} key fingerprint:")))
                .child(mono(fingerprint)),
        )
        .child(text("Trusting it adds the key to known_hosts."))
}

fn sign_in(
    asked: Option<(Phase, String)>,
    typed: Signal<String>,
    submit: impl Fn() + 'static,
) -> quadra::Div {
    let Some((Phase::Ask(ask), _)) = asked else { return column() };
    let (said, name, again, secret) = match ask {
        Ask::User { host, .. } => (format!("No user was given for {host}."), "User", false, false),
        Ask::Password { target, again } => (format!("Password for {target}"), "Password", again, true),
        Ask::Passphrase { file, again } => {
            (format!("Passphrase for {}", file.display()), "Passphrase", again, true)
        }
        _ => return column(),
    };
    let wrong = match name {
        "Passphrase" => "Wrong passphrase, please try again.",
        _ => "Permission denied, please try again.",
    };
    let mut entry = input(typed).label(name).test_id("sign-in");
    if secret {
        entry = entry.password();
    }
    // A try that failed is said under the field, by the field.
    let wrong = if again { wrong } else { "" };
    let entry = field(name, entry.on_submit(move |_| submit())).error(wrong.to_string());
    column().gap(token::SPACE_LG).child(text(said)).child(entry)
}

fn failure(asked: Option<(Phase, String)>) -> quadra::Div {
    let Some((Phase::Ask(Ask::Failed { reason }), label)) = asked else { return column() };
    quadra_design::vstack().child(text(format!("Could not connect to {label}."))).child(quadra_design::alert(
        quadra_design::Tone::Danger,
        "",
        &sentence(&reason),
    ))
}

/// The engine's reason as a sentence: its first letter upper case.
pub(crate) fn sentence(reason: &str) -> String {
    let mut chars = reason.chars();
    chars.next().map_or_else(String::new, |first| first.to_uppercase().chain(chars).collect())
}
