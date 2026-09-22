//! The semantic highlighter benchmark (developer diagnostic, never shipped).
//!
//! ```text
//! highlight-bench [--runs N] [--machine TEXT] [--json PATH]
//! ```
//!
//! **Recorded, never gated in CI** — the same rule `vt-bench` states in its own
//! header, for the same reasons: at the rates a shell produces text there are
//! orders of magnitude of headroom, and a threshold on a shared runner measures
//! the runner. Run it in release
//! (`cargo run -p oneterm-tools --release --bin highlight-bench`), never in
//! parallel with anything else, and compare it by hand against the committed
//! `crates/tools/highlight-bench-baseline.json`. `--json PATH` writes a new
//! table; refreshing the baseline is an explicit act, in a commit that says why
//! the number moved.
//!
//! What it measures: [`oneterm_highlight::scan_line_into`], the whole per-line
//! cost §10 of `docs/terminal-semantic-highlighting.md` tabulates — the prompt
//! regex, the Aho-Corasick keyword pass, the structural regexes and the
//! byte-to-char map. The four content shapes are chosen to exercise one each.
//!
//! **The unit is the logical line, so the wrap width does not change what the
//! scanner sees.** The width is still an axis of the table, and it earns its
//! place twice: the `rows` column is what the width is really for — how many
//! display rows one scan covers, which is how many rows one frame replans — and
//! the three widths of one `(shape, chars, profile)` cell are three independent
//! measurements of identical work, so the spread between them is this table's
//! own noise floor, measured rather than asserted.
//!
//! It does not measure the view-side join and scatter around the scanner:
//! `crates/tools` may only reach down to leaf crates
//! (`docs/agents/crate-dependency-rules.md`) and `oneterm-terminal-view` is a
//! GPUI crate. That half is asserted instead, as `FrameStats::class_scans` and
//! `class_rows_scanned` in `crates/terminal-view/src/render/plan_cache.rs`.

use std::fmt::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use oneterm_highlight::{RowRole, RuleSet, ShellProfile, scan_line_into};

const USAGE: &str = "highlight-bench [--runs N] [--machine TEXT] [--json PATH]";

/// Logical-line lengths, in chars. 8 000 is §10's worst case: one logical line
/// filling a 40x200 viewport.
const CHARS: [usize; 4] = [80, 200, 2_000, 8_000];

/// Viewport widths a logical line is spread over, in columns.
const WRAP_WIDTHS: [usize; 3] = [80, 120, 200];

/// Cycles per cell; the reported figure is the median of them, as in `vt-bench`.
/// Five is odd (so the median is a sample, not a mean of two) and enough to show
/// a spread.
const RUNS: usize = 5;

/// Chars scanned per timed cycle, so a cell of any length takes about the same
/// wall time and the short lines are not measured against the clock's own
/// resolution.
const CHARS_PER_CYCLE: usize = 200_000;

/// The four content shapes, each aimed at one of §10's four per-line costs.
#[derive(Clone, Copy, Debug)]
enum Shape {
    /// A Windows prompt line: the prompt regex and the prompt-path fill.
    Prompt,
    /// Ordinary prose output with no keyword and no structure: the floor.
    Plain,
    /// A keyword-dense log: the Aho-Corasick pass plus the structural regexes
    /// (timestamp, IPv4, path, number).
    KeywordLog,
    /// A line carrying CJK: the byte-to-char map, which is the identity for
    /// ASCII and is not for this (`BUG-0071` F3).
    Cjk,
}

impl Shape {
    const ALL: [Shape; 4] = [Shape::Prompt, Shape::Plain, Shape::KeywordLog, Shape::Cjk];

    fn name(self) -> &'static str {
        match self {
            Shape::Prompt => "prompt",
            Shape::Plain => "plain",
            Shape::KeywordLog => "keyword-log",
            Shape::Cjk => "cjk",
        }
    }

    /// The fixture, exactly `chars` chars long (chars, not bytes).
    ///
    /// Deterministic — no RNG, no captured file — so a run is reproducible from
    /// the repository alone.
    fn fixture(self, chars: usize) -> String {
        let head = match self {
            Shape::Prompt => r"C:\Users\John Doe\customer\acme\backend\gateway>git commit -m ",
            Shape::Plain => "the quick brown fox jumps over the lazy dog and keeps on running ",
            Shape::KeywordLog => {
                "2026-09-22 10:00:00 error connect 192.168.1.10 failed warn retry /var/log/app.log 42 "
            }
            Shape::Cjk => {
                "\u{65e5}\u{672c}\u{8a9e} error at /etc/hosts \u{4e2d}\u{6587}\u{6d4b}\u{8bd5} 2026-09-22 "
            }
        };
        let body = match self {
            // The prompt's tail is the command being typed, which stays in
            // `CommandMode` to the end of the line.
            Shape::Prompt => "src/main.rs ",
            Shape::Plain => "the fox runs over that dog again and again with nothing to tag ",
            Shape::KeywordLog => {
                "info ok 10.0.0.1 debug 7 /usr/bin/thing warn 2026-09-22 11:22:33 error "
            }
            Shape::Cjk => "\u{4e2d}\u{6587} error 192.168.0.1 \u{65e5}\u{672c}\u{8a9e} ",
        };
        let mut out = String::with_capacity(chars * 4);
        out.extend(head.chars().take(chars));
        while out.chars().count() < chars {
            let missing = chars - out.chars().count();
            out.extend(body.chars().take(missing));
        }
        debug_assert_eq!(out.chars().count(), chars);
        out
    }
}

