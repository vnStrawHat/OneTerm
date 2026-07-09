//! Core box-drawing rect computation (light / heavy / double / tees / crosses).

/// Check whether a char belongs to the box-drawing / block / powerline glyphs
/// that Windows Terminal AtlasEngine draws with primitives instead of font glyphs.
/// Includes: U+2500–U+257F (box drawing), U+2580–U+259F (block elements),
/// U+25AC ▬ (TUI horizontal bar), U+E0B0–U+E0BF (powerline symbols).
pub(crate) fn is_box_drawing(c: char) -> bool {
    matches!(
        c,
        '\u{2500}'..='\u{257F}' | '\u{2580}'..='\u{259F}' | '\u{25AC}' | '\u{E0B0}'..='\u{E0BF}'
    )
}

/// Rounded corners (U+256D–U+2570) — drawn with a separate anti-aliased path
/// (`rounded_corner_rects_aa`), not via `box_drawing_rects`. Used for a quick
/// probe in layout instead of computing geometry just to check for emptiness.
pub(crate) fn is_rounded_corner(c: char) -> bool {
    matches!(c, '\u{256D}'..='\u{2570}')
}

/// Append the primitive rects for `c` into `out` (cleared first).
///
/// Allocation-free variant of [`box_drawing_rects`] for the render hot loop.
/// Block elements (the DOOM-fire hot path — nearly every cell is `▀` U+2580)
/// are filled directly into `out` with no intermediate allocation. Rarer
/// glyphs (lines, corners, powerline, shade, dashes) fall back to the tested
/// match; the caller's reusable buffer still avoids re-allocating across cells.
pub(crate) fn box_drawing_rects_into(
    out: &mut Vec<(i32, i32, i32, i32)>,
    c: char,
    cw_d: i32,
    lh_d: i32,
) {
    out.clear();
    // Fast path: block elements — no allocation (see `block::rects_into`).
    if super::block::is_block_element(c) {
        let cx = cw_d / 2;
        let cy = lh_d / 2;
        super::block::rects_into(out, c, cw_d, lh_d, cx, cy);
        return;
    }
    // Cold path: lines / corners / powerline / shade / dashes. These are rare
    // compared to the block hot path, so a transient `Vec` here is acceptable.
    out.extend_from_slice(&box_drawing_rects(c, cw_d, lh_d));
}

/// Whether `c` has custom primitive geometry (box-drawing / block / powerline /
/// shade / rounded). Used by layout to decide whether a cell is drawn with
/// primitives or falls back to a font glyph — without allocating a throwaway
/// `Vec` per cell. Reuses `out` (a caller-owned scratch buffer) so the block
/// hot path stays allocation-free.
pub(crate) fn has_box_geometry(out: &mut Vec<(i32, i32, i32, i32)>, c: char) -> bool {
    if is_rounded_corner(c) {
        return true;
    }
    box_drawing_rects_into(out, c, 16, 16);
    !out.is_empty()
}

