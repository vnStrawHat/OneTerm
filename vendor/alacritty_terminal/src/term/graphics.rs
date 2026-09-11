//! OneTerm fork: Sixel graphics decoded inside the terminal and anchored to grid cells.
//!
//! A `DCS q` sequence is decoded by [`SixelParser`] as its bytes arrive; on `ST` the
//! terminal writes a [`GraphicCell`] reference into every cell the image covers and moves
//! the cursor below it. The pixels wait in [`Graphics::pending`] until the embedder takes
//! them with `Term::take_graphics`. See
//! docs/spec-intakes/IN-0028-sixel-graphics/low-level-design/vendor-graphics.md.

use std::sync::Arc;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::vte::Params;

/// Largest width or height a Sixel image may have; pixels beyond it are dropped.
pub const MAX_DIMENSION: u32 = 4096;

/// The virtual cell size Sixel pixels are measured in (VT240/VT340: 10 x 20), the
/// same value Windows conhost uses, so the cursor rows an image consumes agree
/// with a ConPTY host. The renderer scales the image to the real cell size.
pub const VIRTUAL_CELL: (u32, u32) = (10, 20);

/// A finished Sixel image.
pub(crate) struct DecodedSixel {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    /// Rows the text cursor moves down: the row that holds the top of the last
    /// sixel band (`bands_advanced * 6 / 20`), as DEC terminals and conhost do.
    pub cursor_rows: u32,
}

/// Identifier of a decoded image, unique per `Term`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct GraphicId(pub u64);

/// A cell's share of an image: `(col, row)` is the cell's offset inside the image grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct GraphicCell {
    pub id: GraphicId,
    pub col: u16,
    pub row: u16,
}

/// Decoded image pixels, RGBA row-major, straight (not premultiplied) alpha.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphicData {
    pub id: GraphicId,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Per-terminal graphics state.
pub(crate) struct Graphics {
    next_id: u64,
    /// Images decoded since the embedder last took them.
    pub(crate) pending: Vec<Arc<GraphicData>>,
    /// The Sixel sequence currently being received, if any.
    pub(crate) parser: Option<SixelParser>,
}

impl Default for Graphics {
    fn default() -> Self {
        Self { next_id: 1, pending: Vec::new(), parser: None }
    }
}

impl Graphics {
    pub(crate) fn next_id(&mut self) -> GraphicId {
        let id = GraphicId(self.next_id);
        self.next_id += 1;
        id
    }
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

/// Streaming Sixel decoder (DEC STD 070 subset: raster attributes, colour registers in
/// RGB and HLS, repeat, `$`, `-`). Untouched pixels stay transparent whatever P2 says.
pub(crate) struct SixelParser {
    palette: [[u8; 4]; 256],
    color: usize,
    x: u32,
    band: u32,
    /// Extents of the data received so far (clamped to `MAX_DIMENSION`).
    width: u32,
    height: u32,
    /// `"Pan;Pad;Ph;Pv` size, when given.
    raster: Option<(u32, u32)>,
    state: State,
    args: Vec<u32>,
    /// Pixels of the image so far, `stride` wide, `height` tall.
    pixels: Vec<[u8; 4]>,
    stride: u32,
}

impl SixelParser {
    /// `params` are the DCS parameters `P1;P2;P3`; only their presence matters today.
    pub(crate) fn new(_params: &Params) -> Self {
        Self {
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
            State::Repeat | State::Color | State::Raster => {
                match byte {
                    b'0'..=b'9' => {
                        let last = self.args.last_mut().expect("args never empty in a command");
                        *last = last.saturating_mul(10).saturating_add(u32::from(byte - b'0'));
                    },
                    b';' => self.args.push(0),
                    _ => {
                        let state = self.state;
                        self.state = State::Ground;
                        match state {
                            State::Repeat => {
                                let count = self.args[0].max(1);
                                if (0x3F..=0x7E).contains(&byte) {
                                    self.sixel(byte - 0x3F, count);
                                } else {
                                    self.ground(byte);
                                }
                            },
                            State::Color => {
                                self.color_command();
                                self.ground(byte);
                            },
                            State::Raster => {
                                self.raster_command();
                                self.ground(byte);
                            },
                            State::Ground => unreachable!(),
                        }
                    },
                }
            },
        }
    }

    fn ground(&mut self, byte: u8) {
        match byte {
            0x3F..=0x7E => self.sixel(byte - 0x3F, 1),
            b'!' => self.begin(State::Repeat),
            b'#' => self.begin(State::Color),
            b'"' => self.begin(State::Raster),
            b'$' => self.x = 0,
            b'-' => {
                self.x = 0;
                self.band = self.band.saturating_add(1);
            },
            _ => {},
        }
    }

    fn begin(&mut self, state: State) {
        self.state = state;
        self.args.clear();
        self.args.push(0);
    }

    fn color_command(&mut self) {
        let register = self.args[0] as usize & 0xFF;
        if self.args.len() >= 5 {
            let (mode, a, b, c) = (self.args[1], self.args[2], self.args[3], self.args[4]);
            let rgb = match mode {
                2 => [percent(a), percent(b), percent(c)],
                1 => hls_to_rgb(a, b, c),
                _ => return,
            };
            self.palette[register] = [rgb[0], rgb[1], rgb[2], 255];
        }
        self.color = register;
    }

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

    /// Paint one sixel column (`bits` = rows `y..y+6`) `count` times, then advance.
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

    /// Grow the buffer so `width x height` is addressable; the stride doubles so a
    /// band that widens one column at a time does not reallocate per byte.
    fn ensure_size(&mut self, width: u32, height: u32) {
        self.ensure_stride(width);
        let needed = (height * self.stride) as usize;
        if self.pixels.len() < needed {
            self.pixels.resize(needed, [0; 4]);
        }
    }

    fn ensure_stride(&mut self, width: u32) {
        if width <= self.stride {
            return;
        }
        let new_stride = width.max(self.stride.saturating_mul(2)).max(64).min(MAX_DIMENSION);
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
        let cursor_rows = self.band.saturating_mul(6) / VIRTUAL_CELL.1;
        let mut rgba = vec![0u8; (width * height * 4) as usize];
        let buffered_rows = if self.stride == 0 { 0 } else { self.pixels.len() / self.stride as usize };
        let copy_w = width.min(self.stride) as usize;
        for y in 0..(height as usize).min(buffered_rows) {
            let src = &self.pixels[y * self.stride as usize..y * self.stride as usize + copy_w];
            let dst = &mut rgba[y * width as usize * 4..(y * width as usize + copy_w) * 4];
            for (px, out) in src.iter().zip(dst.chunks_exact_mut(4)) {
                out.copy_from_slice(px);
            }
        }
        Some(DecodedSixel { width, height, rgba, cursor_rows })
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

/// The VT340 default colour registers (percentages), registers 16.. black.
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
