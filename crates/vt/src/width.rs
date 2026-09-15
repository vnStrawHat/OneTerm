//! How many columns a character occupies.
//!
//! Design:
//! <https://github.com/vnStrawHat/OneTerm/blob/main/docs/spec-intakes/IN-0029-vt-engine/low-level-design/cell-and-style.md>
//!
//! Two functions, and the mode decides which the print path uses. With mode
//! 2027 reset — the power-on state, and what the parity recordings pin —
//! width is decided per scalar by [`scalar_width`], which is what most
//! terminals do: a ZWJ family emoji lands as a base plus a zero-width tail and
//! the next emoji starts a new cell. With `? 2027` set, the print path
//! segments the run into grapheme clusters and measures each with
//! [`cluster_width`], so a whole cluster takes one cell.
//!
//! The glyph-width axis is settled outside this module: the pseudo-console is
//! spawned with `PSEUDOCONSOLE_GLYPH_WIDTH_WCSWIDTH`, which matches this module
//! unconditionally. Because the engine never changes that mode mid-session, the
//! column drift a mid-session change would cause cannot occur.

use unicode_width::UnicodeWidthChar;

const ZERO_WIDTH_JOINER: char = '\u{200D}';
/// VS15: ask for the text (narrow) presentation of the preceding base.
const TEXT_PRESENTATION: char = '\u{FE0E}';
/// VS16: ask for the emoji (wide) presentation of the preceding base.
const EMOJI_PRESENTATION: char = '\u{FE0F}';

/// Columns one scalar occupies, or `None` for a control or unassigned
/// character — which the print path drops.
///
/// Width 0 means the scalar attaches to the previous cell's cluster.
pub fn scalar_width(c: char) -> Option<u8> {
    c.width().map(|width| width.min(2) as u8)
}

/// Columns one grapheme cluster occupies, for the mode 2027 print path.
///
/// The rules, in order: a flag (two or more regional indicators) is one wide
/// cell; an explicit presentation selector decides; otherwise the cluster is as
/// wide as its first non-zero-width scalar, clamped to two columns — so a ZWJ
/// sequence and a skin-tone sequence each measure one cell, however many
/// codepoints they carry.
pub fn cluster_width(cluster: &[char]) -> u8 {
    debug_assert!(
        is_at_most_one_cluster(cluster),
        "cluster_width expects a single grapheme cluster"
    );
    if cluster
        .iter()
        .filter(|c| is_regional_indicator(**c))
        .count()
        >= 2
    {
        return 2;
    }
    if cluster.contains(&EMOJI_PRESENTATION) {
        return 2;
    }
    if cluster.contains(&TEXT_PRESENTATION) {
        return 1;
    }
    cluster
        .iter()
        .filter(|c| **c != ZERO_WIDTH_JOINER)
        .find_map(|c| scalar_width(*c).filter(|width| *width > 0))
        .unwrap_or(0)
}

fn is_regional_indicator(c: char) -> bool {
    ('\u{1F1E6}'..='\u{1F1FF}').contains(&c)
}

/// Debug-only integrity check on the caller: `cluster_width` is meaningless for
/// a slice holding more than one cluster, and the mode 2027 print path is the
/// only caller that can satisfy it.
fn is_at_most_one_cluster(cluster: &[char]) -> bool {
    let text: String = cluster.iter().collect();
    unicode_segmentation::UnicodeSegmentation::graphemes(text.as_str(), true).count() <= 1
}

// Kept in a sibling file (the suite is substantial) while staying
// `width::tests`, which is the path the design's verification list names.
#[cfg(test)]
#[path = "width_tests.rs"]
mod tests;
