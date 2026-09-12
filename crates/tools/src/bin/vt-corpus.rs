//! The VT parity corpus tool (developer diagnostic, never shipped).
//!
//! ```text
//! vt-corpus check          [--filter <substring>] [--engine old]
//! vt-corpus bless --engine old [--deviation <id>] [--filter <substring>]
//! vt-corpus cross-check --grid-json <dir> [--filter <substring>]
//! vt-corpus grep-deviations [--out <file.md>]
//! ```
//!
//! `check` replays every recording through the engine being replaced and
//! compares it, cell for cell, against the frozen expectations, applying each
//! recording's declared `expected-diffs.json`. `cargo test -p oneterm-tools`
//! runs the same comparison, so this binary is for reading the diff, not for
//! the gate.
//!
//! `bless` is the only writer, and it is deliberately awkward: the expectations
//! were blessed by the **old** engine at `US-0072` and frozen, so overwriting
//! one needs `--deviation <id>` naming a row in the corrections or deviation
//! tables of `dispatch-and-modes.md` / `grid-and-scrollback.md`. The new engine
//! never blesses at all — if it did, the gate would prove self-consistency and
//! nothing else.

use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use oneterm_tools::corpus::{self, Engine, KNOWN_DEVIATIONS, Recording, alacritty_ref_dir};
use oneterm_tools::corpus_grep;
use oneterm_tools::corpus_replay;
use oneterm_tools::corpus_upstream;

fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("vt-corpus: {error:#}");
            ExitCode::FAILURE
        }
    }
}

#[derive(Default)]
struct Args {
    filter: Option<String>,
    engine: Option<String>,
    deviation: Option<String>,
    grid_json: Option<PathBuf>,
    out: Option<PathBuf>,
}

fn run() -> Result<bool> {
    let mut raw = std::env::args().skip(1);
    let command = raw.next().unwrap_or_else(|| "check".to_owned());
    let mut args = Args::default();
    while let Some(flag) = raw.next() {
        let mut value = || {
            raw.next()
                .ok_or_else(|| anyhow::anyhow!("{flag} needs a value"))
        };
        match flag.as_str() {
            "--filter" => args.filter = Some(value()?),
            "--engine" => args.engine = Some(value()?),
            "--deviation" => args.deviation = Some(value()?),
            "--grid-json" => args.grid_json = Some(PathBuf::from(value()?)),
            "--out" => args.out = Some(PathBuf::from(value()?)),
            "--help" | "-h" => {
                print_usage();
                return Ok(true);
            }
            other => bail!("unknown flag {other:?} (try --help)"),
        }
    }

    match command.as_str() {
        "check" | "replay" => check(&args),
        "bless" => bless(&args),
        "cross-check" => cross_check(&args),
        "grep-deviations" => grep_deviations(&args),
        "--help" | "-h" | "help" => {
            print_usage();
            Ok(true)
        }
        other => bail!("unknown command {other:?} (try --help)"),
    }
}

fn print_usage() {
    println!(
        "vt-corpus check          [--filter <substring>]\n\
         vt-corpus bless --engine old [--deviation <id> --filter <recording>]\n\
         vt-corpus cross-check --grid-json <dir> [--filter <substring>]\n\
         vt-corpus grep-deviations [--out <file.md>]"
    );
}

fn recordings(args: &Args) -> Result<Vec<Recording>> {
    let dir = alacritty_ref_dir();
    let found = corpus::load_all(&dir, args.filter.as_deref())?;
    if found.is_empty() {
        bail!("no recordings under {} matched the filter", dir.display());
    }
    Ok(found)
}

fn engine(args: &Args) -> Result<Engine> {
    args.engine
        .as_deref()
        .unwrap_or("old")
        .parse::<Engine>()
        .context("--engine")
}

fn check(args: &Args) -> Result<bool> {
    if engine(args)? == Engine::New {
        bail!("`--engine new` needs oneterm-vt, which arrives with US-0073");
    }
    let mut failed = 0;
    let recordings = recordings(args)?;
    for recording in &recordings {
        let report = corpus::check_recording(recording)?;
        if report.passed() {
            let accepted: usize = report.accepted.values().sum();
            let note = if accepted == 0 {
                String::new()
            } else {
                format!(" ({accepted} declared differences: {:?})", report.accepted)
            };
            println!("ok    {}{note}", recording.name);
            continue;
        }
        failed += 1;
        println!("FAIL  {}", recording.name);
        for stale in &report.stale {
            println!("        stale declared diff: {stale}");
        }
        for difference in report.undeclared.iter().take(20) {
            println!("        {difference}");
        }
        if report.undeclared.len() > 20 {
            println!("        ... and {} more", report.undeclared.len() - 20);
        }
    }
    println!(
        "\n{} recordings, {} passed, {failed} failed",
        recordings.len(),
        recordings.len() - failed
    );
    Ok(failed == 0)
}

