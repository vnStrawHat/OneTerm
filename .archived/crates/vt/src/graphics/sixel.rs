//! The Sixel decoder: DEC STD 070 subset, streamed byte by byte.
//!
//! Design:
//! <https://github.com/vnStrawHat/OneTerm/blob/main/docs/spec-intakes/IN-0029-vt-engine/low-level-design/graphics.md>
//!
//! The behaviour is pinned by this module's own tests, and it includes two
//! places it deliberately departs from a strict VT340 —
//! **`P2` background-select is ignored and untouched pixels stay fully
//! transparent**, and an out-of-range colour register only selects.
//!
//! The decoder never fails and never panics: a malformed byte is a byte the
//! grammar has no rule for, which is dropped. A corrupt band therefore produces
//! wrong pixels, which is the documented degradation when a host loses bytes
//! mid-image.

use super::{MAX_DIMENSION, MAX_PIXEL_BYTES};

/// A finished image, before it is given an id and placed.
pub(crate) struct DecodedSixel {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) rgba: Vec<u8>,
    /// Pixel height of the bands **above** the last one (`bands * 6`).
    ///
    /// The text cursor moves down `band_pixels / cell_height` rows — the row
    /// holding the top of the last sixel band, as DEC terminals and conhost do.
    /// The division happens in [`place`](super::placement::place), against the
    /// same cell size the footprint uses; at
    /// [`VIRTUAL_CELL`](super::VIRTUAL_CELL) it is the classic `bands * 6 / 20`.
    ///
    /// The cursor therefore lands on the image's **last row**, not past it —
    /// but only while the raster attributes agree with the data. `"Pan;Pad;Ph;Pv`
    /// overrides the measured extents (see [`SixelParser::finish`]), and the
    /// cursor keeps following the bands, so a declaration larger than the data
    /// leaves the cursor high inside the image and one smaller than the data
    /// walks it below. Pre-existing and unchanged by the real-cell footprint.
    pub(crate) band_pixels: u32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Ground,
    /// `!` followed by a repeat count.
    Repeat,
    /// `#` followed by a register and an optional colour definition.
    Color,
    /// `"` followed by raster attributes.
    Raster,
}

/// Streaming decoder for one `DCS q` payload.
///
/// The DCS parameters `P1;P2;P3` are parsed by the parser and used by nothing:
/// aspect ratio is cosmetic, `P2` is deliberately ignored (see the module doc)
/// and `P3` is a grid-select this engine has no registers for.
pub(crate) struct SixelParser {
    palette: [[u8; 4]; 256],
    color: usize,
    x: u32,
    band: u32,
    /// Extents of the data received so far, clamped to [`MAX_DIMENSION`].
    width: u32,
    height: u32,
    /// `"Pan;Pad;Ph;Pv` size, when the stream declared one.
    raster: Option<(u32, u32)>,
    state: State,
    args: Vec<u32>,
    /// Pixels received so far: `stride` wide, `pixels.len() / stride` tall.
    pixels: Vec<[u8; 4]>,
    stride: u32,
}

impl SixelParser {
    pub(crate) fn new() -> SixelParser {
        SixelParser {
            palette: default_palette(),
            color: 0,
            x: 0,
            band: 0,
            width: 0,
            height: 0,
            raster: None,
            state: State::Ground,
            args: Vec::with_capacity(5),
            pixels: Vec::new(),
            stride: 0,
        }
    }

    pub(crate) fn put(&mut self, byte: u8) {
        match self.state {
            State::Ground => self.ground(byte),
            State::Repeat | State::Color | State::Raster => self.argument(byte),
        }
    }