/// One measured cell of the table.
struct Cell {
    shape: &'static str,
    profile: &'static str,
    chars: usize,
    wrap_width: usize,
    /// Display rows this one scan covers: `ceil(chars / wrap_width)`.
    rows: usize,
    /// Microseconds for one scan of the whole logical line, median of [`RUNS`].
    us_per_scan: f64,
    /// The same, divided by [`Cell::rows`] — the per-display-row cost a frame
    /// pays when it replans that run.
    us_per_row: f64,
    /// Nanoseconds per char, the length-independent figure to compare cells by.
    ns_per_char: f64,
    /// Fastest minus slowest cycle as a percentage of the median. The honest
    /// companion to a median: a difference narrower than this is the machine.
    spread_percent: f64,
}

fn profiles() -> [(&'static str, ShellProfile); 3] {
    [
        ("Unix", ShellProfile::Unix),
        ("Cmd", ShellProfile::Cmd),
        ("PowerShell", ShellProfile::PowerShell),
    ]
}

/// One cell: the median time of one whole-line scan, and the spread across
/// cycles.
fn measure(line: &str, profile: ShellProfile, runs: usize) -> (Duration, f64) {
    let rules = RuleSet::global();
    let chars = line.chars().count();
    let iterations = (CHARS_PER_CYCLE / chars.max(1)).max(1);
    let mut out = Vec::with_capacity(chars);
    // One untimed cycle so the first sample is not the one that warms the
    // buffer and the branch predictors.
    for _ in 0..iterations {
        scan_line_into(line, rules, &profile, RowRole::Output, &mut out);
    }

    let mut samples: Vec<Duration> = Vec::with_capacity(runs);
    for _ in 0..runs {
        let start = Instant::now();
        for _ in 0..iterations {
            scan_line_into(line, rules, &profile, RowRole::Output, &mut out);
        }
        samples.push(start.elapsed() / iterations as u32);
    }
    samples.sort_unstable();
    let median = samples[samples.len() / 2];
    let spread = (samples[samples.len() - 1].as_secs_f64() - samples[0].as_secs_f64())
        / median.as_secs_f64()
        * 100.0;
    (median, spread)
}

fn run(runs: usize) -> Vec<Cell> {
    let mut cells = Vec::with_capacity(CHARS.len() * WRAP_WIDTHS.len() * 4 * 3);
    for shape in Shape::ALL {
        for chars in CHARS {
            let line = shape.fixture(chars);
            for (profile_name, profile) in profiles() {
                for wrap_width in WRAP_WIDTHS {
                    let (median, spread_percent) = measure(&line, profile, runs);
                    let us_per_scan = median.as_secs_f64() * 1e6;
                    let rows = chars.div_ceil(wrap_width);
                    cells.push(Cell {
                        shape: shape.name(),
                        profile: profile_name,
                        chars,
                        wrap_width,
                        rows,
                        us_per_scan,
                        us_per_row: us_per_scan / rows as f64,
                        ns_per_char: median.as_secs_f64() * 1e9 / chars as f64,
                        spread_percent,
                    });
                }
            }
        }
    }
    cells
}

fn table(cells: &[Cell]) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{:<12} {:<11} {:>6} {:>5} {:>5} {:>10} {:>10} {:>9} {:>8}",
        "shape", "profile", "chars", "wrap", "rows", "us/scan", "us/row", "ns/char", "spread%"
    );
    for c in cells {
        let _ = writeln!(
            out,
            "{:<12} {:<11} {:>6} {:>5} {:>5} {:>10.3} {:>10.3} {:>9.2} {:>8.1}",
            c.shape,
            c.profile,
            c.chars,
            c.wrap_width,
            c.rows,
            c.us_per_scan,
            c.us_per_row,
            c.ns_per_char,
            c.spread_percent
        );
    }
    out
}

