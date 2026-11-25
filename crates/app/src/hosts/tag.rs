//! A saved host's colour tag (specs/oneterm, "M8 in detail"): the eighth word of its row, one of
//! seven hues by its name (so it follows the theme) or any colour as `#rrggbb`; else none, and
//! the host has its group's.

use oneterm_ssh::Target;
use quadra::{AnyView, Color, Paint, Signal, Token, View, dynamic};
use quadra_design::color_picker;

use super::Host;
use crate::model::Model;

/// The hues a tag can be by name: the word saved, the name the picker gives it, the role.
const TAGS: [(&str, &str, Token); 7] = {
    use quadra_design::token::*;
    [
        ("red", "Red", HUE_RED),
        ("orange", "Orange", HUE_ORANGE),
        ("yellow", "Yellow", HUE_YELLOW),
        ("green", "Green", HUE_GREEN),
        ("cyan", "Cyan", HUE_CYAN),
        ("blue", "Blue", HUE_BLUE),
        ("purple", "Purple", HUE_PURPLE),
    ]
};

/// The tag of a host with none: one by its group, the same for each host of the group.
pub(crate) fn tag_of_group(group: &str) -> (&'static str, Token) {
    let hash = group.bytes().fold(2_166_136_261u32, |h, b| (h ^ u32::from(b)).wrapping_mul(16_777_619));
    let (word, _, role) = TAGS[hash as usize % TAGS.len()];
    (word, role)
}

/// The colour picker's presets: each hue as the dark theme has it.
pub(crate) fn presets() -> [(&'static str, Color); 7] {
    let theme = quadra_design::dark();
    TAGS.map(|(_, name, role)| (name, theme.color(role)))
}

/// `#rrggbb` in lower case, as the picker writes.
fn hex(color: Color) -> String {
    color.hex()[..7].to_string()
}

/// What a saved word paints: a hue by its name, or a colour; anything else is no tag.
fn paint(word: &str) -> Option<Paint> {
    match TAGS.iter().find(|(it, ..)| *it == word) {
        Some(&(.., role)) => Some(role.into()),
        None => Color::parse_hex(word).map(Paint::from),
    }
}

/// What the picker holds for a saved word: a hue as its preset is, else the word.
pub(crate) fn picked(word: &str) -> String {
    match TAGS.iter().position(|(it, ..)| *it == word) {
        Some(at) => hex(presets()[at].1),
        None => word.to_string(),
    }
}

/// The word saved for what the picker holds: a preset's colour by its hue's name, another
/// colour as `#rrggbb`; empty or not a colour, none.
pub(crate) fn word_of(value: &str) -> Option<String> {
    let color = hex(Color::parse_hex(value.trim())?);
    match presets().iter().position(|(_, it)| hex(*it) == color) {
        Some(at) => Some(TAGS[at].0.to_string()),
        None => Some(color),
    }
}

/// The dialog's picker of a tag: the seven hues, and for its default the colour of the group
/// as typed (the picker is made again as the group changes).
pub(crate) fn tag_picker(tag: Signal<String>, group: Signal<String>) -> impl View {
    dynamic(move || {
        let fallback = tag_of_group(group.get().trim()).1;
        let picker = color_picker(tag).label("Colour tag").presets(presets()).fallback(fallback);
        AnyView::new(picker.placeholder("By group").test_id("host-tag"))
    })
}

impl Model {
    /// A host's colour tag, its word and paint: the one saved with it, else its group's.
    pub(crate) fn host_tag(&self, host: &Host) -> (String, Paint) {
        self.saved_tag(&host.name).unwrap_or_else(|| {
            let (word, role) = tag_of_group(&host.group);
            (word.to_string(), role.into())
        })
    }

    /// The tag saved with the host of this name, its word and paint, if one was.
    pub(crate) fn saved_tag(&self, name: &str) -> Option<(String, Paint)> {
        self.prefs().hosts.with(|rows| {
            let word = rows.iter().find(|it| it.first().map(String::as_str) == Some(name))?.get(7)?;
            Some((word.clone(), paint(word)?))
        })
    }

    /// The colour tag of the host reached at a target, else the tag of no group.
    pub(crate) fn target_tag(&self, target: &Target) -> Paint {
        let host = self.hosts().into_iter().find(|it| it.remote.target == *target);
        host.map_or(tag_of_group("").1.into(), |it| self.host_tag(&it).1)
    }

    /// Gives a saved host a colour tag (a word of [`word_of`]), or none: its group's.
    pub(crate) fn set_host_tag(&self, name: &str, tag: Option<&str>) {
        self.prefs().hosts.update(|rows| {
            if let Some(row) = rows.iter_mut().find(|it| it.first().map(String::as_str) == Some(name)) {
                row.resize(row.len().max(7), String::new());
                row.truncate(7);
                row.extend(tag.map(str::to_string));
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tag_is_a_hue_by_name_or_a_colour() {
        assert_eq!(tag_of_group("Production"), tag_of_group("Production"));
        let red = hex(presets()[0].1);
        assert_eq!(picked("red"), red);
        assert_eq!(word_of(&red).as_deref(), Some("red"));
        assert_eq!(word_of(" #123ABC ").as_deref(), Some("#123abc"));
        assert_eq!((word_of(""), word_of("red")), (None, None));
        assert_eq!(picked("#123abc"), "#123abc");
        assert_eq!(paint("red"), Some(quadra_design::token::HUE_RED.into()));
        assert_eq!(paint("#123abc"), Some(Color::rgb(0x12_3abc).into()));
        assert_eq!(paint("mauve"), None);
    }
}
