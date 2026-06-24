//! Powerline symbol primitives (U+E0B0–U+E0BF).

pub(crate) fn is_powerline(c: char) -> bool {
    matches!(c, '\u{E0B0}'..='\u{E0BF}')
}

pub(crate) fn rects(c: char, cw_d: i32, lh_d: i32) -> Vec<(i32, i32, i32, i32)> {
    match c {
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
        // Các powerline còn lại chưa có path custom → vẽ full block.
        _ => vec![(0, 0, cw_d, lh_d)],
    }
}
