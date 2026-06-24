//! Rounded corner primitives (U+256D–U+2570) — anti-aliased.

/// Vẽ góc bo tròn (U+256D-U+2570) bằng cung tròn (quarter-circle)
/// với anti-aliasing (supersample 4x4). Trả (x, y, w, h, alpha) với
/// alpha = độ phủ (coverage) của pixel.
pub(crate) fn rounded_corner_rects_aa(
    c: char,
    cw_d: i32,
    lh_d: i32,
) -> Vec<(i32, i32, i32, i32, f32)> {
    let ht = super::heavy_thickness(cw_d);
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

    const N: i32 = 4;
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
