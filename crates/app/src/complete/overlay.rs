//! The list of suggestions (specs/oneterm, "Completion"): on the plane of the kit's popups, rows as a
//! menu's in the mono face, a window of `max_visible_items` that follows the chosen row.

use oneterm_completion::{Kind, Suggestion};
use quadra::{Align, Div, Role, State, by_text, column, div, dynamic, row, text};
use quadra_design::{fonts, size, token};

use super::Shown;

/// The height of a row, and of a row that says how many more there are.
const ROW: f32 = 24.0;
const MORE: f32 = 20.0;

/// The first row shown and how many, of `n`, at most `most`, keeping `chosen` in view: at
/// the top until a row is chosen, then with the chosen one at the bottom edge as it goes down.
pub(super) fn window(n: usize, chosen: Option<usize>, most: usize) -> (usize, usize) {
    let most = most.max(1);
    if n <= most {
        return (0, n);
    }
    match chosen {
        Some(i) if i >= most => ((i + 1 - most).min(n - most), most),
        _ => (0, most),
    }
}

/// The plane and its rows, rebuilt when the list or the chosen row changes.
pub(super) fn list(shown: Shown, most: impl Fn() -> usize + 'static) -> Div {
    let rows = dynamic(move || {
        let chosen = shown.chosen.get();
        let (count, most) = (shown.list.with(Vec::len), most());
        let (first, many) = window(count, chosen, most);
        let mut rows = column();
        if first > 0 {
            rows = rows.child(more(format!("\u{2191} {first} more")));
        }
        rows = shown.list.with(|list| {
            list[first..first + many]
                .iter()
                .enumerate()
                .fold(rows, |rows, (i, item)| rows.child(face(item, chosen == Some(first + i))))
        });
        match count - first - many {
            0 => rows,
            below => rows.child(more(format!("\u{2193} {below} more"))),
        }
    });
    let plane = column().bg(token::OVERLAY_BG).border(1.0).border_color(token::LINE_STRONG);
    let plane = plane.radius(token::RADIUS_LG).elevation(1).padding(token::SPACE_XS).min_width(180.0);
    plane.font_family(fonts::MONO).role(Role::List).label("Suggestions").child(rows)
}

/// A suggestion: what matches in the accent's text colour, the rest, then the option's hint
/// and the kind in the second text colour; filled as a menu's row when chosen.
fn face(item: &Suggestion, chosen: bool) -> Div {
    let at = (0..=item.matched.min(item.text.len())).rev().find(|&at| item.text.is_char_boundary(at));
    let (matched, rest) = item.text.split_at(at.unwrap_or(0));
    let face =
        row().height(by_text(ROW)).padding_x(token::SPACE_SM).gap(token::SPACE_XL).radius(token::RADIUS_MD);
    let face = face.align_items(Align::Center).role(Role::ListItem).label(item.text.clone());
    let face = face.selected(chosen).when(State::SELECTED, |s| s.bg(token::PRESS));
    let words =
        row().child(text(matched.to_string()).color(token::ACCENT_TEXT)).child(text(rest.to_string()));
    let face = face.child(words).child(div().grow(1.0));
    let small = |words: String| text(words).size(size::TEXT_SM.size).color(token::TEXT_2);
    let face = match &item.detail {
        Some(detail) => face.child(small(detail.clone())),
        None => face,
    };
    let kind = match item.kind {
        Kind::Command => "command",
        Kind::Subcommand => "subcommand",
        Kind::Option => "option",
        Kind::Line => "history",
    };
    face.child(small(kind.into()))
}

/// A row that says how many more there are above or below.
fn more(words: String) -> Div {
    let words = text(words).size(size::TEXT_SM.size).color(token::TEXT_2);
    row().height(by_text(MORE)).padding_x(token::SPACE_SM).align_items(Align::Center).child(words)
}

#[cfg(test)]
#[test]
fn the_window_follows_the_chosen_row() {
    assert_eq!(window(5, None, 8), (0, 5));
    assert_eq!(window(32, None, 8), (0, 8));
    assert_eq!(window(32, Some(7), 8), (0, 8));
    assert_eq!(window(32, Some(8), 8), (1, 8));
    assert_eq!(window(32, Some(31), 8), (24, 8));
}
