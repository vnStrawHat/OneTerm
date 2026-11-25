//! An entry's properties (specs/oneterm, "M9 in detail"): asked of the server again, shown in a
//! dialog as a description list; its permissions are boxes to tick, and Apply sets them.

use oneterm_ssh::{Entry, join};
use quadra::{AnyView, View, div, text, untrack};
use quadra_design::{Notice, Tone, Variant, checkbox, copyable, description_list, dialog, fonts};

use super::Dock;
use crate::dock::Ask;
use crate::model::Model;
use crate::panes::{size, when};

impl Dock {
    /// Asks for an entry of the folder shown, then shows the dialog; what fails is a card.
    pub fn properties(&self, model: &Model, entry: &Entry) {
        let (path, notifies) = (join(&self.dir.get_untracked(), &entry.name), model.clone());
        let asked = path.clone();
        self.with_files(
            model,
            move |files| files.stat(&asked),
            move |dock, got| match got {
                Ok(stat) => {
                    for (at, on) in dock.ask.bits.iter().enumerate() {
                        on.set(stat.mode >> (8 - at) & 1 == 1);
                    }
                    dock.ask.stat.set(Some((path, stat)));
                    dock.ask.properties.set(true);
                }
                Err(why) => notifies.notify(
                    Notice::new("SFTP").body(format!("No properties of {path}: {why}")).tone(Tone::Warning),
                ),
            },
        );
    }
}

/// The boxes' names for a reader and a test: who, then what.
const BOXES: [&str; 9] =
    ["mode-ur", "mode-uw", "mode-ux", "mode-gr", "mode-gw", "mode-gx", "mode-or", "mode-ow", "mode-ox"];

/// The permission bits the nine boxes say, with the special bits `was` has (setuid, setgid,
/// sticky: not the boxes' to change).
fn ticked(ask: &Ask, was: u32) -> u32 {
    let bits = ask.bits.iter().enumerate().fold(0, |all, (at, on)| all | (u32::from(on.get()) << (8 - at)));
    (was & 0o7000) | bits
}

/// The dialog, Properties; it shows what was asked last. Apply sets the permissions ticked,
/// while they are not the entry's.
pub(crate) fn properties(model: &Model, d: Dock) -> impl View + use<> {
    let ask = d.ask;
    let changed = move || {
        let was = ask.stat.with(|it| it.as_ref().map_or(0, |it| it.1.mode));
        Some(ticked(&ask, was)).filter(|it| *it != was & 0o7777)
    };
    let applies = model.clone();
    let body = move || {
        let (path, stat) = ask.stat.get_untracked().unwrap_or_default();
        let name = path.rsplit('/').next().unwrap_or_default().to_string();
        let kind = format!(
            "{}{}",
            if stat.dir { "Folder" } else { "File" },
            if stat.link { " (symlink)" } else { "" }
        );
        let bytes = if stat.dir { "-".into() } else { format!("{} ({} bytes)", size(stat.size), stat.size) };
        let plain = |it: String| AnyView::new(text(it).color(quadra_design::token::TEXT));
        let mono = |it: AnyView| AnyView::new(div().font_family(fonts::MONO).child(it));
        // The permissions as they are ticked: the entry's until a box is changed.
        let was = stat.mode;
        let live = move || mode((was & !0o7777) | ticked(&ask, was));
        // Read, Write and Execute for one of the three, as boxes in a row.
        let boxes = |from: usize| {
            let row = ["Read", "Write", "Execute"].into_iter().enumerate().fold(
                quadra_design::hstack(),
                |row, (at, name)| {
                    row.child(checkbox(ask.bits[from + at]).label(name).test_id(BOXES[from + at]))
                },
            );
            AnyView::new(row)
        };
        let rows = [
            ("Name", plain(name)),
            ("Type", plain(kind)),
            ("Size", plain(bytes)),
            ("Modified", plain(when(stat.modified))),
            ("Accessed", plain(when(stat.accessed))),
            ("Permissions", mono(AnyView::new(text(live).color(quadra_design::token::TEXT)))),
            ("Owner can", boxes(0)),
            ("Group can", boxes(3)),
            ("Others can", boxes(6)),
            ("Owner", plain(owner(stat.owner.as_deref(), stat.uid))),
            ("Group", plain(owner(stat.group.as_deref(), stat.gid))),
            ("Path", mono(copyable(path))),
        ];
        description_list(rows)
    };
    dialog(ask.properties, "Properties", body)
        .action("Close", Variant::Secondary, || {})
        .action("Apply", Variant::Primary, move || {
            if let (Some(mode), Some((path, _))) = (untrack(changed), ask.stat.get_untracked()) {
                d.chmod(&applies, path, mode);
            }
        })
        .disabled(move || changed().is_none())
}

/// `drwxr-xr-x (0755)`: [`letters`], then the octal.
pub(crate) fn mode(mode: u32) -> String {
    format!("{} ({:04o})", letters(mode), mode & 0o7777)
}

/// `drwxr-xr-x`: the kind, rwx for the owner, the group and the others with the special bits
/// as `s`, `S`, `t`, `T`; nothing for no mode.
pub(crate) fn letters(mode: u32) -> String {
    if mode == 0 {
        return String::new();
    }
    let kind = match mode & 0o170000 {
        0o040000 => 'd',
        0o120000 => 'l',
        0o020000 => 'c',
        0o060000 => 'b',
        0o010000 => 'p',
        0o140000 => 's',
        _ => '-',
    };
    let mut out = String::from(kind);
    // Each third: its shift, and the special bit with its letter.
    for (shift, special, letter) in [(6, 0o4000, 's'), (3, 0o2000, 's'), (0, 0o1000, 't')] {
        let bits = (mode >> shift) & 7;
        out.push(if bits & 4 != 0 { 'r' } else { '-' });
        out.push(if bits & 2 != 0 { 'w' } else { '-' });
        out.push(match (bits & 1 != 0, mode & special != 0) {
            (true, true) => letter,
            (false, true) => letter.to_ascii_uppercase(),
            (true, false) => 'x',
            (false, false) => '-',
        });
    }
    out
}

/// `name (id)`, the id alone, or `-`.
pub(crate) fn owner(name: Option<&str>, id: Option<u32>) -> String {
    match (name, id) {
        (Some(name), Some(id)) => format!("{name} ({id})"),
        (Some(name), None) => name.to_string(),
        (None, Some(id)) => id.to_string(),
        (None, None) => "-".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_and_owners() {
        assert_eq!(mode(0o100644), "-rw-r--r-- (0644)");
        assert_eq!((super::letters(0o040755), super::letters(0)), ("drwxr-xr-x".to_string(), String::new()));
        assert_eq!(mode(0o040755), "drwxr-xr-x (0755)");
        assert_eq!(mode(0o120777), "lrwxrwxrwx (0777)");
        assert_eq!(mode(0o104755), "-rwsr-xr-x (4755)");
        assert_eq!(mode(0o104644), "-rwSr--r-- (4644)");
        assert_eq!(mode(0o102755), "-rwxr-sr-x (2755)");
        assert_eq!(mode(0o041777), "drwxrwxrwt (1777)");
        assert_eq!(mode(0o041776), "drwxrwxrwT (1776)");
        assert_eq!(mode(0o020666), "crw-rw-rw- (0666)");
        assert_eq!(owner(Some("root"), Some(0)), "root (0)");
        assert_eq!(owner(None, Some(1000)), "1000");
        assert_eq!(owner(None, None), "-");
    }
}
