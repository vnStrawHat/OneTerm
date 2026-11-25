//! What the host editor shows (specs/oneterm, "M8 in detail"), two fields to a row: name; group and
//! colour tag; address and port; username and jump host; auth and its password or passphrase,
//! the key file; the master password when one is needed; startup command and forwards.

use quadra::{AnyView, Color, Div, Paint, Signal, Track, View, column, div, dynamic, row, text};
use quadra_design::{Tone, alert, field, input, select, size, token};

use super::AUTHS;
use super::form::Form;
use crate::hosts::tag::{tag_of_group, tag_picker};
use crate::locked::shown;
use crate::model::Model;

const H: f32 = size::H_MD;

fn pair(a: impl View + 'static, b: impl View + 'static) -> Div {
    quadra_design::columns(2).child(a).child(b)
}

/// The header: the tag the host will have, and the title.
pub(super) fn head(form: Form, title: String) -> Div {
    let mark = dynamic(move || {
        let paint: Paint = match Color::parse_hex(form.tag.get().trim()) {
            Some(color) => color.into(),
            None => tag_of_group(form.group.get().trim()).1.into(),
        };
        AnyView::new(div().size(10.0, 10.0).radius(token::RADIUS_XS).bg(paint))
    });
    let title = div().font_weight(600).text_size(size::TEXT_LG.size).child(text(title));
    row().grow(1.0).gap(token::SPACE_MD).align_items(quadra::Align::Center).child(mark).child(title)
}

/// The form; `confirming` puts the question of deleting it at its end.
pub(super) fn body(
    model: &Model,
    form: Form,
    editing: Option<String>,
    confirming: Signal<bool>,
) -> quadra_design::Form {
    // No field has a width: each is as wide as its column of the dialog's body.
    let text_field = |value, hint: &str, id: &'static str| input(value).placeholder(hint).test_id(id);
    let auth = form.auth;
    let address = div()
        .grid([Track::fr(1.0), Track::px(88.0)])
        .gap_x(token::SPACE_LG)
        .child(field("Address", text_field(form.target, "Host name or IP", "host-target").mono()))
        .child(field("Port", text_field(form.port, "22", "host-port").mono()));
    let user = field("Username", text_field(form.user, "Ask when connecting", "host-user")).help("Optional.");
    let jump = field("Jump host", select(model.jump_choices(), form.jump).test_id("host-jump"));
    let saved = editing.as_ref().and_then(|it| model.sealed(it)).is_some();
    let secret = dynamic(move || {
        let label = if auth.get() == 2 { "Passphrase" } else { "Password" };
        let entry = input(form.secret).password().reveal().placeholder(if saved { "Saved" } else { "" });
        AnyView::new(field(label, entry.test_id("host-secret").height(H)))
    });
    let secret = div().display(shown(move || matches!(auth.get(), 2 | 3))).child(secret);
    let hint =
        div().text_size(size::TEXT_SM.size).text_color(token::TEXT_2).child(text(move || match auth.get() {
            0 => "Tries the keys in ssh-agent and ~/.ssh, then asks for a password.",
            1 => "Uses the keys loaded in ssh-agent.",
            2 => "The passphrase is optional. Left empty, OneTerm asks for it when you connect.",
            _ => "The password is optional. Left empty, OneTerm asks for it when you connect.",
        }));
    let how = pair(field("Auth method", select(AUTHS, auth)), secret);
    let name = editing.clone().unwrap_or_default();
    let question =
        alert(Tone::Danger, &format!("Delete {name}?"), "Its saved password and startup command go too.");
    quadra_design::form()
        .child(field("Name", text_field(form.name, "The host when empty", "host-name")))
        .child(pair(
            field("Group", text_field(form.group, "Group", "host-group")),
            field("Colour tag", tag_picker(form.tag, form.group)),
        ))
        .child(address)
        .child(pair(user, jump))
        .child(column().gap(token::SPACE_XS).child(how).child(hint))
        .child(
            div()
                .display(shown(move || auth.get() == 2))
                .child(field("Key file", crate::sshkeys::picker(model, form.key, "host-key"))),
        )
        .child(super::secret::fields(model, form))
        .child(
            field("Startup command", crate::startup::picker(model, form.startup, "host-startup"))
                .help("Written once the shell is open. The commands are kept in Settings, SSH & hosts."),
        )
        .child(field(
            "Forwards",
            text_field(form.forwards, "8080:localhost:80 R9000:localhost:3000 D1080", "host-forwards"),
        ))
        .child(div().display(shown(move || confirming.get())).child(question))
}
