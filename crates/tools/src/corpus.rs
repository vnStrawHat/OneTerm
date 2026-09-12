//! The VT parity corpus: recordings, frozen expectations, and the comparison
//! that turns them into a gate.
//!
//! A recording is raw PTY bytes plus the geometry it was captured at. Replaying
//! it through the engine being replaced produces two expectation files
//! (`corpus_replay`), which are blessed once at `US-0072` and then frozen:
//!
//! * `grid.expect` — cell-exact, nothing trimmed. Every column of every row,
//!   including trailing blanks, run-length encoded.
//! * `state.expect` — the OneTerm snapshot upstream's harness never checked:
//!   cursor, pending wrap, modes, palette overrides, title, tab stops and the
//!   scroll region (trap 44).
//!
//! Because the new engine is built correctness-first, a recording may
//! legitimately differ. That is declared per recording and **per cell** in an
//! `expected-diffs.json`, never as a skipped recording. The rules in
//! [`check`] are what keep the gate a gate: an undeclared difference fails, and
//! so does a declared window that produces no difference.
//!
//! Design: `docs/spec-intakes/IN-0029-vt-engine/low-level-design/testing-and-bench.md` § 2.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;

/// Expectation-file format version. Bumping it invalidates every frozen file,
/// so it only moves when the encoding itself changes.
pub const FORMAT_VERSION: u32 = 1;

/// The per-cell fields an expected difference may name, in encoding order.
pub const CELL_FIELDS: [&str; 6] = ["content", "attrs", "fg", "bg", "underline", "hyperlink"];

/// Every deviation and correction id the two low-level designs define.
///
/// `C*` rows are corrections (a defect in the engine being replaced, fixed
/// rather than reproduced); `D*` and `G*` rows are the remaining deliberate
/// deviations. An `expected-diffs.json` naming anything else is a typo, and a
/// typo that silently disabled part of the gate would be the worst outcome
/// this harness can have.
pub const KNOWN_DEVIATIONS: [&str; 31] = [
    // C12 is `US-0075`'s wide-pair repair, C13 and C14 are `US-0077`'s reflow
    // corrections and C15 is `US-0078`'s selection kill. All four are measured
    // free against the 45 recordings (`US-0076`); the ids exist so a later
    // packet can declare a window without reopening this array.
    "C1", "C2", "C3", "C4", "C5", "C6", "C7", "C8", "C9", "C10", "C11", "C12", "C13", "C14",
    "C15", //
    "D1", "D2", "D4", "D7", "D8", "D9", "D10", "D12", "D13", "D14", "D15", //
    "G1", "G2", "G3", "G6", "G7",
];

/// Which engine produced a replay. The new engine never blesses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    /// The vendored `alacritty_terminal` fork being replaced.
    Old,
    /// `oneterm-vt`, once it exists.
    New,
}

impl std::str::FromStr for Engine {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "old" => Ok(Self::Old),
            "new" => Ok(Self::New),
            other => bail!("unknown engine {other:?} (expected `old` or `new`)"),
        }
    }
}

/// One vendored recording: bytes plus the geometry it was captured at.
#[derive(Debug, Clone)]
pub struct Recording {
    /// Directory name, which is also the test name.
    pub name: String,
    /// Directory holding `recording`, the expectations and any declared diffs.
    pub dir: PathBuf,
    /// Raw captured PTY bytes.
    pub bytes: Vec<u8>,
    /// Columns the capture ran at.
    pub columns: usize,
    /// Visible lines the capture ran at.
    pub screen_lines: usize,
    /// Scrollback the capture ran with.
    pub history_size: usize,
}

#[derive(Deserialize)]
struct SizeJson {
    columns: usize,
    screen_lines: usize,
}

#[derive(Deserialize)]
struct ConfigJson {
    history_size: usize,
}

/// Locate the corpus root from the `oneterm-tools` manifest directory.
///
/// The corpus lives at its final home under `crates/vt/` even though that crate
/// does not exist yet; data placed there now is zero moves later.
pub fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates/tools has a parent")
        .join("vt/tests/corpus")
}

/// The 45 vendored alacritty reference recordings, sorted by name.
pub fn alacritty_ref_dir() -> PathBuf {
    corpus_root().join("alacritty-ref")
}

