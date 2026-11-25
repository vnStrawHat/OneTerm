//! The Connect dialog while it connects (specs/oneterm, "The rail"): the steps, done, under way and
//! to come, and what the server asks on the way, answered in the dialog.

use std::rc::Rc;

use oneterm_ssh::Ask;
use quadra::{AnyView, Div, Signal, column, div, dynamic, row, text};
use quadra_design::{field, icon, icons, input, spinner, token};

use super::form::mono;
use super::{AUTHS, Form};
use crate::remote::{Conn, Phase};

/// A step's label and what it is about.
struct Step {
    label: &'static str,
    detail: String,
}

/// The steps to the shell: through the jump host if there is one, the host, its key, signing in.
fn steps(jump: &str, place: String, known: bool, user: String, auth: usize) -> Vec<Step> {
    let mut out = Vec::new();
    if !jump.is_empty() {
        out.push(Step { label: "Reach jump host", detail: jump.to_string() });
    }
    out.push(Step { label: "Reach host", detail: place });
    let key = if known { "known" } else { "trusted just now" };
    out.push(Step { label: "Verify host key", detail: key.into() });
    out.push(Step { label: "Authenticate", detail: format!("{user} \u{B7} {}", AUTHS[auth]) });
    out
}

/// The step under way, by what the engine told: signing in to `last` is the last step; before
/// that the host is being reached, or first the jump host.
fn under_way(conn: &Conn, last: &str, jump: bool, count: usize) -> usize {
    // Read with care: the space ends as the dialog goes back to its form.
    let (label, authing) =
        (conn.label.try_get().unwrap_or_default(), conn.authing.try_get().unwrap_or_default());
    match () {
        _ if authing == last => count - 1,
        _ if jump && authing.is_empty() => 0,
        _ if label == last => usize::from(jump),
        _ => 0,
    }
}

/// The steps and, while the server asks something, its question.
pub(super) fn view(
    conn: Rc<Conn>,
    last: String,
    jump: String,
    known: bool,
    form: Form,
    name: String,
    answer: Rc<dyn Fn()>,
) -> quadra_design::Form {
    let place = last.split_once('@').map_or(last.clone(), |it| it.1.to_string());
    let user = last.split_once('@').map_or(String::new(), |it| it.0.to_string());
    let list = steps(&jump, place, known, user, form.auth.get_untracked());
    let count = list.len();
    let has_jump = !jump.is_empty();
    let following = conn.clone();
    let lines = dynamic(move || {
        let now = under_way(&following, &last, has_jump, count);
        let mut lines = column().gap(token::SPACE_SM);
        for (at, step) in list.iter().enumerate() {
            lines = lines.child(line(step, at.cmp(&now)));
        }
        AnyView::new(lines)
    });
    let asked = dynamic(move || match conn.phase.try_get() {
        Some(Phase::Ask(ask)) => AnyView::new(question(ask, form.answer, &name, answer.clone())),
        _ => AnyView::new(div()),
    });
    quadra_design::form().child(lines).child(asked)
}

/// A step's line: a check when done, a spinner under way, a dot to come.
fn line(step: &Step, when: std::cmp::Ordering) -> Div {
    use std::cmp::Ordering::*;
    let mark = row()
        .size(16.0, 23.0)
        .shrink(0.0)
        .align_items(quadra::Align::Center)
        .justify(quadra::Justify::Center);
    let color = if when == Greater { token::TEXT_2 } else { token::TEXT };
    quadra_design::hstack()
        .child(mark.child(match when {
            Less => AnyView::new(icon(icons::CIRCLE_CHECK).size(14.0).color(token::OK)),
            Equal => AnyView::new(spinner().size(14.0)),
            Greater => AnyView::new(div().size(6.0, 6.0).radius(1.0).bg(token::TEXT_3)),
        }))
        .child(div().text_color(color).child(text(step.label)))
        .child(mono(step.detail.clone(), 12.0, token::TEXT_2))
}

/// What the server asks: a key to trust, a user, a password or passphrase (keyboard-interactive
/// asks its own the same way), answered by Continue or Enter.
fn question(ask: Ask, typed: Signal<String>, name: &str, answer: Rc<dyn Fn()>) -> Div {
    let passes = format!("Asked by {name}. OneTerm passes your answer through and does not store it.");
    let (label, secret, again) = match ask {
        Ask::HostKey { host, kind, fingerprint } => {
            // Which host's: with a jump host two keys may be asked about.
            let words = format!(
                "OneTerm has not seen the key of {host} before. Check its fingerprint with the host's owner; Continue trusts it and adds it to known hosts."
            );
            return column()
                .gap(token::SPACE_XS)
                .child(div().text_color(token::TEXT).child(text(words)))
                .child(mono(format!("{kind} {fingerprint}"), 12.0, token::TEXT));
        }
        Ask::User { .. } => ("Username", false, false),
        Ask::Password { again, .. } => ("Password", true, again),
        Ask::Passphrase { again, .. } => ("Passphrase", true, again),
        Ask::Failed { .. } => return column(),
    };
    let mut entry = input(typed).test_id("connect-answer").on_submit(move |_| answer());
    if secret {
        entry = entry.password().reveal();
    }
    let entry =
        div().on_mount(|tree, id| tree.after_layout(move |tree| _ = tree.focus_first(id))).child(entry);
    let mut asked = field(label, entry).help(passes);
    if again {
        asked = asked.error("Refused, please try again.".to_string());
    }
    column().child(asked)
}
