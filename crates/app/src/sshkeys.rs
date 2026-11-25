//! The key files hosts sign in with (specs/oneterm, "Keys"): a list in the setting `ssh.keys`,
//! kept on the SSH & hosts page, chosen by name in the hosts' dialogs.

use std::path::{Path, PathBuf};

use quadra::{Align, AnyView, Signal, View, column, div, dynamic, effect, row, signal, text};
use quadra_design::{SettingsGroup, Variant, button, fonts, select, size, token};

use crate::model::Model;

/// A path as the dialogs show it: the home folder as `~`, with forward slashes.
pub(crate) fn shown(path: &str) -> String {
    let home = std::env::var("USERPROFILE").unwrap_or_default();
    let short = match path.strip_prefix(&home) {
        Some(rest) if !home.is_empty() => format!("~{rest}"),
        _ => path.to_string(),
    };
    short.replace('\\', "/")
}

/// A select of the keys that writes the path chosen to `key`; a key a host has that is not in
/// the list is offered too.
pub(crate) fn picker(model: &Model, key: Signal<String>, id: &'static str) -> impl View + use<> {
    let mut keys = model.prefs().ssh_keys.get_untracked();
    let had = key.get_untracked();
    if !had.is_empty() && !keys.contains(&had) {
        keys.push(had.clone());
    }
    let chosen = signal(keys.iter().position(|it| *it == had).unwrap_or(usize::MAX));
    let names: Vec<String> = keys.iter().map(|it| shown(it)).collect();
    let none = if keys.is_empty() { "No keys: add one in Settings" } else { "Choose a key" };
    effect(move || {
        if let Some(path) = keys.get(chosen.get()).filter(|it| **it != key.get_untracked()) {
            key.set(path.clone());
        }
    });
    select(names, chosen).placeholder(none).test_id(id)
}

/// The settings' group: Add key… (the system's dialog, from `~/.ssh`), Find in ~/.ssh, and
/// each key with its kind, fingerprint and Remove.
pub(crate) fn group(model: &Model) -> SettingsGroup {
    let keys = model.prefs().ssh_keys;
    let add = move |path: String| {
        if !keys.with_untracked(|it| it.contains(&path)) {
            keys.update(|it| it.push(path));
        }
    };
    let dialogs = quadra::use_context::<std::rc::Rc<dyn quadra_shell::Dialogs>>();
    let adds = add;
    let choose = button("Add key\u{2026}").variant(Variant::Secondary).on_click(move || {
        let mut asked = quadra_shell::FileDialog::new("Key file");
        if let Some(ssh) = ssh_folder() {
            asked = asked.folder(ssh);
        }
        for chosen in dialogs.as_ref().map(|it| it.open_files(&asked)).unwrap_or_default() {
            adds(chosen.display().to_string());
        }
    });
    let find = button("Find in ~/.ssh").variant(Variant::Secondary).on_click(move || {
        for path in ssh_folder().map(|it| found(&it)).unwrap_or_default() {
            add(path.display().to_string());
        }
    });
    let list = dynamic(move || {
        let all = keys.get();
        if all.is_empty() {
            return AnyView::new(div().text_color(token::TEXT_2).child(text("No keys yet.")));
        }
        AnyView::new(
            all.into_iter()
                .fold(column().grow(1.0).gap(token::SPACE_MD).align_items(Align::Stretch), |list, path| {
                    list.child(line(keys, path))
                }),
        )
    })
    .container(column().grow(1.0).align_items(Align::Stretch));
    SettingsGroup::new("Keys")
        .about("Private key files your hosts sign in with. A host's dialog chooses one by name.")
        .row(
            "Add a key",
            "A private key file, from ~/.ssh or anywhere.",
            row().gap(token::SPACE_MD).child(choose).child(find),
        )
        .wide(list)
}

/// A key: its file's name over its path, its kind and fingerprint (or why it cannot be read),
/// Remove.
fn line(keys: Signal<Vec<String>>, path: String) -> quadra::Div {
    let name = Path::new(&path).file_name().map_or(path.clone(), |it| it.to_string_lossy().into_owned());
    let about = match oneterm_ssh::key_info(Path::new(&path)) {
        Ok((print, true)) => format!("{print} \u{B7} passphrase"),
        Ok((print, false)) => print,
        Err(why) => why,
    };
    let small = |words: String| {
        div().font_family(fonts::MONO).text_size(12.0).text_color(token::TEXT_2).child(text(words))
    };
    let words = column()
        .grow(1.0)
        .min_width(0.0)
        .gap(token::SPACE_XXS)
        .child(div().font_weight(600).text_size(size::TEXT_MD.size).child(text(name)))
        .child(small(shown(&path)))
        .child(small(about));
    let gone = path.clone();
    let remove = button("Remove")
        .variant(Variant::Ghost)
        .on_click(move || keys.update(|it| it.retain(|k| *k != gone)));
    row().gap(token::SPACE_LG).align_items(Align::Center).label(path).child(words).child(remove)
}

fn ssh_folder() -> Option<PathBuf> {
    let home = std::env::var_os("USERPROFILE").map(PathBuf::from)?;
    Some(home.join(".ssh")).filter(|it| it.is_dir())
}

/// The private keys of a folder: the files beside a `.pub` of the same name.
fn found(folder: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(folder) else { return Vec::new() };
    let mut keys: Vec<PathBuf> = entries
        .filter_map(|it| it.ok().map(|it| it.path()))
        .filter(|it| it.extension().is_some_and(|it| it == "pub"))
        .map(|it| it.with_extension(""))
        .filter(|it| it.is_file())
        .collect();
    keys.sort();
    keys
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_folder_s_keys_are_the_files_beside_a_pub() {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.tmp/ssh-keys-found");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["id_ed25519", "id_ed25519.pub", "config", "known_hosts", "lone.pub"] {
            std::fs::write(dir.join(name), "").unwrap();
        }
        assert_eq!(super::found(&dir), [dir.join("id_ed25519")]);
    }
}
