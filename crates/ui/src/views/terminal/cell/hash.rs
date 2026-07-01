//! FNV-1a hash of one display line — detects content changes that
//! Term::damage() doesn't track.

use alacritty_terminal::vte::ansi::Color;

use oneterm_core::terminal::IndexedCell;

/// Hash includes: char, fg, bg, flags, zerowidth, hyperlink.
pub(crate) fn line_hash(cells: &[&IndexedCell]) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x100_0000_01b3;
    let mut h = FNV_OFFSET;
    for ic in cells {
        let cell = &ic.cell;
        h ^= cell.c as u64;
        h = h.wrapping_mul(FNV_PRIME);
        h ^= color_hash(cell.fg);
        h = h.wrapping_mul(FNV_PRIME);
        h ^= color_hash(cell.bg);
        h = h.wrapping_mul(FNV_PRIME);
        h ^= cell.flags.bits() as u64;
        h = h.wrapping_mul(FNV_PRIME);
        if let Some(zw) = cell.zerowidth() {
            for &c in zw {
                h ^= c as u64;
                h = h.wrapping_mul(FNV_PRIME);
            }
        }
        if let Some(hl) = cell.hyperlink() {
            for b in hl.uri().bytes() {
                h ^= b as u64;
                h = h.wrapping_mul(FNV_PRIME);
            }
        }
    }
    h
}

fn color_hash(c: Color) -> u64 {
    match c {
        Color::Named(n) => n as u64,
        Color::Spec(rgb) => {
            0x1_0000 | (rgb.r as u64) | ((rgb.g as u64) << 8) | ((rgb.b as u64) << 16)
        }
        Color::Indexed(i) => 0x2_0000 | i as u64,
    }
}
