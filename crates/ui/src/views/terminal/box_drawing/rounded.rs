//! Rounded corner primitives (U+256D–U+2570) — anti-aliased.

/// Vẽ góc bo tròn (U+256D-U+2570) bằng cung tròn (quarter-circle)
/// với anti-aliasing (supersample 4x4). Trả (x, y, w, h, alpha) với
/// alpha = độ phủ (coverage) của pixel.
///
/// Stroke của góc bo tròn phải **trùng khít** với đường thẳng light
/// trong `box_drawing_rects`: cùng độ dày `w = round(cw/6)` và cùng vị
/// trí bắt đầu tại tâm cell (`cx`, `cy`). Cụ thể line thẳng dùng:
///   - dọc  │ : x ∈ [cx, cx + w)
///   - ngang ─: y ∈ [cy, cy + w)
/// Nếu góc lệch vị trí/độ dày so với line, cạnh ngang/dọc của các cell
/// kề bên sẽ không nối liền với góc (xuất hiện khe hở / gấp khúc).
pub(crate) fn rounded_corner_rects_aa(
    c: char,
    cw_d: i32,
    lh_d: i32,
) -> Vec<(i32, i32, i32, i32, f32)> {
    let cx = cw_d / 2;
    let cy = lh_d / 2;
    // Độ dày light — giống hệt `t` trong `box_drawing_rects` để arm
    // dọc/ngang của góc khớp với line │ ─ ở cell kề.
    let w = (cw_d as f32 / 6.0).round().max(1.0) as i32;
    // Arm **căn giữa** quanh trục tâm cell, giống hệt vd!/hr! của line
    // thẳng (đều dùng `center - thick/2`). Nhờ vậy điểm nối góc bo tròn
    // với line dọc/ngang nằm đúng tâm.
    let hw = w / 2;
    let xlo = (cx - hw) as f32;
    let xhi = (cx - hw + w) as f32;
    let ylo = (cy - hw) as f32;
    let yhi = (cy - hw + w) as f32;
    // Bán kính ngoài: đủ nhỏ để chừa arm thẳng chạm mép phải/dưới cell
    // (arc_c{x,y} = {xlo,ylo} + r_out phải < {cw_d, lh_d}), đủ lớn để
    // còn annulus dày `w`.
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
