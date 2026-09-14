//! The VT engine benchmark (developer diagnostic, never shipped).
//!
//! ```text
//! vt-bench all|parser|grid|render|resize|rss|fixtures [--mib N] [--frames N] [--json] [--out DIR]
//! ```
//!
//! **Recorded, never gated.** No number here is an exit criterion for any work
//! packet: at realistic shell and SSH rates the engine has two to three orders
//! of magnitude of headroom, so a threshold would be a flaky test measuring the
//! machine. Run it in release (`cargo run -p oneterm-tools --release --bin
//! vt-bench -- all`), never in parallel with anything else, and read every
//! number next to the ConPTY transport ceiling `pty-throughput` reports.

use std::fmt::Write as _;
use std::path::PathBuf;
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
                     [--mib N] [--frames N] [--json] [--out DIR]";

fn main() -> ExitCode {
    let mut raw = std::env::args().skip(1);
    let command = raw.next().unwrap_or_else(|| "all".to_owned());
    // `--help` as the first argument is a request for usage, not a command.
    if matches!(command.as_str(), "--help" | "-h" | "help") {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let mut mib = DEFAULT_MIB;
    let mut frames = 600;
    let mut json = false;
    let mut out: Option<PathBuf> = None;

    while let Some(flag) = raw.next() {
        match flag.as_str() {
            "--mib" => {
                mib = raw
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(DEFAULT_MIB)
            }
            "--frames" => frames = raw.next().and_then(|v| v.parse().ok()).unwrap_or(frames),
            "--json" => json = true,
            "--out" => out = raw.next().map(PathBuf::from),
            "--help" | "-h" => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("vt-bench: unknown flag {other:?}");
                return ExitCode::FAILURE;
            }
        }
    }

    if command == "fixtures" {
        let Some(dir) = out else {
            eprintln!("vt-bench fixtures needs --out <dir>");
            return ExitCode::FAILURE;
        };
        if let Err(error) = dump_fixtures(&dir, mib) {
            eprintln!("vt-bench: {error}");
            return ExitCode::FAILURE;
        }
        return ExitCode::SUCCESS;
    }

    let report = run(&command, mib, frames, json);
    match out {
        Some(path) => match std::fs::write(&path, &report) {
            Ok(()) => println!("wrote {}", path.display()),
            Err(error) => {
                eprintln!("vt-bench: writing {}: {error}", path.display());
                return ExitCode::FAILURE;
            }
        },
        None => print!("{report}"),
    }
    ExitCode::SUCCESS
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

fn run(command: &str, mib: usize, frames: usize, json: bool) -> String {
    let mut out = String::new();
    if !json {
        let _ = writeln!(
            out,
            "# vt-bench — oneterm-vt\n\n\
             Geometry {}x{}, {mib} MiB per fixture, median of {} runs. Recorded, never gated.\n",
            bench::COLS,
            bench::ROWS,
            bench::RUNS
        );
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
            let _ = writeln!(out, "## Tiers 1-2: parser only, and parse plus grid\n");
            let _ = writeln!(
                out,
                "| Fixture | parser MiB/s | parser ns/B | parse+grid MiB/s | parse+grid ns/B |"
            );
            let _ = writeln!(out, "| --- | ---: | ---: | ---: | ---: |");
            for (index, fixture) in FIXTURES.iter().enumerate() {
                let p = parser.as_ref().map(|rows| &rows[index]);
                let g = grid.as_ref().map(|rows| &rows[index]);
                let _ = writeln!(
                    out,
                    "| `{}` | {} | {} | {} | {} |",
                    fixture.name,
                    p.map_or("-".into(), |r| format!("{:.1}", r.mib_per_second)),
                    p.map_or("-".into(), |r| format!("{:.2}", r.ns_per_byte)),
                    g.map_or("-".into(), |r| format!("{:.1}", r.mib_per_second)),
                    g.map_or("-".into(), |r| format!("{:.2}", r.ns_per_byte)),
                );
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
                })
            })
            .collect(),
    )
}
