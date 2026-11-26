//! What the host editor shows (specs/oneterm, "M8 in detail"): three tabs. Connection, two fields
//! to a row: name; group and colour tag; address and port; username and auth; the key file and
//! its passphrase, or the password; the master password when one is needed; jump host. Shell:
//! shell integration and the startup command, a row each. Forwards: the rules (`forwards.rs`).

use quadra::{AnyView, Color, Display, Div, Paint, Signal, Track, View, column, div, dynamic, row, text};
use quadra_design::{Tone, alert, field, hstack, input, label, select, size, spacer, switch, tabs, token};

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

/// The tabs and the page of the one chosen; `confirming` puts the question of deleting the
/// host at the end, whatever the tab.
pub(super) fn body(
    model: &Model,
    form: Form,
    editing: Option<String>,
    confirming: Signal<bool>,
) -> quadra_design::Form {
    let name = editing.clone().unwrap_or_default();
    let question =
        alert(Tone::Danger, &format!("Delete {name}?"), "Its saved password and startup command go too.");
    // Each page keeps what it holds while another shows.
    let page = |at: usize, fields: quadra_design::Form| {
        column().display(shown(move || form.tab.get() == at)).child(fields)
    };
    quadra_design::form()
        .child(tabs(["Connection", "Shell", "Forwards"], form.tab))
        // The focus starts in the first field, not on the tabs.
        .child(
            page(0, connection(model, form, editing))
                .on_mount(|tree, id| tree.after_layout(move |tree| _ = tree.focus_first(id))),
        )
        .child(page(1, shell(model, form)))
        .child(page(2, super::forwards::list(form.forwards)))
        .child(column().display(shown(move || confirming.get())).child(question))
}

/// Who the host is and how it is signed in to.
fn connection(model: &Model, form: Form, editing: Option<String>) -> quadra_design::Form {
    // No field has a width: each is as wide as its column of the form.
    let text_field = |value, hint: &str, id: &'static str| input(value).placeholder(hint).test_id(id);
    let auth = form.auth;
    let address = div()
        .grid([Track::fr(1.0), Track::px(88.0)])
        .gap_x(token::SPACE_LG)
        .child(field("Address", text_field(form.target, "Host name or IP", "host-target").mono()))
        .child(field("Port", text_field(form.port, "22", "host-port").mono()));
    let user = field("Username", text_field(form.user, "Ask when connecting", "host-user")).help("Optional.");
    let hint =
        div().text_size(size::TEXT_SM.size).text_color(token::TEXT_2).child(text(move || match auth.get() {
            0 => "ssh-agent, ~/.ssh keys, then a password.",
            1 => "The keys loaded in ssh-agent.",
            2 => "Passphrase optional: asked on connect.",
            _ => "Password optional: asked on connect.",
        }));
    let how = column().gap(token::SPACE_XS).child(field("Auth method", select(AUTHS, auth))).child(hint);
    // The key beside its passphrase, or the password as wide as the form, as the Connect
    // dialog has them: each a child of the form, shown by the auth, so that it keeps the
    // form's width (one field edits the same secret in both).
    let saved = editing.as_ref().and_then(|it| model.sealed(it)).is_some();
    let entry = move || {
        let entry = input(form.secret).password().reveal().placeholder(if saved { "Saved" } else { "" });
        entry.test_id("host-secret").height(H)
    };
    let key = field("Key file", crate::sshkeys::picker(model, form.key, "host-key"));
    let keyed = pair(key, field("Passphrase", entry()))
        .display(move || if auth.get() == 2 { Display::Grid } else { Display::None });
    let password = column().display(shown(move || auth.get() == 3)).child(field("Password", entry()));
    quadra_design::form()
        .child(field("Name", text_field(form.name, "The host when empty", "host-name")))
        .child(pair(
            field("Group", text_field(form.group, "Group", "host-group")),
            field("Colour tag", tag_picker(form.tag, form.group)),
        ))
        .child(address)
        .child(pair(user, how))
        .child(keyed)
        .child(password)
        .child(super::secret::fields(model, form))
        .child(field("Jump host", select(model.jump_choices(), form.jump).test_id("host-jump")))
}

/// What the host's shell is typed once it opens.
fn shell(model: &Model, form: Form) -> quadra_design::Form {
    // Its name at the left and its switch at the right, on one line; what it does under them.
    let integration = switch(form.integration).test_id("host-integration");
    let named = hstack().role(quadra::Role::Group).label("Shell integration");
    let named = named.child(text("Shell integration")).child(spacer()).child(integration);
    let about = label(
        "Types a line into the shell once it opens, so it reports its folder and its prompts. \
         Turn it off for a shell that is not bash, zsh or another POSIX shell.",
    );
    quadra_design::form().child(column().gap(token::SPACE_XS).child(named).child(about)).child(
        field("Startup command", crate::startup::picker(model, form.startup, "host-startup"))
            .help("Written once the shell is open. The commands are kept in Settings, SSH & hosts."),
    )
}
