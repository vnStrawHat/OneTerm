//! Unit tests for the corpus encoding and the expected-difference mechanism.
//!
//! The mechanism matters more than the encoding: a bug here does not make the
//! gate fail loudly, it makes it stop being a gate.

use super::*;

fn cell(content: &str) -> String {
    format!("{content};-;nForeground;nBackground;-;-")
}

fn grid(rows: &[&[&str]]) -> GridExpect {
    GridExpect {
        columns: rows.first().map_or(0, |row| row.len()),
        lines: rows.len(),
        display_offset: 0,
        rows: rows
            .iter()
            .map(|row| RowExpect {
                wrap: false,
                cells: row.iter().map(|content| cell(content)).collect(),
            })
            .collect(),
    }
}

fn window(deviation: &str, rows: &str, cols: &str, fields: &[&str]) -> DiffWindow {
    DiffWindow {
        deviation: deviation.to_owned(),
        file: "grid".to_owned(),
        rows: Some(rows.to_owned()),
        cols: Some(cols.to_owned()),
        fields: fields.iter().map(|f| (*f).to_owned()).collect(),
        reason: "unit test".to_owned(),
    }
}

fn report(expected: &GridExpect, actual: &GridExpect, windows: Vec<DiffWindow>) -> CheckReport {
    let state = StateExpect::default();
    check(expected, actual, &state, &state, &ExpectedDiffs { windows })
        .expect("windows are well formed")
}

/// The 46 frozen `grid.expect` files are read by [`GridExpect::decode`] and
/// written by nothing — the engine that blessed them was deleted at `US-0087`.
/// So the fixture here is the **file format itself**, spelled out, rather than
/// a round trip through a writer: a round trip proves the pair is
/// self-consistent, which says nothing about whether the frozen files still
/// parse.
#[test]
fn the_frozen_grid_format_decodes_including_run_lengths() {
    let a = cell("0041");
    let b = cell("0042");
    let space = cell("0020");
    let text = format!(
        "# OneTerm VT parity expectation: the grid, cell-exact, nothing trimmed.\n\
         # Blessed by the vendored alacritty fork at US-0072 and FROZEN.\n\
         version 1\n\
         columns 5\n\
         lines 2\n\
         display_offset 3\n\
         rows 2\n\
         row 0 wrap=1 5*{a}\n\
         row 1 wrap=0 2*{b}|1*{space}|1*{b}|1*{space}\n"
    );

    let decoded = GridExpect::decode(&text).expect("the frozen format parses");

    assert_eq!(
        decoded,
        GridExpect {
            columns: 5,
            lines: 2,
            display_offset: 3,
            rows: vec![
                RowExpect {
                    wrap: true,
                    cells: vec![cell("0041"); 5],
                },
                RowExpect {
                    wrap: false,
                    cells: vec![
                        cell("0042"),
                        cell("0042"),
                        cell("0020"),
                        cell("0042"),
                        cell("0020"),
                    ],
                },
            ],
        }
    );
}

#[test]
fn a_zero_column_row_decodes_to_no_cells() {
    let text = "version 1\ncolumns 0\nlines 1\ndisplay_offset 0\nrows 1\nrow 0 wrap=0 \n";

    assert_eq!(
        GridExpect::decode(text).expect("an empty run list is not an error"),
        GridExpect {
            columns: 0,
            lines: 1,
            display_offset: 0,
            rows: vec![RowExpect {
                wrap: false,
                cells: Vec::new(),
            }],
        }
    );
}

#[test]
fn an_undeclared_difference_fails_the_gate() {
    let expected = grid(&[&["0041", "0042"]]);
    let actual = grid(&[&["0041", "0043"]]);

    let report = report(&expected, &actual, Vec::new());

    assert!(!report.passed());
    assert_eq!(report.undeclared.len(), 1);
    assert_eq!(report.undeclared[0].row, Some(0));
    assert_eq!(report.undeclared[0].col, Some(1));
    assert_eq!(report.undeclared[0].field, "content");
}

#[test]
fn a_declared_window_turns_a_difference_into_a_pass() {
    let expected = grid(&[&["0041", "0042"]]);
    let actual = grid(&[&["0041", "0043"]]);

    let report = report(
        &expected,
        &actual,
        vec![window("C1", "0", "1..2", &["content"])],
    );

    assert!(report.passed(), "{:?}", report.undeclared);
    assert_eq!(report.accepted.get("C1"), Some(&1));
}

#[test]
fn a_declared_window_covers_only_the_cells_it_names() {
    let expected = grid(&[&["0041", "0042"], &["0041", "0042"]]);
    let actual = grid(&[&["0041", "0043"], &["0044", "0042"]]);

    let report = report(
        &expected,
        &actual,
        vec![window("C1", "0", "1..2", &["content"])],
    );

    assert!(!report.passed());
    assert_eq!(report.undeclared.len(), 1);
    assert_eq!(report.undeclared[0].row, Some(1));
    assert_eq!(report.undeclared[0].col, Some(0));
}

