//! Geometry for the code points OneTerm draws itself instead of asking the font
//! for a glyph: box drawing (U+2500-257F), block elements (U+2580-259F), U+25AC,
//! braille (U+2800-28FF) and powerline (U+E0B0-E0BF).
//!
//! Fonts disagree about these glyphs and their strokes rarely meet across cell
//! boundaries, so DEC-0007 makes them geometry. Every definition lives in a
//! cell-local device-pixel space whose origin is the cell's top-left corner and
//! is expressed as offsets from the cell **center**; mirrored code points reuse
//! a canonical definition through a reflection of that definition. Snapping to
//! whole device pixels preserves the reflection (see [`snap_interval`]), so a
//! glyph is symmetric about both cell axes and identical features in adjacent
//! cells land on identical intervals: a horizontal line in cell *n* abuts the
//! one in cell *n + 1*, and the cross glyph is exactly the union of the two
//! lines it is made of.
//!
//! The module is pure: no GPUI, no alacritty, no state. The only allocation is
//! into the caller's output vector, except that [`ShapePath::ops`] owns a small
//! `Vec` - paths exist for eight code points that occur at most a few times per
//! row (arcs, diagonals, powerline), so the allocation is not on a hot path.

#[cfg(test)]
#[path = "shapes_tests.rs"]
mod shapes_tests;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Device-pixel size of one terminal cell. Both dimensions are treated as `>= 1`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct CellSizeDevicePx {
    pub w: i32,
    pub h: i32,
}

/// An axis-aligned rectangle in cell-relative device pixels. Row planning adds
/// the cell origin and converts to logical pixels (`device / scale_factor`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct DeviceRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// A point of a [`ShapePath`], in cell-relative device pixels.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct DevicePoint {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum PathOp {
    Move(DevicePoint),
    Line(DevicePoint),
    /// Cubic bezier: two control points and the end point.
    Cubic(DevicePoint, DevicePoint, DevicePoint),
    Close,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum PathStyle {
    Fill,
    /// Centered stroke of `width` device pixels.
    Stroke {
        width: f32,
    },
}

/// One sub-path of a shape. `ops` always starts with [`PathOp::Move`].
#[derive(Clone, PartialEq, Debug)]
pub(crate) struct ShapePath {
    pub style: PathStyle,
    pub ops: Vec<PathOp>,
}

/// Stroke widths derived from the cell width, in device pixels.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct StrokeThickness {
    pub light: i32,
    pub heavy: i32,
    /// Thickness of one rail of a double line.
    pub rail: i32,
    /// Nominal empty space between the two rails of a double line.
    pub gap: i32,
}

/// Light/heavy/double stroke widths for a cell. Both axes use the cell **width**
/// so that a horizontal and a vertical stroke start from the same nominal.
pub(crate) fn stroke_thickness(cell: CellSizeDevicePx) -> StrokeThickness {
    let w = cell.w.max(1) as f32;
    let light = ((w / 8.0).round() as i32).max(1);
    let heavy = (w / 3.0).round() as i32;
    StrokeThickness {
        light,
        heavy: heavy.max(light + 2),
        rail: light,
        gap: light,
    }
}

// ---------------------------------------------------------------------------
// Symmetric snapping
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Anchor {
    /// The centerline may not move; the thickness grows by one pixel on a
    /// parity mismatch. Used for anything centered on the cell axes.
    Fixed,
    /// The thickness may not change; the centerline moves half a pixel away
    /// from the cell center on a parity mismatch. Used for rails, braille dots
    /// and dash segments, where a fatter stroke would close a gap.
    Nearest,
}

/// Round `v` to an integer, breaking ties toward `center`.
///
/// `f32::round` breaks ties away from zero, which does **not** commute with
/// reflection: mirroring `0.5` in a cell of width 7 gives `6.5`, and
/// `round(0.5) = 1` while `7 - round(6.5) = 0`. Ties resolved toward the
/// reflection's fixed point do commute, which is what every mirrored code point
/// depends on.
fn round_ties_toward(v: f32, center: f32) -> i32 {
    let lower = v.floor();
    let frac = v - lower;
    let up = if (frac - 0.5).abs() <= 1e-4 {
        v < center
    } else {
        frac > 0.5
    };
    (if up { lower + 1.0 } else { lower }) as i32
}

