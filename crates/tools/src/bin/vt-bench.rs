//! The VT engine benchmark (developer diagnostic, never shipped).
//!
//! ```text
//! vt-bench all|parser|grid|render|resize|rss|fixtures [--mib N] [--frames N] [--json] [--out DIR]
//! vt-bench grid --check [--baseline FILE] [--tolerance R] [--mib N]
//! ```
//!
//! **Recorded, never gated in CI.** No number here is an exit criterion for any
//! work packet: at realistic shell and SSH rates the engine has two to three
//! orders of magnitude of headroom, so a threshold would be a flaky test
//! measuring the machine. Run it in release (`cargo run -p oneterm-tools
//! --release --bin vt-bench -- all`), never in parallel with anything else, and
//! read every number next to the ConPTY transport ceiling `pty-throughput`
//! reports.
//!
//! `grid --check` is the single exception, and a trip-wire rather than a target:
//! it fails a fixture only once that fixture has become twice as slow as the
//! committed baseline, and it runs by hand on the machine the baseline came
//! from, because a shared runner varies by more than that band.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use oneterm_tools::bench::{
    self, CountingAllocator, DEFAULT_MIB, FIXTURES, MEMORY_ROWS, RESIZE_DEPTHS,
};

/// Tier 5 measures live heap, so the allocator has to be counted from process
/// start; installing it here rather than in the library keeps
/// `cargo test -p oneterm-tools` on the system allocator.
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

const USAGE: &str = "vt-bench all|parser|grid|render|resize|rss|fixtures \
                     [--mib N] [--frames N] [--json] [--out DIR] [--machine TEXT]\n\
                     vt-bench grid --check [--baseline FILE] [--tolerance R] [--mib N]";

/// The committed baseline, beside this crate's manifest, so `--check` needs no
/// path and no assumption about the working directory.
const BASELINE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/bench-baseline.json");

/// A fixture has to get twice as slow before `--check` fails it.
const DEFAULT_TOLERANCE: f64 = 0.5;

/// The value after a flag, or a message naming the flag that lacked one.
///
/// A flag that silently falls back to its default is the worst outcome for a
/// benchmark: `--mib abc` would measure 100 MiB and report it as whatever the
/// caller believed they asked for. Every one of these is a hard error.
fn value(raw: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    raw.next()
        .ok_or_else(|| format!("vt-bench: {flag} needs a value"))
}

fn number<T: std::str::FromStr>(
    raw: &mut impl Iterator<Item = String>,
    flag: &str,
) -> Result<T, String> {
    let text = value(raw, flag)?;
    text.parse()
        .map_err(|_| format!("vt-bench: {flag} wants a number, not {text:?}"))
}