    /// A byte inside `!`, `#` or `"`: another digit, another parameter, or the
    /// byte that ends the command and is then re-read in the ground state.
    fn argument(&mut self, byte: u8) {
        match byte {
            b'0'..=b'9' => {
                // `begin` always pushes one slot, and nothing pops.
                if let Some(last) = self.args.last_mut() {
                    *last = last
                        .saturating_mul(10)
                        .saturating_add(u32::from(byte - b'0'));
                }
            }
            b';' => self.args.push(0),
            _ => {
                let state = self.state;
                self.state = State::Ground;
                match state {
                    State::Repeat => {
                        let count = self.args.first().copied().unwrap_or(0).max(1);
                        if (0x3F..=0x7E).contains(&byte) {
                            self.sixel(byte - 0x3F, count);
                        } else {
                            self.ground(byte);
                        }
                    }
                    State::Color => {
                        self.color_command();
                        self.ground(byte);
                    }
                    State::Raster => {
                        self.raster_command();
                        self.ground(byte);
                    }
                    State::Ground => {}
                }
            }
        }
    }

    fn ground(&mut self, byte: u8) {
        match byte {
            0x3F..=0x7E => self.sixel(byte - 0x3F, 1),
            b'!' => self.begin(State::Repeat),
            b'#' => self.begin(State::Color),
            b'"' => self.begin(State::Raster),
            // Graphics carriage return.
            b'$' => self.x = 0,
            // Graphics newline.
            b'-' => {
                self.x = 0;
                self.band = self.band.saturating_add(1);
            }
            _ => {}
        }
    }

    fn begin(&mut self, state: State) {
        self.state = state;
        self.args.clear();
        self.args.push(0);
    }

    /// `#Pr` selects; `#Pr;Pu;Px;Py;Pz` defines and then selects. `Pu == 2` is
    /// RGB in **percent 0-100**, `Pu == 1` is DEC HLS with hue 0 = blue; any
    /// other mode only selects.
    fn color_command(&mut self) {
        let register = self.args.first().copied().unwrap_or(0) as usize & 0xFF;
        if self.args.len() >= 5 {
            let (mode, a, b, c) = (self.args[1], self.args[2], self.args[3], self.args[4]);
            let rgb = match mode {
                2 => [percent(a), percent(b), percent(c)],
                1 => hls_to_rgb(a, b, c),
                _ => {
                    self.color = register;
                    return;
                }
            };
            self.palette[register] = [rgb[0], rgb[1], rgb[2], 255];
        }
        self.color = register;
    }

    /// `"Pan;Pad;Ph;Pv`: the declared size wins over the measured extents; the
    /// aspect ratio is parsed and ignored.
    fn raster_command(&mut self) {
        if self.args.len() >= 4 {
            let width = self.args[2].min(MAX_DIMENSION);
            let height = self.args[3].min(MAX_DIMENSION);
            if width > 0 && height > 0 {
                self.raster = Some((width, height));
                self.ensure_stride(width);
            }
        }
    }

    /// Paint one sixel column (`bits` = rows `y..y + 6`, least significant bit
    /// at the top) `count` times, then advance.
    fn sixel(&mut self, bits: u8, count: u32) {
        let y0 = self.band.saturating_mul(6);
        if y0 >= MAX_DIMENSION {
            return;
        }
        let x_end = self.x.saturating_add(count).min(MAX_DIMENSION);
        let y_end = y0.saturating_add(6).min(MAX_DIMENSION);
        if bits != 0 && x_end > self.x {
            self.ensure_size(x_end, y_end);
            let color = self.palette[self.color];
            for bit in 0..6u32 {
                let y = y0 + bit;
                if bits & (1 << bit) == 0 || y >= y_end {
                    continue;
                }
                let row = (y * self.stride) as usize;
                for x in self.x..x_end {
                    self.pixels[row + x as usize] = color;
                }
            }
        }
        self.x = x_end;
        self.width = self.width.max(x_end);
        self.height = self.height.max(y_end);
    }