/// Compute pixel-perfect geometry for a box-drawing char within a cell.
/// Returns a list of rects (x, y, w, h) in **device pixels** relative to the
/// cell origin. The caller converts to logical px when painting.
/// Like AtlasEngine: light = 1 device px, heavy = 2, double = 2 lines.
pub(crate) fn box_drawing_rects(c: char, cw_d: i32, lh_d: i32) -> Vec<(i32, i32, i32, i32)> {
    let cx = cw_d / 2;
    let cy = lh_d / 2;
    // Windows Terminal AtlasEngine: light stroke width scales with the
    // cell width so adjacent single-line segments stay visually connected
    // (no hairline gaps) and corners remain solid.  Heavy is ~2x light.
    let t = (cw_d as f32 / 6.0).round().max(1.0) as i32; // light thickness
    let ht = super::heavy_thickness(cw_d); // heavy thickness
    // Double-line stroke: make the two parallel strokes as thick as light
    // lines so they look solid instead of hairline. Center each stroke
    // around its offset so the pair stays symmetric in the cell.
    let dt = (cw_d as f32 / 5.0).round().max(1.0) as i32;
    let dl = dt;
    let dv = dt;
    let half_dt = dt / 2;
    let x_out = (cx - dl).max(half_dt);
    let x_in = (cx + dl).min(cw_d - half_dt);
    let y_out = (cy - dv).max(half_dt);
    let y_in = (cy + dv).min(lh_d - half_dt);
    let y_out_top = (y_out - half_dt).max(0);
    let y_in_top = (y_in - half_dt).min(lh_d - dt);
    let y_out_bot = y_out_top + dt;
    let y_in_bot = y_in_top + dt;
    let x_out_left = (x_out - half_dt).max(0);
    let x_in_left = (x_in - half_dt).min(cw_d - dt);

    // All strokes are **centered** around the cell's center axis: a line of
    // thickness `thick` occupies [center - thick/2, center - thick/2 + thick).
    // This keeps a heavy line (ht ≈ 5px) from drifting right (vertical) or
    // down (horizontal), and the join with a rounded corner sits exactly at center.
    //   - `$cy` / `$cx`: the line's center axis (always `cy` / `cx`).
    //   - half-lines (hr/hl/vd/vu) start `thick/2` back toward the
    //     perpendicular axis to match the centered line, keeping corners solid.
    macro_rules! h {
        ($cy:expr, $thick:expr) => {
            (0, $cy - $thick / 2, cw_d, $thick)
        };
    }
    macro_rules! v {
        ($cx:expr, $thick:expr) => {
            ($cx - $thick / 2, 0, $thick, lh_d)
        };
    }
    macro_rules! hr {
        ($cy:expr, $thick:expr) => {
            (
                cx - $thick / 2,
                $cy - $thick / 2,
                cw_d - (cx - $thick / 2),
                $thick,
            )
        };
    }
    macro_rules! hl {
        ($cy:expr, $thick:expr) => {
            (
                0,
                $cy - $thick / 2,
                (cx + $thick - $thick / 2).min(cw_d),
                $thick,
            )
        };
    }
    macro_rules! vd {
        ($cx:expr, $thick:expr) => {
            (
                $cx - $thick / 2,
                cy - $thick / 2,
                $thick,
                lh_d - (cy - $thick / 2),
            )
        };
    }
    macro_rules! vu {
        ($cx:expr, $thick:expr) => {
            (
                $cx - $thick / 2,
                0,
                $thick,
                (cy + $thick - $thick / 2).min(lh_d),
            )
        };
    }

    match c {
        '\u{2500}' => vec![h!(cy, t)],
        '\u{2501}' => vec![h!(cy, ht)],
        '\u{2502}' => vec![v!(cx, t)],
        '\u{2503}' => vec![v!(cx, ht)],
        '\u{250C}' => vec![vd!(cx, t), hr!(cy, t)],
        '\u{250D}' => vec![vd!(cx, ht), hr!(cy, t)],
        '\u{250E}' => vec![vd!(cx, t), hr!(cy, ht)],
        '\u{250F}' => vec![vd!(cx, ht), hr!(cy, ht)],
        '\u{2510}' => vec![vd!(cx, t), hl!(cy, t)],
        '\u{2511}' => vec![vd!(cx, ht), hl!(cy, t)],
        '\u{2512}' => vec![vd!(cx, t), hl!(cy, ht)],
        '\u{2513}' => vec![vd!(cx, ht), hl!(cy, ht)],
        '\u{2514}' => vec![vu!(cx, t), hr!(cy, t)],
        '\u{2515}' => vec![vu!(cx, ht), hr!(cy, t)],
        '\u{2516}' => vec![vu!(cx, t), hr!(cy, ht)],
        '\u{2517}' => vec![vu!(cx, ht), hr!(cy, ht)],
        '\u{2518}' => vec![vu!(cx, t), hl!(cy, t)],
        '\u{2519}' => vec![vu!(cx, ht), hl!(cy, t)],
        '\u{251A}' => vec![vu!(cx, t), hl!(cy, ht)],
        '\u{251B}' => vec![vu!(cx, ht), hl!(cy, ht)],
        '\u{251C}' => vec![v!(cx, t), hr!(cy, t)],
        '\u{251D}' => vec![v!(cx, ht), hr!(cy, t)],
        '\u{251E}' => vec![vu!(cx, ht), vd!(cx, t), hr!(cy, t)],
        '\u{251F}' => vec![vu!(cx, t), vd!(cx, ht), hr!(cy, t)],
        '\u{2520}' => vec![v!(cx, ht), hr!(cy, ht)],
        '\u{2521}' => vec![vu!(cx, ht), vd!(cx, t), hr!(cy, ht)],
        '\u{2522}' => vec![vu!(cx, t), vd!(cx, ht), hr!(cy, ht)],
        '\u{2523}' => vec![v!(cx, ht), hr!(cy, ht)],
        '\u{2524}' => vec![v!(cx, t), hl!(cy, t)],
        '\u{2525}' => vec![v!(cx, ht), hl!(cy, t)],
        '\u{2526}' => vec![vu!(cx, ht), vd!(cx, t), hl!(cy, t)],
        '\u{2527}' => vec![vu!(cx, t), vd!(cx, ht), hl!(cy, t)],
        '\u{2528}' => vec![v!(cx, ht), hl!(cy, ht)],
        '\u{2529}' => vec![vu!(cx, ht), vd!(cx, t), hl!(cy, ht)],
        '\u{252A}' => vec![vu!(cx, t), vd!(cx, ht), hl!(cy, ht)],
        '\u{252B}' => vec![v!(cx, ht), hl!(cy, ht)],
        '\u{252C}' => vec![h!(cy, t), vd!(cx, t)],
        '\u{252D}' => vec![hl!(cy, ht), hr!(cy, t), vd!(cx, t)],
        '\u{252E}' => vec![hl!(cy, t), hr!(cy, ht), vd!(cx, t)],
        '\u{252F}' => vec![h!(cy, ht), vd!(cx, t)],
        '\u{2530}' => vec![h!(cy, t), vd!(cx, ht)],
        '\u{2531}' => vec![hl!(cy, ht), hr!(cy, t), vd!(cx, ht)],
        '\u{2532}' => vec![hl!(cy, t), hr!(cy, ht), vd!(cx, ht)],
        '\u{2533}' => vec![h!(cy, ht), vd!(cx, ht)],
        '\u{2534}' => vec![h!(cy, t), vu!(cx, t)],
        '\u{2535}' => vec![hl!(cy, ht), hr!(cy, t), vu!(cx, t)],
        '\u{2536}' => vec![hl!(cy, t), hr!(cy, ht), vu!(cx, t)],
        '\u{2537}' => vec![h!(cy, ht), vu!(cx, t)],
        '\u{2538}' => vec![h!(cy, t), vu!(cx, ht)],
        '\u{2539}' => vec![hl!(cy, ht), hr!(cy, t), vu!(cx, ht)],
        '\u{253A}' => vec![hl!(cy, t), hr!(cy, ht), vu!(cx, ht)],
        '\u{253B}' => vec![h!(cy, ht), vu!(cx, ht)],
        '\u{253C}' => vec![h!(cy, t), v!(cx, t)],
        '\u{253D}' => vec![hl!(cy, ht), hr!(cy, t), v!(cx, t)],
        '\u{253E}' => vec![hl!(cy, t), hr!(cy, ht), v!(cx, t)],
        '\u{253F}' => vec![h!(cy, ht), v!(cx, t)],
        '\u{2540}' => vec![h!(cy, t), vu!(cx, ht), vd!(cx, t)],
        '\u{2541}' => vec![h!(cy, t), vu!(cx, t), vd!(cx, ht)],
        '\u{2542}' => vec![h!(cy, ht), v!(cx, ht)],
        '\u{2543}' => vec![hl!(cy, ht), hr!(cy, t), vu!(cx, ht), vd!(cx, t)],
        '\u{2544}' => vec![hl!(cy, t), hr!(cy, ht), vu!(cx, ht), vd!(cx, t)],
        '\u{2545}' => vec![hl!(cy, ht), hr!(cy, t), vu!(cx, t), vd!(cx, ht)],
        '\u{2546}' => vec![hl!(cy, t), hr!(cy, ht), vu!(cx, t), vd!(cx, ht)],
        '\u{2547}' => vec![h!(cy, ht), vu!(cx, ht), vd!(cx, t)],
        '\u{2548}' => vec![h!(cy, ht), vu!(cx, t), vd!(cx, ht)],
        '\u{2549}' => vec![hl!(cy, ht), hr!(cy, ht), vu!(cx, ht), vd!(cx, t)],
        '\u{254A}' => vec![hl!(cy, ht), hr!(cy, ht), vu!(cx, t), vd!(cx, ht)],
        '\u{254B}' => vec![h!(cy, ht), v!(cx, ht)],
        // Triple / quadruple dash lines.
        '\u{2504}' | '\u{2508}' => dash_h(cy, cw_d, t),
        '\u{2505}' | '\u{2509}' => dash_h(cy, cw_d, ht),
        '\u{2506}' | '\u{250A}' => dash_v(cx, lh_d, t),
        '\u{2507}' | '\u{250B}' => dash_v(cx, lh_d, ht),
        // double lines
        '\u{2550}' => vec![(0, y_out_top, cw_d, dt), (0, y_in_top, cw_d, dt)],
        '\u{2551}' => vec![(x_out_left, 0, dt, lh_d), (x_in_left, 0, dt, lh_d)],
        // double corners
        '\u{2554}' => vec![
            (x_out_left, y_out_top, dt, lh_d - y_out_top),
            (x_out_left, y_out_top, cw_d - x_out_left, dt),
            (x_in_left, y_in_top, dt, lh_d - y_in_top),
            (x_in_left, y_in_top, cw_d - x_in_left, dt),
        ],
        '\u{2557}' => vec![
            (x_in_left, y_out_top, dt, lh_d - y_out_top),
            (0, y_out_top, x_in_left + dt, dt),
            (x_out_left, y_in_top, dt, lh_d - y_in_top),
            (0, y_in_top, x_out_left + dt, dt),
        ],
        '\u{255A}' => vec![
            (x_out_left, 0, dt, y_in_bot),
            (x_out_left, y_in_top, cw_d - x_out_left, dt),
            (x_in_left, 0, dt, y_out_bot),
            (x_in_left, y_out_top, cw_d - x_in_left, dt),
        ],
        '\u{255D}' => vec![
            (x_in_left, 0, dt, y_in_bot),
            (0, y_in_top, x_in_left + dt, dt),
            (x_out_left, 0, dt, y_out_bot),
            (0, y_out_top, x_out_left + dt, dt),
        ],
        // Mixed-light double corners
        '\u{2552}' => vec![
            (cx, 0, t, lh_d),
            (cx, y_out, cw_d - cx, t),
            (cx, y_in, cw_d - cx, t),
        ],
        '\u{2553}' => vec![
            (x_out, 0, t, lh_d),
            (x_in, 0, t, lh_d),
            (cx, y_out, cw_d - cx, t),
        ],
        '\u{2555}' => vec![(cx, 0, t, lh_d), (0, y_out, cx, t), (0, y_in, cx, t)],
        '\u{2556}' => vec![(x_out, 0, t, lh_d), (x_in, 0, t, lh_d), (0, y_out, cx, t)],
        '\u{2558}' => vec![
            (cx, 0, t, lh_d),
            (cx, y_out, cw_d - cx, t),
            (cx, y_in, cw_d - cx, t),
        ],
        '\u{2559}' => vec![
            (x_out, 0, t, lh_d),
            (x_in, 0, t, lh_d),
            (cx, y_out, cw_d - cx, t),
        ],
        '\u{255B}' => vec![(cx, 0, t, lh_d), (0, y_out, cx, t), (0, y_in, cx, t)],
        '\u{255C}' => vec![(x_out, 0, t, lh_d), (x_in, 0, t, lh_d), (0, y_out, cx, t)],
        // Tee/cross pieces
        '\u{255E}' => vec![
            (cx, 0, t, lh_d),
            (cx, y_out, cw_d - cx, t),
            (cx, y_in, cw_d - cx, t),
        ],
        '\u{255F}' => vec![
            (x_out, 0, t, lh_d),
            (x_in, 0, t, lh_d),
            (x_in, y_out, cw_d - x_in, t),
        ],
        '\u{2560}' => vec![
            (x_out, 0, t, lh_d),
            (x_in, 0, t, lh_d),
            (x_out + 1, y_out, cw_d - x_out - 1, t),
            (x_in + 1, y_in, cw_d - x_in - 1, t),
        ],
        '\u{2561}' => vec![(cx, 0, t, lh_d), (0, y_out, cx, t), (0, y_in, cx, t)],
        '\u{2562}' => vec![
            (x_out, 0, t, lh_d),
            (x_in, 0, t, lh_d),
            (0, y_out, x_out + 1, t),
        ],
        '\u{2563}' => vec![
            (x_out, 0, t, lh_d),
            (x_in, 0, t, lh_d),
            (0, y_out, x_out + 1, t),
            (0, y_in, x_in + 1, t),
        ],
        '\u{2564}' => vec![
            (cx, y_out, t, lh_d - y_out),
            (0, y_out, cw_d, t),
            (0, y_in, cw_d, t),
        ],
        '\u{2565}' => vec![
            (x_out, y_out, t, lh_d - y_out),
            (x_in, y_out, t, lh_d - y_out),
            (0, cy, cw_d, t),
        ],
        '\u{2566}' => vec![
            (x_out, y_out, t, lh_d - y_out),
            (x_in, y_out, t, lh_d - y_out),
            (x_out, y_in, t, lh_d - y_in),
            (x_in, y_in, t, lh_d - y_in),
            (0, y_out, cw_d, t),
            (0, y_in, cw_d, t),
        ],
        '\u{2567}' => vec![
            (cx, 0, t, y_out + 1),
            (0, y_out, cw_d, t),
            (0, y_in, cw_d, t),
        ],
        '\u{2568}' => vec![
            (x_out, 0, t, y_out + 1),
            (x_in, 0, t, y_out + 1),
            (0, cy, cw_d, t),
        ],
        '\u{2569}' => vec![
            (x_out, 0, t, y_out + 1),
            (x_in, 0, t, y_out + 1),
            (x_out, y_in, t, lh_d - y_in),
            (x_in, y_in, t, lh_d - y_in),
            (0, y_out, cw_d, t),
            (0, y_in, cw_d, t),
        ],
        // Crosses
        '\u{256A}' => vec![(cx, 0, t, lh_d), (0, y_out, cw_d, t), (0, y_in, cw_d, t)],
        '\u{256B}' => vec![(x_out, 0, t, lh_d), (x_in, 0, t, lh_d), (0, cy, cw_d, t)],
        '\u{256C}' => vec![
            (x_out, 0, t, lh_d),
            (x_in, 0, t, lh_d),
            (0, y_out, cw_d, t),
            (0, y_in, cw_d, t),
        ],
        // Rounded corners — handled by AA path in `paint`, not here.
        '\u{256D}' | '\u{256E}' | '\u{256F}' | '\u{2570}' => Vec::new(),
        // Half lines
        '\u{2574}' => vec![hl!(cy, t)],
        '\u{2575}' => vec![vu!(cx, t)],
        '\u{2576}' => vec![hr!(cy, t)],
        '\u{2577}' => vec![vd!(cx, t)],
        '\u{2578}' => vec![hl!(cy, ht)],
        '\u{2579}' => vec![vu!(cx, ht)],
        '\u{257A}' => vec![hr!(cy, ht)],
        '\u{257B}' => vec![vd!(cx, ht)],
        // Horizontal bar
        '\u{25AC}' => vec![(0, lh_d / 4, cw_d, lh_d / 2)],
        // Block elements
        c if super::block::is_block_element(c) => super::block::rects(c, cw_d, lh_d, cx, cy),
        // Powerline symbols
        c if super::powerline::is_powerline(c) => super::powerline::rects(c, cw_d, lh_d),
        // Shade blocks
        c @ ('\u{2591}' | '\u{2592}' | '\u{2593}') => super::shade::rects(c, cw_d, lh_d),
        // diagonal / quadruple-dash → fallback font
        _ => vec![],
    }
}