/// OneTerm's own recordings, for behaviour the vendored set does not reach.
///
/// Kept apart from `alacritty-ref/` so the vendored set stays exactly what
/// upstream published, and so its NOTICE keeps covering only those files. Both
/// directories are gated identically: blessed once by the **old** engine and
/// then frozen (R-58).
pub fn oneterm_dir() -> PathBuf {
    corpus_root().join("oneterm")
}

/// Load every recording under `dir`, sorted by name, optionally filtered by a
/// substring of the name.
pub fn load_all(dir: &Path, filter: Option<&str>) -> Result<Vec<Recording>> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .with_context(|| format!("reading corpus directory {}", dir.display()))?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| filter.is_none_or(|f| name.contains(f)))
        .collect();
    names.sort();

    names.iter().map(|name| load(dir, name)).collect()
}

/// Load a single recording directory.
pub fn load(dir: &Path, name: &str) -> Result<Recording> {
    let recording_dir = dir.join(name);
    let read = |file: &str| -> Result<Vec<u8>> {
        let path = recording_dir.join(file);
        fs::read(&path).with_context(|| format!("reading {}", path.display()))
    };

    let size: SizeJson = serde_json::from_slice(&read("size.json")?)
        .with_context(|| format!("parsing {name}/size.json"))?;
    let config: ConfigJson = serde_json::from_slice(&read("config.json")?)
        .with_context(|| format!("parsing {name}/config.json"))?;

    let bytes = read("recording")?;

    Ok(Recording {
        name: name.to_owned(),
        dir: recording_dir,
        bytes,
        columns: size.columns,
        screen_lines: size.screen_lines,
        history_size: config.history_size,
    })
}

impl Recording {
    /// Path of the frozen cell-exact grid expectation.
    pub fn grid_expect_path(&self) -> PathBuf {
        self.dir.join("grid.expect")
    }

    /// Path of the frozen OneTerm state expectation.
    pub fn state_expect_path(&self) -> PathBuf {
        self.dir.join("state.expect")
    }

    /// Path of the declared per-cell expected differences, if any.
    pub fn expected_diffs_path(&self) -> PathBuf {
        self.dir.join("expected-diffs.json")
    }
}

// ---------------------------------------------------------------------------
// Grid expectation
// ---------------------------------------------------------------------------

/// A cell-exact grid, in the form the expectation file stores.
///
/// Rows are ordered **newest first**, matching the order upstream's `Storage`
/// serializes after `truncate()` rezeroes the ring: `inner[i]` is
/// `Line(screen_lines - i - 1)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GridExpect {
    /// Column count.
    pub columns: usize,
    /// Visible line count.
    pub lines: usize,
    /// Viewport position as distance from the newest row.
    pub display_offset: usize,
    /// Every row, newest first.
    pub rows: Vec<RowExpect>,
}

/// One row: the derived wrap flag plus one encoded token per column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowExpect {
    /// `WRAPLINE` on the row's last cell, lifted to the row for the new
    /// engine's row-flag model (deviation G1). It is *derived*: the flag is
    /// still carried per cell in `cells`, so nothing is trimmed.
    pub wrap: bool,
    /// One encoded cell token per column, including trailing blanks.
    pub cells: Vec<String>,
}

impl GridExpect {
    /// Render the expectation file body.
    pub fn encode(&self) -> String {
        let mut out = String::with_capacity(self.rows.len() * 64);
        out.push_str(
            "# OneTerm VT parity expectation: the grid, cell-exact, nothing trimmed.\n\
             # Blessed by the vendored alacritty_terminal engine at US-0072 and FROZEN.\n\
             # Rows are newest first. Each run is `<count>*<content>;<attrs>;<fg>;<bg>;<underline>;<hyperlink>`.\n\
             # Do not hand-edit: regenerate with `vt-corpus bless --engine old --deviation <id>`.\n",
        );
        let _ = writeln!(out, "version {FORMAT_VERSION}");
        let _ = writeln!(out, "columns {}", self.columns);
        let _ = writeln!(out, "lines {}", self.lines);
        let _ = writeln!(out, "display_offset {}", self.display_offset);
        let _ = writeln!(out, "rows {}", self.rows.len());
        for (index, row) in self.rows.iter().enumerate() {
            let _ = writeln!(
                out,
                "row {index} wrap={} {}",
                u8::from(row.wrap),
                encode_runs(&row.cells)
            );
        }
        out
    }

