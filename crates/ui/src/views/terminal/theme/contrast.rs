//! WCAG contrast helpers.

use std::collections::HashMap;

use gpui::Hsla;

/// Cache key: (fg l+chroma bits, bg l+chroma bits, min_contrast bits).
/// Hsla is (h, s, l, a) — we only adjust `l` in `ensure_minimum_contrast`,
/// and the result depends on the full (h, s, l, a) of both colors + min.
type ContrastKey = (u64, u64, u32);

thread_local! {
    /// PERF-10: Cache contrast-adjusted foreground colors. The same (fg, bg,
    /// min_contrast) triple recurs across cells (16 ANSI colors + default
    /// fg/bg), so the cache hit rate is very high. Automatically invalidated
    /// when `min_contrast` changes (part of the key).
    static CONTRAST_CACHE: std::cell::RefCell<HashMap<ContrastKey, Hsla>> =
        std::cell::RefCell::new(HashMap::with_capacity(64));
}

fn hsla_bits(c: Hsla) -> u64 {
    // Pack h, s, l, a into a u64 key. Using to_bits avoids f32 equality issues.
    let h = c.h.to_bits() as u64;
    let s = c.s.to_bits() as u64;
    let l = c.l.to_bits() as u64;
    let a = c.a.to_bits() as u64;
    h | (s << 16) | (l << 32) | (a << 48)
}

/// Relative luminance (WCAG) from `Hsla`.
fn relative_luminance(c: Hsla) -> f32 {
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

/// WCAG contrast ratio between two colors (≥1.0).
pub fn contrast_ratio(a: Hsla, b: Hsla) -> f32 {
    let la = relative_luminance(a);
    let lb = relative_luminance(b);
    let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// Adjust `fg` to reach a contrast ≥ `min` against `bg`.
///
/// PERF-10: Results are cached in a thread-local HashMap keyed by
/// (fg, bg, min_contrast). The same (fg, bg) pairs recur across cells
/// (16 ANSI colors + default fg/bg), so the cache hit rate is very high.
pub fn ensure_minimum_contrast(fg: Hsla, bg: Hsla, min: f32) -> Hsla {
    if contrast_ratio(fg, bg) >= min || min <= 1.0 {
        return fg;
    }

    let key = (hsla_bits(fg), hsla_bits(bg), min.to_bits());
    if let Some(cached) = CONTRAST_CACHE.with(|c| c.borrow().get(&key).copied()) {
        return cached;
    }

    let result = ensure_minimum_contrast_uncached(fg, bg, min);
    CONTRAST_CACHE.with(|c| {
        c.borrow_mut().insert(key, result);
    });
    result
}

fn ensure_minimum_contrast_uncached(fg: Hsla, bg: Hsla, min: f32) -> Hsla {
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