fn json(cells: &[Cell], machine: &str, runs: usize) -> String {
    let rows: Vec<serde_json::Value> = cells
        .iter()
        .map(|c| {
            serde_json::json!({
                "shape": c.shape,
                "profile": c.profile,
                "chars": c.chars,
                "wrap_width": c.wrap_width,
                "rows": c.rows,
                "us_per_scan": c.us_per_scan,
                "us_per_row": c.us_per_row,
                "ns_per_char": c.ns_per_char,
                "spread_percent": c.spread_percent,
            })
        })
        .collect();
    let doc = serde_json::json!({
        "engine": "oneterm-highlight",
        "measured": "scan_line_into, RowRole::Output",
        "machine": machine,
        "runs": runs,
        "note": "Recorded, never gated. When this file is a committed baseline, \
                 refresh it only in a commit that says why the number moved. The \
                 wrap width does not change what the scanner sees (the unit is \
                 the logical line): the rows column is how many display rows one \
                 scan covers, and the spread between the three widths of one cell \
                 is the table's noise floor.",
        "cells": rows,
    });
    format!(
        "{}\n",
        serde_json::to_string_pretty(&doc).unwrap_or_default()
    )
}

fn main() -> ExitCode {
    match parse_and_run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn parse_and_run() -> Result<(), String> {
    let mut raw = std::env::args().skip(1);
    let mut runs = RUNS;
    let mut machine = "unrecorded".to_owned();
    let mut json_path: Option<PathBuf> = None;

    while let Some(flag) = raw.next() {
        // A flag that silently falls back to its default is the worst outcome
        // for a benchmark, so every missing or unparsable value is a hard error.
        let mut value = |flag: &str| {
            raw.next()
                .ok_or_else(|| format!("highlight-bench: {flag} needs a value"))
        };
        match flag.as_str() {
            "--runs" => {
                let text = value("--runs")?;
                runs = text
                    .parse()
                    .map_err(|_| format!("highlight-bench: --runs wants a number, not {text:?}"))?;
            }
            "--machine" => machine = value("--machine")?,
            "--json" => json_path = Some(PathBuf::from(value("--json")?)),
            "--help" | "-h" => {
                println!("{USAGE}");
                return Ok(());
            }
            other => return Err(format!("highlight-bench: unknown flag {other:?}")),
        }
    }
    if runs == 0 {
        return Err("highlight-bench: --runs must be at least 1".into());
    }

    let cells = run(runs);
    print!("{}", table(&cells));
    if let Some(path) = json_path {
        std::fs::write(&path, json(&cells, &machine, runs))
            .map_err(|e| format!("highlight-bench: cannot write {}: {e}", path.display()))?;
        println!("\nwrote {}", path.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fixtures produce the line lengths and the shapes they claim, for
    /// every length in the table — a fixture that is not the length it says
    /// makes every ns/char figure in its row wrong.
    #[test]
    fn every_fixture_is_exactly_the_length_it_claims() {
        for shape in Shape::ALL {
            for chars in CHARS {
                let line = shape.fixture(chars);
                assert_eq!(
                    line.chars().count(),
                    chars,
                    "{} at {chars} chars",
                    shape.name()
                );
            }
        }
    }

    /// ...and each shape reaches the cost it is there to exercise: the prompt
    /// shape is a prompt, the plain shape has no keyword, the log shape has
    /// several, and the CJK shape is not ASCII (so the byte-to-char map is not
    /// the identity).
    #[test]
    fn each_shape_exercises_the_cost_it_names() {
        let rules = RuleSet::global();
        let classes = |shape: Shape, profile: ShellProfile| {
            let line = shape.fixture(2_000);
            let mut out = Vec::new();
            scan_line_into(&line, rules, &profile, RowRole::Output, &mut out);
            (line, out)
        };

        let (_, prompt) = classes(Shape::Prompt, ShellProfile::Cmd);
        assert!(
            prompt.contains(&(oneterm_highlight::Class::PromptSign as u8)),
            "the prompt shape must reach the prompt branch"
        );

        let (_, plain) = classes(Shape::Plain, ShellProfile::Unix);
        for class in [
            oneterm_highlight::Class::Error,
            oneterm_highlight::Class::Warn,
            oneterm_highlight::Class::Info,
            oneterm_highlight::Class::Success,
            oneterm_highlight::Class::DateTime,
            oneterm_highlight::Class::Ip,
            oneterm_highlight::Class::Path,
            oneterm_highlight::Class::Number,
        ] {
            assert!(
                !plain.contains(&(class as u8)),
                "the plain shape is the floor, and it reached {class:?}"
            );
        }

        let (_, log) = classes(Shape::KeywordLog, ShellProfile::Unix);
        for class in [
            oneterm_highlight::Class::Error,
            oneterm_highlight::Class::Warn,
            oneterm_highlight::Class::DateTime,
            oneterm_highlight::Class::Ip,
            oneterm_highlight::Class::Path,
        ] {
            assert!(
                log.contains(&(class as u8)),
                "the keyword log must reach {class:?}"
            );
        }

        let (line, _) = classes(Shape::Cjk, ShellProfile::Unix);
        assert!(
            line.len() > line.chars().count(),
            "the CJK shape must be multi-byte"
        );
    }
}
