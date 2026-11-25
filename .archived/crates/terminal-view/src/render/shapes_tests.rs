use super::*;

/// Cell sizes every geometry test runs over: odd and even widths, matching and
/// mismatched parity on one or both axes (9 x 18 is the owner's cell size at
/// 1x), a large HiDPI cell, and the degenerate one-pixel cell.
const SIZES: [(i32, i32); 9] = [
    (7, 15),
    (8, 16),
    (8, 17),
    (9, 18),
    (9, 19),
    (14, 29),
    (18, 38),
    (36, 76),
    (1, 1),
];

/// Cell sizes at which some nominal thickness cannot be centred on some axis,
/// the ones the parity rule is about.
const PARITY_SIZES: [(i32, i32); 6] = [(9, 18), (8, 16), (9, 19), (8, 17), (14, 28), (14, 29)];

/// Font weights the geometry suites run at: the baseline and SGR bold on a
/// normal-weight font (`400 + 300`).
const WEIGHTS: [f32; 2] = [NORMAL, 700.0];

/// The weight at which strokes have their nominal thickness.
const NORMAL: f32 = 400.0;

fn cell(size: (i32, i32)) -> CellSizeDevicePx {
    CellSizeDevicePx {
        w: size.0,
        h: size.1,
    }
}

fn quads_at(c: char, size: (i32, i32), weight: f32) -> Vec<DeviceRect> {
    let mut out = Vec::new();
    shape_quads(c, cell(size), weight, &mut out);
    out
}

fn quads(c: char, size: (i32, i32)) -> Vec<DeviceRect> {
    quads_at(c, size, NORMAL)
}

fn thickness(size: (i32, i32), weight: f32) -> StrokeThickness {
    stroke_thickness(cell(size), weight)
}

/// The rasterized families: rounded corners, diagonals and powerline.
fn is_coverage_shape(c: char) -> bool {
    matches!(c as u32, 0x256D..=0x2573 | 0xE0B0..=0xE0BF)
}

/// Every code point this module claims, in one iterator.
fn all_shape_chars() -> impl Iterator<Item = char> {
    (0x2500..=0x257Fu32)
        .chain(0x2580..=0x259F)
        .chain(std::iter::once(0x25AC))
        .chain(0x2800..=0x28FF)
        .chain(0xE0B0..=0xE0BF)
        .filter_map(char::from_u32)
}

// ---------------------------------------------------------------------------
// Coverage maps
// ---------------------------------------------------------------------------

/// Per-pixel coverage of a rect set: the maximum alpha of the rects over each
/// pixel (solid families overlap at joints, rasterized ones never overlap).
#[derive(Clone, PartialEq)]
struct Bitmap {
    w: i32,
    h: i32,
    alpha: Vec<f32>,
}

impl Bitmap {
    fn new(size: (i32, i32)) -> Self {
        Self {
            w: size.0,
            h: size.1,
            alpha: vec![0.0; (size.0 * size.1) as usize],
        }
    }

    fn of(c: char, size: (i32, i32)) -> Self {
        Self::of_at(c, size, NORMAL)
    }

    fn of_at(c: char, size: (i32, i32), weight: f32) -> Self {
        Self::from_rects(size, &quads_at(c, size, weight))
    }

    fn from_rects(size: (i32, i32), rects: &[DeviceRect]) -> Self {
        let mut bitmap = Self::new(size);
        for rect in rects {
            for y in rect.y..rect.y + rect.h {
                for x in rect.x..rect.x + rect.w {
                    let index = (y * bitmap.w + x) as usize;
                    bitmap.alpha[index] = bitmap.alpha[index].max(rect.alpha);
                }
            }
        }
        bitmap
    }

    fn alpha(&self, x: i32, y: i32) -> f32 {
        self.alpha[(y * self.w + x) as usize]
    }

    fn get(&self, x: i32, y: i32) -> bool {
        self.alpha(x, y) > 0.0
    }

    fn union(&self, other: &Self) -> Self {
        let mut merged = self.clone();
        for (slot, value) in merged.alpha.iter_mut().zip(other.alpha.iter()) {
            *slot = slot.max(*value);
        }
        merged
    }

    fn count(&self) -> usize {
        self.alpha.iter().filter(|a| **a > 0.0).count()
    }

    fn is_subset_of(&self, other: &Self) -> bool {
        self.alpha
            .iter()
            .zip(other.alpha.iter())
            .all(|(mine, theirs)| *mine <= 0.0 || *theirs > 0.0)
    }

    /// The bitmap moved one pixel toward the top/left on the chosen axes. The
    /// far edge keeps its own pixels: an arm or stub that reaches the cell edge
    /// continues into the neighbouring cell, so nothing new comes in from there.
    /// That is a guess for a feature that merely *touches* the near edge because
    /// of the bias (bold rails on a 7 px cell): see [`Bitmap::without_far_edge`].
    fn shifted(&self, x: bool, y: bool) -> Self {
        let mut out = Self::new((self.w, self.h));
        for py in 0..self.h {
            for px in 0..self.w {
                let sx = if x && px + 1 < self.w { px + 1 } else { px };
                let sy = if y && py + 1 < self.h { py + 1 } else { py };
                out.alpha[(py * self.w + px) as usize] = self.alpha(sx, sy);
            }
        }
        out
    }

    /// The bitmap with the far-edge column (`x`) and row (`y`) cleared. On a
    /// parity-mismatched axis a feature biased onto the near edge of one glyph
    /// ends one pixel short of the far edge in its reflection (the thickness
    /// never changes), while an arm reaching the far edge still reaches it;
    /// the far edge line is the one place where "reflect and move one pixel"
    /// is ambiguous, so it is compared through the abutting tests instead.
    fn without_far_edge(&self, x: bool, y: bool) -> Self {
        let mut out = self.clone();
        for py in 0..self.h {
            for px in 0..self.w {
                if (x && px == self.w - 1) || (y && py == self.h - 1) {
                    out.alpha[(py * self.w + px) as usize] = 0.0;
                }
            }
        }
        out
    }

    /// `.` empty, `#` solid, a digit for `floor(alpha * 10)` in between.
    fn ascii(&self) -> String {
        let mut text = String::new();
        for y in 0..self.h {
            for x in 0..self.w {
                let alpha = self.alpha(x, y);
                text.push(if alpha <= 0.0 {
                    '.'
                } else if alpha >= 1.0 {
                    '#'
                } else {
                    char::from(b'0' + ((alpha * 10.0) as u8).min(9))
                });
            }
            text.push('\n');
        }
        text
    }
}

fn sorted(mut rects: Vec<DeviceRect>) -> Vec<DeviceRect> {
    rects.sort_by_key(|rect| (rect.x, rect.y, rect.w, rect.h, rect.alpha.to_bits()));
    rects
}

fn mirror_rects(rects: &[DeviceRect], size: (i32, i32), mirror: Mirror) -> Vec<DeviceRect> {
    sorted(
        rects
            .iter()
            .map(|rect| DeviceRect {
                x: if mirror.x {
                    size.0 - rect.x - rect.w
                } else {
                    rect.x
                },
                y: if mirror.y {
                    size.1 - rect.y - rect.h
                } else {
                    rect.y
                },
                w: rect.w,
                h: rect.h,
                alpha: rect.alpha,
            })
            .collect(),
    )
}