#[test]
fn a_declared_window_covers_only_the_fields_it_names() {
    let expected = grid(&[&["0041"]]);
    let mut actual = grid(&[&["0041"]]);
    actual.rows[0].cells[0] = "0041;BOLD;nForeground;nBackground;-;-".to_owned();

    let report = report(
        &expected,
        &actual,
        vec![window("C1", "0", "0", &["content"])],
    );

    assert!(!report.passed());
    assert_eq!(report.undeclared[0].field, "attrs");
}

#[test]
fn a_window_that_produces_no_difference_is_stale_and_fails() {
    let expected = grid(&[&["0041", "0042"]]);

    let report = report(
        &expected,
        &expected.clone(),
        vec![window("C1", "0", "0..2", &["content"])],
    );

    assert!(!report.passed());
    assert_eq!(report.stale.len(), 1);
    assert!(report.stale[0].contains("C1"), "{:?}", report.stale);
}

#[test]
fn a_geometry_difference_is_reported_without_a_cell_walk() {
    let expected = grid(&[&["0041"], &["0041"]]);
    let actual = grid(&[&["0041"]]);

    let report = report(&expected, &actual, Vec::new());

    assert!(!report.passed());
    assert!(report.undeclared.iter().all(|d| d.row.is_none()));
    assert!(report.undeclared.iter().any(|d| d.field == "row_count"));
}

#[test]
fn a_state_difference_is_matched_by_key_and_by_key_prefix() {
    let mut expected = StateExpect::default();
    expected.push("palette.9", "#ff0000");
    let mut actual = StateExpect::default();
    actual.push("palette.9", "#00ff00");

    let empty = GridExpect {
        columns: 0,
        lines: 0,
        display_offset: 0,
        rows: Vec::new(),
    };
    let declared = ExpectedDiffs {
        windows: vec![DiffWindow {
            deviation: "C6".to_owned(),
            file: "state".to_owned(),
            rows: None,
            cols: None,
            fields: vec!["palette.".to_owned()],
            reason: "RIS resets the palette".to_owned(),
        }],
    };

    let undeclared = check(
        &empty,
        &empty,
        &expected,
        &actual,
        &ExpectedDiffs::default(),
    )
    .expect("well formed");
    let covered = check(&empty, &empty, &expected, &actual, &declared).expect("well formed");

    assert!(!undeclared.passed());
    assert_eq!(undeclared.undeclared[0].field, "palette.9");
    assert!(covered.passed(), "{:?}", covered.undeclared);
}

#[test]
fn an_unknown_deviation_id_is_refused() {
    let diffs = ExpectedDiffs {
        windows: vec![window("C99", "0", "0", &["content"])],
    };

    let error = validate_expected_diffs(&diffs, Path::new("expected-diffs.json"))
        .expect_err("C99 is not a row in either design table");

    assert!(
        error.to_string().contains("unknown deviation id"),
        "{error}"
    );
}

#[test]
fn an_unknown_cell_field_is_refused() {
    let diffs = ExpectedDiffs {
        windows: vec![window("C1", "0", "0", &["colour"])],
    };

    let error = validate_expected_diffs(&diffs, Path::new("expected-diffs.json"))
        .expect_err("`colour` is not a cell field");

    assert!(error.to_string().contains("unknown grid field"), "{error}");
}

#[test]
fn a_grid_window_without_a_column_range_is_refused() {
    let mut diffs = ExpectedDiffs {
        windows: vec![window("C1", "0", "0", &["content"])],
    };
    diffs.windows[0].cols = None;

    let error = validate_expected_diffs(&diffs, Path::new("expected-diffs.json"))
        .expect_err("a grid window needs both ranges");

    assert!(error.to_string().contains("needs both"), "{error}");
}

#[test]
fn the_declared_shape_from_the_design_parses() {
    // The example in testing-and-bench.md § 2, same schema, JSON spelling.
    let text = r#"{
      "diff": [
        {
          "deviation": "C1",
          "rows": "4",
          "cols": "70..80",
          "fields": ["content", "attrs"],
          "reason": "DCH is a plain shift left; the reference clamps end to cols - 1"
        }
      ]
    }"#;

    let diffs: ExpectedDiffs = serde_json::from_str(text).expect("the design's example parses");
    validate_expected_diffs(&diffs, Path::new("expected-diffs.json")).expect("and validates");

    assert_eq!(diffs.windows.len(), 1);
    assert_eq!(diffs.windows[0].file, "grid");
}

#[test]
fn every_known_deviation_id_is_unique() {
    let mut sorted = KNOWN_DEVIATIONS.to_vec();
    sorted.sort_unstable();
    let mut deduped = sorted.clone();
    deduped.dedup();

    assert_eq!(sorted, deduped);
}
