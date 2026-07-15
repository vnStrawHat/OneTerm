//! Rounded corner primitives (U+256D–U+2570) — anti-aliased.

/// Draw a rounded corner (U+256D-U+2570) using a quarter-circle arc with
/// anti-aliasing (4x4 supersampling). Returns (x, y, w, h, alpha) where
/// alpha = the pixel's coverage.
///
/// The rounded-corner stroke must **align exactly** with the light straight
/// line in `box_drawing_rects`: same thickness `w = round(cw/6)` and same
/// start position at the cell center (`cx`, `cy`). Specifically the straight
/// line uses:
///   - vertical   │ : x ∈ [cx, cx + w)
///   - horizontal ─ : y ∈ [cy, cy + w)
/// If the corner's position/thickness differs from the line, the horizontal/
/// vertical edges of adjacent cells won't connect to the corner (gaps / kinks appear).
pub(crate) fn rounded_corner_rects_aa(
    c: char,
    cw_d: i32,
    lh_d: i32,
) -> Vec<(i32, i32, i32, i32, f32)> {
    let cx = cw_d / 2;
    let cy = lh_d / 2;
    // Light thickness — identical to `t` in `box_drawing_rects` so the corner's
    // vertical/horizontal arms align with the │ ─ lines of adjacent cells.
    let w = (cw_d as f32 / 6.0).round().max(1.0) as i32;
    // Arms **centered** around the cell's center axis, identical to the vd!/hr!
    // of the straight line (both use `center - thick/2`). This makes the join
    // between the rounded corner and the vertical/horizontal line sit exactly at center.
    let hw = w / 2;
    let xlo = (cx - hw) as f32;
    let xhi = (cx - hw + w) as f32;
    let ylo = (cy - hw) as f32;
    let yhi = (cy - hw + w) as f32;
    // Outer radius: small enough to leave the straight arm reaching the right/bottom
    // edge of the cell (arc_c{x,y} = {xlo,ylo} + r_out must be < {cw_d, lh_d}), large
    // enough to still leave an annulus of thickness `w`.
    let r_out = ((cx.min(cy) - 1).max(w + 1)) as f32;
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
