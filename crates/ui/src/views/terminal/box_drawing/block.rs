//! Block element primitives (U+2580–U+259F, U+2590).

pub(crate) fn is_block_element(c: char) -> bool {
    matches!(c, '\u{2580}'..='\u{259F}')
}

/// Append the block-element rects for `c` into `out` (cleared first).
///
/// Allocation-free: this is the DOOM-fire hot path (nearly every cell is a
/// block glyph, most often `▀` U+2580), so it must not allocate a `Vec` per
/// cell. The caller supplies a reusable buffer.
pub(crate) fn rects_into(
    out: &mut Vec<(i32, i32, i32, i32)>,
    c: char,
    cw_d: i32,
    lh_d: i32,
    cx: i32,
    cy: i32,
) {
    out.clear();
    match c {
        '\u{2580}' => out.push((0, 0, cw_d, cy)), // ▀ upper half
        '\u{2581}' => out.push((0, lh_d - lh_d / 8, cw_d, lh_d / 8)), // ▁ lower 1/8
        '\u{2582}' => out.push((0, lh_d - lh_d / 4, cw_d, lh_d / 4)), // ▂ lower 1/4
        '\u{2583}' => out.push((0, lh_d - 3 * lh_d / 8, cw_d, 3 * lh_d / 8)), // ▃
        '\u{2584}' => out.push((0, cy, cw_d, lh_d - cy)), // ▄ lower half
        '\u{2585}' => out.push((0, lh_d - 5 * lh_d / 8, cw_d, 5 * lh_d / 8)), // ▅
        '\u{2586}' => out.push((0, lh_d - lh_d / 4, cw_d, lh_d / 4 * 3)), // ▆ lower 3/4
        '\u{2587}' => out.push((0, lh_d / 8, cw_d, lh_d - lh_d / 8)), // ▇ lower 7/8
        '\u{2588}' => out.push((0, 0, cw_d, lh_d)), // █ full block
        '\u{2589}' => out.push((0, 0, 7 * cw_d / 8, lh_d)), // ▉ left 7/8
        '\u{258A}' => out.push((0, 0, 3 * cw_d / 4, lh_d)), // ▊ left 3/4
        '\u{258B}' => out.push((0, 0, 5 * cw_d / 8, lh_d)), // ▋ left 5/8
        '\u{258C}' => out.push((0, 0, cx, lh_d)), // ▌ left half
        '\u{258D}' => out.push((0, 0, 3 * cw_d / 8, lh_d)), // ▍ left 3/8
        '\u{258E}' => out.push((0, 0, cw_d / 4, lh_d)), // ▎ left 1/4
        '\u{258F}' => out.push((0, 0, cw_d / 8, lh_d)), // ▏ left 1/8
        '\u{2594}' => out.push((0, 0, cw_d, lh_d / 8)), // ▔ upper 1/8
        '\u{2595}' => out.push((cw_d - cw_d / 8, 0, cw_d / 8, lh_d)), // ▕ right 1/8
        // Right half block (U+2590)
        '\u{2590}' => out.push((cw_d - cx, 0, cx, lh_d)), // ▐ right half
        // Quadrant blocks
        '\u{2596}' => out.push((0, cy, cx, lh_d - cy)), // ▖ lower-left
        '\u{2597}' => out.push((cx, cy, cw_d - cx, lh_d - cy)), // ▗ lower-right
        '\u{2598}' => out.push((0, 0, cx, cy)),         // ▘ upper-left
        '\u{2599}' => out.extend_from_slice(&[
            (0, 0, cx, cy),
            (0, cy, cx, lh_d - cy),
            (cx, 0, cw_d - cx, cy),
        ]), // ▙
        '\u{259A}' => out.extend_from_slice(&[(cx, 0, cw_d - cx, cy), (0, cy, cx, lh_d - cy)]), // ▚
        '\u{259B}' => out.extend_from_slice(&[(0, 0, cw_d, cy), (0, cy, cx, lh_d - cy)]), // ▛
        '\u{259C}' => out.extend_from_slice(&[(0, 0, cw_d, cy), (cx, cy, cw_d - cx, lh_d - cy)]), // ▜
        '\u{259D}' => out.push((cx, 0, cw_d - cx, cy)), // ▝ upper-right
        '\u{259E}' => out.extend_from_slice(&[(cx, 0, cw_d - cx, cy), (0, cy, cx, lh_d - cy)]), // ▞
        '\u{259F}' => out.extend_from_slice(&[
            (0, 0, cx, cy),
            (cx, 0, cw_d - cx, cy),
            (cx, cy, cw_d - cx, lh_d - cy),
        ]), // ▟
        _ => {}
    }
}

/// Vec-returning wrapper over [`rects_into`]. Kept for the cold (non-hot-loop)
/// callers and tests.
pub(crate) fn rects(c: char, cw_d: i32, lh_d: i32, cx: i32, cy: i32) -> Vec<(i32, i32, i32, i32)> {
    let mut out = Vec::new();
    rects_into(&mut out, c, cw_d, lh_d, cx, cy);
    out
}
