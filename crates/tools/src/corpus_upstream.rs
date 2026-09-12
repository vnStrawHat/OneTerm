//! Convert upstream alacritty's `grid.json` into the OneTerm `grid.expect`
//! form, so the expectation format can be proved to lose nothing.
//!
//! Run once, at `US-0072` (`vt-corpus cross-check --grid-json <dir>`); after
//! that the 44 MiB of JSON is never needed again and is not committed.
//!
//! The conversion deliberately reads only the fields upstream's own comparison
//! reads (`research/engine-semantics.md` § 7.3):
//!
//! * `Grid::eq` → `raw`, `columns`, `lines`, `display_offset`. `max_scroll_limit`
//!   is **not** compared.
//! * `Storage::eq` → asserts `zero == 0` on both sides and compares `inner` and
//!   `len`. `visible_lines` is **not** compared (trap 45).
//! * `Row::eq` → `inner` only. `occ` is **not** compared.
//! * Cursors are `#[serde(skip)]` and so are absent from the file entirely
//!   (trap 44) — which is exactly why `state.expect` exists.

use std::fmt::Write as _;
use std::path::Path;

use alacritty_terminal::term::cell::Flags;
use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::corpus::{GridExpect, RowExpect};
use crate::corpus_replay::{LinkIds, encode_flags, percent_encode};

#[derive(Deserialize)]
struct UpstreamGrid {
    raw: UpstreamStorage,
    columns: usize,
    lines: usize,
    display_offset: usize,
}

#[derive(Deserialize)]
struct UpstreamStorage {
    inner: Vec<UpstreamRow>,
    zero: usize,
    len: usize,
}

#[derive(Deserialize)]
struct UpstreamRow {
    inner: Vec<UpstreamCell>,
}

#[derive(Deserialize)]
struct UpstreamCell {
    c: char,
    fg: JsonColor,
    bg: JsonColor,
    flags: String,
    #[serde(default)]
    extra: Option<UpstreamExtra>,
}

#[derive(Deserialize)]
struct UpstreamExtra {
    #[serde(default)]
    zerowidth: Vec<char>,
    #[serde(default)]
    underline_color: Option<JsonColor>,
    #[serde(default)]
    hyperlink: Option<JsonHyperlink>,
}

#[derive(Deserialize)]
struct JsonHyperlink {
    inner: JsonHyperlinkInner,
}

#[derive(Deserialize)]
struct JsonHyperlinkInner {
    id: String,
    uri: String,
}

#[derive(Deserialize)]
enum JsonColor {
    Named(String),
    Spec { r: u8, g: u8, b: u8 },
    Indexed(u8),
}

impl JsonColor {
    fn encode(&self) -> String {
        match self {
            // `Color::Named` is encoded by its variant name on both sides, so
            // the two encoders agree without a name-to-index table.
            Self::Named(name) => format!("n{name}"),
            Self::Indexed(index) => format!("i{index}"),
            Self::Spec { r, g, b } => format!("#{r:02x}{g:02x}{b:02x}"),
        }
    }
}

/// Read `<dir>/<name>.json` and convert it to the expectation form.
pub fn load_grid_json(dir: &Path, name: &str) -> Result<GridExpect> {
    let path = dir.join(format!("{name}.json"));
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let grid: UpstreamGrid =
        serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;

    if grid.raw.zero != 0 {
        // Upstream's own `Storage::PartialEq` asserts this; a non-zero `zero`
        // means the file was serialized without `truncate()` and its row order
        // is rotated (trap 45).
        bail!(
            "{}: raw.zero is {}, expected 0",
            path.display(),
            grid.raw.zero
        );
    }
    if grid.raw.inner.len() < grid.raw.len {
        bail!(
            "{}: raw.len is {} but only {} rows are present",
            path.display(),
            grid.raw.len,
            grid.raw.inner.len()
        );
    }

    let mut links = LinkIds::default();
    let rows = grid.raw.inner[..grid.raw.len]
        .iter()
        .map(|row| convert_row(row, &path, &mut links))
        .collect::<Result<Vec<_>>>()?;

    Ok(GridExpect {
        columns: grid.columns,
        lines: grid.lines,
        display_offset: grid.display_offset,
        rows,
    })
}

fn convert_row(row: &UpstreamRow, path: &Path, links: &mut LinkIds) -> Result<RowExpect> {
    let cells = row
        .inner
        .iter()
        .map(|cell| convert_cell(cell, path, links))
        .collect::<Result<Vec<_>>>()?;
    let wrap = row
        .inner
        .last()
        .map(|cell| parse_flags(&cell.flags, path))
        .transpose()?
        .is_some_and(|flags| flags & Flags::WRAPLINE.bits() != 0);
    Ok(RowExpect { wrap, cells })
}

fn convert_cell(cell: &UpstreamCell, path: &Path, links: &mut LinkIds) -> Result<String> {
    let mut content = format!("{:04x}", cell.c as u32);
    let empty = UpstreamExtra {
        zerowidth: Vec::new(),
        underline_color: None,
        hyperlink: None,
    };
    let extra = cell.extra.as_ref().unwrap_or(&empty);
    for zero_width in &extra.zerowidth {
        let _ = write!(content, "+{:04x}", *zero_width as u32);
    }

    let attrs = encode_flags(parse_flags(&cell.flags, path)?);
    let fg = cell.fg.encode();
    let bg = cell.bg.encode();
    let underline = extra
        .underline_color
        .as_ref()
        .map_or_else(|| "-".to_owned(), JsonColor::encode);
    let hyperlink = extra.hyperlink.as_ref().map_or_else(
        || "-".to_owned(),
        |link| {
            format!(
                "{}~{}",
                links.normalize(&link.inner.id),
                percent_encode(&link.inner.uri)
            )
        },
    );

    Ok(format!(
        "{content};{attrs};{fg};{bg};{underline};{hyperlink}"
    ))
}

/// `bitflags` serializes to `"NAME | NAME"`. `Flags::from_name` is used rather
/// than a hand-written table so the composite aliases (`BOLD_ITALIC`,
/// `DIM_BOLD`, `ALL_UNDERLINES`) resolve to their full bit patterns.
fn parse_flags(text: &str, path: &Path) -> Result<u16> {
    let mut bits = 0;
    for name in text
        .split('|')
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        let flag = Flags::from_name(name)
            .with_context(|| format!("{}: unknown cell flag {name:?}", path.display()))?;
        bits |= flag.bits();
    }
    Ok(bits)
}