/// Nominal thickness of the strokes of `c`, which decides on which cell axes the
/// glyph sits half a pixel toward the top/left. `None` for shapes that never
/// shift: blocks, `▬`, shades, braille, and the edge-anchored rasterized
/// diagonals and powerline glyphs. Box glyphs mixing light and heavy arms are
/// not classified (none is in the symmetry lists).
fn stroke_nominal(c: char, size: (i32, i32), weight: f32) -> Option<i32> {
    let t = thickness(size, weight);
    match c as u32 {
        0x2571..=0x2573 => None,
        0x2500..=0x257F => {
            let heavy = box_arms(c)
                .map(|arms| arms.contains(&Some(Weight::Heavy)))
                .or_else(|| dash_def(c).map(|(_, weight, _)| weight == Weight::Heavy))
                .unwrap_or(false);
            Some(if heavy { t.heavy } else { t.light })
        }
        _ => None,
    }
}

/// On which mirrored axes the reflection of `c` is one pixel off: exactly those
/// where the cell size and the stroke thickness have different parity, so the
/// stroke cannot be centred and sits half a pixel toward the top/left instead.
fn expected_shift(c: char, size: (i32, i32), mirror: Mirror, weight: f32) -> (bool, bool) {
    let Some(t) = stroke_nominal(c, size, weight) else {
        return (false, false);
    };
    (
        mirror.x && (size.0 - t) % 2 != 0,
        mirror.y && (size.1 - t) % 2 != 0,
    )
}

/// `b` is the reflection of `a`: pixel for pixel including coverage alpha, and
/// rect set for rect set (run-length merging mirrors exactly too). On an axis
/// whose parity does not match the stroke, `b` is the reflection moved exactly
/// one pixel toward the top/left (never the other way, never more), because both
/// glyphs carry the same half-pixel bias; the far-edge line of such an axis is
/// left out of the comparison (see [`Bitmap::without_far_edge`]).
fn assert_mirrored(a: char, b: char, mirror: Mirror, size: (i32, i32)) {
    assert_mirrored_at(a, b, mirror, size, NORMAL);
}

fn assert_mirrored_at(a: char, b: char, mirror: Mirror, size: (i32, i32), weight: f32) {
    let mirrored = mirror_rects(&quads_at(a, size, weight), size, mirror);
    let (shift_x, shift_y) = expected_shift(a, size, mirror, weight);
    let expected = Bitmap::from_rects(size, &mirrored)
        .shifted(shift_x, shift_y)
        .without_far_edge(shift_x, shift_y);
    let actual = Bitmap::of_at(b, size, weight).without_far_edge(shift_x, shift_y);
    assert!(
        expected == actual,
        "{a:?} mirrored (shift x {shift_x}, y {shift_y}) is not {b:?} at {size:?} weight {weight}\nexpected:\n{}\nactual:\n{}",
        expected.ascii(),
        actual.ascii()
    );
    if !shift_x && !shift_y {
        assert_eq!(
            mirrored,
            sorted(quads_at(b, size, weight)),
            "{a:?} mirrored rect set is not {b:?}'s at {size:?} weight {weight}"
        );
    }
}

/// `[lo, hi)` moved `shift` pixels toward the origin; a boundary on the cell
/// edge stays there (the arm continues into the next cell). A far boundary on
/// the edge may also move: a rail biased onto the near edge of the original
/// stops one pixel short of the far edge in the reflection (bold rails on a
/// 7 px cell), the same ambiguity as [`Bitmap::without_far_edge`].
fn shift_interval(lo: i32, hi: i32, size: i32, shift: i32) -> Vec<(i32, i32)> {
    let lo = if lo == 0 { 0 } else { lo - shift };
    if hi == size && shift != 0 {
        vec![(lo, size), (lo, size - shift)]
    } else {
        vec![(lo, if hi == size { size } else { hi - shift })]
    }
}

// ---------------------------------------------------------------------------
// Coverage and bounds
// ---------------------------------------------------------------------------

#[test]
fn every_supported_code_point_emits_geometry() {
    for size in SIZES {
        for c in all_shape_chars() {
            assert!(is_shape_char(c), "{c:?} is not a shape char");
            // U+2800 is the empty braille pattern: a shape char that draws
            // nothing, so the font is still skipped.
            if c == '\u{2800}' {
                continue;
            }
            assert!(
                !quads(c, size).is_empty(),
                "{c:?} emits no geometry at {size:?}"
            );
        }
    }
    assert!(!is_shape_char('a'));
    assert!(!is_shape_char('\u{24FF}'));
    assert!(!is_shape_char('\u{E0C0}'));
}

#[test]
fn all_rects_within_cell_bounds() {
    for size in SIZES {
        for c in all_shape_chars() {
            for rect in quads(c, size) {
                assert!(
                    rect.w > 0 && rect.h > 0,
                    "{c:?} emits an empty rect {rect:?} at {size:?}"
                );
                assert!(
                    rect.x >= 0
                        && rect.y >= 0
                        && rect.x + rect.w <= size.0
                        && rect.y + rect.h <= size.1,
                    "{c:?} emits {rect:?} outside {size:?}"
                );
                assert!(
                    rect.alpha > 0.0 && rect.alpha <= 1.0,
                    "{c:?} emits {rect:?} with coverage outside (0, 1] at {size:?}"
                );
            }
        }
    }
}