/// Snap a centered extent to whole device pixels without breaking symmetry.
///
/// Returns the half-open interval `[lo, hi)` clipped to `[0, size]`. A symmetric
/// interval needs an even thickness around an integer center and an odd
/// thickness around a half-integer center; `anchor` decides which of the two
/// gives way when the parities disagree.
fn snap_interval(center: f32, half: f32, size: i32, anchor: Anchor) -> Interval {
    let mut thickness = ((2.0 * half).round() as i32).max(1);
    // Work in half pixels so the center stays exact.
    let mut center_halves = round_ties_toward(2.0 * center, size as f32);
    if (center_halves - thickness) % 2 != 0 {
        match anchor {
            Anchor::Fixed => thickness += 1,
            Anchor::Nearest => {
                // Both half-pixel moves are equally near, so the tie is broken
                // away from the cell center: paired features keep their gap.
                match center_halves.cmp(&size) {
                    std::cmp::Ordering::Less => center_halves -= 1,
                    std::cmp::Ordering::Greater => center_halves += 1,
                    // Exactly on the cell center there is no "away"; keep the
                    // centerline and widen instead, or the feature would stop
                    // being symmetric.
                    std::cmp::Ordering::Equal => thickness += 1,
                }
            }
        }
    }
    // A feature that would land outside the cell is shifted in rather than
    // clipped away: on a tiny cell the rails of a double line and the braille
    // dots must still show something. The shift is symmetric, so reflection
    // still commutes with snapping.
    let lo = ((center_halves - thickness) / 2)
        .clamp((size - thickness).min(0), (size - thickness).max(0));
    Interval {
        lo: lo.max(0),
        hi: (lo + thickness).min(size),
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Interval {
    lo: i32,
    hi: i32,
}

impl Interval {
    const EMPTY: Self = Self { lo: 0, hi: 0 };

    fn is_empty(self) -> bool {
        self.hi <= self.lo
    }

    fn len(self) -> i32 {
        (self.hi - self.lo).max(0)
    }

    fn touches(self, other: Self) -> bool {
        self.lo <= other.hi && other.lo <= self.hi
    }

    fn union(self, other: Self) -> Self {
        Self {
            lo: self.lo.min(other.lo),
            hi: self.hi.max(other.hi),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Axis {
    X,
    Y,
}

impl Axis {
    fn other(self) -> Self {
        match self {
            Axis::X => Axis::Y,
            Axis::Y => Axis::X,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Side {
    Minus,
    Plus,
}

impl Side {
    fn sign(self) -> f32 {
        match self {
            Side::Minus => -1.0,
            Side::Plus => 1.0,
        }
    }
}

// ---------------------------------------------------------------------------
// Cell geometry
// ---------------------------------------------------------------------------

/// The cell a shape is built in: size, center and derived thicknesses.
struct Geometry {
    w: i32,
    h: i32,
    cx: f32,
    cy: f32,
    thickness: StrokeThickness,
    /// Distance from the cell axis to a rail's centerline.
    rail_offset: f32,
}

impl Geometry {
    fn new(cell: CellSizeDevicePx) -> Self {
        let w = cell.w.max(1);
        let h = cell.h.max(1);
        let thickness = stroke_thickness(CellSizeDevicePx { w, h });
        Self {
            w,
            h,
            cx: w as f32 / 2.0,
            cy: h as f32 / 2.0,
            thickness,
            rail_offset: (thickness.rail + thickness.gap) as f32 / 2.0,
        }
    }

    fn span(&self, axis: Axis) -> (f32, i32) {
        match axis {
            Axis::X => (self.cx, self.w),
            Axis::Y => (self.cy, self.h),
        }
    }

    fn full(&self, axis: Axis) -> Interval {
        Interval {
            lo: 0,
            hi: self.span(axis).1,
        }
    }

    /// Interval covered on `axis` by a stroke of `thickness` centered on the cell.
    fn joint(&self, axis: Axis, thickness: i32) -> Interval {
        let (center, size) = self.span(axis);
        snap_interval(center, thickness as f32 / 2.0, size, Anchor::Fixed)
    }

    /// Interval covered on `axis` by one rail of a double line.
    fn rail(&self, axis: Axis, side: Side) -> Interval {
        let (center, size) = self.span(axis);
        snap_interval(
            center + side.sign() * self.rail_offset,
            self.thickness.rail as f32 / 2.0,
            size,
            Anchor::Nearest,
        )
    }

    /// Round a definition coordinate to a pixel boundary on `axis`.
    fn snap_edge(&self, axis: Axis, value: f32) -> i32 {
        let (center, size) = self.span(axis);
        round_ties_toward(value, center).clamp(0, size)
    }
}

fn push_rect(out: &mut Vec<DeviceRect>, x: Interval, y: Interval) {
    if x.is_empty() || y.is_empty() {
        return;
    }
    out.push(DeviceRect {
        x: x.lo,
        y: y.lo,
        w: x.len(),
        h: y.len(),
    });
}

// ---------------------------------------------------------------------------
// Family A / C - lines, corners, tees, crosses, half lines, double lines
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Weight {
    Light,
    Heavy,
    Double,
}

impl Weight {
    fn stroke(self, thickness: &StrokeThickness) -> i32 {
        match self {
            Weight::Light => thickness.light,
            Weight::Heavy => thickness.heavy,
            Weight::Double => thickness.rail,
        }
    }
}

/// Arms of one box-drawing code point, indexed by [`Dir`].
type ArmSet = [Option<Weight>; 4];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Dir {
    Up,
    Down,
    Left,
    Right,
}

const DIRS: [Dir; 4] = [Dir::Up, Dir::Down, Dir::Left, Dir::Right];

impl Dir {
    fn index(self) -> usize {
        match self {
            Dir::Up => 0,
            Dir::Down => 1,
            Dir::Left => 2,
            Dir::Right => 3,
        }
    }

    /// The axis the arm runs along.
    fn along_axis(self) -> Axis {
        match self {
            Dir::Up | Dir::Down => Axis::Y,
            Dir::Left | Dir::Right => Axis::X,
        }
    }

    fn opposite(self) -> Dir {
        match self {
            Dir::Up => Dir::Down,
            Dir::Down => Dir::Up,
            Dir::Left => Dir::Right,
            Dir::Right => Dir::Left,
        }
    }

    /// The perpendicular arm lying on `side` of this arm's cross axis.
    fn perpendicular(self, side: Side) -> Dir {
        match (self.along_axis(), side) {
            (Axis::X, Side::Minus) => Dir::Up,
            (Axis::X, Side::Plus) => Dir::Down,
            (Axis::Y, Side::Minus) => Dir::Left,
            (Axis::Y, Side::Plus) => Dir::Right,
        }
    }

    /// The cross side this arm points at: the rail of a perpendicular double
    /// line that the arm reaches first.
    fn near_side(self) -> Side {
        match self {
            Dir::Down | Dir::Right => Side::Plus,
            Dir::Up | Dir::Left => Side::Minus,
        }
    }

    /// Where an arm growing in this direction starts (or ends) given the
    /// interval it has to meet.
    fn edge_of(self, interval: Interval) -> i32 {
        match self {
            Dir::Down | Dir::Right => interval.lo,
            Dir::Up | Dir::Left => interval.hi,
        }
    }
}

/// Reflection of a *definition* (never of emitted rects): mirrored code points
/// store the canonical definition plus the reflection that produces them.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Mirror {
    x: bool,
    y: bool,
}

const ID: Mirror = Mirror { x: false, y: false };
const MX: Mirror = Mirror { x: true, y: false };
const MY: Mirror = Mirror { x: false, y: true };
const MXY: Mirror = Mirror { x: true, y: true };

impl Mirror {
    /// Reflecting an arm set is a permutation of its arm slots.
    fn apply(self, arms: ArmSet) -> ArmSet {
        let [up, down, left, right] = arms;
        let (up, down) = if self.y { (down, up) } else { (up, down) };
        let (left, right) = if self.x { (right, left) } else { (left, right) };
        [up, down, left, right]
    }
}

const N: Option<Weight> = None;
const L: Option<Weight> = Some(Weight::Light);
const H: Option<Weight> = Some(Weight::Heavy);
const D: Option<Weight> = Some(Weight::Double);

/// Arm sets straight from the Unicode chart names, in code point order:
/// `(code point, canonical arms [up, down, left, right], reflection)`.
/// `BOX DRAWINGS HEAVY DOWN AND LIGHT RIGHT` is `[N, H, N, L]`, and every
/// mirrored name reuses the canonical entry through its reflection.
#[rustfmt::skip]
const BOX_ARMS: &[(char, ArmSet, Mirror)] = &[
    ('\u{2500}', [N, N, L, L], ID),  // light horizontal
    ('\u{2501}', [N, N, H, H], ID),  // heavy horizontal
    ('\u{2502}', [L, L, N, N], ID),  // light vertical
    ('\u{2503}', [H, H, N, N], ID),  // heavy vertical
    ('\u{250C}', [N, L, N, L], ID),  // light down and right
    ('\u{250D}', [N, L, N, H], ID),  // down light and right heavy
    ('\u{250E}', [N, H, N, L], ID),  // down heavy and right light
    ('\u{250F}', [N, H, N, H], ID),  // heavy down and right
    ('\u{2510}', [N, L, N, L], MX),  // light down and left
    ('\u{2511}', [N, L, N, H], MX),  // down light and left heavy
    ('\u{2512}', [N, H, N, L], MX),  // down heavy and left light
    ('\u{2513}', [N, H, N, H], MX),  // heavy down and left
    ('\u{2514}', [N, L, N, L], MY),  // light up and right
    ('\u{2515}', [N, L, N, H], MY),  // up light and right heavy
    ('\u{2516}', [N, H, N, L], MY),  // up heavy and right light
    ('\u{2517}', [N, H, N, H], MY),  // heavy up and right
    ('\u{2518}', [N, L, N, L], MXY), // light up and left
    ('\u{2519}', [N, L, N, H], MXY), // up light and left heavy
    ('\u{251A}', [N, H, N, L], MXY), // up heavy and left light
    ('\u{251B}', [N, H, N, H], MXY), // heavy up and left
    ('\u{251C}', [L, L, N, L], ID),  // light vertical and right
    ('\u{251D}', [L, L, N, H], ID),  // vertical light and right heavy
    ('\u{251E}', [H, L, N, L], ID),  // up heavy and right down light
    ('\u{251F}', [L, H, N, L], ID),  // down heavy and right up light
    ('\u{2520}', [H, H, N, L], ID),  // vertical heavy and right light
    ('\u{2521}', [H, L, N, H], ID),  // down light and right up heavy
    ('\u{2522}', [L, H, N, H], ID),  // up light and right down heavy
    ('\u{2523}', [H, H, N, H], ID),  // heavy vertical and right
    ('\u{2524}', [L, L, N, L], MX),  // light vertical and left
    ('\u{2525}', [L, L, N, H], MX),  // vertical light and left heavy
    ('\u{2526}', [H, L, N, L], MX),  // up heavy and left down light
    ('\u{2527}', [L, H, N, L], MX),  // down heavy and left up light
    ('\u{2528}', [H, H, N, L], MX),  // vertical heavy and left light
    ('\u{2529}', [H, L, N, H], MX),  // down light and left up heavy
    ('\u{252A}', [L, H, N, H], MX),  // up light and left down heavy
    ('\u{252B}', [H, H, N, H], MX),  // heavy vertical and left
    ('\u{252C}', [N, L, L, L], ID),  // light down and horizontal
    ('\u{252D}', [N, L, H, L], ID),  // left heavy and right down light
    ('\u{252E}', [N, L, H, L], MX),  // right heavy and left down light
    ('\u{252F}', [N, L, H, H], ID),  // down light and horizontal heavy
    ('\u{2530}', [N, H, L, L], ID),  // down heavy and horizontal light
    ('\u{2531}', [N, H, H, L], ID),  // right light and left down heavy
    ('\u{2532}', [N, H, H, L], MX),  // left light and right down heavy
    ('\u{2533}', [N, H, H, H], ID),  // heavy down and horizontal
    ('\u{2534}', [N, L, L, L], MY),  // light up and horizontal
    ('\u{2535}', [N, L, H, L], MY),  // left heavy and right up light
    ('\u{2536}', [N, L, H, L], MXY), // right heavy and left up light
    ('\u{2537}', [N, L, H, H], MY),  // up light and horizontal heavy
    ('\u{2538}', [N, H, L, L], MY),  // up heavy and horizontal light
    ('\u{2539}', [N, H, H, L], MY),  // right light and left up heavy
    ('\u{253A}', [N, H, H, L], MXY), // left light and right up heavy
    ('\u{253B}', [N, H, H, H], MY),  // heavy up and horizontal
    ('\u{253C}', [L, L, L, L], ID),  // light vertical and horizontal
    ('\u{253D}', [L, L, H, L], ID),  // left heavy and right vertical light
    ('\u{253E}', [L, L, H, L], MX),  // right heavy and left vertical light
    ('\u{253F}', [L, L, H, H], ID),  // vertical light and horizontal heavy
    ('\u{2540}', [H, L, L, L], ID),  // up heavy and down horizontal light
    ('\u{2541}', [H, L, L, L], MY),  // down heavy and up horizontal light
    ('\u{2542}', [H, H, L, L], ID),  // vertical heavy and horizontal light
    ('\u{2543}', [H, L, H, L], ID),  // left up heavy and right down light
    ('\u{2544}', [H, L, H, L], MX),  // right up heavy and left down light
    ('\u{2545}', [H, L, H, L], MY),  // left down heavy and right up light
    ('\u{2546}', [H, L, H, L], MXY), // right down heavy and left up light
    ('\u{2547}', [H, L, H, H], ID),  // down light and up horizontal heavy
    ('\u{2548}', [H, L, H, H], MY),  // up light and down horizontal heavy
    ('\u{2549}', [H, H, H, L], ID),  // right light and left vertical heavy
    ('\u{254A}', [H, H, H, L], MX),  // left light and right vertical heavy
    ('\u{254B}', [H, H, H, H], ID),  // heavy vertical and horizontal
    ('\u{2550}', [N, N, D, D], ID),  // double horizontal
    ('\u{2551}', [D, D, N, N], ID),  // double vertical
    ('\u{2552}', [N, L, N, D], ID),  // down single and right double
    ('\u{2553}', [N, D, N, L], ID),  // down double and right single
    ('\u{2554}', [N, D, N, D], ID),  // double down and right
    ('\u{2555}', [N, L, N, D], MX),  // down single and left double
    ('\u{2556}', [N, D, N, L], MX),  // down double and left single
    ('\u{2557}', [N, D, N, D], MX),  // double down and left
    ('\u{2558}', [N, L, N, D], MY),  // up single and right double
    ('\u{2559}', [N, D, N, L], MY),  // up double and right single
    ('\u{255A}', [N, D, N, D], MY),  // double up and right
    ('\u{255B}', [N, L, N, D], MXY), // up single and left double
    ('\u{255C}', [N, D, N, L], MXY), // up double and left single
    ('\u{255D}', [N, D, N, D], MXY), // double up and left
    ('\u{255E}', [L, L, N, D], ID),  // vertical single and right double
    ('\u{255F}', [D, D, N, L], ID),  // vertical double and right single
    ('\u{2560}', [D, D, N, D], ID),  // double vertical and right
    ('\u{2561}', [L, L, N, D], MX),  // vertical single and left double
    ('\u{2562}', [D, D, N, L], MX),  // vertical double and left single
    ('\u{2563}', [D, D, N, D], MX),  // double vertical and left
    ('\u{2564}', [N, L, D, D], ID),  // down single and horizontal double
    ('\u{2565}', [N, D, L, L], ID),  // down double and horizontal single
    ('\u{2566}', [N, D, D, D], ID),  // double down and horizontal
    ('\u{2567}', [N, L, D, D], MY),  // up single and horizontal double
    ('\u{2568}', [N, D, L, L], MY),  // up double and horizontal single
    ('\u{2569}', [N, D, D, D], MY),  // double up and horizontal
    ('\u{256A}', [L, L, D, D], ID),  // vertical single and horizontal double
    ('\u{256B}', [D, D, L, L], ID),  // vertical double and horizontal single
    ('\u{256C}', [D, D, D, D], ID),  // double vertical and horizontal
    ('\u{2574}', [N, N, N, L], MX),  // light left
    ('\u{2575}', [N, L, N, N], MY),  // light up
    ('\u{2576}', [N, N, N, L], ID),  // light right
    ('\u{2577}', [N, L, N, N], ID),  // light down
    ('\u{2578}', [N, N, N, H], MX),  // heavy left
    ('\u{2579}', [N, H, N, N], MY),  // heavy up
    ('\u{257A}', [N, N, N, H], ID),  // heavy right
    ('\u{257B}', [N, H, N, N], ID),  // heavy down
    ('\u{257C}', [N, N, L, H], ID),  // light left and heavy right
    ('\u{257D}', [L, H, N, N], ID),  // light up and heavy down
    ('\u{257E}', [N, N, L, H], MX),  // heavy left and light right
    ('\u{257F}', [L, H, N, N], MY),  // heavy up and light down
];

fn box_arms(c: char) -> Option<ArmSet> {
    let index = BOX_ARMS.binary_search_by(|entry| entry.0.cmp(&c)).ok()?;
    let (_, arms, mirror) = BOX_ARMS[index];
    Some(mirror.apply(arms))
}

/// One emitted bar: an extent along the arm's own axis and across it.
#[derive(Clone, Copy)]
struct Piece {
    axis: Axis,
    along: Interval,
    cross: Interval,
}

/// Where an arm stops meeting the rest of the glyph.
///
/// The arm runs from the cell edge inwards until it reaches the joint of the
/// perpendicular arms: their joint interval for single lines, one of their two
/// rails for double lines. `side` is the rail of *this* arm when the arm itself
/// is double.
fn arm_boundary(dir: Dir, side: Option<Side>, own: Weight, arms: &ArmSet, g: &Geometry) -> i32 {
    let along_axis = dir.along_axis();
    let perp_minus = arms[dir.perpendicular(Side::Minus).index()];
    let perp_plus = arms[dir.perpendicular(Side::Plus).index()];

    if perp_minus != Some(Weight::Double) && perp_plus != Some(Weight::Double) {
        // Single perpendicular arms: everything meets in the joint square, which
        // uses the heaviest weight present on the perpendicular axis so that a
        // light arm still reaches under a heavy one.
        let weight = perp_minus.max(perp_plus).unwrap_or(own);
        return dir.edge_of(g.joint(along_axis, weight.stroke(&g.thickness)));
    }

    let use_near = match side {
        // A rail of a double arm stops at the near rail of the perpendicular
        // double when that side is enclosed by a perpendicular arm, and at the
        // far rail otherwise, which closes the outer corner.
        Some(Side::Minus) => perp_minus.is_some(),
        Some(Side::Plus) => perp_plus.is_some(),
        // A single arm crossing a double line runs through to the far rail when
        // it continues on the other side of the cell, and hangs off the near
        // rail when it is a stub between two perpendicular arms.
        None => {
            arms[dir.opposite().index()].is_none() && perp_minus.is_some() && perp_plus.is_some()
        }
    };
    let rail_side = if use_near {
        dir.near_side()
    } else {
        match dir.near_side() {
            Side::Minus => Side::Plus,
            Side::Plus => Side::Minus,
        }
    };
    dir.edge_of(g.rail(along_axis, rail_side))
}

fn arm_along(dir: Dir, side: Option<Side>, own: Weight, arms: &ArmSet, g: &Geometry) -> Interval {
    let size = g.span(dir.along_axis()).1;
    let cut = arm_boundary(dir, side, own, arms, g);
    match dir {
        Dir::Down | Dir::Right => Interval { lo: cut, hi: size },
        Dir::Up | Dir::Left => Interval { lo: 0, hi: cut },
    }
}

/// Build the quads of one arm set.
///
/// Opposite arms of equal weight are not special-cased: each arm is built from
/// the cell edge to the joint, and pieces that share a cross interval and touch
/// are merged, which turns them back into one full-length stroke.
fn build_arms(arms: &ArmSet, g: &Geometry, out: &mut Vec<DeviceRect>) {
    // At most four arms, two rails each; no allocation.
    let mut pieces = [Piece {
        axis: Axis::X,
        along: Interval::EMPTY,
        cross: Interval::EMPTY,
    }; 8];
    let mut count = 0usize;

    for dir in DIRS {
        let Some(weight) = arms[dir.index()] else {
            continue;
        };
        let along_axis = dir.along_axis();
        let cross_axis = along_axis.other();
        if weight == Weight::Double {
            for side in [Side::Minus, Side::Plus] {
                let piece = Piece {
                    axis: along_axis,
                    along: arm_along(dir, Some(side), weight, arms, g),
                    cross: g.rail(cross_axis, side),
                };
                add_piece(&mut pieces, &mut count, piece);
            }
        } else {
            let piece = Piece {
                axis: along_axis,
                along: arm_along(dir, None, weight, arms, g),
                cross: g.joint(cross_axis, weight.stroke(&g.thickness)),
            };
            add_piece(&mut pieces, &mut count, piece);
        }
    }

    for piece in pieces.iter().take(count) {
        match piece.axis {
            Axis::X => push_rect(out, piece.along, piece.cross),
            Axis::Y => push_rect(out, piece.cross, piece.along),
        }
    }
}

fn add_piece(pieces: &mut [Piece; 8], count: &mut usize, piece: Piece) {
    if piece.along.is_empty() || piece.cross.is_empty() {
        return;
    }
    for existing in pieces.iter_mut().take(*count) {
        if existing.axis == piece.axis
            && existing.cross == piece.cross
            && existing.along.touches(piece.along)
        {
            existing.along = existing.along.union(piece.along);
            return;
        }
    }
    if *count < pieces.len() {
        pieces[*count] = piece;
        *count += 1;
    }
}

// ---------------------------------------------------------------------------
// Family B - dashes
// ---------------------------------------------------------------------------

/// `(code point, segment count, weight, axis the dash runs along)`.
#[rustfmt::skip]
const DASHES: &[(char, i32, Weight, Axis)] = &[
    ('\u{2504}', 3, Weight::Light, Axis::X), // light triple dash horizontal
    ('\u{2505}', 3, Weight::Heavy, Axis::X),
    ('\u{2506}', 3, Weight::Light, Axis::Y),
    ('\u{2507}', 3, Weight::Heavy, Axis::Y),
    ('\u{2508}', 4, Weight::Light, Axis::X), // light quadruple dash horizontal
    ('\u{2509}', 4, Weight::Heavy, Axis::X),
    ('\u{250A}', 4, Weight::Light, Axis::Y),
    ('\u{250B}', 4, Weight::Heavy, Axis::Y),
    ('\u{254C}', 2, Weight::Light, Axis::X), // light double dash horizontal
    ('\u{254D}', 2, Weight::Heavy, Axis::X),
    ('\u{254E}', 2, Weight::Light, Axis::Y),
    ('\u{254F}', 2, Weight::Heavy, Axis::Y),
];

fn dash_def(c: char) -> Option<(i32, Weight, Axis)> {
    let index = DASHES.binary_search_by(|entry| entry.0.cmp(&c)).ok()?;
    let (_, count, weight, axis) = DASHES[index];
    Some((count, weight, axis))
}

fn build_dashes(axis: Axis, count: i32, weight: Weight, g: &Geometry, out: &mut Vec<DeviceRect>) {
    let (center, size) = g.span(axis);
    let cross = g.joint(axis.other(), weight.stroke(&g.thickness));
    if cross.is_empty() {
        return;
    }
    let segments_count = count as usize;
    let pitch = size as f32 / count as f32;
    // The dash gap equals the light thickness at every cell size.
    let gap = g.thickness.light as f32;
    let mut half = ((pitch - gap) / 2.0).max(0.5);
    let mut segments = [Interval::EMPTY; 4];
    loop {
        for (k, segment) in segments.iter_mut().take(segments_count).enumerate() {
            let segment_center = center + (k as f32 + 0.5 - count as f32 / 2.0) * pitch;
            // The middle segment of an odd count sits on the cell axis and has
            // to stay there; the others may shift half a pixel outwards.
            let anchor = if count % 2 == 1 && k == (count / 2) as usize {
                Anchor::Fixed
            } else {
                Anchor::Nearest
            };
            *segment = snap_interval(segment_center, half, size, anchor);
        }
        let overlaps = (1..segments_count).any(|k| segments[k].lo < segments[k - 1].hi);
        if !overlaps || half <= 0.5 {
            break;
        }
        // Shrinking every segment by the same amount keeps the set symmetric,
        // unlike nudging whichever segment happens to overlap.
        half -= 0.5;
    }

    let mut k = 0usize;
    while k < segments_count {
        // Segments can only still overlap on a cell too narrow to hold them.
        let mut merged = segments[k];
        while k + 1 < segments_count && segments[k + 1].lo < merged.hi {
            merged = merged.union(segments[k + 1]);
            k += 1;
        }
        k += 1;
        match axis {
            Axis::X => push_rect(out, merged, cross),
            Axis::Y => push_rect(out, cross, merged),
        }
    }
}

// ---------------------------------------------------------------------------
// Family E - block elements and U+25AC
// ---------------------------------------------------------------------------

/// `k / 8` of `size`, in device pixels. Never zero: a one-eighth block that
/// vanishes on a small cell would lose the character entirely.
fn eighth(k: i32, size: i32) -> i32 {
    ((k * size) as f32 / 8.0).round().max(1.0).min(size as f32) as i32
}

const QUADRANT_UPPER_LEFT: u8 = 1;
const QUADRANT_UPPER_RIGHT: u8 = 2;
const QUADRANT_LOWER_LEFT: u8 = 4;
const QUADRANT_LOWER_RIGHT: u8 = 8;

fn quadrant_mask(code: u32) -> Option<u8> {
    Some(match code {
        0x2596 => QUADRANT_LOWER_LEFT,
        0x2597 => QUADRANT_LOWER_RIGHT,
        0x2598 => QUADRANT_UPPER_LEFT,
        0x2599 => QUADRANT_UPPER_LEFT | QUADRANT_LOWER_LEFT | QUADRANT_LOWER_RIGHT,
        0x259A => QUADRANT_UPPER_LEFT | QUADRANT_LOWER_RIGHT,
        0x259B => QUADRANT_UPPER_LEFT | QUADRANT_UPPER_RIGHT | QUADRANT_LOWER_LEFT,
        0x259C => QUADRANT_UPPER_LEFT | QUADRANT_UPPER_RIGHT | QUADRANT_LOWER_RIGHT,
        0x259D => QUADRANT_UPPER_RIGHT,
        0x259E => QUADRANT_UPPER_RIGHT | QUADRANT_LOWER_LEFT,
        0x259F => QUADRANT_UPPER_RIGHT | QUADRANT_LOWER_LEFT | QUADRANT_LOWER_RIGHT,
        _ => return None,
    })
}

fn build_block(code: u32, g: &Geometry, out: &mut Vec<DeviceRect>) {
    let full_x = g.full(Axis::X);
    let full_y = g.full(Axis::Y);
    match code {
        // Upper half and upper one eighth are the reflections of the lower ones.
        0x2580 => push_rect(
            out,
            full_x,
            Interval {
                lo: 0,
                hi: eighth(4, g.h),
            },
        ),
        0x2594 => push_rect(
            out,
            full_x,
            Interval {
                lo: 0,
                hi: eighth(1, g.h),
            },
        ),
        0x2581..=0x2588 => {
            let k = (code - 0x2580) as i32;
            push_rect(
                out,
                full_x,
                Interval {
                    lo: g.h - eighth(k, g.h),
                    hi: g.h,
                },
            );
        }
        0x2589..=0x258F => {
            let k = 8 - (code - 0x2588) as i32;
            push_rect(
                out,
                Interval {
                    lo: 0,
                    hi: eighth(k, g.w),
                },
                full_y,
            );
        }
        // Right half and right one eighth are the reflections of the left ones.
        0x2590 => push_rect(
            out,
            Interval {
                lo: g.w - eighth(4, g.w),
                hi: g.w,
            },
            full_y,
        ),
        0x2595 => push_rect(
            out,
            Interval {
                lo: g.w - eighth(1, g.w),
                hi: g.w,
            },
            full_y,
        ),
        0x2596..=0x259F => {
            let Some(mask) = quadrant_mask(code) else {
                return;
            };
            // Half extents round up so that the four quadrants always cover the
            // whole cell, overlapping by one pixel on an odd dimension.
            let half_w = (g.w + 1) / 2;
            let half_h = (g.h + 1) / 2;
            let left = Interval { lo: 0, hi: half_w };
            let right = Interval {
                lo: g.w - half_w,
                hi: g.w,
            };
            let top = Interval { lo: 0, hi: half_h };
            let bottom = Interval {
                lo: g.h - half_h,
                hi: g.h,
            };
            for (bit, x, y) in [
                (QUADRANT_UPPER_LEFT, left, top),
                (QUADRANT_UPPER_RIGHT, right, top),
                (QUADRANT_LOWER_LEFT, left, bottom),
                (QUADRANT_LOWER_RIGHT, right, bottom),
            ] {
                if mask & bit != 0 {
                    push_rect(out, x, y);
                }
            }
        }
        // U+25AC BLACK RECTANGLE: one centered half-height bar.
        0x25AC => push_rect(
            out,
            full_x,
            snap_interval(g.cy, g.h as f32 / 4.0, g.h, Anchor::Fixed),
        ),
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Family F - shades
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Shade {
    Light,
    Medium,
    Dark,
}

/// Shade patterns are a tile grid whose pitch follows the cell size, so the
/// quad count is bounded at any cell size instead of falling off a cliff.
fn build_shade(shade: Shade, g: &Geometry, out: &mut Vec<DeviceRect>) {
    let pitch = ((g.w as f32 / 3.0).round() as i32).max(2);
    // Odd tile counts keep the grid symmetric: reflection maps tile i to n-1-i.
    let tiles_x = ((g.w + pitch - 1) / pitch) | 1;
    let tiles_y = ((g.h + pitch - 1) / pitch) | 1;
    for j in 0..tiles_y {
        let y = Interval {
            lo: tile_edge(g, Axis::Y, j, tiles_y, pitch),
            hi: tile_edge(g, Axis::Y, j + 1, tiles_y, pitch),
        };
        if y.is_empty() {
            continue;
        }
        // Horizontally adjacent tiles of the same row merge into one quad.
        let mut run: Option<Interval> = None;
        for i in 0..tiles_x {
            let on = match shade {
                Shade::Light => (i + j) % 2 == 0 && j % 2 == 0,
                Shade::Medium => (i + j) % 2 == 0,
                Shade::Dark => !((i + j) % 2 == 0 && j % 2 == 1),
            };
            let x = Interval {
                lo: tile_edge(g, Axis::X, i, tiles_x, pitch),
                hi: tile_edge(g, Axis::X, i + 1, tiles_x, pitch),
            };
            if on && !x.is_empty() {
                run = Some(match run {
                    Some(previous) if previous.touches(x) => previous.union(x),
                    Some(previous) => {
                        push_rect(out, previous, y);
                        x
                    }
                    None => x,
                });
            } else if let Some(previous) = run.take() {
                push_rect(out, previous, y);
            }
        }
        if let Some(previous) = run {
            push_rect(out, previous, y);
        }
    }
}

fn tile_edge(g: &Geometry, axis: Axis, index: i32, count: i32, pitch: i32) -> i32 {
    let center = g.span(axis).0;
    g.snap_edge(
        axis,
        center + (index as f32 - count as f32 / 2.0) * pitch as f32,
    )
}

// ---------------------------------------------------------------------------
// Family G - braille
// ---------------------------------------------------------------------------

/// Dot bit 0..7 (0x01 .. 0x80) to its `(column, row)` slot.
const BRAILLE_SLOTS: [(i32, i32); 8] = [
    (0, 0),
    (0, 1),
    (0, 2),
    (1, 0),
    (1, 1),
    (1, 2),
    (0, 3),
    (1, 3),
];

fn build_braille(c: char, g: &Geometry, out: &mut Vec<DeviceRect>) {
    let bits = (c as u32 - 0x2800) as u8;
    let diameter = (((g.w as f32 / 2.0).min(g.h as f32 / 4.0)) * 0.6)
        .round()
        .max(1.0);
    let half = diameter / 2.0;
    for (bit, (column, row)) in BRAILLE_SLOTS.iter().enumerate() {
        if bits & (1 << bit) == 0 {
            continue;
        }
        let cx = g.cx + (*column as f32 - 0.5) * g.w as f32 / 2.0;
        let cy = g.cy + (*row as f32 - 1.5) * g.h as f32 / 4.0;
        push_rect(
            out,
            snap_interval(cx, half, g.w, Anchor::Nearest),
            snap_interval(cy, half, g.h, Anchor::Nearest),
        );
    }
}

// ---------------------------------------------------------------------------
// Families D and H - paths (arcs, diagonals, powerline)
// ---------------------------------------------------------------------------

/// Control point ratio that turns a cubic bezier into a quarter circle.
const KAPPA: f32 = 0.5523;

/// Builds a path in canonical orientation and reflects every point as it is
/// written, so a mirrored code point is the reflection of the definition.
struct Pen<'a> {
    g: &'a Geometry,
    mirror: Mirror,
    ops: Vec<PathOp>,
}

impl<'a> Pen<'a> {
    fn new(g: &'a Geometry, mirror: Mirror) -> Self {
        Self {
            g,
            mirror,
            ops: Vec::new(),
        }
    }

    fn point(&self, x: f32, y: f32) -> DevicePoint {
        DevicePoint {
            x: if self.mirror.x {
                self.g.w as f32 - x
            } else {
                x
            },
            y: if self.mirror.y {
                self.g.h as f32 - y
            } else {
                y
            },
        }
    }

    fn move_to(&mut self, x: f32, y: f32) {
        let point = self.point(x, y);
        self.ops.push(PathOp::Move(point));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let point = self.point(x, y);
        self.ops.push(PathOp::Line(point));
    }

    fn cubic_to(&mut self, c1: (f32, f32), c2: (f32, f32), end: (f32, f32)) {
        let c1 = self.point(c1.0, c1.1);
        let c2 = self.point(c2.0, c2.1);
        let end = self.point(end.0, end.1);
        self.ops.push(PathOp::Cubic(c1, c2, end));
    }

    fn close(&mut self) {
        self.ops.push(PathOp::Close);
    }

    fn fill(self) -> ShapePath {
        ShapePath {
            style: PathStyle::Fill,
            ops: self.ops,
        }
    }

    fn stroke(self) -> ShapePath {
        let width = self.g.thickness.light as f32;
        ShapePath {
            style: PathStyle::Stroke { width },
            ops: self.ops,
        }
    }
}

fn rounded_mirror(code: u32) -> Option<Mirror> {
    Some(match code {
        0x256D => ID,  // light arc down and right
        0x256E => MX,  // light arc down and left
        0x256F => MXY, // light arc up and left
        0x2570 => MY,  // light arc up and right
        _ => return None,
    })
}

/// Radius of a rounded corner: the arc always fits inside the cell.
fn corner_radius(g: &Geometry) -> f32 {
    g.w.min(g.h) as f32 / 2.0
}

fn rounded_corner_path(mirror: Mirror, g: &Geometry) -> ShapePath {
    let r = corner_radius(g);
    let mut pen = Pen::new(g, mirror);
    // Canonical: the arc of U+256D, from the vertical stub up into the
    // horizontal one, centered on the cell axes so both ends meet the joints.
    pen.move_to(g.cx, g.cy + r);
    pen.cubic_to(
        (g.cx, g.cy + r - KAPPA * r),
        (g.cx + r - KAPPA * r, g.cy),
        (g.cx + r, g.cy),
    );
    pen.stroke()
}

/// The straight parts of a rounded corner are quads; only the arc is a path.
fn rounded_corner_stubs(mirror: Mirror, g: &Geometry, out: &mut Vec<DeviceRect>) {
    let r = corner_radius(g);
    let joint_x = g.joint(Axis::X, g.thickness.light);
    let joint_y = g.joint(Axis::Y, g.thickness.light);
    let vertical = if mirror.y {
        Interval {
            lo: 0,
            hi: g.snap_edge(Axis::Y, g.cy - r),
        }
    } else {
        Interval {
            lo: g.snap_edge(Axis::Y, g.cy + r),
            hi: g.h,
        }
    };
    let horizontal = if mirror.x {
        Interval {
            lo: 0,
            hi: g.snap_edge(Axis::X, g.cx - r),
        }
    } else {
        Interval {
            lo: g.snap_edge(Axis::X, g.cx + r),
            hi: g.w,
        }
    };
    push_rect(out, joint_x, vertical);
    push_rect(out, horizontal, joint_y);
}

/// Canonical diagonal: U+2572, upper left to lower right.
fn diagonal_path(mirror: Mirror, g: &Geometry) -> ShapePath {
    let mut pen = Pen::new(g, mirror);
    pen.move_to(0.0, 0.0);
    pen.line_to(g.w as f32, g.h as f32);
    pen.stroke()
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Powerline {
    /// Filled triangle pointing at the cell edge, apex on the center line.
    Triangle,
    /// Outline of the same triangle, without the vertical edge.
    Chevron,
    /// Filled half disc.
    HalfDisc,
    /// Outline of the same half disc.
    HalfDiscOutline,
    /// Filled triangle over one half of the cell, split by a diagonal.
    Corner,
    /// The diagonal of that triangle.
    CornerOutline,
}

/// All sixteen powerline code points have real geometry: `(shape, reflection)`.
#[rustfmt::skip]
const POWERLINE: [(Powerline, Mirror); 16] = [
    (Powerline::Triangle, ID),         // E0B0 right-pointing triangle
    (Powerline::Chevron, ID),          // E0B1
    (Powerline::Triangle, MX),         // E0B2
    (Powerline::Chevron, MX),          // E0B3
    (Powerline::HalfDisc, ID),         // E0B4
    (Powerline::HalfDiscOutline, ID),  // E0B5
    (Powerline::HalfDisc, MX),         // E0B6
    (Powerline::HalfDiscOutline, MX),  // E0B7
    (Powerline::Corner, ID),           // E0B8 lower left triangle
    (Powerline::CornerOutline, ID),    // E0B9
    (Powerline::Corner, MX),           // E0BA
    (Powerline::CornerOutline, MX),    // E0BB
    (Powerline::Corner, MY),           // E0BC
    (Powerline::CornerOutline, MY),    // E0BD
    (Powerline::Corner, MXY),          // E0BE
    (Powerline::CornerOutline, MXY),   // E0BF
];

fn powerline_path(code: u32, g: &Geometry) -> Option<ShapePath> {
    let (shape, mirror) = *POWERLINE.get((code.checked_sub(0xE0B0)?) as usize)?;
    let w = g.w as f32;
    let h = g.h as f32;
    let mut pen = Pen::new(g, mirror);
    Some(match shape {
        Powerline::Triangle | Powerline::Chevron => {
            pen.move_to(0.0, 0.0);
            pen.line_to(w, g.cy);
            pen.line_to(0.0, h);
            if shape == Powerline::Triangle {
                pen.close();
                pen.fill()
            } else {
                pen.stroke()
            }
        }
        Powerline::HalfDisc | Powerline::HalfDiscOutline => {
            pen.move_to(0.0, 0.0);
            pen.cubic_to((KAPPA * w, 0.0), (w, g.cy - KAPPA * g.cy), (w, g.cy));
            pen.cubic_to((w, g.cy + KAPPA * (h - g.cy)), (KAPPA * w, h), (0.0, h));
            if shape == Powerline::HalfDisc {
                pen.close();
                pen.fill()
            } else {
                pen.stroke()
            }
        }
        Powerline::Corner => {
            pen.move_to(0.0, 0.0);
            pen.line_to(w, h);
            pen.line_to(0.0, h);
            pen.close();
            pen.fill()
        }
        Powerline::CornerOutline => {
            pen.move_to(0.0, 0.0);
            pen.line_to(w, h);
            pen.stroke()
        }
    })
}

// ---------------------------------------------------------------------------
// Entry points
// ---------------------------------------------------------------------------

/// True for every code point this module draws. Row planning skips the font for
/// them, including U+2800, which draws nothing but must not fall back.
pub(crate) fn is_shape_char(c: char) -> bool {
    matches!(
        c as u32,
        0x2500..=0x257F | 0x2580..=0x259F | 0x25AC | 0x2800..=0x28FF | 0xE0B0..=0xE0BF
    )
}

/// Append the quads of `c`. Returns whether anything was appended: `false` for a
/// code point this module does not draw, and for the few shape characters made
/// only of paths.
///
/// Rects are appended in a deterministic order (arms up, down, left, right;
/// rails minus then plus) so that row plans are reproducible.
pub(crate) fn shape_quads(c: char, cell: CellSizeDevicePx, out: &mut Vec<DeviceRect>) -> bool {
    if !is_shape_char(c) {
        return false;
    }
    let g = Geometry::new(cell);
    let start = out.len();
    let code = c as u32;
    match code {
        0x2500..=0x257F => {
            if let Some(arms) = box_arms(c) {
                build_arms(&arms, &g, out);
            } else if let Some((count, weight, axis)) = dash_def(c) {
                build_dashes(axis, count, weight, &g, out);
            } else if let Some(mirror) = rounded_mirror(code) {
                rounded_corner_stubs(mirror, &g, out);
            }
        }
        0x2591 => build_shade(Shade::Light, &g, out),
        0x2592 => build_shade(Shade::Medium, &g, out),
        0x2593 => build_shade(Shade::Dark, &g, out),
        0x2580..=0x259F | 0x25AC => build_block(code, &g, out),
        0x2800..=0x28FF => build_braille(c, &g, out),
        _ => {}
    }
    out.len() > start
}

/// Append the paths of `c` (rounded corners, diagonals, powerline). Returns
/// whether anything was appended.
pub(crate) fn shape_paths(c: char, cell: CellSizeDevicePx, out: &mut Vec<ShapePath>) -> bool {
    if !is_shape_char(c) {
        return false;
    }
    let g = Geometry::new(cell);
    let code = c as u32;
    match code {
        0x256D..=0x2570 => {
            let Some(mirror) = rounded_mirror(code) else {
                return false;
            };
            out.push(rounded_corner_path(mirror, &g));
        }
        // U+2571 is the reflection of U+2572; U+2573 is both diagonals.
        0x2571 => out.push(diagonal_path(MX, &g)),
        0x2572 => out.push(diagonal_path(ID, &g)),
        0x2573 => {
            out.push(diagonal_path(ID, &g));
            out.push(diagonal_path(MX, &g));
        }
        0xE0B0..=0xE0BF => {
            let Some(path) = powerline_path(code, &g) else {
                return false;
            };
            out.push(path);
        }
        _ => return false,
    }
    true
}
