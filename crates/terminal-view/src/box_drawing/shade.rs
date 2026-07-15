//! Shade block primitives (U+2591 light, U+2592 medium, U+2593 dark).

pub(crate) fn rects(c: char, cw_d: i32, lh_d: i32) -> Vec<(i32, i32, i32, i32)> {
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
