use super::*;

/// Cell sizes every geometry test runs over: odd and even widths, matching and
/// mismatched parity, a large HiDPI cell, and the degenerate one-pixel cell.
const SIZES: [(i32, i32); 7] = [
    (7, 15),
    (8, 16),
    (9, 19),
    (14, 29),
    (18, 38),
    (36, 76),
    (1, 1),
];

fn cell(size: (i32, i32)) -> CellSizeDevicePx {
    CellSizeDevicePx {
        w: size.0,
        h: size.1,
    }
}

fn quads(c: char, size: (i32, i32)) -> Vec<DeviceRect> {
    let mut out = Vec::new();
    shape_quads(c, cell(size), &mut out);
    out
}

fn paths(c: char, size: (i32, i32)) -> Vec<ShapePath> {
    let mut out = Vec::new();
    shape_paths(c, cell(size), &mut out);
    out
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
// Pixel sets
// ---------------------------------------------------------------------------

#[derive(Clone, PartialEq, Eq)]
struct Bitmap {
    w: i32,
    h: i32,
    on: Vec<bool>,
}

impl Bitmap {
    fn new(size: (i32, i32)) -> Self {
        Self {
            w: size.0,
            h: size.1,
            on: vec![false; (size.0 * size.1) as usize],
        }
    }

    fn of(c: char, size: (i32, i32)) -> Self {
        Self::from_rects(size, &quads(c, size))
    }

    fn from_rects(size: (i32, i32), rects: &[DeviceRect]) -> Self {
        let mut bitmap = Self::new(size);
        for rect in rects {
            for y in rect.y..rect.y + rect.h {
                for x in rect.x..rect.x + rect.w {
                    let index = (y * bitmap.w + x) as usize;
                    bitmap.on[index] = true;
                }
            }
        }
        bitmap
    }

    fn get(&self, x: i32, y: i32) -> bool {
        self.on[(y * self.w + x) as usize]
    }

    fn union(&self, other: &Self) -> Self {
        let mut merged = self.clone();
        for (slot, value) in merged.on.iter_mut().zip(other.on.iter()) {
            *slot |= *value;
        }
        merged
    }

    fn count(&self) -> usize {
        self.on.iter().filter(|on| **on).count()
    }

    fn is_subset_of(&self, other: &Self) -> bool {
        self.on
            .iter()
            .zip(other.on.iter())
            .all(|(mine, theirs)| !*mine || *theirs)
    }

    fn ascii(&self) -> String {
        let mut text = String::new();
        for y in 0..self.h {
            for x in 0..self.w {
                text.push(if self.get(x, y) { '#' } else { '.' });
            }
            text.push('\n');
        }
        text
    }
}

fn sorted(mut rects: Vec<DeviceRect>) -> Vec<DeviceRect> {
    rects.sort_by_key(|rect| (rect.x, rect.y, rect.w, rect.h));
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
            })
            .collect(),
    )
}

/// Points of a path set, mirrored and ordered so that two reflections can be
/// compared without depending on the direction the path was drawn in.
fn path_points(paths: &[ShapePath], size: (i32, i32), mirror: Mirror) -> Vec<(i32, i32)> {
    let mut points: Vec<(i32, i32)> = paths
        .iter()
        .flat_map(|path| path.ops.iter())
        .flat_map(|op| match op {
            PathOp::Move(p) | PathOp::Line(p) => vec![*p],
            PathOp::Cubic(a, b, c) => vec![*a, *b, *c],
            PathOp::Close => Vec::new(),
        })
        .map(|point| {
            let x = if mirror.x {
                size.0 as f32 - point.x
            } else {
                point.x
            };
            let y = if mirror.y {
                size.1 as f32 - point.y
            } else {
                point.y
            };
            // Compare at 1/16 pixel: the reflection is exact in theory but the
            // arithmetic runs through f32.
            ((x * 16.0).round() as i32, (y * 16.0).round() as i32)
        })
        .collect();
    points.sort_unstable();
    points
}

fn assert_mirrored(a: char, b: char, mirror: Mirror, size: (i32, i32)) {
    let expected = Bitmap::from_rects(size, &mirror_rects(&quads(a, size), size, mirror));
    let actual = Bitmap::of(b, size);
    assert!(
        expected == actual,
        "{a:?} mirrored is not {b:?} at {size:?}\nexpected:\n{}\nactual:\n{}",
        expected.ascii(),
        actual.ascii()
    );
    assert_eq!(
        path_points(&paths(a, size), size, mirror),
        path_points(&paths(b, size), size, ID),
        "{a:?} mirrored paths are not {b:?} at {size:?}"
    );
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
                !quads(c, size).is_empty() || !paths(c, size).is_empty(),
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
            }
        }
    }
}