/// Rect sets of the solid families at three cell sizes. Recorded before the
/// curved families moved to coverage rasterization and regenerated on
/// 2026-09-09 for the parity decision (DEC-0007 item 4 amendment): strokes keep
/// their nominal thickness and sit half a pixel toward the top/left where the
/// grid cannot centre them, so at 9 x 18 `─` is row 8 (was rows 8-9) and at
/// 14 x 29 the light strokes are 2 px (were 3) with the double rails and the
/// heavy dash cross following; 9 x 19 is unchanged. Every rect is fully covered.
/// Reviewed by eye through `shape_bitmaps_for_visual_review`.
#[rustfmt::skip]
const SOLID_SNAPSHOT: &[(char, (i32, i32), &[(i32, i32, i32, i32)])] = &[
    ('┌', (9, 18), &[(4, 8, 1, 10), (4, 8, 5, 1)]),
    ('┼', (9, 18), &[(4, 0, 1, 18), (0, 8, 9, 1)]),
    ('╬', (9, 18), &[(3, 0, 1, 8), (5, 0, 1, 8), (3, 9, 1, 9), (5, 9, 1, 9), (0, 7, 4, 1), (0, 9, 4, 1), (5, 7, 4, 1), (5, 9, 4, 1)]),
    ('╤', (9, 18), &[(4, 9, 1, 9), (0, 7, 9, 1), (0, 9, 9, 1)]),
    ('┄', (9, 18), &[(0, 8, 2, 1), (3, 8, 3, 1), (7, 8, 2, 1)]),
    ('┋', (9, 18), &[(3, 0, 3, 4), (3, 5, 3, 4), (3, 9, 3, 4), (3, 14, 3, 4)]),
    ('▄', (9, 18), &[(0, 9, 9, 9)]),
    ('▚', (9, 18), &[(0, 0, 5, 9), (4, 9, 5, 9)]),
    ('░', (9, 18), &[(0, 0, 3, 2), (6, 0, 3, 2), (0, 5, 3, 3), (6, 5, 3, 3), (0, 10, 3, 3), (6, 10, 3, 3), (0, 16, 3, 2), (6, 16, 3, 2)]),
    ('▒', (9, 18), &[(0, 0, 3, 2), (6, 0, 3, 2), (3, 2, 3, 3), (0, 5, 3, 3), (6, 5, 3, 3), (3, 8, 3, 2), (0, 10, 3, 3), (6, 10, 3, 3), (3, 13, 3, 3), (0, 16, 3, 2), (6, 16, 3, 2)]),
    ('▓', (9, 18), &[(0, 0, 9, 2), (0, 2, 3, 3), (6, 2, 3, 3), (0, 5, 9, 3), (0, 8, 3, 2), (6, 8, 3, 2), (0, 10, 9, 3), (0, 13, 3, 3), (6, 13, 3, 3), (0, 16, 9, 2)]),
    ('⣿', (9, 18), &[(1, 1, 3, 3), (1, 5, 3, 3), (1, 10, 3, 3), (5, 1, 3, 3), (5, 5, 3, 3), (5, 10, 3, 3), (1, 14, 3, 3), (5, 14, 3, 3)]),
    ('▬', (9, 18), &[(0, 4, 9, 10)]),
    ('━', (9, 18), &[(0, 7, 9, 3)]),
    ('╟', (9, 18), &[(3, 0, 1, 18), (5, 0, 1, 18), (5, 8, 4, 1)]),
    ('┌', (9, 19), &[(4, 9, 1, 10), (4, 9, 5, 1)]),
    ('┼', (9, 19), &[(4, 0, 1, 19), (0, 9, 9, 1)]),
    ('╬', (9, 19), &[(3, 0, 1, 9), (5, 0, 1, 9), (3, 10, 1, 9), (5, 10, 1, 9), (0, 8, 4, 1), (0, 10, 4, 1), (5, 8, 4, 1), (5, 10, 4, 1)]),
    ('╤', (9, 19), &[(4, 10, 1, 9), (0, 8, 9, 1), (0, 10, 9, 1)]),
    ('┄', (9, 19), &[(0, 9, 2, 1), (3, 9, 3, 1), (7, 9, 2, 1)]),
    ('┋', (9, 19), &[(3, 0, 3, 4), (3, 5, 3, 4), (3, 10, 3, 4), (3, 15, 3, 4)]),
    ('▄', (9, 19), &[(0, 9, 9, 10)]),
    ('▚', (9, 19), &[(0, 0, 5, 10), (4, 9, 5, 10)]),
    ('░', (9, 19), &[(0, 0, 3, 2), (6, 0, 3, 2), (0, 5, 3, 3), (6, 5, 3, 3), (0, 11, 3, 3), (6, 11, 3, 3), (0, 17, 3, 2), (6, 17, 3, 2)]),
    ('▒', (9, 19), &[(0, 0, 3, 2), (6, 0, 3, 2), (3, 2, 3, 3), (0, 5, 3, 3), (6, 5, 3, 3), (3, 8, 3, 3), (0, 11, 3, 3), (6, 11, 3, 3), (3, 14, 3, 3), (0, 17, 3, 2), (6, 17, 3, 2)]),
    ('▓', (9, 19), &[(0, 0, 9, 2), (0, 2, 3, 3), (6, 2, 3, 3), (0, 5, 9, 3), (0, 8, 3, 3), (6, 8, 3, 3), (0, 11, 9, 3), (0, 14, 3, 3), (6, 14, 3, 3), (0, 17, 9, 2)]),
    ('⣿', (9, 19), &[(1, 1, 3, 3), (1, 5, 3, 3), (1, 11, 3, 3), (5, 1, 3, 3), (5, 5, 3, 3), (5, 11, 3, 3), (1, 15, 3, 3), (5, 15, 3, 3)]),
    ('▬', (9, 19), &[(0, 4, 9, 11)]),
    ('━', (9, 19), &[(0, 8, 9, 3)]),
    ('╟', (9, 19), &[(3, 0, 1, 19), (5, 0, 1, 19), (5, 9, 4, 1)]),
    ('┌', (14, 29), &[(6, 13, 2, 16), (6, 13, 8, 2)]),
    ('┼', (14, 29), &[(6, 0, 2, 29), (0, 13, 14, 2)]),
    ('╬', (14, 29), &[(4, 0, 2, 13), (8, 0, 2, 13), (4, 15, 2, 14), (8, 15, 2, 14), (0, 11, 6, 2), (0, 15, 6, 2), (8, 11, 6, 2), (8, 15, 6, 2)]),
    ('╤', (14, 29), &[(6, 15, 2, 14), (0, 11, 14, 2), (0, 15, 14, 2)]),
    ('┄', (14, 29), &[(1, 13, 3, 2), (5, 13, 4, 2), (10, 13, 3, 2)]),
    ('┋', (14, 29), &[(4, 1, 5, 5), (4, 8, 5, 5), (4, 16, 5, 5), (4, 23, 5, 5)]),
    ('▄', (14, 29), &[(0, 14, 14, 15)]),
    ('▚', (14, 29), &[(0, 0, 7, 15), (7, 14, 7, 15)]),
    ('░', (14, 29), &[(0, 0, 5, 2), (9, 0, 5, 2), (0, 7, 5, 5), (9, 7, 5, 5), (0, 17, 5, 5), (9, 17, 5, 5), (0, 27, 5, 2), (9, 27, 5, 2)]),
    ('▒', (14, 29), &[(0, 0, 5, 2), (9, 0, 5, 2), (5, 2, 4, 5), (0, 7, 5, 5), (9, 7, 5, 5), (5, 12, 4, 5), (0, 17, 5, 5), (9, 17, 5, 5), (5, 22, 4, 5), (0, 27, 5, 2), (9, 27, 5, 2)]),
    ('▓', (14, 29), &[(0, 0, 14, 2), (0, 2, 5, 5), (9, 2, 5, 5), (0, 7, 14, 5), (0, 12, 5, 5), (9, 12, 5, 5), (0, 17, 14, 5), (0, 22, 5, 5), (9, 22, 5, 5), (0, 27, 14, 2)]),
    ('⣿', (14, 29), &[(1, 1, 4, 4), (1, 9, 4, 4), (1, 16, 4, 4), (9, 1, 4, 4), (9, 9, 4, 4), (9, 16, 4, 4), (1, 24, 4, 4), (9, 24, 4, 4)]),
    ('▬', (14, 29), &[(0, 7, 14, 15)]),
    ('━', (14, 29), &[(0, 12, 14, 5)]),
    ('╟', (14, 29), &[(4, 0, 2, 29), (8, 0, 2, 29), (8, 13, 6, 2)]),
];

#[test]
fn solid_families_unchanged_and_opaque() {
    for (c, size, rects) in SOLID_SNAPSHOT {
        let expected: Vec<DeviceRect> = rects
            .iter()
            .map(|&(x, y, w, h)| DeviceRect {
                x,
                y,
                w,
                h,
                alpha: 1.0,
            })
            .collect();
        assert_eq!(quads(*c, *size), expected, "{c:?} changed at {size:?}");
    }
    for size in SIZES {
        for c in all_shape_chars().filter(|c| !is_coverage_shape(*c)) {
            for rect in quads(c, size) {
                assert_eq!(
                    rect.alpha, 1.0,
                    "{c:?} emits a translucent rect at {size:?}"
                );
            }
        }
    }
}

