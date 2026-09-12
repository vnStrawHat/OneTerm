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

#[cfg(test)]
mod tests {
    use super::*;

    fn write_grid_json(name: &str, body: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join("oneterm-vt-corpus-upstream-tests");
        std::fs::create_dir_all(&dir).expect("temp dir");
        std::fs::write(dir.join(format!("{name}.json")), body).expect("fixture");
        dir
    }

    /// A 1x1 grid in upstream's serialized shape.
    fn one_cell(zero: usize, len: usize) -> String {
        format!(
            r#"{{"raw":{{"inner":[{{"inner":[{{"c":"A","fg":{{"Named":"Foreground"}},
               "bg":{{"Indexed":3}},"flags":"BOLD | ITALIC","extra":null}}],"occ":1}}],
               "zero":{zero},"visible_lines":1,"len":{len}}},
               "columns":1,"lines":1,"display_offset":0,"max_scroll_limit":0}}"#
        )
    }

    #[test]
    fn a_rotated_ring_is_refused_rather_than_compared() {
        // Upstream's own `Storage::PartialEq` asserts `zero == 0` and panics
        // otherwise (trap 45). A rotated ring would compare rows in the wrong
        // order, so it must be rejected, never silently accepted.
        let dir = write_grid_json("rotated", &one_cell(2, 1));

        let error = load_grid_json(&dir, "rotated").expect_err("a rotated ring is not comparable");

        assert!(error.to_string().contains("raw.zero is 2"), "{error}");
    }

    #[test]
    fn a_len_beyond_the_rows_present_is_refused() {
        let dir = write_grid_json("short", &one_cell(0, 4));

        let error = load_grid_json(&dir, "short").expect_err("len exceeds the rows present");

        assert!(error.to_string().contains("raw.len is 4"), "{error}");
    }

    #[test]
    fn a_well_formed_grid_encodes_to_the_same_cell_token_as_a_replay() {
        let dir = write_grid_json("ok", &one_cell(0, 1));

        let grid = load_grid_json(&dir, "ok").expect("a well-formed grid");

        assert_eq!(grid.columns, 1);
        assert_eq!(grid.rows.len(), 1);
        // Colours are encoded by variant name and index, flags in bit order,
        // exactly as `corpus_replay::encode_cell` writes them.
        assert_eq!(grid.rows[0].cells[0], "0041;BOLD.ITALIC;nForeground;i3;-;-");
        assert!(!grid.rows[0].wrap);
    }

    #[test]
    fn composite_flag_aliases_resolve_to_their_full_bit_pattern() {
        let path = Path::new("grid.json");

        // `bitflags` may serialize `BOLD | ITALIC` as the alias `BOLD_ITALIC`;
        // a hand-written name table would read that as an unknown flag.
        assert_eq!(
            parse_flags("BOLD_ITALIC", path).expect("alias"),
            Flags::BOLD.bits() | Flags::ITALIC.bits()
        );
        assert_eq!(
            parse_flags("DIM_BOLD", path).expect("alias"),
            Flags::DIM.bits() | Flags::BOLD.bits()
        );
        assert_eq!(
            parse_flags("ALL_UNDERLINES", path).expect("alias"),
            Flags::ALL_UNDERLINES.bits()
        );
        assert_eq!(parse_flags("", path).expect("no flags"), 0);
        assert!(parse_flags("NOT_A_FLAG", path).is_err());
    }

    #[test]
    fn a_wrapline_on_the_last_cell_lifts_to_the_row() {
        let body = r#"{"raw":{"inner":[{"inner":[
            {"c":"A","fg":{"Named":"Foreground"},"bg":{"Named":"Background"},"flags":"","extra":null},
            {"c":"B","fg":{"Named":"Foreground"},"bg":{"Named":"Background"},"flags":"WRAPLINE","extra":null}
            ],"occ":2}],"zero":0,"visible_lines":1,"len":1},
            "columns":2,"lines":1,"display_offset":0,"max_scroll_limit":0}"#;
        let dir = write_grid_json("wrapped", body);

        let grid = load_grid_json(&dir, "wrapped").expect("a well-formed grid");

        assert!(grid.rows[0].wrap);
        // Still carried per cell too: nothing is trimmed.
        assert!(grid.rows[0].cells[1].contains("WRAPLINE"));
    }
}