    /// Grow the buffer so `width x height` is addressable. The stride doubles,
    /// so a band that widens one column at a time does not reallocate per byte.
    fn ensure_size(&mut self, width: u32, height: u32) {
        self.ensure_stride(width);
        let needed = (height * self.stride) as usize;
        if self.pixels.len() < needed {
            self.pixels.resize(needed, [0; 4]);
        }
        // Both axes are clamped to MAX_DIMENSION above, so this is the whole
        // pixel budget the design allows a single image (`graphics.md`: the
        // decoder refuses to grow past 4096 * 4096 * 4). It holds by
        // construction rather than by a check, and the assertion says so.
        debug_assert!(self.pixels.len() * 4 <= MAX_PIXEL_BYTES, "pixel budget");
    }

    fn ensure_stride(&mut self, width: u32) {
        if width <= self.stride {
            return;
        }
        // `MAX_DIMENSION` is far above the 64-column floor, so the clamp cannot
        // be inverted.
        let new_stride = width
            .max(self.stride.saturating_mul(2))
            .clamp(64, MAX_DIMENSION);
        if self.stride == 0 {
            self.stride = new_stride;
            return;
        }
        let rows = self.pixels.len() / self.stride as usize;
        let mut pixels = vec![[0u8; 4]; rows * new_stride as usize];
        for row in 0..rows {
            let src = row * self.stride as usize;
            let dst = row * new_stride as usize;
            pixels[dst..dst + self.stride as usize]
                .copy_from_slice(&self.pixels[src..src + self.stride as usize]);
        }
        self.pixels = pixels;
        self.stride = new_stride;
    }

    /// The finished image; `None` when nothing was drawn.
    pub(crate) fn finish(self) -> Option<DecodedSixel> {
        let (width, height) = self.raster.unwrap_or((self.width, self.height));
        if width == 0 || height == 0 {
            return None;
        }
        let band_pixels = self.band.saturating_mul(6);
        let mut rgba = vec![0u8; (width as usize) * (height as usize) * 4];
        let buffered_rows = if self.stride == 0 {
            0
        } else {
            self.pixels.len() / self.stride as usize
        };
        let copy_w = width.min(self.stride) as usize;
        for y in 0..(height as usize).min(buffered_rows) {
            let src = &self.pixels[y * self.stride as usize..y * self.stride as usize + copy_w];
            let dst = &mut rgba[y * width as usize * 4..(y * width as usize + copy_w) * 4];
            for (pixel, out) in src.iter().zip(dst.chunks_exact_mut(4)) {
                out.copy_from_slice(pixel);
            }
        }
        Some(DecodedSixel {
            width,
            height,
            rgba,
            band_pixels,
        })
    }
}

fn percent(value: u32) -> u8 {
    (value.min(100) * 255 / 100) as u8
}

/// DEC HLS: hue 0 is blue, lightness and saturation are percentages.
fn hls_to_rgb(hue: u32, lightness: u32, saturation: u32) -> [u8; 3] {
    let h = ((hue + 240) % 360) as f32;
    let l = lightness.min(100) as f32 / 100.0;
    let s = saturation.min(100) as f32 / 100.0;
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let to_u8 = |v: f32| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    [to_u8(r), to_u8(g), to_u8(b)]
}

/// The VT340 default colour registers (percentages); 16..255 are opaque black.
fn default_palette() -> [[u8; 4]; 256] {
    const VT340: [[u32; 3]; 16] = [
        [0, 0, 0],
        [20, 20, 80],
        [80, 13, 13],
        [20, 80, 20],
        [80, 20, 80],
        [20, 80, 80],
        [80, 80, 20],
        [53, 53, 53],
        [26, 26, 26],
        [33, 33, 60],
        [60, 26, 26],
        [33, 60, 33],
        [60, 33, 60],
        [33, 60, 60],
        [60, 60, 33],
        [80, 80, 80],
    ];
    let mut palette = [[0, 0, 0, 255]; 256];
    for (slot, rgb) in palette.iter_mut().zip(VT340.iter()) {
        *slot = [percent(rgb[0]), percent(rgb[1]), percent(rgb[2]), 255];
    }
    palette
}