fn main() -> ExitCode {
    match parse_and_run() {
        Ok(code) => code,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn parse_and_run() -> Result<ExitCode, String> {
    let mut raw = std::env::args().skip(1);
    let command = raw.next().unwrap_or_else(|| "all".to_owned());
    // `--help` as the first argument is a request for usage, not a command.
    if matches!(command.as_str(), "--help" | "-h" | "help") {
        println!("{USAGE}");
        return Ok(ExitCode::SUCCESS);
    }
    let mut mib = DEFAULT_MIB;
    let mut frames = 600;
    let mut json = false;
    let mut out: Option<PathBuf> = None;
    let mut machine: Option<String> = None;
    let mut check = false;
    let mut baseline = PathBuf::from(BASELINE);
    let mut tolerance = DEFAULT_TOLERANCE;

    while let Some(flag) = raw.next() {
        match flag.as_str() {
            "--machine" => machine = Some(value(&mut raw, "--machine")?),
            "--check" => check = true,
            "--baseline" => baseline = PathBuf::from(value(&mut raw, "--baseline")?),
            "--tolerance" => tolerance = number(&mut raw, "--tolerance")?,
            "--mib" => mib = number(&mut raw, "--mib")?,
            "--frames" => frames = number(&mut raw, "--frames")?,
            "--json" => json = true,
            "--out" => out = Some(PathBuf::from(value(&mut raw, "--out")?)),
            "--help" | "-h" => {
                println!("{USAGE}");
                return Ok(ExitCode::SUCCESS);
            }
            other => return Err(format!("vt-bench: unknown flag {other:?}")),
        }
    }

    if check {
        if command != "grid" {
            return Err("vt-bench: --check guards tier 2 only; run `vt-bench grid --check`".into());
        }
        return Ok(match guard(&baseline, tolerance, mib) {
            Ok(report) => {
                print!("{report}");
                ExitCode::SUCCESS
            }
            Err(report) => {
                print!("{report}");
                ExitCode::FAILURE
            }
        });
    }

    if command == "fixtures" {
        let dir = out.ok_or("vt-bench fixtures needs --out <dir>")?;
        dump_fixtures(&dir, mib).map_err(|error| format!("vt-bench: {error}"))?;
        return Ok(ExitCode::SUCCESS);
    }

    let report = run(&command, mib, frames, json, machine.as_deref());
    match out {
        Some(path) => {
            std::fs::write(&path, &report)
                .map_err(|error| format!("vt-bench: writing {}: {error}", path.display()))?;
            println!("wrote {}", path.display());
        }
        None => print!("{report}"),
    }
    Ok(ExitCode::SUCCESS)
}

fn dump_fixtures(dir: &PathBuf, mib: usize) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    for fixture in FIXTURES {
        let path = dir.join(format!("{}.vt", fixture.name));
        std::fs::write(&path, (fixture.make)(mib * 1024 * 1024))?;
        println!("{} — {}", path.display(), fixture.about);
    }
    Ok(())
}

/// The tier-2 regression trip-wire: fail a fixture that has become slower than
/// `tolerance` times its committed baseline.
///
/// The comparison is strict (`ratio < tolerance`), so a fixture sitting exactly
/// on the band passes: the rule is "twice as slow fails", and something exactly
/// half as fast is not yet *below* half. That matters when testing the guard by
/// editing a baseline, because doubling every figure lands the ratio precisely
/// on 0.50 and a fixture a hair above it will pass. Multiply by three instead --
/// a proof of the guard should not be a boundary case.
///
/// `Err` carries the report as well as the verdict, so a failure prints the
/// whole table rather than one line about the first fixture that tripped.
fn guard(baseline: &Path, tolerance: f64, mib: usize) -> Result<String, String> {
    // A debug number against a release baseline, or a number taken while the
    // engine walks its whole history after every feed, is a guaranteed false
    // failure. Refusing is the only honest verdict.
    if cfg!(debug_assertions) {
        return Err("vt-bench: --check needs a release build (cargo run --release)\n".to_owned());
    }
    if cfg!(feature = "vt-paranoid") {
        return Err(
            "vt-bench: --check is meaningless under vt-paranoid; drop the feature\n".into(),
        );
    }

    let text = std::fs::read_to_string(baseline)
        .map_err(|error| format!("vt-bench: no baseline at {}: {error}\n", baseline.display()))?;
    let parsed: serde_json::Value = serde_json::from_str(&text)
        .map_err(|error| format!("vt-bench: {} is not JSON: {error}\n", baseline.display()))?;
    let rows = parsed["tiers"]["tier2_grid"].as_array().ok_or_else(|| {
        format!(
            "vt-bench: {} has no tiers.tier2_grid; regenerate it with \
             `vt-bench grid --mib {mib} --json --out <file>`\n",
            baseline.display()
        )
    })?;

    let mut report = format!(
        "# vt-bench grid --check\n\n\
         Baseline: {}\nA fixture fails *below* {tolerance:.2} x its baseline figure; \
         exactly {tolerance:.2} passes.\n\n\
         | Fixture | baseline MiB/s | now MiB/s | ratio | |\n\
         | --- | ---: | ---: | ---: | --- |\n",
        parsed["machine"]
            .as_str()
            .unwrap_or("(machine not recorded)")
    );
    let mut failures = Vec::new();
    let measured = bench::run_grid(mib);
    for row in &measured {
        let Some(was) = rows
            .iter()
            .find(|entry| entry["fixture"] == row.fixture)
            .and_then(|entry| entry["mib_per_second"].as_f64())
        else {
            failures.push(format!("fixture `{}` has no baseline entry", row.fixture));
            continue;
        };
        let ratio = row.mib_per_second / was;
        let verdict = if ratio < tolerance {
            failures.push(format!("`{}` is at {ratio:.2} x baseline", row.fixture));
            "SLOWER"
        } else {
            "ok"
        };
        let _ = writeln!(
            report,
            "| `{}` | {was:.1} | {:.1} | {ratio:.2} | {verdict} |",
            row.fixture, row.mib_per_second
        );
    }
    // A baseline entry with no fixture behind it means the fixture set moved and
    // the baseline was not refreshed with it. Silently ignoring it would let a
    // renamed fixture drop out of the guard unnoticed.
    for entry in rows {
        let name = entry["fixture"].as_str().unwrap_or("?");
        if !measured.iter().any(|row| row.fixture == name) {
            failures.push(format!("baseline entry `{name}` has no fixture"));
        }
    }

    if failures.is_empty() {
        let _ = writeln!(report, "\nPASS: every fixture is within the band.");
        return Ok(report);
    }
    let _ = writeln!(report, "\nFAIL:");
    for failure in &failures {
        let _ = writeln!(report, "- {failure}");
    }
    Err(report)
}

fn run(command: &str, mib: usize, frames: usize, json: bool, machine: Option<&str>) -> String {
    let mut out = String::new();
    if !json {
        let _ = writeln!(
            out,
            "# vt-bench — oneterm-vt\n\n\
             Geometry {}x{}, {mib} MiB per fixture, median of {} interleaved cycles.\n\
             Recorded, never gated; `grid --check` is the one comparison with a verdict.\n",
            bench::COLS,
            bench::ROWS,
            bench::RUNS
        );
        if let Some(machine) = machine {
            let _ = writeln!(out, "Machine: {machine}\n");
        }
    }

    let all = command == "all";
    let mut sections: Vec<(&str, serde_json::Value)> = Vec::new();

    if all || command == "parser" || command == "grid" {
        let parser = (all || command == "parser").then(|| bench::run_parser(mib));
        let grid = (all || command == "grid").then(|| bench::run_grid(mib));
        if json {
            if let Some(rows) = &parser {
                sections.push(("tier1_parser", throughput_json(rows)));
            }
            if let Some(rows) = &grid {
                sections.push(("tier2_grid", throughput_json(rows)));
            }
        } else {
            let _ = match (parser.is_some(), grid.is_some()) {
                (true, true) => writeln!(out, "## Tiers 1-2: parser only, and parse plus grid\n"),
                (true, false) => writeln!(out, "## Tier 1: the parser alone\n"),
                _ => writeln!(out, "## Tier 2: parse plus grid\n"),
            };
            // A tier that was not asked for contributes no columns at all: a
            // table of `-` cells is what makes a published single-tier table
            // look like a failed run. Each tier that *is* present carries its
            // own spread, labelled -- one bare `spread` column beside two tiers
            // silently reports the second tier's and hides the first's.
            let mut header = String::from("| Fixture |");
            let mut rule = String::from("| --- |");
            for (present, label) in [(&parser, "parser"), (&grid, "parse+grid")] {
                if present.is_some() {
                    let _ = write!(header, " {label} MiB/s | {label} ns/B | {label} spread |");
                    rule.push_str(" ---: | ---: | ---: |");
                }
            }
            let _ = writeln!(out, "{header}\n{rule}");
            for (index, fixture) in FIXTURES.iter().enumerate() {
                let mut row = format!("| `{}` |", fixture.name);
                for tier in [&parser, &grid].into_iter().flatten() {
                    let measured = &tier[index];
                    let _ = write!(
                        row,
                        " {:.1} | {:.2} | {:.0}% |",
                        measured.mib_per_second, measured.ns_per_byte, measured.spread_percent
                    );
                }
                let _ = writeln!(out, "{row}");
            }
            let _ = writeln!(out);
        }
    }

    if all || command == "render" {
        let rows = bench::run_render(frames);
        if json {
            sections.push((
                "tier3_render",
                serde_json::Value::Array(
                    rows.iter()
                        .map(|r| {
                            serde_json::json!({
                                "fixture": r.fixture,
                                "us_per_frame": r.us_per_frame,
                                "cells": r.cells,
                            })
                        })
                        .collect(),
                ),
            ));
        } else {
            let _ = writeln!(
                out,
                "## Tier 3: parse, grid and one render update per frame ({frames} frames)\n"
            );
            let _ = writeln!(out, "| Fixture | us/frame | cells/frame |");
            let _ = writeln!(out, "| --- | ---: | ---: |");
            for row in rows {
                let _ = writeln!(
                    out,
                    "| `{}` | {:.1} | {} |",
                    row.fixture, row.us_per_frame, row.cells
                );
            }
            let _ = writeln!(out);
        }
    }

    if all || command == "resize" {
        let rows = bench::run_resize(&RESIZE_DEPTHS);
        if json {
            sections.push((
                "tier4_resize",
                serde_json::Value::Array(
                    rows.iter()
                        .map(|r| {
                            serde_json::json!({
                                "scrollback_rows": r.depth,
                                "grow_us": r.grow_us,
                                "shrink_us": r.shrink_us,
                            })
                        })
                        .collect(),
                ),
            ));
        } else {
            let _ = writeln!(
                out,
                "## Tier 4: resize latency, 80x24 to 100x40 (its own geometry, deliberately)\n"
            );
            let _ = writeln!(out, "| Scrollback rows | grow us | shrink us |");
            let _ = writeln!(out, "| ---: | ---: | ---: |");
            for row in rows {
                let _ = writeln!(
                    out,
                    "| {} | {:.0} | {:.0} |",
                    row.depth, row.grow_us, row.shrink_us
                );
            }
            let _ = writeln!(out);
        }
    }

    if all || command == "rss" {
        let rows = bench::run_memory(MEMORY_ROWS);
        if json {
            sections.push((
                "tier5_memory",
                serde_json::Value::Array(
                    rows.iter()
                        .map(|r| {
                            serde_json::json!({
                                "content": r.content,
                                "heap_bytes": r.bytes,
                                "heap_bytes_per_row": r.bytes_per_row,
                            })
                        })
                        .collect(),
                ),
            ));
        } else {
            let _ = writeln!(
                out,
                "## Tier 5: live heap after {MEMORY_ROWS} scrollback rows\n\n\
                 Live heap, not process RSS: the design claim is what a grid of N rows costs, and \
                 RSS also counts retained allocator pages.\n"
            );
            let _ = writeln!(out, "| Content | heap bytes | bytes/row |");
            let _ = writeln!(out, "| --- | ---: | ---: |");
            for row in rows {
                let _ = writeln!(
                    out,
                    "| {} | {} | {:.0} |",
                    row.content, row.bytes, row.bytes_per_row
                );
            }
            let _ = writeln!(out);
        }
    }

    if json {
        let map: serde_json::Map<String, serde_json::Value> = sections
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect();
        return serde_json::to_string_pretty(&serde_json::json!({
            "engine": "oneterm-vt",
            // Named by hand, because no dependency-free way to read a CPU model
            // exists. A baseline with no machine in it is a number with no
            // meaning, so `--check` prints whatever is here, however vague.
            "machine": machine.unwrap_or("(machine not recorded)"),
            // Emitted into every JSON run so the rule travels with the file it
            // governs: JSON has no comment syntax, and a baseline regenerated
            // to make `--check` pass is the failure the guard exists to catch.
            "note": "When this file is a committed baseline, refresh it only in a commit \
                     that says why the number moved.",
            "columns": bench::COLS,
            "lines": bench::ROWS,
            "mib_per_fixture": mib,
            "runs": bench::RUNS,
            "tiers": map,
        }))
        .unwrap_or_default();
    }
    out
}

fn throughput_json(rows: &[bench::Throughput]) -> serde_json::Value {
    serde_json::Value::Array(
        rows.iter()
            .map(|r| {
                serde_json::json!({
                    "fixture": r.fixture,
                    "mib_per_second": r.mib_per_second,
                    "ns_per_byte": r.ns_per_byte,
                    "spread_percent": r.spread_percent,
                })
            })
            .collect(),
    )
}
