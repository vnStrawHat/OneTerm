//! Box-drawing / block / powerline glyph primitives cho `TerminalElement`.
//!
//! Vẽ bằng `paint_quad` thay vì font glyph → pixel-perfect, không AA blur.

/// Kiểm tra char có thuộc box-drawing / block / powerline glyphs mà
/// Windows Terminal AtlasEngine vẽ bằng primitive thay vì font glyph.
/// Bao gồm: U+2500–U+257F (box drawing), U+2580–U+259F (block elements),
/// U+25AC ▬ (TUI horizontal bar), U+E0B0–U+E0BF (powerline symbols).
pub(crate) fn is_box_drawing(c: char) -> bool {
    matches!(
        c,
        '\u{2500}'..='\u{257F}' | '\u{2580}'..='\u{259F}' | '\u{25AC}' | '\u{E0B0}'..='\u{E0BF}'
    )
}

/// Be day heavy line (device px). Tach ham de path AA goc bo tron dung
/// chung cong thuc voi `box_drawing_rects`.
pub(crate) fn heavy_thickness(cw_d: i32) -> i32 {
    (cw_d as f32 / 3.0).round().max(2.0) as i32 + 1
}

/// Tính geometry (pixel-perfect) cho box-drawing char trong cell.
/// Trả list rect (x, y, w, h) tính bằng **device pixel** relative tới
/// cell origin. Caller convert sang logical px khi paint.
/// Giống AtlasEngine: light = 1 device px, heavy = 2, double = 2 line.
pub(crate) fn box_drawing_rects(c: char, cw_d: i32, lh_d: i32) -> Vec<(i32, i32, i32, i32)> {
    let cx = cw_d / 2;
    let cy = lh_d / 2;
    // Windows Terminal AtlasEngine: light stroke width scales with the
    // cell width so adjacent single-line segments stay visually connected
    // (no hairline gaps) and corners remain solid.  Heavy is ~2x light.
    let t = (cw_d as f32 / 6.0).round().max(1.0) as i32; // light thickness
    // Heavy day hon light ~2x, da tang them 1 muc (+1px) cho net dam/ro hon.
    let ht = heavy_thickness(cw_d); // heavy thickness
    // Double-line stroke width.  Using cellWidth/8 keeps two distinct strokes
    // even on smaller cells while staying closer to font glyph proportions.
    let dt = (cw_d as f32 / 8.0).round().max(1.0) as i32;
    let dl = dt; // offset from center to each double stroke
    let dv = dt;
    // Double-line stroke positions (device-pixel columns/rows).
    // `out` = closer to the cell edge that forms the corner's outer serif,
    // `in`  = closer to the cell center, forming the inner serif.
    let x_out = (cx - dl).max(0);
    let x_in = (cx + dl).min(cw_d - dt);
    let y_out = (cy - dv).max(0);
    let y_in = (cy + dv).min(lh_d - dt);
    // For horizontal strokes the pixel row is the top of the rect, so
    // we need the bottom row to sit on `y_out`/`y_in`.  Offset by dt
    // so the 1-px thick stroke occupies exactly that row.
    let y_out_top = (y_out - dt).max(0);
    let y_in_top = (y_in - dt).max(0);
    // Similarly, vertical strokes' left edge should sit on `x_out`/`x_in`.
    let x_out_left = (x_out - dt).max(0);
    let x_in_left = (x_in - dt).max(0);

    macro_rules! h {
        ($y:expr, $thick:expr) => {
            (0, $y, cw_d, $thick)
        };
    }
    macro_rules! v {
        ($x:expr, $thick:expr) => {
            ($x, 0, $thick, lh_d)
        };
    }
    // half horizontal: bắt đầu từ tâm cell
    macro_rules! hr {
        ($y:expr, $thick:expr) => {
            (cx, $y, cw_d - cx, $thick)
        };
    }
    macro_rules! hl {
        ($y:expr, $thick:expr) => {
            (0, $y, (cx + $thick).min(cw_d), $thick)
        };
    }
    macro_rules! vd {
        ($x:expr, $thick:expr) => {
            ($x, cy, $thick, lh_d - cy)
        };
    }
    macro_rules! vu {
        ($x:expr, $thick:expr) => {
            ($x, 0, $thick, (cy + $thick).min(lh_d))
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
        // dash — rải đoạn 2px on / 2px off
        '\u{2504}' | '\u{2506}' => dash_h(cy, cw_d, t),
        '\u{2505}' | '\u{2507}' => dash_h(cy, cw_d, ht),
        '\u{2508}' => dash_v(cx, lh_d, t),
        '\u{2509}' => dash_v(cx, lh_d, ht),
        // double lines
        '\u{2550}' => vec![(0, y_out_top, cw_d, dt), (0, y_in_top, cw_d, dt)],
        '\u{2551}' => vec![(x_out_left, 0, dt, lh_d), (x_in_left, 0, dt, lh_d)],
        // Corners: two nested empty rectangles.
        // ╔ double down-and-right
        '\u{2554}' => vec![
            (x_out_left, y_out_top, dt, lh_d - y_out_top),
            (x_out_left, y_out_top, cw_d - x_out_left, dt),
            (x_in_left, y_in_top, dt, lh_d - y_in_top),
            (x_in_left, y_in_top, cw_d - x_in_left, dt),
        ],
        // ╗ double down-and-left
        '\u{2557}' => vec![
            (x_in_left, y_out_top, dt, lh_d - y_out_top),
            (0, y_out_top, x_in_left + dt, dt),
            (x_out_left, y_in_top, dt, lh_d - y_in_top),
            (0, y_in_top, x_out_left + dt, dt),
        ],
        // ╚ double up-and-right
        '\u{255A}' => vec![
            (x_out_left, 0, dt, y_in),
            (x_out_left, y_in_top, cw_d - x_out_left, dt),
            (x_in_left, 0, dt, y_out),
            (x_in_left, y_out_top, cw_d - x_in_left, dt),
        ],
        // ╝ double up-and-left
        '\u{255D}' => vec![
            (x_in_left, 0, dt, y_in),
            (0, y_in_top, x_in_left + dt, dt),
            (x_out_left, 0, dt, y_out),
            (0, y_out_top, x_out_left + dt, dt),
        ],
        // Mixed-light double corners (best-effort, use center line for the light arm).
        // ╒ down single-and-right-double
        '\u{2552}' => vec![
            (cx, 0, t, lh_d),
            (cx, y_out, cw_d - cx, t),
            (cx, y_in, cw_d - cx, t),
        ],
        // ╓ down double-and-right-single
        '\u{2553}' => vec![
            (x_out, 0, t, lh_d),
            (x_in, 0, t, lh_d),
            (cx, y_out, cw_d - cx, t),
        ],
        // ╕ down single-and-left-double
        '\u{2555}' => vec![(cx, 0, t, lh_d), (0, y_out, cx, t), (0, y_in, cx, t)],
        // ╖ down double-and-left-single
        '\u{2556}' => vec![(x_out, 0, t, lh_d), (x_in, 0, t, lh_d), (0, y_out, cx, t)],
        // ╘ up single-and-right-double
        '\u{2558}' => vec![
            (cx, 0, t, lh_d),
            (cx, y_out, cw_d - cx, t),
            (cx, y_in, cw_d - cx, t),
        ],
        // ╙ up double-and-right-single
        '\u{2559}' => vec![
            (x_out, 0, t, lh_d),
            (x_in, 0, t, lh_d),
            (cx, y_out, cw_d - cx, t),
        ],
        // ╛ up single-and-left-double
        '\u{255B}' => vec![(cx, 0, t, lh_d), (0, y_out, cx, t), (0, y_in, cx, t)],
        // ╜ up double-and-left-single
        '\u{255C}' => vec![(x_out, 0, t, lh_d), (x_in, 0, t, lh_d), (0, y_out, cx, t)],
        // Tee/cross pieces.
        // ╞ single vertical and right double
        '\u{255E}' => vec![
            (cx, 0, t, lh_d),
            (cx, y_out, cw_d - cx, t),
            (cx, y_in, cw_d - cx, t),
        ],
        // ╟ double vertical and right single
        '\u{255F}' => vec![
            (x_out, 0, t, lh_d),
            (x_in, 0, t, lh_d),
            (x_in, y_out, cw_d - x_in, t),
        ],
        // ╠ double vertical and right double
        '\u{2560}' => vec![
            (x_out, 0, t, lh_d),
            (x_in, 0, t, lh_d),
            (x_out + 1, y_out, cw_d - x_out - 1, t),
            (x_in + 1, y_in, cw_d - x_in - 1, t),
        ],
        // ╡ single vertical and left double
        '\u{2561}' => vec![(cx, 0, t, lh_d), (0, y_out, cx, t), (0, y_in, cx, t)],
        // ╢ double vertical and left single
        '\u{2562}' => vec![
            (x_out, 0, t, lh_d),
            (x_in, 0, t, lh_d),
            (0, y_out, x_out + 1, t),
        ],
        // ╣ double vertical and left double
        '\u{2563}' => vec![
            (x_out, 0, t, lh_d),
            (x_in, 0, t, lh_d),
            (0, y_out, x_out + 1, t),
            (0, y_in, x_in + 1, t),
        ],
        // ╤ down single and horizontal double
        '\u{2564}' => vec![
            (cx, y_out, t, lh_d - y_out),
            (0, y_out, cw_d, t),
            (0, y_in, cw_d, t),
        ],
        // ╥ down double and horizontal single
        '\u{2565}' => vec![
            (x_out, y_out, t, lh_d - y_out),
            (x_in, y_out, t, lh_d - y_out),
            (0, cy, cw_d, t),
        ],
        // ╦ down double and horizontal double
        '\u{2566}' => vec![
            (x_out, y_out, t, lh_d - y_out),
            (x_in, y_out, t, lh_d - y_out),
            (x_out, y_in, t, lh_d - y_in),
            (x_in, y_in, t, lh_d - y_in),
            (0, y_out, cw_d, t),
            (0, y_in, cw_d, t),
        ],
        // ╧ up single and horizontal double
        '\u{2567}' => vec![
            (cx, 0, t, y_out + 1),
            (0, y_out, cw_d, t),
            (0, y_in, cw_d, t),
        ],
        // ╨ up double and horizontal single
        '\u{2568}' => vec![
            (x_out, 0, t, y_out + 1),
            (x_in, 0, t, y_out + 1),
            (0, cy, cw_d, t),
        ],
        // ╩ up double and horizontal double
        '\u{2569}' => vec![
            (x_out, 0, t, y_out + 1),
            (x_in, 0, t, y_out + 1),
            (x_out, y_in, t, lh_d - y_in),
            (x_in, y_in, t, lh_d - y_in),
            (0, y_out, cw_d, t),
            (0, y_in, cw_d, t),
        ],
        // Crosses.
        // ╪ vertical single and horizontal double
        '\u{256A}' => vec![(cx, 0, t, lh_d), (0, y_out, cw_d, t), (0, y_in, cw_d, t)],
        // ╫ vertical double and horizontal single
        '\u{256B}' => vec![(x_out, 0, t, lh_d), (x_in, 0, t, lh_d), (0, cy, cw_d, t)],
        // ╬ double vertical and horizontal double
        '\u{256C}' => vec![
            (x_out, 0, t, lh_d),
            (x_in, 0, t, lh_d),
            (0, y_out, cw_d, t),
            (0, y_in, cw_d, t),
        ],
        // Rounded corners ╭╮╯╰ — vẽ cung tròn (quarter-circle) thay vì
        // góc vuông, và canh tâm nét nối với heavy line (━/┃) để mối nối
        // tiếp xúc ở chính giữa, không lệch. Xem `rounded_corner_rects`.
        '\u{256D}' | '\u{256E}' | '\u{256F}' | '\u{2570}' => {
            rounded_corner_rects(c, cw_d, lh_d, t, ht)
        }
        '\u{2574}' => vec![hl!(cy, t)],
        '\u{2575}' => vec![vu!(cx, t)],
        '\u{2576}' => vec![hr!(cy, t)],
        '\u{2577}' => vec![vd!(cx, t)],
        '\u{2578}' => vec![hl!(cy, ht)],
        '\u{2579}' => vec![vu!(cx, ht)],
        '\u{257A}' => vec![hr!(cy, ht)],
        '\u{257B}' => vec![vd!(cx, ht)],
        // ── Geometric shapes used as TUI bars ──
        // ▬ U+25AC Black Medium Small Square — often repeated to draw
        // solid horizontal bars/underlines.  Drawn primitive so adjacent
        // cells merge into a seamless line instead of leaving font gaps.
        '\u{25AC}' => vec![(0, lh_d / 4, cw_d, lh_d / 2)],
        // ── Block elements (U+2580–U+259F) ──
        // pi dùng ▀▄ cho input box padding, ▌ cho diff marker, █ cho fill.
        // Vẽ primitive → pixel-perfect, không font AA blur.
        '\u{2580}' => vec![(0, 0, cw_d, cy)], // ▀ upper half
        '\u{2581}' => vec![(0, lh_d - lh_d / 8, cw_d, lh_d / 8)], // ▁ lower 1/8
        '\u{2582}' => vec![(0, lh_d - lh_d / 4, cw_d, lh_d / 4)], // ▂ lower 1/4
        '\u{2583}' => vec![(0, lh_d - 3 * lh_d / 8, cw_d, 3 * lh_d / 8)], // ▃
        '\u{2584}' => vec![(0, cy, cw_d, lh_d - cy)], // ▄ lower half
        '\u{2585}' => vec![(0, lh_d - 5 * lh_d / 8, cw_d, 5 * lh_d / 8)], // ▅
        '\u{2586}' => vec![(0, lh_d - lh_d / 4, cw_d, lh_d / 4 * 3)], // ▆ lower 3/4
        '\u{2587}' => vec![(0, lh_d / 8, cw_d, lh_d - lh_d / 8)], // ▇ lower 7/8
        '\u{2588}' => vec![(0, 0, cw_d, lh_d)], // █ full block
        '\u{2589}' => vec![(0, 0, 7 * cw_d / 8, lh_d)], // ▉ left 7/8
        '\u{258A}' => vec![(0, 0, 3 * cw_d / 4, lh_d)], // ▊ left 3/4
        '\u{258B}' => vec![(0, 0, 5 * cw_d / 8, lh_d)], // ▋ left 5/8
        '\u{258C}' => vec![(0, 0, cx, lh_d)], // ▌ left half
        '\u{258D}' => vec![(0, 0, 3 * cw_d / 8, lh_d)], // ▍ left 3/8
        '\u{258E}' => vec![(0, 0, cw_d / 4, lh_d)], // ▎ left 1/4
        '\u{258F}' => vec![(0, 0, cw_d / 8, lh_d)], // ▏ left 1/8
        '\u{2594}' => vec![(0, 0, cw_d, lh_d / 8)], // ▔ upper 1/8
        '\u{2595}' => vec![(cw_d - cw_d / 8, 0, cw_d / 8, lh_d)], // ▕ right 1/8
        // Quadrant blocks
        '\u{2596}' => vec![(0, cy, cx, lh_d - cy)], // ▖ quad lower-left
        '\u{2597}' => vec![(cx, cy, cw_d - cx, lh_d - cy)], // ▗ quad lower-right
        '\u{2598}' => vec![(0, 0, cx, cy)],         // ▘ quad upper-left
        '\u{2599}' => vec![
            (0, 0, cx, cy),
            (0, cy, cx, lh_d - cy),
            (cx, 0, cw_d - cx, cy),
        ], // ▙
        '\u{259A}' => vec![(cx, 0, cw_d - cx, cy), (0, cy, cx, lh_d - cy)], // ▚
        '\u{259B}' => vec![(0, 0, cw_d, cy), (0, cy, cx, lh_d - cy)], // ▛
        '\u{259C}' => vec![(0, 0, cw_d, cy), (cx, cy, cw_d - cx, lh_d - cy)], // ▜
        '\u{259D}' => vec![(cx, 0, cw_d - cx, cy)], // ▝ quad upper-right
        '\u{259E}' => vec![(cx, 0, cw_d - cx, cy), (0, cy, cx, lh_d - cy)], // ▞
        '\u{259F}' => vec![
            (0, 0, cx, cy),
            (cx, 0, cw_d - cx, cy),
            (cx, cy, cw_d - cx, lh_d - cy),
        ], // ▟
        // ── Right half block (U+2590) — mirror của ▌ left half ──
        '\u{2590}' => vec![(cw_d - cx, 0, cx, lh_d)], // ▐ right half
        // ── Powerline symbols (U+E0B0–U+E0BF) ──
        // Vẽ primitive fill để statusline / prompt separators không bị
        // font metrics làm méo hoặc hở giữa các cell.
        '\u{E0B0}' => {
            // Right triangle solid ▶ (filled, points right).
            let mut v = vec![];
            for y in 0..lh_d {
                let x_start = y * cw_d / lh_d;
                v.push((x_start, y, cw_d - x_start, 1));
            }
            v
        }
        '\u{E0B2}' => {
            // Left triangle solid ◀ (filled, points left).
            let mut v = vec![];
            for y in 0..lh_d {
                let w = (lh_d - y) * cw_d / lh_d;
                v.push((0, y, w, 1));
            }
            v
        }
        '\u{E0B4}' => {
            // Right semi-circle solid ▶ (filled half-ellipse on the right).
            let r = cw_d.min(lh_d) / 2;
            let cy = lh_d / 2;
            let mut v = vec![];
            for y in 0..lh_d {
                let dy = (y - cy).abs();
                if dy > r {
                    v.push((0, y, cw_d, 1));
                } else {
                    let x_cut = (cw_d as f32
                        * (1.0 - ((1.0 - (dy as f32 / r as f32).powi(2)).sqrt())).max(0.0))
                        as i32;
                    v.push((0, y, cw_d - x_cut, 1));
                }
            }
            v
        }
        '\u{E0B6}' => {
            // Left semi-circle solid ◀ (filled half-ellipse on the left).
            let r = cw_d.min(lh_d) / 2;
            let cy = lh_d / 2;
            let mut v = vec![];
            for y in 0..lh_d {
                let dy = (y - cy).abs();
                if dy > r {
                    v.push((0, y, cw_d, 1));
                } else {
                    let x_cut = (cw_d as f32
                        * (1.0 - ((1.0 - (dy as f32 / r as f32).powi(2)).sqrt())).max(0.0))
                        as i32;
                    v.push((x_cut, y, cw_d - x_cut, 1));
                }
            }
            v
        }
        // Các powerline còn lại chưa có path custom → vẽ full block để
        // tránh hiển thị glyph lạ hoặc ô trống trong prompt/statusline.
        c @ ('\u{E0B1}' | '\u{E0B3}' | '\u{E0B5}' | '\u{E0B7}' | '\u{E0B8}' | '\u{E0B9}'
        | '\u{E0BA}' | '\u{E0BB}' | '\u{E0BC}' | '\u{E0BD}' | '\u{E0BE}' | '\u{E0BF}') => {
            let _ = c;
            vec![(0, 0, cw_d, lh_d)]
        }
        // ── Shade blocks (U+2591 light, U+2592 medium, U+2593 dark). ──
        c @ ('\u{2591}' | '\u{2592}' | '\u{2593}') => shade_rects(c, cw_d, lh_d),
        // diagonal / quadruple-dash → fallback font (hiếm trong TUI)
        _ => vec![],
    }
}

/// Ve goc bo tron (U+256D-U+2570) bang cung tron (quarter-circle)
/// rasterize per-pixel -> run-length rects moi dong.
///
/// 1. Bo tron that: net di theo cung tron ban kinh `r` o vung re, thay vi
///    gap nhau vuong goc.
/// 2. Moi noi dong do day + tiep xuc chinh giua: trong font SLICK goc bo
///    tron noi voi heavy line (day `ht`). Heavy line ve edge-aligned tai
///    `cx`/`cy` (chiem `[cx, cx+ht]`), tam o `cx + ht/2`. Ta ve net goc
///    cung do day `ht` va canh tam vao tam heavy line -> arm trung khit
///    `[cx, cx+ht]`, net lien mach, dong do day, khong lech.
fn rounded_corner_rects(
    c: char,
    cw_d: i32,
    lh_d: i32,
    t: i32,
    ht: i32,
) -> Vec<(i32, i32, i32, i32)> {
    let _ = t;
    let cx = cw_d / 2;
    let cy = lh_d / 2;
    // === KNOB do day net cong ===
    // Be day net cong MONG HON HAN heavy line `ht` dung doc/ngang.
    // Floor 2px de tranh net 1px bi rang cua (aliasing).
    let w = (ht / 3).max(2);
    // Net cong duoc canh GIUA trong dai pixel cua heavy line `[cx, cx+ht)`
    // (va `[cy, cy+ht)`), tuc lui vao `(ht - w)/2` moi ben -> moi noi giua
    // net cong va canh doc/ngang nam dung CHINH GIUA, khong lech.
    let off = (ht - w) / 2;
    let xlo = cx + off;
    let xhi = xlo + w; // arm doc: x trong [xlo, xhi)
    let ylo = cy + off;
    let yhi = ylo + w; // arm ngang: y trong [ylo, yhi)
    // Ban kinh ngoai cung: lon nhat ma arm van cham bien o ben canh.
    let r_out = (cx.min(cy) - off).max(w + 1);
    let r_in = (r_out - w).max(0);
    let r_out_sq = r_out * r_out;
    let r_in_sq = r_in * r_in;

    // (arc_cx, arc_cy) = tam cung; `down`/`right` = huong keo dai arm.
    // Tam dat sao cho bien ngoai cung trung bien ngoai cua arm.
    let (arc_cx, arc_cy, down, right) = match c {
        '\u{256D}' => (xlo + r_out, ylo + r_out, true, true), // bo goc tren-trai
        '\u{256E}' => (xhi - r_out, ylo + r_out, true, false), // bo goc tren-phai
        '\u{256F}' => (xhi - r_out, yhi - r_out, false, false), // bo goc duoi-phai
        '\u{2570}' => (xlo + r_out, yhi - r_out, false, true), // bo goc duoi-trai
        _ => return Vec::new(),
    };

    let mut out: Vec<(i32, i32, i32, i32)> = Vec::new();
    for y in 0..lh_d {
        let mut run_start: Option<i32> = None;
        for x in 0..cw_d {
            // Arm doc: dung be rong `w`, keo dai tu tiep tuyen cung.
            let v_arm = x >= xlo && x < xhi && if down { y >= arc_cy } else { y <= arc_cy };
            // Arm ngang: dung be rong `w`, keo dai tu tiep tuyen cung.
            let h_arm = y >= ylo && y < yhi && if right { x >= arc_cx } else { x <= arc_cx };
            // Cung tron: phan tu huong ve goc, khoang cach toi tam trong dai.
            let dx = x - arc_cx;
            let dy = y - arc_cy;
            let x_side = if right { x <= arc_cx } else { x >= arc_cx };
            let y_side = if down { y <= arc_cy } else { y >= arc_cy };
            let dist2 = dx * dx + dy * dy;
            let arc = x_side && y_side && dist2 >= r_in_sq && dist2 <= r_out_sq;

            let filled = v_arm || h_arm || arc;
            match (filled, run_start) {
                (true, None) => run_start = Some(x),
                (false, Some(start)) => {
                    out.push((start, y, x - start, 1));
                    run_start = None;
                }
                _ => {}
            }
        }
        if let Some(start) = run_start {
            out.push((start, y, cw_d - start, 1));
        }
    }
    out
}

/// Phien ban ANTI-ALIASED cua `rounded_corner_rects`: tra (x, y, w, h,
/// alpha) voi alpha = do phu (coverage) cua pixel (supersample 4x4).
pub(crate) fn rounded_corner_rects_aa(
    c: char,
    cw_d: i32,
    lh_d: i32,
) -> Vec<(i32, i32, i32, i32, f32)> {
    let ht = heavy_thickness(cw_d);
    let cx = cw_d / 2;
    let cy = lh_d / 2;
    let w = (ht / 3).max(2);
    let off = (ht - w) / 2;
    let xlo = (cx + off) as f32;
    let xhi = (cx + off + w) as f32;
    let ylo = (cy + off) as f32;
    let yhi = (cy + off + w) as f32;
    let r_out = ((cx.min(cy) - off).max(w + 1)) as f32;
    let r_in = (r_out - w as f32).max(0.0);

    let (arc_cx, arc_cy, down, right) = match c {
        '\u{256D}' => (xlo + r_out, ylo + r_out, true, true),
        '\u{256E}' => (xhi - r_out, ylo + r_out, true, false),
        '\u{256F}' => (xhi - r_out, yhi - r_out, false, false),
        '\u{2570}' => (xlo + r_out, yhi - r_out, false, true),
        _ => return Vec::new(),
    };
    let r_in_sq = r_in * r_in;
    let r_out_sq = r_out * r_out;

    let inside = |fx: f32, fy: f32| -> bool {
        let v_arm = fx >= xlo && fx < xhi && if down { fy >= arc_cy } else { fy <= arc_cy };
        let h_arm = fy >= ylo && fy < yhi && if right { fx >= arc_cx } else { fx <= arc_cx };
        let dx = fx - arc_cx;
        let dy = fy - arc_cy;
        let x_side = if right { fx <= arc_cx } else { fx >= arc_cx };
        let y_side = if down { fy <= arc_cy } else { fy >= arc_cy };
        let dist2 = dx * dx + dy * dy;
        let arc = x_side && y_side && dist2 >= r_in_sq && dist2 <= r_out_sq;
        v_arm || h_arm || arc
    };

    const N: i32 = 4; // 4x4 = 16 sub-sample / pixel
    let inv = 1.0 / N as f32;
    let total = (N * N) as f32;
    let coverage = |x: i32, y: i32| -> f32 {
        let mut cnt = 0;
        for j in 0..N {
            let fy = y as f32 + (j as f32 + 0.5) * inv;
            for i in 0..N {
                let fx = x as f32 + (i as f32 + 0.5) * inv;
                if inside(fx, fy) {
                    cnt += 1;
                }
            }
        }
        cnt as f32 / total
    };

    let mut out: Vec<(i32, i32, i32, i32, f32)> = Vec::new();
    for y in 0..lh_d {
        let mut run_start: Option<i32> = None;
        for x in 0..cw_d {
            let cov = coverage(x, y);
            if cov >= 0.999 {
                if run_start.is_none() {
                    run_start = Some(x);
                }
            } else {
                if let Some(start) = run_start.take() {
                    out.push((start, y, x - start, 1, 1.0));
                }
                if cov > 0.0 {
                    out.push((x, y, 1, 1, cov));
                }
            }
        }
        if let Some(start) = run_start {
            out.push((start, y, cw_d - start, 1, 1.0));
        }
    }
    out
}

/// Shade blocks (U+2591 light, U+2592 medium, U+2593 dark).
/// Vẽ stipple pattern bang 1x1 device pixel dots.
fn shade_rects(c: char, cw_d: i32, lh_d: i32) -> Vec<(i32, i32, i32, i32)> {
    if cw_d * lh_d > 1024 {
        return vec![];
    }
    let mut out = Vec::new();
    match c {
        '\u{2591}' => {
            for y in 0..lh_d {
                for x in 0..cw_d {
                    if (x + y) % 2 == 0 {
                        out.push((x, y, 1, 1));
                    }
                }
            }
        }
        '\u{2592}' => {
            for y in 0..lh_d {
                for x in 0..cw_d {
                    if x % 2 == 0 {
                        out.push((x, y, 1, 1));
                    }
                }
            }
        }
        '\u{2593}' => {
            for y in 0..lh_d {
                for x in 0..cw_d {
                    if (x + y) % 2 != 0 {
                        out.push((x, y, 1, 1));
                    }
                }
            }
        }
        _ => {}
    }
    out
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