    /// Parse an expectation file body.
    pub fn decode(text: &str) -> Result<Self> {
        let mut columns = None;
        let mut lines = None;
        let mut display_offset = None;
        let mut declared_rows = None;
        let mut rows = Vec::new();

        for (number, line) in text.lines().enumerate() {
            let line = line.trim_end();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (key, rest) = line.split_once(' ').unwrap_or((line, ""));
            let at = || format!("line {}", number + 1);
            match key {
                "version" => {
                    let version: u32 = rest.parse().with_context(at)?;
                    if version != FORMAT_VERSION {
                        bail!(
                            "grid.expect format version {version}, this build writes \
                             {FORMAT_VERSION}"
                        );
                    }
                }
                "columns" => columns = Some(rest.parse().with_context(at)?),
                "lines" => lines = Some(rest.parse().with_context(at)?),
                "display_offset" => display_offset = Some(rest.parse().with_context(at)?),
                "rows" => declared_rows = Some(rest.parse::<usize>().with_context(at)?),
                "row" => {
                    let mut parts = rest.splitn(3, ' ');
                    let index: usize = parts
                        .next()
                        .ok_or_else(|| anyhow!("{}: row without an index", at()))?
                        .parse()
                        .with_context(at)?;
                    if index != rows.len() {
                        bail!("{}: row {index} out of order", at());
                    }
                    let wrap = parts
                        .next()
                        .ok_or_else(|| anyhow!("{}: row without a wrap flag", at()))?;
                    let wrap = match wrap {
                        "wrap=0" => false,
                        "wrap=1" => true,
                        other => bail!("{}: bad wrap flag {other:?}", at()),
                    };
                    let cells = decode_runs(parts.next().unwrap_or("")).with_context(at)?;
                    rows.push(RowExpect { wrap, cells });
                }
                other => bail!("{}: unknown key {other:?}", at()),
            }
        }

        let grid = GridExpect {
            columns: columns.ok_or_else(|| anyhow!("grid.expect has no `columns`"))?,
            lines: lines.ok_or_else(|| anyhow!("grid.expect has no `lines`"))?,
            display_offset: display_offset
                .ok_or_else(|| anyhow!("grid.expect has no `display_offset`"))?,
            rows,
        };
        if let Some(declared) = declared_rows
            && declared != grid.rows.len()
        {
            bail!(
                "grid.expect declares {declared} rows but carries {}",
                grid.rows.len()
            );
        }
        Ok(grid)
    }
}

/// Run-length encode one row's cell tokens.
fn encode_runs(cells: &[String]) -> String {
    let mut out = String::new();
    let mut index = 0;
    while index < cells.len() {
        let token = &cells[index];
        let mut count = 1;
        while index + count < cells.len() && &cells[index + count] == token {
            count += 1;
        }
        if !out.is_empty() {
            out.push('|');
        }
        let _ = write!(out, "{count}*{token}");
        index += count;
    }
    out
}

/// Expand a run-length encoded row back to one token per column.
fn decode_runs(text: &str) -> Result<Vec<String>> {
    let mut cells = Vec::new();
    if text.is_empty() {
        return Ok(cells);
    }
    for run in text.split('|') {
        let (count, token) = run
            .split_once('*')
            .ok_or_else(|| anyhow!("run {run:?} has no `*` separator"))?;
        let count: usize = count
            .parse()
            .with_context(|| format!("run {run:?} has a bad count"))?;
        cells.extend(std::iter::repeat_n(token.to_owned(), count));
    }
    Ok(cells)
}

// ---------------------------------------------------------------------------
// State expectation
// ---------------------------------------------------------------------------

/// The OneTerm snapshot upstream's grid comparison never checked (trap 44).
///
/// An ordered key/value list rather than a struct, because it is a frozen file
/// read by a diff, and because the fields a correction may change are named by
/// key in `expected-diffs.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StateExpect {
    /// Key/value pairs in write order.
    pub entries: Vec<(String, String)>,
}

impl StateExpect {
    /// Append one entry.
    pub fn push(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.entries.push((key.into(), value.into()));
    }

    /// Render the expectation file body.
    pub fn encode(&self) -> String {
        let mut out = String::new();
        out.push_str(
            "# OneTerm VT parity expectation: cursor, modes, palette, title, tab stops,\n\
             # scroll region. Blessed by the vendored engine at US-0072 and FROZEN.\n",
        );
        let _ = writeln!(out, "version {FORMAT_VERSION}");
        for (key, value) in &self.entries {
            let _ = writeln!(out, "{key} {value}");
        }
        out
    }

