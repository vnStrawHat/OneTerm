//! WCAG contrast helpers.

use std::collections::HashMap;

use gpui::Hsla;

/// Cache key: the full `(h, s, l, a)` bit patterns of both colours plus the
/// `min_contrast` bits. Each component keeps its own 32-bit slot (CORR-24:
/// packing four floats into one `u64` at a 16-bit stride let distinct pairs
/// collide and return the wrong adjusted colour).
type ContrastKey = ([u32; 4], [u32; 4], u32);

/// Entries kept before the cache is dropped and rebuilt. The working set is
/// tiny (16 ANSI colours × a few backgrounds), so this only bounds pathological
/// true-colour output that would otherwise grow the map without limit.
const CONTRAST_CACHE_MAX_ENTRIES: usize = 4096;

thread_local! {
    /// PERF-10: Cache contrast-adjusted foreground colors. The same (fg, bg,
    /// min_contrast) triple recurs across cells (16 ANSI colors + default
    /// fg/bg), so the cache hit rate is very high. Automatically invalidated
    /// when `min_contrast` changes (part of the key).
    static CONTRAST_CACHE: std::cell::RefCell<HashMap<ContrastKey, Hsla>> =
        std::cell::RefCell::new(HashMap::with_capacity(64));
}

fn hsla_bits(c: Hsla) -> [u32; 4] {
    // `to_bits` avoids f32 equality issues (NaN, -0.0) in the key.
    [c.h.to_bits(), c.s.to_bits(), c.l.to_bits(), c.a.to_bits()]
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
pub(crate) fn contrast_ratio(a: Hsla, b: Hsla) -> f32 {
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
pub(crate) fn ensure_minimum_contrast(fg: Hsla, bg: Hsla, min: f32) -> Hsla {
    if contrast_ratio(fg, bg) >= min || min <= 1.0 {
        return fg;
    }

    let key = (hsla_bits(fg), hsla_bits(bg), min.to_bits());
    if let Some(cached) = CONTRAST_CACHE.with(|c| c.borrow().get(&key).copied()) {
        return cached;
    }

    let result = ensure_minimum_contrast_uncached(fg, bg, min);
    CONTRAST_CACHE.with(|c| {
        let mut cache = c.borrow_mut();
        if cache.len() >= CONTRAST_CACHE_MAX_ENTRIES {
            cache.clear();
        }
        cache.insert(key, result);
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

#[cfg(test)]
mod tests {
    use gpui::hsla;

    use super::{CONTRAST_CACHE, ensure_minimum_contrast, hsla_bits};

    /// Regression for CORR-24: two colour pairs whose packed 16-bit-stride
    /// keys used to collide must resolve independently.
    #[test]
    fn distinct_pairs_do_not_share_a_cache_entry() {
        let bg = hsla(0.0, 0.0, 0.5, 1.0);
        let fg_a = hsla(0.0, 0.0, 0.5, 1.0);
        let fg_b = hsla(0.55, 0.9, 0.5, 1.0);
        assert_ne!(hsla_bits(fg_a), hsla_bits(fg_b));
        let a = ensure_minimum_contrast(fg_a, bg, 4.5);
        let b = ensure_minimum_contrast(fg_b, bg, 4.5);
        // Hue is never touched by the adjustment, so a wrong cache hit would
        // hand back the other colour's hue.
        assert!((a.h - fg_a.h).abs() < f32::EPSILON);
        assert!((b.h - fg_b.h).abs() < f32::EPSILON);
    }

    #[test]
    fn cache_is_bounded() {
        for i in 0..(super::CONTRAST_CACHE_MAX_ENTRIES + 10) {
            let l = 0.45 + (i as f32) * 1e-6;
            let _ = ensure_minimum_contrast(hsla(0.0, 0.0, l, 1.0), hsla(0.0, 0.0, 0.5, 1.0), 4.5);
        }
        let len = CONTRAST_CACHE.with(|c| c.borrow().len());
        assert!(len <= super::CONTRAST_CACHE_MAX_ENTRIES);
    }
}
