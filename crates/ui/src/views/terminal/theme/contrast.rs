//! WCAG contrast helpers.

/// Relative luminance (WCAG) từ `Hsla`.
fn relative_luminance(c: gpui::Hsla) -> f32 {
    let rgba = c.to_rgb();
    let lin = |ch: f32| {
        if ch <= 0.03928 {
            ch / 12.92
        } else {
            ((ch + 0.055) / 1.055).powi(2)
        }
    };
    let r = lin(rgba.r);
    let g = lin(rgba.g);
    let b = lin(rgba.b);
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// WCAG contrast ratio giữa hai màu (≥1.0).
pub fn contrast_ratio(a: gpui::Hsla, b: gpui::Hsla) -> f32 {
    let la = relative_luminance(a);
    let lb = relative_luminance(b);
    let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// Điều chỉnh `fg` để đạt contrast ≥ `min` với `bg`.
pub fn ensure_minimum_contrast(fg: gpui::Hsla, bg: gpui::Hsla, min: f32) -> gpui::Hsla {
    if contrast_ratio(fg, bg) >= min || min <= 1.0 {
        return fg;
    }
    let mut up = fg;
    let mut down = fg;
    let mut up_ok = false;
    let mut down_ok = false;
    for _ in 0..40 {
        if !up_ok {
            up.l = (up.l + 0.03).clamp(0.0, 1.0);
            if contrast_ratio(up, bg) >= min {
                up_ok = true;
            }
        }
        if !down_ok {
            down.l = (down.l - 0.03).clamp(0.0, 1.0);
            if contrast_ratio(down, bg) >= min {
                down_ok = true;
            }
        }
        if up_ok && down_ok {
            break;
        }
    }
    match (up_ok, down_ok) {
        (true, true) => {
            if (up.l - fg.l).abs() <= (fg.l - down.l).abs() {
                up
            } else {
                down
            }
        }
        (true, false) => up,
        (false, true) => down,
        (false, false) => {
            if contrast_ratio(up, bg) >= contrast_ratio(down, bg) {
                up
            } else {
                down
            }
        }
    }
}
