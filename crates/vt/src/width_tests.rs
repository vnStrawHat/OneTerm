use unicode_segmentation::UnicodeSegmentation;

use super::*;

/// The clusters of `text`, as the print path will produce them once mode 2027
/// lands.
fn clusters(text: &str) -> Vec<Vec<char>> {
    text.graphemes(true)
        .map(|cluster| cluster.chars().collect())
        .collect()
}

#[test]
fn scalar_widths_cjk_emoji_combining() {
    assert_eq!(scalar_width('a'), Some(1));
    assert_eq!(scalar_width(' '), Some(1));
    assert_eq!(scalar_width('漢'), Some(2));
    assert_eq!(scalar_width('か'), Some(2));
    assert_eq!(scalar_width('🙂'), Some(2));
    // Combining marks attach to the previous cell.
    assert_eq!(scalar_width('\u{301}'), Some(0));
    assert_eq!(scalar_width('\u{200D}'), Some(0));
    // Controls are dropped by the print path, not printed.
    assert_eq!(scalar_width('\u{0}'), None);
    assert_eq!(scalar_width('\u{7}'), None);
    assert_eq!(scalar_width('\u{1B}'), None);
    // Nothing is ever wider than a wide pair.
    for c in ['a', '漢', '🙂', '\u{301}'] {
        assert!(scalar_width(c).unwrap_or(0) <= 2);
    }
}

#[test]
fn zwj_family_splits_without_mode_2027() {
    // Pins today's behaviour, which the parity corpus records: per scalar, a
    // ZWJ family emoji is four wide glyphs and three zero-width joiners, so it
    // occupies eight columns instead of two.
    let family = "👨\u{200D}👩\u{200D}👧\u{200D}👦";
    let per_scalar: u32 = family
        .chars()
        .map(|c| u32::from(scalar_width(c).unwrap_or(0)))
        .sum();
    assert_eq!(per_scalar, 8);
    assert_eq!(clusters(family).len(), 1);

    // And the fix, ready for the print-path packet that turns the mode on.
    assert_eq!(cluster_width(&clusters(family)[0]), 2);
}

#[test]
fn cluster_width_is_correct_for_emoji_flags_and_skin_tones() {
    // Every one of these is one cell wide, however many codepoints it carries.
    for text in [
        "👨\u{200D}👩\u{200D}👧\u{200D}👦", // family, ZWJ sequence
        "👨\u{200D}💻",                     // person + ZWJ + laptop
        "👍🏽",                               // skin tone modifier
        "👋🏿",                               // skin tone modifier
        "🇺🇸",                               // flag, two regional indicators
        "🇻🇳",                               // flag
        "🏳\u{FE0F}\u{200D}🌈",              // flag + VS16 + ZWJ + rainbow
        "漢",
        "🙂",
    ] {
        let cluster = &clusters(text)[0];
        assert_eq!(
            cluster_width(cluster),
            2,
            "{text:?} should be one wide cell"
        );
    }

    // Narrow clusters stay narrow.
    for text in ["a", "e\u{301}", "#\u{FE0E}", "1\u{FE0E}"] {
        let cluster = &clusters(text)[0];
        assert_eq!(cluster_width(cluster), 1, "{text:?} should be one column");
    }

    // A keycap asks for the emoji presentation, so it is wide.
    assert_eq!(cluster_width(&clusters("#\u{FE0F}\u{20E3}")[0]), 2);

    // Degenerate input never panics.
    assert_eq!(cluster_width(&[]), 0);
    assert_eq!(cluster_width(&['\u{301}']), 0);
    assert_eq!(cluster_width(&['\u{7}']), 0);
}

#[test]
fn a_skin_tone_sequence_is_capped_by_the_grapheme_arena_not_by_width() {
    // The longest sequences the research names still fit the 16-codepoint cap,
    // so the width rule and the storage cap agree on real input.
    let handshake = "🧑🏻\u{200D}🤝\u{200D}🧑🏿";
    let cluster = &clusters(handshake)[0];
    assert!(cluster.len() <= crate::intern::GRAPHEME_MAX_LEN);
    assert_eq!(cluster_width(cluster), 2);
}