fn dash_h(y: i32, w: i32, thick: i32) -> Vec<(i32, i32, i32, i32)> {
    let mut out = Vec::new();
    let mut x = 0;
    while x < w {
        let ew = 2.min(w - x);
        out.push((x, y, ew, thick));
        x += 4;
    }
    out
}

fn dash_v(x: i32, h: i32, thick: i32) -> Vec<(i32, i32, i32, i32)> {
    let mut out = Vec::new();
    let mut y = 0;
    while y < h {
        let eh = 2.min(h - y);
        out.push((x, y, thick, eh));
        y += 4;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{box_drawing_rects, box_drawing_rects_into, has_box_geometry, is_rounded_corner};

    /// Every char that `box_drawing_rects` (the tested match) can emit.
    fn all_primitive_chars() -> impl Iterator<Item = char> {
        (0x2500u32..=0x259F)
            .chain(0x25ACu32..=0x25AC)
            .chain(0xE0B0u32..=0xE0BF)
            .filter_map(char::from_u32)
    }

    #[test]
    fn rects_into_matches_vec_variant() {
        // The allocation-free hot path (`box_drawing_rects_into`, including the
        // block fast path) must produce exactly the same geometry as the
        // Vec-returning reference for every supported glyph and cell size.
        let mut buf = Vec::new();
        for cw_d in [8, 16, 24, 33] {
            for lh_d in [12, 24, 36, 40] {
                for c in all_primitive_chars() {
                    box_drawing_rects_into(&mut buf, c, cw_d, lh_d);
                    let reference = box_drawing_rects(c, cw_d, lh_d);
                    assert_eq!(
                        buf, reference,
                        "geometry mismatch for U+{:04X} at {cw_d}x{lh_d}",
                        c as u32
                    );
                }
            }
        }
    }

    #[test]
    fn rects_into_clears_previous_contents() {
        // Reused across cells: a glyph with no geometry must leave an empty buf
        // even after a prior glyph filled it.
        let mut buf = Vec::new();
        box_drawing_rects_into(&mut buf, '\u{2588}', 16, 24); // █ full block
        assert!(!buf.is_empty());
        box_drawing_rects_into(&mut buf, '\u{2571}', 16, 24); // ╱ diagonal → font fallback
        assert!(buf.is_empty(), "buffer must be cleared for empty glyphs");
    }

    #[test]
    fn has_box_geometry_matches_old_predicate() {
        // `has_box_geometry` replaced the old layout guard
        // `is_rounded_corner(c) || !box_drawing_rects(c, 16, 16).is_empty()`.
        // It must be identical so no glyph flips between primitive and font.
        let mut buf = Vec::new();
        for c in all_primitive_chars() {
            let expected = is_rounded_corner(c) || !box_drawing_rects(c, 16, 16).is_empty();
            assert_eq!(
                has_box_geometry(&mut buf, c),
                expected,
                "has_box_geometry disagrees for U+{:04X}",
                c as u32
            );
        }
    }

    #[test]
    fn upper_half_block_is_upper_half() {
        // The DOOM-fire workhorse glyph ▀ (U+2580) covers the top half only.
        let mut buf = Vec::new();
        box_drawing_rects_into(&mut buf, '\u{2580}', 16, 24);
        assert_eq!(buf, vec![(0, 0, 16, 12)]);
    }

    #[test]
    fn dash_lines_keep_orientation() {
        let cw = 24;
        let lh = 36;
        let cx = cw / 2;
        let cy = lh / 2;

        for (ch, expected_axis) in [
            ('\u{2504}', "h"), // light triple dash horizontal
            ('\u{2505}', "h"), // heavy triple dash horizontal
            ('\u{2508}', "h"), // light quadruple dash horizontal
            ('\u{2509}', "h"), // heavy quadruple dash horizontal
            ('\u{2506}', "v"), // light triple dash vertical
            ('\u{2507}', "v"), // heavy triple dash vertical
            ('\u{250A}', "v"), // light quadruple dash vertical
            ('\u{250B}', "v"), // heavy quadruple dash vertical
        ] {
            let rects = box_drawing_rects(ch, cw, lh);
            assert!(
                !rects.is_empty(),
                "char {:04X} should produce rects",
                ch as u32
            );
            if expected_axis == "h" {
                assert!(
                    rects.iter().all(|r| r.1 == cy),
                    "char {:04X} should be a horizontal dash line",
                    ch as u32
                );
            } else {
                assert!(
                    rects.iter().all(|r| r.0 == cx),
                    "char {:04X} should be a vertical dash line",
                    ch as u32
                );
            }
        }
    }
}