fn bless(args: &Args) -> Result<bool> {
    match engine(args)? {
        Engine::New => bail!(
            "the new engine never blesses: the expectations are what it is measured against, \
             and a self-blessed gate proves only self-consistency (R-58)"
        ),
        Engine::Old => {}
    }
    if let Some(deviation) = args.deviation.as_deref() {
        if !KNOWN_DEVIATIONS.contains(&deviation) {
            bail!(
                "unknown deviation id {deviation:?}: it must name a row in the corrections or \
                 deviation tables of dispatch-and-modes.md or grid-and-scrollback.md"
            );
        }
        // A correction changes named recordings, never all of them. Without
        // this, one `--deviation` would rewrite all 45 expectations at once and
        // the reviewable diff the flag exists to produce would be the whole
        // corpus.
        if args.filter.is_none() {
            bail!(
                "--deviation needs --filter <recording>: re-blessing every recording under one \
                 deviation id is not a reviewable diff"
            );
        }
    }

    let mut written = 0;
    for recording in recordings(args)? {
        let grid_path = recording.grid_expect_path();
        let state_path = recording.state_expect_path();
        let frozen = grid_path.exists() || state_path.exists();
        if frozen && args.deviation.is_none() {
            bail!(
                "{} is already blessed and frozen; re-blessing needs --deviation <id> so the \
                 expectation diff is reviewable with a stated reason",
                recording.name
            );
        }

        let (grid, state) = corpus_replay::replay_old(&recording);
        fs::write(&grid_path, grid.encode())
            .with_context(|| format!("writing {}", grid_path.display()))?;
        fs::write(&state_path, state.encode())
            .with_context(|| format!("writing {}", state_path.display()))?;
        written += 1;
        println!(
            "blessed {} ({} rows, {} state entries)",
            recording.name,
            grid.rows.len(),
            state.entries.len()
        );
    }
    println!("\n{written} recordings blessed by the vendored engine");
    Ok(true)
}

fn cross_check(args: &Args) -> Result<bool> {
    let dir = args
        .grid_json
        .as_deref()
        .context("cross-check needs --grid-json <dir> holding upstream's <name>.json set")?;

    let mut mismatched = 0;
    let recordings = recordings(args)?;
    println!("| Recording | Rows | Result |");
    println!("| --- | ---: | --- |");
    for recording in &recordings {
        let (ours, _) = corpus_replay::replay_old(recording);
        let theirs = corpus_upstream::load_grid_json(dir, &recording.name)?;
        let differences = corpus::check(
            &theirs,
            &ours,
            &corpus::StateExpect::default(),
            &corpus::StateExpect::default(),
            &corpus::ExpectedDiffs::default(),
        )?;
        if differences.passed() {
            println!("| `{}` | {} | match |", recording.name, ours.rows.len());
        } else {
            mismatched += 1;
            let first = differences
                .undeclared
                .first()
                .map(ToString::to_string)
                .unwrap_or_default();
            println!(
                "| `{}` | {} | **{} differences**, first: {} |",
                recording.name,
                ours.rows.len(),
                differences.undeclared.len(),
                first
            );
        }
    }
    println!(
        "\n{} recordings, {} match upstream `grid.json`, {mismatched} differ",
        recordings.len(),
        recordings.len() - mismatched
    );
    Ok(mismatched == 0)
}

fn grep_deviations(args: &Args) -> Result<bool> {
    let traces: Vec<(String, corpus_grep::Trace)> = recordings(args)?
        .iter()
        .map(|recording| (recording.name.clone(), corpus_grep::trace(recording)))
        .collect();

    let mut out = String::new();
    out.push_str(
        "| Id | Correction or deviation | Sequences searched | Recordings that carry them |\n",
    );
    out.push_str("| --- | --- | --- | --- |\n");
    for row in corpus_grep::risk_report(&traces) {
        let hits = if row.hits.is_empty() {
            "none of the 45".to_owned()
        } else {
            row.hits
                .iter()
                .map(|(name, evidence)| format!("`{name}` ({evidence})"))
                .collect::<Vec<_>>()
                .join("; ")
        };
        out.push_str(&format!(
            "| {} | {} | {} | {hits} |\n",
            row.id, row.what, row.sequences
        ));
    }

    match args.out.as_deref() {
        Some(path) => {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).ok();
            }
            fs::write(path, &out).with_context(|| format!("writing {}", path.display()))?;
            println!("wrote {}", path.display());
        }
        None => print!("{out}"),
    }
    Ok(true)
}