    /// Parse an expectation file body.
    pub fn decode(text: &str) -> Result<Self> {
        let mut state = Self::default();
        for (number, line) in text.lines().enumerate() {
            let line = line.trim_end();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (key, value) = line.split_once(' ').unwrap_or((line, ""));
            if key == "version" {
                let version: u32 = value
                    .parse()
                    .with_context(|| format!("line {}", number + 1))?;
                if version != FORMAT_VERSION {
                    bail!(
                        "state.expect format version {version}, this build writes {FORMAT_VERSION}"
                    );
                }
                continue;
            }
            state.push(key, value);
        }
        Ok(state)
    }
}

// ---------------------------------------------------------------------------
// Declared expected differences
// ---------------------------------------------------------------------------

/// A recording's `expected-diffs.json`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ExpectedDiffs {
    /// Declared windows; each must produce at least one difference.
    #[serde(default, rename = "diff")]
    pub windows: Vec<DiffWindow>,
}

/// One declared window of cells a correction is allowed to change.
#[derive(Debug, Clone, Deserialize)]
pub struct DiffWindow {
    /// A `C*` / `D*` / `G*` id from the two low-level designs.
    pub deviation: String,
    /// `"grid"` (default) or `"state"`.
    #[serde(default = "default_file")]
    pub file: String,
    /// Row index or `a..b` range. Grid windows only.
    #[serde(default)]
    pub rows: Option<String>,
    /// Column index or `a..b` range. Grid windows only.
    #[serde(default)]
    pub cols: Option<String>,
    /// Which fields may differ: [`CELL_FIELDS`] for a grid window, state keys
    /// (or key prefixes ending in `.`) for a state window.
    pub fields: Vec<String>,
    /// Why the difference is correct. Read by humans, required by the parser.
    pub reason: String,
}

fn default_file() -> String {
    "grid".to_owned()
}

/// An inclusive-exclusive index window parsed from `"4"` or `"4..9"`.
#[derive(Debug, Clone, Copy)]
struct Span {
    start: usize,
    end: usize,
}

impl Span {
    fn parse(text: &str) -> Result<Self> {
        if let Some((start, end)) = text.split_once("..") {
            Ok(Self {
                start: start.trim().parse().context("window start")?,
                end: end.trim().parse().context("window end")?,
            })
        } else {
            let only: usize = text.trim().parse().context("window index")?;
            Ok(Self {
                start: only,
                end: only + 1,
            })
        }
    }

    fn contains(&self, index: usize) -> bool {
        index >= self.start && index < self.end
    }
}

impl DiffWindow {
    fn rows_span(&self) -> Result<Option<Span>> {
        self.rows.as_deref().map(Span::parse).transpose()
    }

    fn cols_span(&self) -> Result<Option<Span>> {
        self.cols.as_deref().map(Span::parse).transpose()
    }

    fn covers_field(&self, field: &str) -> bool {
        self.fields.iter().any(|declared| {
            declared == field || (declared.ends_with('.') && field.starts_with(declared.as_str()))
        })
    }
}

/// Read a recording's declared differences, or an empty set when the file is
/// absent. Every `deviation` id is validated here, so a typo fails loudly
/// instead of quietly widening the gate.
pub fn load_expected_diffs(recording: &Recording) -> Result<ExpectedDiffs> {
    let path = recording.expected_diffs_path();
    if !path.exists() {
        return Ok(ExpectedDiffs::default());
    }
    let text = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let diffs: ExpectedDiffs =
        serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
    validate_expected_diffs(&diffs, &path)?;
    Ok(diffs)
}