#[test]
fn curved_edges_have_fractional_coverage() {
    for size in [(9, 19), (14, 29)] {
        for c in ['\u{256D}', '\u{E0B4}', '\u{E0B0}', '\u{2571}'] {
            let rects = quads(c, size);
            let fractional = rects
                .iter()
                .filter(|rect| rect.alpha > 0.0 && rect.alpha < 1.0)
                .count();
            assert!(
                fractional > 0,
                "{c:?} has no anti-aliased edge at {size:?}:\n{}",
                Bitmap::from_rects(size, &rects).ascii()
            );
            // Sixteen samples per pixel: every alpha is a multiple of 1/16.
            for rect in &rects {
                let sixteenths = rect.alpha * 16.0;
                assert_eq!(
                    sixteenths,
                    sixteenths.round(),
                    "{c:?} alpha {} at {size:?}",
                    rect.alpha
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Symmetry
// ---------------------------------------------------------------------------

#[test]
fn mirror_x_pairs_match() {
    let pairs = [
        ('┌', '┐'),
        ('┗', '┛'),
        ('├', '┤'),
        ('╭', '╮'),
        ('╔', '╗'),
        ('╠', '╣'),
        ('▌', '▐'),
        ('▏', '▕'),
        ('▘', '▝'),
        ('╱', '╲'),
        ('\u{E0B0}', '\u{E0B2}'),
        ('\u{E0B4}', '\u{E0B6}'),
        ('\u{E0B8}', '\u{E0BA}'),
        ('\u{2801}', '\u{2808}'),
    ];
    for (size, weight) in SIZES.iter().flat_map(|s| WEIGHTS.map(|w| (*s, w))) {
        for (a, b) in pairs {
            assert_mirrored_at(a, b, MX, size, weight);
        }
    }
}

#[test]
fn mirror_y_pairs_match() {
    let pairs = [
        ('┌', '└'),
        ('┬', '┴'),
        ('╭', '╰'),
        ('╒', '╘'),
        ('╓', '╙'),
        ('╔', '╚'),
        ('▀', '▄'),
        ('▔', '▁'),
        ('▘', '▖'),
        ('\u{E0B8}', '\u{E0BC}'),
        ('\u{2801}', '\u{2840}'),
    ];
    for (size, weight) in SIZES.iter().flat_map(|s| WEIGHTS.map(|w| (*s, w))) {
        for (a, b) in pairs {
            assert_mirrored_at(a, b, MY, size, weight);
        }
    }
}

#[test]
fn self_symmetric_glyphs() {
    let glyphs = [
        '─', '│', '┼', '━', '┃', '╋', '═', '║', '╬', '█', '╳', '▒', '▬',
    ];
    for (size, weight) in SIZES.iter().flat_map(|s| WEIGHTS.map(|w| (*s, w))) {
        for glyph in glyphs {
            assert_mirrored_at(glyph, glyph, MX, size, weight);
            assert_mirrored_at(glyph, glyph, MY, size, weight);
        }
    }
}

#[test]
fn builder_commutes_with_transforms() {
    for (size, weight) in SIZES.iter().flat_map(|s| WEIGHTS.map(|w| (*s, w))) {
        let g = Geometry::new(cell(size), weight);
        let t = g.thickness;
        // On an axis where a nominal thickness cannot be centred, a rect of that
        // weight sits one pixel toward the top/left of its reflection; where
        // both weights centre, the reflection is exact. Never the other way.
        let allowed = |n: i32| {
            if (n - t.light) % 2 != 0 || (n - t.heavy) % 2 != 0 {
                vec![0, 1]
            } else {
                vec![0]
            }
        };
        let (shifts_x, shifts_y) = (allowed(size.0), allowed(size.1));
        for (c, arms, mirror) in BOX_ARMS.iter().copied() {
            let canonical = mirror.apply(arms);
            for reflection in [MX, MY, MXY] {
                let mut direct = Vec::new();
                build_arms(&reflection.apply(canonical), &g, &mut direct);
                let mut built = Vec::new();
                build_arms(&canonical, &g, &mut built);
                let mut remaining = mirror_rects(&built, size, reflection);
                for rect in &direct {
                    let matches = |m: &DeviceRect| {
                        shifts_x.iter().any(|&sx| {
                            shifts_y.iter().any(|&sy| {
                                let sx = if reflection.x { sx } else { 0 };
                                let sy = if reflection.y { sy } else { 0 };
                                let xs = shift_interval(m.x, m.x + m.w, size.0, sx);
                                let ys = shift_interval(m.y, m.y + m.h, size.1, sy);
                                xs.iter().any(|&(x0, x1)| {
                                    ys.iter().any(|&(y0, y1)| {
                                        (x0, x1, y0, y1)
                                            == (rect.x, rect.x + rect.w, rect.y, rect.y + rect.h)
                                    })
                                })
                            })
                        })
                    };
                    let index = remaining.iter().position(matches).unwrap_or_else(|| {
                        panic!(
                            "builder does not commute with {reflection:?} for {c:?} at {size:?} weight {weight}: {rect:?} has no reflected counterpart in {remaining:?}"
                        )
                    });
                    remaining.swap_remove(index);
                }
                assert!(
                    remaining.is_empty(),
                    "builder does not commute with {reflection:?} for {c:?} at {size:?} weight {weight}: unmatched {remaining:?}"
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Joints
// ---------------------------------------------------------------------------

#[test]
fn horizontal_line_abuts_across_cells() {
    for (size, weight) in SIZES.iter().flat_map(|s| WEIGHTS.map(|w| (*s, w))) {
        for line in ['─', '━'] {
            let rects = quads_at(line, size, weight);
            assert_eq!(rects.len(), 1, "{line:?} at {size:?} is not one bar");
            assert_eq!(rects[0].x, 0);
            assert_eq!(rects[0].x + rects[0].w, size.0);
        }
        // Both rails of a double line span the full cell width, so they meet the
        // rails of the neighbouring cell.
        let rails = quads_at('═', size, weight);
        for rail in &rails {
            assert_eq!(rail.x, 0, "double rail does not start at the cell edge");
            assert_eq!(rail.x + rail.w, size.0);
        }
        // The outer dash segments end within one dash gap of the cell edge, so
        // the pitch of a dashed run carries across the cell boundary.
        let dashes = sorted(quads_at('┄', size, weight));
        let gap = thickness(size, weight).dash_gap;
        let head = dashes.first().map(|rect| rect.x).unwrap_or_default();
        let tail = dashes
            .last()
            .map(|rect| size.0 - rect.x - rect.w)
            .unwrap_or_default();
        assert_eq!(head, tail, "dash insets are not symmetric at {size:?}");
        assert!(head <= gap, "dash inset {head} exceeds the gap at {size:?}");
    }
}

#[test]
fn cross_equals_union_of_lines() {
    for (size, weight) in SIZES.iter().flat_map(|s| WEIGHTS.map(|w| (*s, w))) {
        let of = |c: char| Bitmap::of_at(c, size, weight);
        assert!(
            of('┼') == of('─').union(&of('│')),
            "light cross is not the union of its lines at {size:?} weight {weight}"
        );
        assert!(
            of('╋') == of('━').union(&of('┃')),
            "heavy cross is not the union of its lines at {size:?} weight {weight}"
        );

        let double_cross = of('╬');
        let double_lines = of('═').union(&of('║'));
        assert!(double_cross.is_subset_of(&double_lines));

        // The middle of a double cross stays open. A one pixel cell has no
        // room for the two rails, let alone the hole between them.
        if size.0 < 5 || size.1 < 5 {
            continue;
        }
        let g = Geometry::new(cell(size), weight);
        let joint_x = g.joint(Axis::X, g.thickness.light);
        let joint_y = g.joint(Axis::Y, g.thickness.light);
        for y in joint_y.lo..joint_y.hi {
            for x in joint_x.lo..joint_x.hi {
                assert!(
                    !double_cross.get(x, y),
                    "double cross fills its center at {size:?} weight {weight}\n{}",
                    double_cross.ascii()
                );
            }
        }
    }
}

#[test]
fn corner_arms_meet_at_joint() {
    for (size, weight) in SIZES.iter().flat_map(|s| WEIGHTS.map(|w| (*s, w))) {
        let of = |c: char| Bitmap::of_at(c, size, weight);
        assert!(
            of('┌') == of('╶').union(&of('╷')),
            "light corner is not its two half lines at {size:?} weight {weight}"
        );
        assert!(
            of('┏') == of('╺').union(&of('╻')),
            "heavy corner is not its two half lines at {size:?} weight {weight}"
        );
    }
}

#[test]
fn double_corner_up_and_down_differ() {
    // Below five pixels a double line has no room for two rails and every
    // corner collapses onto the same pixels.
    for size in SIZES
        .iter()
        .copied()
        .filter(|size| size.0 >= 5 && size.1 >= 5)
    {
        for (down, up) in [('╒', '╘'), ('╓', '╙'), ('╕', '╛'), ('╖', '╜')] {
            assert_ne!(
                sorted(quads(down, size)),
                sorted(quads(up, size)),
                "{down:?} and {up:?} are identical at {size:?}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Families B, E, F, G
// ---------------------------------------------------------------------------

#[test]
fn dash_segment_counts() {
    for size in SIZES.iter().copied().filter(|size| size.0 >= 8) {
        for (dash, count) in [('╌', 2), ('┄', 3), ('┈', 4), ('╍', 2), ('┅', 3), ('┉', 4)]
        {
            let rects = sorted(quads(dash, size));
            assert_eq!(rects.len(), count, "{dash:?} at {size:?}");
            for pair in rects.windows(2) {
                assert!(
                    pair[0].x + pair[0].w < pair[1].x,
                    "{dash:?} segments touch at {size:?}"
                );
            }
            // Segment lengths are exactly symmetric along the dash's own axis
            // at every size: the middle segment widens rather than shifting.
            assert_eq!(
                rects,
                mirror_rects(&rects, size, MX),
                "{dash:?} segments are not symmetric at {size:?}"
            );
        }
        if size.1 >= 8 {
            for (dash, count) in [('╎', 2), ('┆', 3), ('┊', 4), ('╏', 2), ('┇', 3), ('┋', 4)]
            {
                let rects = sorted(quads(dash, size));
                assert_eq!(rects.len(), count, "{dash:?} at {size:?}");
                assert_eq!(
                    rects,
                    mirror_rects(&rects, size, MY),
                    "{dash:?} segments are not symmetric at {size:?}"
                );
            }
        }
    }
}

#[test]
fn block_eighth_fractions_monotone() {
    for size in SIZES {
        let mut previous = 0;
        for code in 0x2581..=0x2588u32 {
            let c = char::from_u32(code).expect("valid block code point");
            let rects = quads(c, size);
            assert_eq!(rects.len(), 1);
            let rect = rects[0];
            assert_eq!(rect.x, 0);
            assert_eq!(rect.w, size.0);
            assert_eq!(rect.y + rect.h, size.1, "lower blocks sit on the baseline");
            assert!(rect.h >= previous, "{c:?} shrank at {size:?}");
            previous = rect.h;
        }
        assert_eq!(previous, size.1, "full block does not fill the cell");

        let lower_half = quads('▄', size);
        assert_eq!(lower_half[0].h, (size.1 + 1) / 2);
        assert_eq!(lower_half[0].y, size.1 - (size.1 + 1) / 2);

        let mut previous = 0;
        for code in (0x2589..=0x258Fu32).rev() {
            let c = char::from_u32(code).expect("valid block code point");
            let rects = quads(c, size);
            assert_eq!(rects.len(), 1);
            assert_eq!(rects[0].x, 0);
            assert_eq!(rects[0].h, size.1);
            assert!(rects[0].w >= previous, "{c:?} shrank at {size:?}");
            previous = rects[0].w;
        }
    }
}

#[test]
fn quadrant_union_is_full_block() {
    for size in SIZES {
        let union = Bitmap::of('▘', size)
            .union(&Bitmap::of('▝', size))
            .union(&Bitmap::of('▖', size))
            .union(&Bitmap::of('▗', size));
        assert_eq!(
            union.count(),
            (size.0 * size.1) as usize,
            "quadrants leave a gap at {size:?}\n{}",
            union.ascii()
        );
        assert!(union == Bitmap::of('█', size));
    }
}

#[test]
fn braille_dot_slots() {
    for size in SIZES {
        let mut dots = Vec::new();
        for bit in 0..8u32 {
            let c = char::from_u32(0x2800 + (1 << bit)).expect("valid braille code point");
            let rects = quads(c, size);
            assert_eq!(
                rects.len(),
                1,
                "braille bit {bit} is not one dot at {size:?}"
            );
            dots.push(rects[0]);
        }
        // Slot order: bits 0,1,2,6 are the left column top to bottom, bits
        // 3,4,5,7 the right column.
        let left = [dots[0], dots[1], dots[2], dots[6]];
        let right = [dots[3], dots[4], dots[5], dots[7]];
        for (l, r) in left.iter().zip(right.iter()) {
            assert_eq!(l.y, r.y, "braille rows disagree at {size:?}");
            if size.0 >= 7 {
                assert!(l.x < r.x, "braille columns are not ordered at {size:?}");
            }
        }
        for column in [left, right] {
            for pair in column.windows(2) {
                if size.1 >= 8 {
                    assert!(pair[0].y < pair[1].y, "braille rows unordered at {size:?}");
                }
            }
        }

        let all_dots = quads('\u{28FF}', size);
        assert_eq!(all_dots.len(), 8);
        if size.0 >= 7 {
            let filled = Bitmap::from_rects(size, &all_dots);
            let area: i32 = all_dots.iter().map(|rect| rect.w * rect.h).sum();
            assert_eq!(
                filled.count(),
                area as usize,
                "braille dots overlap at {size:?}"
            );
        }
        assert!(quads('\u{2800}', size).is_empty());
        assert!(is_shape_char('\u{2800}'));
    }
}

#[test]
fn shade_density_and_budget() {
    for size in SIZES {
        for (shade, target) in [('░', 0.25), ('▒', 0.50), ('▓', 0.75)] {
            let rects = quads(shade, size);
            assert!(
                rects.len() <= 24,
                "{shade:?} emits {} rects at {size:?}",
                rects.len()
            );
            assert_mirrored(shade, shade, MX, size);
            assert_mirrored(shade, shade, MY, size);
            if size.0 < 7 {
                continue;
            }
            let coverage =
                Bitmap::from_rects(size, &rects).count() as f32 / (size.0 * size.1) as f32;
            // The tile grid has an odd number of columns, so on a cell whose
            // pitch yields three of them the light and dark patterns run about
            // a tenth heavier than their nominal density.
            assert!(
                (coverage - target).abs() <= 0.13,
                "{shade:?} covers {coverage} of {size:?}, expected {target} +/- 0.13"
            );
        }
    }
}

#[test]
fn powerline_triangle_apex_at_center_height() {
    for size in SIZES {
        let (w, h) = size;
        let middle = (h - 1) / 2;
        for (c, base, apex) in [('\u{E0B0}', 0, w - 1), ('\u{E0B2}', w - 1, 0)] {
            let map = Bitmap::of(c, size);
            // The base spans the whole edge (its corner pixels are cut by the
            // sloping edges) and is solid on the center line.
            if w > 1 {
                assert_eq!(map.alpha(base, middle), 1.0, "{c:?} base at {size:?}");
            }
            for y in 0..h {
                assert!(map.alpha(base, y) > 0.0, "{c:?} base column at {size:?}");
                // The apex column is symmetric about the center line and peaks
                // there, so the apex sits at `C.y`.
                assert_eq!(
                    map.alpha(apex, y),
                    map.alpha(apex, h - 1 - y),
                    "{c:?} apex column is lopsided at {size:?}\n{}",
                    map.ascii()
                );
                assert!(
                    map.alpha(apex, y) <= map.alpha(apex, middle),
                    "{c:?} apex column peaks off center at {size:?}\n{}",
                    map.ascii()
                );
            }
            assert!(
                map.alpha(apex, middle) > 0.0,
                "{c:?} has no apex at {size:?}"
            );
        }

        // All sixteen powerline code points have real geometry, and the eight
        // filled ones are all different shapes. The four corner outlines are
        // only two diagonals by definition: the outline of the lower left
        // triangle is the same stroke as the outline of the upper right one,
        // and both are the box drawing diagonal.
        let mut filled: Vec<Bitmap> = Vec::new();
        for code in 0xE0B0..=0xE0BFu32 {
            let c = char::from_u32(code).expect("valid powerline code point");
            let rects = quads(c, size);
            assert!(!rects.is_empty(), "{c:?} has no geometry at {size:?}");
            if code % 2 == 0 && w > 1 && h > 1 {
                let map = Bitmap::from_rects(size, &rects);
                assert!(!filled.contains(&map), "{c:?} duplicates another fill");
                filled.push(map);
            }
        }
        if w > 1 && h > 1 {
            assert_eq!(filled.len(), 8);
        }
        assert!(
            Bitmap::of('\u{E0B9}', size) == Bitmap::of('\u{2572}', size),
            "corner outline is not the box diagonal at {size:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Primitives
// ---------------------------------------------------------------------------

#[test]
fn snap_interval_is_even_about_center() {
    let mut state = 0x2545_F491u32;
    let mut next = move || {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        state >> 8
    };
    for _ in 0..10_000 {
        let size = 1 + (next() % 64) as i32;
        let center = (next() % (size as u32 * 4 + 1)) as f32 / 4.0;
        let half = (next() % 12) as f32 / 2.0;
        for anchor in [Anchor::Fixed, Anchor::Nearest] {
            let interval = snap_interval(center, half, size, anchor);
            let mirrored = snap_interval(size as f32 - center, half, size, anchor);
            let reflected = Interval {
                lo: size - interval.hi,
                hi: size - interval.lo,
            };
            let thickness = ((2.0 * half).round() as i32).max(1);
            let center_halves = round_ties_toward(2.0 * center, size as f32);
            let biased = anchor == Anchor::Fixed && (center_halves - thickness) % 2 != 0;
            let interior =
                interval.lo > 0 && interval.hi < size && mirrored.lo > 0 && mirrored.hi < size;
            if biased {
                // Both intervals keep the nominal thickness and sit half a pixel
                // toward the top/left of their centre, so the mirror image is
                // one pixel off unless the cell edge clamps it back.
                let (lo, hi) = (reflected.lo - 1, reflected.hi - 1);
                assert!(
                    mirrored == reflected || mirrored == Interval { lo, hi },
                    "snap_interval({center}, {half}, {size}, {anchor:?}) is off by more than the parity bias: {interval:?} vs {mirrored:?}"
                );
                if interior {
                    assert_eq!(mirrored, Interval { lo, hi });
                    assert_eq!(interval.len(), thickness, "Fixed changed the thickness");
                    assert_eq!(
                        (interval.lo + interval.hi) as f32 / 2.0,
                        (center_halves - 1) as f32 / 2.0,
                        "Fixed did not move exactly half a pixel toward the top/left"
                    );
                }
            } else {
                assert_eq!(
                    mirrored, reflected,
                    "snap_interval({center}, {half}, {size}, {anchor:?}) is not mirror symmetric"
                );
            }
            assert!(interval.lo >= 0 && interval.hi <= size);
            if !interval.is_empty() && interval.lo > 0 && interval.hi < size {
                let midpoint = (interval.lo + interval.hi) as f32 / 2.0;
                // A quarter pixel of rounding plus half a pixel of parity.
                assert!(
                    (midpoint - center).abs() <= 0.75 + 1e-4,
                    "snapped midpoint {midpoint} strayed from {center}"
                );
            }
        }
    }
}

#[test]
fn heavy_thicker_than_light() {
    for (size, weight) in SIZES
        .iter()
        .filter(|size| size.0 >= 5)
        .flat_map(|s| WEIGHTS.map(|w| (*s, w)))
    {
        let thickness = thickness(size, weight);
        assert!(thickness.heavy >= thickness.light + 2);
        let light = quads_at('─', size, weight)[0];
        let heavy = quads_at('━', size, weight)[0];
        assert!(
            heavy.h > light.h,
            "heavy line is not thicker at {size:?}: {heavy:?} vs {light:?}"
        );
    }
}

/// The owner's complaint (2026-09-09): at 9 x 18 the horizontal light line was
/// 2 px and the vertical one 1 px. Both axes now paint the nominal thickness,
/// centred exactly or half a pixel toward the top/left, never the other way.
#[test]
fn stroke_thickness_uniform_across_axes() {
    for (size, weight) in PARITY_SIZES.iter().flat_map(|s| WEIGHTS.map(|w| (*s, w))) {
        let t = thickness(size, weight);
        for (horizontal, vertical, nominal) in [('─', '│', t.light), ('━', '┃', t.heavy)] {
            let h = quads_at(horizontal, size, weight);
            let v = quads_at(vertical, size, weight);
            assert_eq!(h.len(), 1);
            assert_eq!(v.len(), 1);
            assert_eq!(h[0].h, nominal, "{horizontal:?} at {size:?}: {:?}", h[0]);
            assert_eq!(v[0].w, nominal, "{vertical:?} at {size:?}: {:?}", v[0]);
            for (lo, len, center) in [
                (h[0].y, h[0].h, size.1 as f32 / 2.0),
                (v[0].x, v[0].w, size.0 as f32 / 2.0),
            ] {
                let midpoint = lo as f32 + len as f32 / 2.0;
                assert!(
                    midpoint == center || midpoint == center - 0.5,
                    "stroke at {size:?} is centred on {midpoint}, cell centre {center}"
                );
            }
        }
    }
}

/// The rails of a double line sit at equal distance from the light stroke that
/// runs between them, on the biased position too, so `═` and `─` (or `║` and
/// `│`) combine into `╪` / `╫` without a rail touching the single stroke.
#[test]
fn double_rails_equidistant_from_joint() {
    for (size, weight) in SIZES
        .iter()
        .chain(PARITY_SIZES.iter())
        .filter(|size| size.0 >= 5 && size.1 >= 5)
        .flat_map(|s| WEIGHTS.map(|w| (*s, w)))
    {
        let g = Geometry::new(cell(size), weight);
        let of = |c: char| Bitmap::of_at(c, size, weight);
        for axis in [Axis::X, Axis::Y] {
            let joint = g.joint(axis, g.thickness.light);
            let minus = g.rail(axis, Side::Minus);
            let plus = g.rail(axis, Side::Plus);
            assert!(
                minus.hi <= joint.lo && joint.hi <= plus.lo,
                "rails overlap the light stroke on {axis:?} at {size:?}: {minus:?} {joint:?} {plus:?}"
            );
            assert_eq!(
                joint.lo - minus.hi,
                plus.lo - joint.hi,
                "rails are not equidistant from the stroke on {axis:?} at {size:?}"
            );
            assert_eq!(minus.len(), plus.len());
        }
        assert!(
            of('╪') == of('═').union(&of('│')),
            "double horizontal and single vertical do not combine at {size:?} weight {weight}"
        );
        assert!(
            of('╫') == of('║').union(&of('─')),
            "double vertical and single horizontal do not combine at {size:?} weight {weight}"
        );
    }
}

#[test]
fn rails_disjoint_with_gap() {
    for (size, weight) in SIZES
        .iter()
        .filter(|size| size.0 >= 5)
        .flat_map(|s| WEIGHTS.map(|w| (*s, w)))
    {
        let g = Geometry::new(cell(size), weight);
        let minus = g.rail(Axis::X, Side::Minus);
        let plus = g.rail(Axis::X, Side::Plus);
        assert!(
            minus.hi < plus.lo,
            "horizontal rails touch at {size:?}: {minus:?} {plus:?}"
        );
        if size.1 >= 5 {
            let minus = g.rail(Axis::Y, Side::Minus);
            let plus = g.rail(Axis::Y, Side::Plus);
            assert!(
                minus.hi < plus.lo,
                "vertical rails touch at {size:?}: {minus:?} {plus:?}"
            );
        }
    }
}

#[test]
fn rounded_corner_matches_straight_stubs() {
    for (size, weight) in SIZES.iter().flat_map(|s| WEIGHTS.map(|w| (*s, w))) {
        let (w, h) = size;
        let g = Geometry::new(cell(size), weight);
        let joint_x = g.joint(Axis::X, g.thickness.light);
        let joint_y = g.joint(Axis::Y, g.thickness.light);
        let in_x = |x: i32| x >= joint_x.lo && x < joint_x.hi;
        let in_y = |y: i32| y >= joint_y.lo && y < joint_y.hi;
        let map = Bitmap::of_at('\u{256D}', size, weight);
        let ascii = map.ascii();
        // The arc's radius: its ends are tangent to the stubs at `r` from the
        // (parity-biased) stroke position on each axis.
        let hx = joint_x.len() as f32 / 2.0;
        let hy = joint_y.len() as f32 / 2.0;
        let (ax, ay) = (g.axis_center(Axis::X), g.axis_center(Axis::Y));
        let r = (g.w as f32 - ax - hx).min(g.h as f32 - ay - hy).max(0.0);

        // Below the arc's lower end the glyph is exactly the vertical stub:
        // solid on `J_x`, nothing beside it (no overshoot).
        for y in (ay + r).ceil() as i32..h {
            for x in 0..w {
                let expected = if in_x(x) { 1.0 } else { 0.0 };
                assert_eq!(
                    map.alpha(x, y),
                    expected,
                    "vertical stub at {size:?}\n{ascii}"
                );
            }
        }
        // Right of the arc's upper end the glyph is exactly the horizontal stub.
        for x in (ax + r).ceil() as i32..w {
            for y in 0..h {
                let expected = if in_y(y) { 1.0 } else { 0.0 };
                assert_eq!(
                    map.alpha(x, y),
                    expected,
                    "horizontal stub at {size:?}\n{ascii}"
                );
            }
        }
        if w < 3 || h < 3 {
            continue;
        }
        // The joins to the neighbouring cells are solid: the tangent ends of
        // the band sit inside the cell, so the light lines below and to the
        // right continue the stroke with neither a gap nor a lighter pixel.
        for x in joint_x.lo..joint_x.hi {
            assert_eq!(map.alpha(x, h - 1), 1.0, "bottom join at {size:?}\n{ascii}");
        }
        for y in joint_y.lo..joint_y.hi {
            assert_eq!(map.alpha(w - 1, y), 1.0, "right join at {size:?}\n{ascii}");
        }
        // No gap anywhere along the stroke: every row from the arc's top and
        // every column from the arc's left edge has coverage.
        let top = (0..h)
            .find(|&y| (0..w).any(|x| map.get(x, y)))
            .expect("arc top");
        let left = (0..w)
            .find(|&x| (0..h).any(|y| map.get(x, y)))
            .expect("arc left");
        for y in top..h {
            assert!(
                (0..w).any(|x| map.get(x, y)),
                "gap in row {y} at {size:?}\n{ascii}"
            );
        }
        for x in left..w {
            assert!(
                (0..h).any(|y| map.get(x, y)),
                "gap in column {x} at {size:?}\n{ascii}"
            );
        }
        // The arc is not a mitre: somewhere off both stub axes there is
        // coverage.
        assert!(
            (0..h).any(|y| (0..w).any(|x| map.get(x, y) && !in_x(x) && !in_y(y))),
            "no arc between the stubs at {size:?}\n{ascii}"
        );
    }
}

#[test]
fn per_cell_quad_budget() {
    for (size, weight) in SIZES.iter().flat_map(|s| WEIGHTS.map(|w| (*s, w))) {
        for c in all_shape_chars() {
            let count = quads_at(c, size, weight).len();
            let h = size.1 as usize;
            let budget = match c as u32 {
                0x2800..=0x28FF => 8,
                0x2591..=0x2593 => 24,
                // Coverage rasterized: a stroke is one or two runs per
                // scanline plus its anti-aliased edge pixels, and the cross
                // is two strokes; the measured counts at the reference sizes
                // are in `coverage_quad_counts`.
                0x2573 => 6 * h,
                0x256D..=0x2572 | 0xE0B0..=0xE0BF => 5 * h,
                _ => 8,
            };
            assert!(
                count <= budget,
                "{c:?} emits {count} rects at {size:?} weight {weight}, budget {budget}"
            );
        }
    }
}

/// Actual quad counts of the rasterized shapes at the two reference sizes,
/// recorded as upper bounds so a regression in run-length merging shows up.
#[test]
fn coverage_quad_counts() {
    let bounds: [(char, (i32, i32), usize); 8] = [
        // The arc glyph stays well under 2 x H; the filled and diagonal
        // shapes need a solid run plus one or two edge pixels per scanline
        // and land between 2 x H and 2.7 x H.
        ('\u{256D}', (9, 19), 13),
        ('\u{256D}', (14, 29), 22),
        ('\u{E0B4}', (9, 19), 39),
        ('\u{E0B4}', (14, 29), 61),
        ('\u{E0B0}', (9, 19), 42),
        ('\u{E0B0}', (14, 29), 66),
        ('\u{2571}', (9, 19), 41),
        ('\u{2571}', (14, 29), 77),
    ];
    for (c, size, bound) in bounds {
        let count = quads(c, size).len();
        println!("{c:?} at {size:?}: {count} rects");
        assert!(
            count <= bound,
            "{c:?} emits {count} rects at {size:?}, bound {bound}"
        );
    }
}

// ---------------------------------------------------------------------------
// Font weight
// ---------------------------------------------------------------------------

/// Weight 400 is the nominal geometry, and any lighter weight draws exactly
/// the same thing: `k = clamp(weight / 400, 1, 2.25)` is `1.0` for all of them.
#[test]
fn weight_400_matches_baseline_geometry() {
    for size in SIZES {
        assert_eq!(thickness(size, NORMAL), thickness(size, 100.0));
        assert_eq!(thickness(size, NORMAL), thickness(size, 399.0));
        for c in all_shape_chars() {
            let baseline = quads(c, size);
            for weight in [100.0, 300.0, 399.0] {
                assert_eq!(
                    quads_at(c, size, weight),
                    baseline,
                    "{c:?} at {size:?} changes at weight {weight}"
                );
            }
        }
    }
    // The snapshot in `solid_families_unchanged_and_opaque` is the weight-400
    // geometry; the light thickness there is the width-only rule.
    for (w, light, heavy) in [(7, 1, 3), (8, 1, 3), (9, 1, 3), (14, 2, 5), (18, 2, 6)] {
        let t = thickness((w, 2 * w), NORMAL);
        assert_eq!(
            (t.light, t.heavy, t.rail, t.gap, t.dash_gap),
            (light, heavy, light, light, light)
        );
    }
}

/// SGR bold (700 on a normal-weight font) thickens every stroke family that
/// has a thickness: single lines, heavy lines, the rails of a double line and
/// the arc band; the rails stay disjoint and the dash gap stays put.
#[test]
fn heavier_weight_thickens_strokes() {
    for size in [(9, 19), (14, 29)] {
        let bar = |c: char, weight: f32| {
            let rects = quads_at(c, size, weight);
            assert_eq!(rects.len(), 1, "{c:?} at {size:?} weight {weight}");
            rects[0]
        };
        assert!(bar('─', 700.0).h > bar('─', NORMAL).h, "light at {size:?}");
        assert!(bar('━', 700.0).h > bar('━', NORMAL).h, "heavy at {size:?}");
        assert!(
            bar('┃', 700.0).w > bar('┃', NORMAL).w,
            "heavy vertical at {size:?}"
        );
        assert!(
            bar('│', 700.0).w > bar('│', NORMAL).w,
            "light vertical at {size:?}"
        );

        let rails = |weight: f32| sorted(quads_at('═', size, weight));
        let (normal, bold) = (rails(NORMAL), rails(700.0));
        assert_eq!(bold.len(), 2);
        assert!(bold[0].h > normal[0].h, "rails at {size:?}");
        assert!(
            bold[0].y + bold[0].h < bold[1].y,
            "bold rails touch at {size:?}: {bold:?}"
        );

        // The arc: more covered pixels on the diagonal through the band, and a
        // thicker stub (the band meets a thicker `│`).
        let arc = |weight: f32| Bitmap::of_at('\u{256D}', size, weight);
        let (normal, bold) = (arc(NORMAL), arc(700.0));
        assert!(bold.count() > normal.count(), "arc band at {size:?}");
        let stub_width = |map: &Bitmap| (0..size.0).filter(|&x| map.get(x, size.1 - 1)).count();
        assert!(
            stub_width(&bold) > stub_width(&normal),
            "arc stub at {size:?}"
        );

        // A diagonal and a powerline outline are bands of `t_l` too.
        for c in ['\u{2572}', '\u{E0B1}', '\u{E0B5}'] {
            assert!(
                Bitmap::of_at(c, size, 700.0).count() > Bitmap::of_at(c, size, NORMAL).count(),
                "{c:?} band at {size:?}"
            );
        }

        // Fills, blocks, shades and braille are not strokes.
        for c in ['█', '▚', '▒', '\u{28FF}', '\u{E0B0}', '\u{E0B4}'] {
            assert_eq!(
                quads_at(c, size, 700.0),
                quads(c, size),
                "{c:?} changes with the weight at {size:?}"
            );
        }

        let t = thickness(size, 700.0);
        assert_eq!(t.dash_gap, thickness(size, NORMAL).light);
        assert_eq!(t.rail, t.light);
        assert_eq!(t.gap, t.light);
    }
    // Weights above 900 clamp to the 900 scale.
    for size in SIZES {
        assert_eq!(thickness(size, 1000.0), thickness(size, 900.0));
    }
}

/// Not an assertion: prints the thickness table documented in
/// `low-level-design/shapes.md` (`cargo test -p oneterm-terminal-view
/// thickness_table -- --ignored --nocapture`).
#[test]
#[ignore = "documentation table only"]
fn thickness_table_for_docs() {
    for w in [7, 8, 9, 14, 18] {
        for weight in [400.0, 600.0, 700.0, 900.0] {
            let t = thickness((w, 2 * w), weight);
            println!(
                "W {w:2} weight {weight:3}: light {} heavy {} rail {} gap {} dash_gap {}",
                t.light, t.heavy, t.rail, t.gap, t.dash_gap
            );
        }
    }
}

/// Not an assertion: renders the shapes that are easiest to get subtly wrong so
/// they can be eyeballed with `cargo test -p oneterm-terminal-view
/// shape_bitmaps -- --ignored --nocapture`.
#[test]
#[ignore = "visual review only"]
fn shape_bitmaps_for_visual_review() {
    for size in [(9, 18), (9, 19), (8, 16)] {
        for c in [
            '┌', '┼', '╔', '╬', '╒', '╘', '╤', '╟', '▚', '░', '▒', '▓', '\u{28FF}', '╭', '╯', '╱',
            '╳', '\u{E0B0}', '\u{E0B1}', '\u{E0B4}', '\u{E0B5}', '\u{E0B8}', '\u{E0B9}',
        ] {
            println!("{c:?} at {size:?}\n{}", Bitmap::of(c, size).ascii());
            if is_coverage_shape(c) {
                for rect in quads(c, size) {
                    println!("  {rect:?}");
                }
            }
        }
    }
}
