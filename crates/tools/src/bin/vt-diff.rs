//! The old-versus-new differential (developer diagnostic, never shipped).
//!
//! ```text
//! vt-diff [--filter <substring>] [--dir <corpus dir>] [--chunk <bytes>] [--quiet]
//! ```
//!
//! Feeds the same bytes to the vendored `Term` and to `oneterm-vt` and compares
//! in the **`grid.expect` form**, not in a trimmed text form (R-43). That
//! matters: the trimmed form could not see BCE backgrounds on erased trailing
//! cells, per-cell attributes, wrap flags, or the viewport position — which is
//! most of what a reimplementation gets wrong.
//!
//! It is deliberately *not* the gate. The gate is the frozen expectations
//! (`vt-corpus check --engine new`), which no engine can move; this tool
//! answers "where exactly do the two disagree" while the migration runs, and it
//! retires with the fork at `US-0087`.
//!
//! Unlike the gate it ignores `expected-diffs.json`: every difference is
//! reported, because the point is to see them.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use oneterm_tools::corpus::{self, Difference, Recording, alacritty_ref_dir};
use oneterm_tools::{corpus_replay, corpus_replay_new};

fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("vt-diff: {error:#}");
            ExitCode::FAILURE
        }
    }
}

/// The geometry `vt-bench` runs its tiers at, so a fixture diff and a bench
/// number describe the same stream.
const FIXTURE_SIZE: (usize, usize) = (160, 45);

fn run() -> Result<bool> {
    let mut raw = std::env::args().skip(1);
    let mut filter: Option<String> = None;
    let mut dir: Option<PathBuf> = None;
    let mut quiet = false;
    let mut fixtures = false;
    let mut kib = 256usize;
    while let Some(flag) = raw.next() {
        let mut value = || {
            raw.next()
                .ok_or_else(|| anyhow::anyhow!("{flag} needs a value"))
        };
        match flag.as_str() {
            "--filter" => filter = Some(value()?),
            "--dir" => dir = Some(PathBuf::from(value()?)),
            "--fixtures" => fixtures = true,
            "--kib" => kib = value()?.parse().context("--kib")?,
            "--quiet" => quiet = true,
            "--help" | "-h" => {
                println!(
                    "vt-diff [--filter <substring>] [--dir <corpus dir>] [--quiet]\n\
                     vt-diff --fixtures [--kib <size>] [--filter <substring>]\n\
                     \n\
                     Feeds the same bytes to the vendored alacritty_terminal engine and to\n\
                     oneterm-vt and diffs the grids in `grid.expect` form. `--fixtures` runs\n\
                     the vt-bench stream generators at 160x45 instead of the corpus."
                );
                return Ok(true);
            }
            other => bail!("unknown flag {other:?} (try --help)"),
        }
    }

    let recordings = if fixtures {
        fixture_recordings(kib, filter.as_deref())
    } else {
        let dir = dir.unwrap_or_else(alacritty_ref_dir);
        corpus::load_all(&dir, filter.as_deref())
            .with_context(|| format!("reading {}", dir.display()))?
    };
    if recordings.is_empty() {
        bail!("nothing matched the filter");
    }

    let mut differing = 0;
    println!("| Recording | Bytes | Rows | Result |");
    println!("| --- | ---: | ---: | --- |");
    for recording in &recordings {
        let (old, _) = corpus_replay::replay_old(recording);
        let (new, _) = corpus_replay_new::replay_new(recording);
        let report = corpus::check(
            &old,
            &new,
            &corpus::StateExpect::default(),
            &corpus::StateExpect::default(),
            &corpus::ExpectedDiffs::default(),
        )?;
        if report.undeclared.is_empty() {
            println!(
                "| `{}` | {} | {} | identical |",
                recording.name,
                recording.bytes.len(),
                new.rows.len()
            );
            continue;
        }
        differing += 1;
        let first = &report.undeclared[0];
        println!(
            "| `{}` | {} | {} | **{} differences**, first: {} |",
            recording.name,
            recording.bytes.len(),
            new.rows.len(),
            report.undeclared.len(),
            first
        );
        if !quiet {
            print_context(recording, first);
        }
    }

    println!(
        "\n{} recordings, {} identical, {differing} differing",
        recordings.len(),
        recordings.len() - differing
    );
    Ok(differing == 0)
}

/// The `vt-bench` stream generators as in-memory recordings, so the differential
/// covers the synthetic hot-path streams as well as the captured sessions: the
/// corpus has no 24-bit SGR churn, no CJK-filled screen and no Sixel.
fn fixture_recordings(kib: usize, filter: Option<&str>) -> Vec<Recording> {
    let (columns, screen_lines) = FIXTURE_SIZE;
    oneterm_tools::bench::FIXTURES
        .iter()
        .filter(|fixture| filter.is_none_or(|needle| fixture.name.contains(needle)))
        .map(|fixture| Recording {
            name: fixture.name.to_owned(),
            dir: PathBuf::new(),
            bytes: (fixture.make)(kib * 1024),
            columns,
            screen_lines,
            history_size: 10_000,
        })
        .collect()
}

/// The 40 bytes of input around the end of the stream, escaped. The corpus is
/// replayed as one chunk, so there is no per-difference offset to point at; the
/// tail is what a human reads first when a whole recording diverges.
fn print_context(recording: &Recording, difference: &Difference) {
    let tail = recording.bytes.len().saturating_sub(40);
    let context: String = recording.bytes[tail..]
        .iter()
        .map(|&byte| match byte {
            0x20..=0x7e => (byte as char).to_string(),
            0x1b => "<ESC>".to_owned(),
            other => format!("<{other:02x}>"),
        })
        .collect();
    println!("|  |  |  | `{difference}` |");
    println!("|  |  |  | last 40 bytes: `{context}` |");
}
