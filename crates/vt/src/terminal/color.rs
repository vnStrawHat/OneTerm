//! The OSC colour-override table and its typed keys.
//!
//! Design: `docs/spec-intakes/IN-0029-vt-engine/low-level-design/dispatch-and-modes.md`
//! section "Colour model".
//!
//! [`ColorKey`] replaces the magic indices 256 / 257 / 258 that appear in three
//! files today (deviation D1). The *storage* is still the reference's 269-slot
//! index space, because that is the space the frozen parity expectations are
//! written in and a translation table would buy nothing.

use crate::cell::Rgb;

/// Slots in the override table: 256 indexed colours, then foreground,
/// background, cursor, the eight dim variants, bright foreground and dim
/// foreground.
pub(crate) const COLOR_COUNT: usize = 269;

const FOREGROUND: usize = 256;
const BACKGROUND: usize = 257;
const CURSOR: usize = 258;
const DIM_BASE: usize = 259;
const BRIGHT_FOREGROUND: usize = 267;
const DIM_FOREGROUND: usize = 268;

/// What an OSC colour sequence names.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ColorKey {
    Palette(u8),
    Foreground,
    Background,
    Cursor,
    BrightForeground,
    DimForeground,
    /// One of the eight dim ANSI colours, `0..8`.
    Dim(u8),
}

impl ColorKey {
    pub const fn index(self) -> usize {
        match self {
            ColorKey::Palette(index) => index as usize,
            ColorKey::Foreground => FOREGROUND,
            ColorKey::Background => BACKGROUND,
            ColorKey::Cursor => CURSOR,
            ColorKey::BrightForeground => BRIGHT_FOREGROUND,
            ColorKey::DimForeground => DIM_FOREGROUND,
            ColorKey::Dim(index) => DIM_BASE + (index as usize & 7),
        }
    }

    /// `None` for an index outside the table.
    pub const fn from_index(index: usize) -> Option<ColorKey> {
        Some(match index {
            0..=255 => ColorKey::Palette(index as u8),
            FOREGROUND => ColorKey::Foreground,
            BACKGROUND => ColorKey::Background,
            CURSOR => ColorKey::Cursor,
            DIM_BASE..=266 => ColorKey::Dim((index - DIM_BASE) as u8),
            BRIGHT_FOREGROUND => ColorKey::BrightForeground,
            DIM_FOREGROUND => ColorKey::DimForeground,
            _ => return None,
        })
    }

    /// The number an OSC query echoes back: `4;{index}` for a palette entry,
    /// `10` / `11` / `12` for the three dynamic colours.
    pub fn query_prefix(self) -> String {
        match self {
            ColorKey::Palette(index) => format!("4;{index}"),
            ColorKey::Foreground => "10".to_owned(),
            ColorKey::Background => "11".to_owned(),
            ColorKey::Cursor => "12".to_owned(),
            // Not reachable from any sequence the engine accepts; the index is
            // the honest answer if one ever becomes queryable.
            other => other.index().to_string(),
        }
    }
}

/// The OSC-override layer only. `None` means "use the theme", exactly as the
/// 269-slot table works today.
#[derive(Clone, Debug)]
pub struct ColorOverrides {
    slots: Box<[Option<Rgb>; COLOR_COUNT]>,
}

impl Default for ColorOverrides {
    fn default() -> ColorOverrides {
        ColorOverrides {
            slots: Box::new([None; COLOR_COUNT]),
        }
    }
}

impl ColorOverrides {
    pub fn get(&self, key: ColorKey) -> Option<Rgb> {
        self.slots[key.index()]
    }

    /// Returns whether anything visible changed, which is what decides a full
    /// damage stamp: a cursor colour that did not move is free.
    pub fn set(&mut self, key: ColorKey, color: Rgb) -> bool {
        let slot = &mut self.slots[key.index()];
        let changed = *slot != Some(color);
        *slot = Some(color);
        changed && key != ColorKey::Cursor
    }

    pub fn reset(&mut self, key: ColorKey) -> bool {
        let slot = &mut self.slots[key.index()];
        let changed = slot.is_some();
        *slot = None;
        changed && key != ColorKey::Cursor
    }

    /// `OSC 104` with no parameter: indices 0..=255 only, and **not**
    /// foreground / background / cursor (trap 26).
    pub fn reset_indexed(&mut self) -> bool {
        let mut changed = false;
        for slot in self.slots[..256].iter_mut() {
            changed |= slot.is_some();
            *slot = None;
        }
        changed
    }

    /// `RIS`, correction C6: the reference leaves the overrides in place
    /// (trap 39).
    pub fn reset_all(&mut self) -> bool {
        let changed = self.slots.iter().any(Option::is_some);
        self.slots.fill(None);
        changed
    }

    /// Every override, by slot index. The parity snapshot's `palette.{index}`
    /// keys, and the only reason the index space is still visible.
    pub fn iter(&self) -> impl Iterator<Item = (usize, Rgb)> + '_ {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| slot.map(|rgb| (index, rgb)))
    }
}

/// `#rrggbb` or `rgb:rr/gg/bb`, the two forms `xparse_color` accepts.
pub(crate) fn parse_color(spec: &[u8]) -> Option<Rgb> {
    match spec {
        [b'#', rest @ ..] => parse_legacy_color(rest),
        [b'r', b'g', b'b', b':', rest @ ..] => parse_rgb_color(rest),
        _ => None,
    }
}

/// `rgb:r(rrr)/g(ggg)/b(bbb)`: values are scaled, not zero-filled.
fn parse_rgb_color(spec: &[u8]) -> Option<Rgb> {
    let text = str::from_utf8(spec).ok()?;
    let mut parts = text.split('/');
    let (red, green, blue) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    let scale = |input: &str| -> Option<u8> {
        if input.is_empty() || input.len() > 4 {
            return None;
        }
        let max = 16u32.pow(input.len() as u32) - 1;
        let value = u32::from_str_radix(input, 16).ok()?;
        Some((255 * value / max) as u8)
    };
    Some(Rgb {
        r: scale(red)?,
        g: scale(green)?,
        b: scale(blue)?,
    })
}

/// `#r(rrr)g(ggg)b(bbb)`: truncated or filled to two-byte precision.
fn parse_legacy_color(spec: &[u8]) -> Option<Rgb> {
    let len = spec.len() / 3;
    if len == 0 {
        return None;
    }
    let channel = |slice: &[u8]| -> Option<u8> {
        let value = usize::from_str_radix(str::from_utf8(slice).ok()?, 16).ok()? << 4;
        Some((value >> (4 * slice.len().saturating_sub(1))) as u8)
    };
    Some(Rgb {
        r: channel(&spec[..len])?,
        g: channel(&spec[len..len * 2])?,
        b: channel(&spec[len * 2..])?,
    })
}