/// Reject a declaration that would quietly widen the gate: an id that names no
/// row in the design tables, a field that is not a real field, or a window with
/// no stated reason.
pub(crate) fn validate_expected_diffs(diffs: &ExpectedDiffs, path: &Path) -> Result<()> {
    for window in &diffs.windows {
        if !KNOWN_DEVIATIONS.contains(&window.deviation.as_str()) {
            bail!(
                "{}: unknown deviation id {:?} (not a row in dispatch-and-modes.md or \
                 grid-and-scrollback.md)",
                path.display(),
                window.deviation
            );
        }
        if window.file != "grid" && window.file != "state" {
            bail!(
                "{}: `file` must be \"grid\" or \"state\", found {:?}",
                path.display(),
                window.file
            );
        }
        if window.file == "grid" {
            if window.rows.is_none() || window.cols.is_none() {
                bail!(
                    "{}: a grid window needs both `rows` and `cols`",
                    path.display()
                );
            }
            for field in &window.fields {
                if !CELL_FIELDS.contains(&field.as_str()) {
                    bail!(
                        "{}: unknown grid field {field:?} (expected one of {CELL_FIELDS:?})",
                        path.display()
                    );
                }
            }
        }
        window
            .rows_span()
            .with_context(|| format!("{}: `rows`", path.display()))?;
        window
            .cols_span()
            .with_context(|| format!("{}: `cols`", path.display()))?;
        if window.reason.trim().is_empty() {
            bail!("{}: every declared diff needs a `reason`", path.display());
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Comparison
// ---------------------------------------------------------------------------

/// One difference between a replay and the frozen expectation.
#[derive(Debug, Clone)]
pub struct Difference {
    /// `"grid"` or `"state"`.
    pub file: &'static str,
    /// Row index for a grid difference; `None` for state.
    pub row: Option<usize>,
    /// Column index for a grid difference; `None` for state.
    pub col: Option<usize>,
    /// Cell field or state key.
    pub field: String,
    /// The frozen value.
    pub expected: String,
    /// What the replay produced.
    pub actual: String,
}

impl std::fmt::Display for Difference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (self.row, self.col) {
            (Some(row), Some(col)) => write!(
                f,
                "{}: row {row} col {col} {}: expected {:?}, got {:?}",
                self.file, self.field, self.expected, self.actual
            ),
            _ => write!(
                f,
                "{}: {}: expected {:?}, got {:?}",
                self.file, self.field, self.expected, self.actual
            ),
        }
    }
}

/// The verdict for one recording.
#[derive(Debug, Default)]
pub struct CheckReport {
    /// Differences that no declared window covers. Any entry fails the gate.
    pub undeclared: Vec<Difference>,
    /// Declared windows that produced no difference at all.
    pub stale: Vec<String>,
    /// Differences a declared window accounted for, by deviation id.
    pub accepted: BTreeMap<String, usize>,
}

impl CheckReport {
    /// Whether the recording passes the gate.
    pub fn passed(&self) -> bool {
        self.undeclared.is_empty() && self.stale.is_empty()
    }
}

/// Compare a replay against the frozen expectations, applying the recording's
/// declared expected differences.
///
/// The rules, in order, are what keep this a gate rather than a rubber stamp:
/// a difference inside a declared window in a declared field passes; anything
/// else fails; and a declared window that produces no difference fails too, so
/// a correction later re-implemented as parity cannot leave a permanent hole.
pub fn check(
    expected_grid: &GridExpect,
    actual_grid: &GridExpect,
    expected_state: &StateExpect,
    actual_state: &StateExpect,
    diffs: &ExpectedDiffs,
) -> Result<CheckReport> {
    let mut report = CheckReport::default();
    let mut used = vec![false; diffs.windows.len()];

    let mut differences = diff_grid(expected_grid, actual_grid);
    differences.extend(diff_state(expected_state, actual_state));

    for difference in differences {
        let mut covered = false;
        for (index, window) in diffs.windows.iter().enumerate() {
            if !window_covers(window, &difference)? {
                continue;
            }
            used[index] = true;
            *report.accepted.entry(window.deviation.clone()).or_default() += 1;
            covered = true;
            break;
        }
        if !covered {
            report.undeclared.push(difference);
        }
    }

    for (index, window) in diffs.windows.iter().enumerate() {
        if !used[index] {
            report.stale.push(format!(
                "{} ({}, rows {}, cols {}, fields {:?}) declared a difference that did not happen",
                window.deviation,
                window.file,
                window.rows.as_deref().unwrap_or("-"),
                window.cols.as_deref().unwrap_or("-"),
                window.fields,
            ));
        }
    }

    Ok(report)
}

fn window_covers(window: &DiffWindow, difference: &Difference) -> Result<bool> {
    if window.file != difference.file {
        return Ok(false);
    }
    if !window.covers_field(&difference.field) {
        return Ok(false);
    }
    if difference.file == "state" {
        return Ok(true);
    }
    let (Some(row), Some(col)) = (difference.row, difference.col) else {
        return Ok(false);
    };
    let rows = window
        .rows_span()?
        .ok_or_else(|| anyhow!("grid window without `rows`"))?;
    let cols = window
        .cols_span()?
        .ok_or_else(|| anyhow!("grid window without `cols`"))?;
    Ok(rows.contains(row) && cols.contains(col))
}

/// Field-by-field grid comparison. Geometry mismatches are reported first and
/// stop the cell walk, because a shifted grid produces thousands of useless
/// per-cell differences.
fn diff_grid(expected: &GridExpect, actual: &GridExpect) -> Vec<Difference> {
    let mut out = Vec::new();
    let geometry: [(&str, usize, usize); 4] = [
        ("columns", expected.columns, actual.columns),
        ("lines", expected.lines, actual.lines),
        (
            "display_offset",
            expected.display_offset,
            actual.display_offset,
        ),
        ("row_count", expected.rows.len(), actual.rows.len()),
    ];
    for (field, expected_value, actual_value) in geometry {
        if expected_value != actual_value {
            out.push(Difference {
                file: "grid",
                row: None,
                col: None,
                field: field.to_owned(),
                expected: expected_value.to_string(),
                actual: actual_value.to_string(),
            });
        }
    }
    if !out.is_empty() {
        return out;
    }

    for (index, (expected_row, actual_row)) in
        expected.rows.iter().zip(actual.rows.iter()).enumerate()
    {
        if expected_row.wrap != actual_row.wrap {
            out.push(Difference {
                file: "grid",
                row: Some(index),
                col: None,
                field: "wrap".to_owned(),
                expected: u8::from(expected_row.wrap).to_string(),
                actual: u8::from(actual_row.wrap).to_string(),
            });
        }
        for (col, (expected_cell, actual_cell)) in expected_row
            .cells
            .iter()
            .zip(actual_row.cells.iter())
            .enumerate()
        {
            if expected_cell == actual_cell {
                continue;
            }
            let expected_fields: Vec<&str> = expected_cell.split(';').collect();
            let actual_fields: Vec<&str> = actual_cell.split(';').collect();
            for (position, name) in CELL_FIELDS.iter().enumerate() {
                let expected_field = expected_fields.get(position).copied().unwrap_or("");
                let actual_field = actual_fields.get(position).copied().unwrap_or("");
                if expected_field != actual_field {
                    out.push(Difference {
                        file: "grid",
                        row: Some(index),
                        col: Some(col),
                        field: (*name).to_owned(),
                        expected: expected_field.to_owned(),
                        actual: actual_field.to_owned(),
                    });
                }
            }
        }
    }
    out
}

fn diff_state(expected: &StateExpect, actual: &StateExpect) -> Vec<Difference> {
    let expected_map: BTreeMap<&str, &str> = expected
        .entries
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    let actual_map: BTreeMap<&str, &str> = actual
        .entries
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();

    let mut keys: Vec<&str> = expected_map.keys().copied().collect();
    keys.extend(actual_map.keys().copied());
    keys.sort_unstable();
    keys.dedup();

    keys.into_iter()
        .filter_map(|key| {
            let expected_value = expected_map.get(key).copied().unwrap_or("<absent>");
            let actual_value = actual_map.get(key).copied().unwrap_or("<absent>");
            (expected_value != actual_value).then(|| Difference {
                file: "state",
                row: None,
                col: None,
                field: key.to_owned(),
                expected: expected_value.to_owned(),
                actual: actual_value.to_owned(),
            })
        })
        .collect()
}

/// Replay one recording through `engine` and compare it with the frozen
/// expectations.
///
/// This is the gate: `vt-corpus check` prints its report, and
/// `crates/tools/tests/corpus_check.rs` asserts on it inside
/// `cargo test --workspace`, so an expectation that drifts fails the build.
/// [`Engine::Old`] against the frozen files is the comparator's own self-test;
/// [`Engine::New`] is `US-0076`'s exit criterion.
pub fn check_recording(recording: &Recording, engine: Engine) -> Result<CheckReport> {
    let (grid, state) = match engine {
        Engine::Old => crate::corpus_replay::replay_old(recording),
        Engine::New => crate::corpus_replay_new::replay_new(recording),
    };
    let read = |path: PathBuf| -> Result<String> {
        fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))
    };
    let expected_grid = GridExpect::decode(&read(recording.grid_expect_path())?)
        .with_context(|| format!("{}: grid.expect", recording.name))?;
    let expected_state = StateExpect::decode(&read(recording.state_expect_path())?)
        .with_context(|| format!("{}: state.expect", recording.name))?;
    let diffs = load_expected_diffs(recording)?;
    check(&expected_grid, &grid, &expected_state, &state, &diffs)
}

#[cfg(test)]
#[path = "corpus_tests.rs"]
mod corpus_tests;