#[test]
fn all_path_points_within_cell_bounds() {
    for size in SIZES {
        for c in all_shape_chars() {
            for path in paths(c, size) {
                for (x, y) in path_points(std::slice::from_ref(&path), size, ID) {
                    let (x, y) = (x as f32 / 16.0, y as f32 / 16.0);
                    assert!(
                        x >= -0.5 && x <= size.0 as f32 + 0.5,
                        "{c:?} path x {x} outside {size:?}"
                    );
                    assert!(
                        y >= -0.5 && y <= size.1 as f32 + 0.5,
                        "{c:?} path y {y} outside {size:?}"
                    );
                }
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
    for size in SIZES {
        for (a, b) in pairs {
            assert_mirrored(a, b, MX, size);
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
    for size in SIZES {
        for (a, b) in pairs {
            assert_mirrored(a, b, MY, size);
        }
    }
}

#[test]
fn self_symmetric_glyphs() {
    let glyphs = [
        '─', '│', '┼', '━', '┃', '╋', '═', '║', '╬', '█', '╳', '▒', '▬',
    ];
    for size in SIZES {
        for glyph in glyphs {
            assert_mirrored(glyph, glyph, MX, size);
            assert_mirrored(glyph, glyph, MY, size);
        }
    }
}

#[test]
fn builder_commutes_with_transforms() {
    for size in SIZES {
        let g = Geometry::new(cell(size));
        for (c, arms, mirror) in BOX_ARMS.iter().copied() {
            let canonical = mirror.apply(arms);
            for reflection in [MX, MY, MXY] {
                let mut direct = Vec::new();
                build_arms(&reflection.apply(canonical), &g, &mut direct);
                let mut built = Vec::new();
                build_arms(&canonical, &g, &mut built);
                assert_eq!(
                    sorted(direct),
                    mirror_rects(&built, size, reflection),
                    "builder does not commute with {reflection:?} for {c:?} at {size:?}"
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
    for size in SIZES {
        for line in ['─', '━'] {
            let rects = quads(line, size);
            assert_eq!(rects.len(), 1, "{line:?} at {size:?} is not one bar");
            assert_eq!(rects[0].x, 0);
            assert_eq!(rects[0].x + rects[0].w, size.0);
        }
        // Both rails of a double line span the full cell width, so they meet the
        // rails of the neighbouring cell.
        let rails = quads('═', size);
        for rail in &rails {
            assert_eq!(rail.x, 0, "double rail does not start at the cell edge");
            assert_eq!(rail.x + rail.w, size.0);
        }
        // The outer dash segments end within one dash gap of the cell edge, so
        // the pitch of a dashed run carries across the cell boundary.
        let dashes = sorted(quads('┄', size));
        let gap = stroke_thickness(cell(size)).light;
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
    for size in SIZES {
        assert!(
            Bitmap::of('┼', size) == Bitmap::of('─', size).union(&Bitmap::of('│', size)),
            "light cross is not the union of its lines at {size:?}"
        );
        assert!(
            Bitmap::of('╋', size) == Bitmap::of('━', size).union(&Bitmap::of('┃', size)),
            "heavy cross is not the union of its lines at {size:?}"
        );

        let double_cross = Bitmap::of('╬', size);
        let double_lines = Bitmap::of('═', size).union(&Bitmap::of('║', size));
        assert!(double_cross.is_subset_of(&double_lines));

        // The middle of a double cross stays open. A one pixel cell has no
        // room for the two rails, let alone the hole between them.
        if size.0 < 5 || size.1 < 5 {
            continue;
        }
        let g = Geometry::new(cell(size));
        let joint_x = g.joint(Axis::X, g.thickness.light);
        let joint_y = g.joint(Axis::Y, g.thickness.light);
        for y in joint_y.lo..joint_y.hi {
            for x in joint_x.lo..joint_x.hi {
                assert!(
                    !double_cross.get(x, y),
                    "double cross fills its center at {size:?}\n{}",
                    double_cross.ascii()
                );
            }
        }
    }
}

#[test]
fn corner_arms_meet_at_joint() {
    for size in SIZES {
        assert!(
            Bitmap::of('┌', size) == Bitmap::of('╶', size).union(&Bitmap::of('╷', size)),
            "light corner is not its two half lines at {size:?}"
        );
        assert!(
            Bitmap::of('┏', size) == Bitmap::of('╺', size).union(&Bitmap::of('╻', size)),
            "heavy corner is not its two half lines at {size:?}"
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
            assert_mirrored(dash, dash, MX, size);
        }
        if size.1 >= 8 {
            for (dash, count) in [('╎', 2), ('┆', 3), ('┊', 4), ('╏', 2), ('┇', 3), ('┋', 4)]
            {
                let rects = quads(dash, size);
                assert_eq!(rects.len(), count, "{dash:?} at {size:?}");
                assert_mirrored(dash, dash, MY, size);
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
        let apex = |c: char, x: f32| {
            let points = path_points(&paths(c, size), size, ID);
            let expected = (
                (x * 16.0).round() as i32,
                (size.1 as f32 / 2.0 * 16.0) as i32,
            );
            assert!(
                points.contains(&expected),
                "{c:?} has no apex at {expected:?} in {points:?} at {size:?}"
            );
        };
        apex('\u{E0B0}', size.0 as f32);
        apex('\u{E0B2}', 0.0);

        // All sixteen powerline code points have real geometry, and the eight
        // filled ones are all different shapes. The four corner outlines are
        // only two diagonals by definition: the outline of the lower left
        // triangle is the same stroke as the outline of the upper right one.
        let mut filled: Vec<Vec<(i32, i32)>> = Vec::new();
        for code in 0xE0B0..=0xE0BFu32 {
            let c = char::from_u32(code).expect("valid powerline code point");
            let built = paths(c, size);
            assert_eq!(built.len(), 1, "{c:?} has no path at {size:?}");
            let points = path_points(&built, size, ID);
            assert!(points.len() >= 2, "{c:?} has a degenerate path at {size:?}");
            if built[0].style == PathStyle::Fill && size.0 > 1 && size.1 > 1 {
                assert!(!filled.contains(&points), "{c:?} duplicates another fill");
                filled.push(points);
            }
        }
        if size.0 > 1 && size.1 > 1 {
            assert_eq!(filled.len(), 8);
        }
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
            assert_eq!(
                mirrored,
                Interval {
                    lo: size - interval.hi,
                    hi: size - interval.lo,
                },
                "snap_interval({center}, {half}, {size}, {anchor:?}) is not mirror symmetric"
            );
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
    for size in SIZES.iter().copied().filter(|size| size.0 >= 5) {
        let thickness = stroke_thickness(cell(size));
        assert!(thickness.heavy >= thickness.light + 2);
        let light = quads('─', size)[0];
        let heavy = quads('━', size)[0];
        assert!(
            heavy.h > light.h,
            "heavy line is not thicker at {size:?}: {heavy:?} vs {light:?}"
        );
    }
}

#[test]
fn rails_disjoint_with_gap() {
    for size in SIZES.iter().copied().filter(|size| size.0 >= 5) {
        let g = Geometry::new(cell(size));
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
fn rounded_corner_meets_neighbors() {
    for size in SIZES {
        let g = Geometry::new(cell(size));
        let joint_x = g.joint(Axis::X, g.thickness.light);
        let joint_y = g.joint(Axis::Y, g.thickness.light);
        let built = paths('╭', size);
        assert_eq!(built.len(), 1);
        let ops = &built[0].ops;
        let PathOp::Move(start) = ops[0] else {
            panic!("arc does not start with a move");
        };
        let PathOp::Cubic(_, _, end) = ops[1] else {
            panic!("arc is not a cubic");
        };
        assert!(
            (start.x - (joint_x.lo + joint_x.hi) as f32 / 2.0).abs() < 1e-3,
            "arc does not start on the vertical joint at {size:?}"
        );
        assert!(
            (end.y - (joint_y.lo + joint_y.hi) as f32 / 2.0).abs() < 1e-3,
            "arc does not end on the horizontal joint at {size:?}"
        );
    }
}

#[test]
fn per_cell_quad_budget() {
    for size in SIZES {
        for c in all_shape_chars() {
            let count = quads(c, size).len();
            let budget = match c as u32 {
                0x2800..=0x28FF => 8,
                0x2591..=0x2593 => 24,
                _ => 8,
            };
            assert!(
                count <= budget,
                "{c:?} emits {count} rects at {size:?}, budget {budget}"
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
    for size in [(9, 19), (8, 16)] {
        for c in [
            '┌', '┼', '╔', '╬', '╒', '╘', '╤', '╟', '▚', '░', '▒', '▓', '\u{28FF}', '\u{E0B0}',
        ] {
            println!("{c:?} at {size:?}\n{}", Bitmap::of(c, size).ascii());
        }
    }
}
