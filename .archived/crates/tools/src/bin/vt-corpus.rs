//! The VT parity corpus tool (developer diagnostic, never shipped).
//!
//! ```text
//! vt-corpus check [--filter <substring>] [--engine new] [--dir <corpus dir>]
//! vt-corpus grep-deviations [--out <file.md>]
//! ```
//!
//! `check` replays every recording through `oneterm-vt` and compares it, cell
//! for cell, against the frozen expectations, applying each recording's declared
//! `expected-diffs.json`. `cargo test -p oneterm-tools` runs the same
//! comparison, so this binary is for reading the diff, not for the gate.
//!
//! **Nothing blesses any more.** The expectations were produced by the vendored
//! `alacritty` fork at `US-0072`, reviewed once and frozen; `US-0087` deleted
//! that engine, so `bless` and the upstream `cross-check` went with it. The
//! engine under test never blessed and never will — if it did, the gate would
//! prove self-consistency and nothing else (R-58). A correction that must move a
//! frozen cell is declared in that recording's `expected-diffs.json` with a
//! deviation id, which is a reviewable diff with a stated reason.

use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Result, bail};
use oneterm_tools::corpus::{self, Recording, alacritty_ref_dir};
use oneterm_tools::corpus_grep;

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
    out: Option<PathBuf>,
    /// Which corpus directory to read; the vendored alacritty set by default.
    dir: Option<PathBuf>,
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
            "--dir" => args.dir = Some(PathBuf::from(value()?)),
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
        "vt-corpus check [--filter <substring>] [--engine new] [--dir <corpus dir>]\n\
         vt-corpus grep-deviations [--out <file.md>]\n\
         \n\
         --dir defaults to crates/tools/corpus/alacritty-ref; OneTerm's own\n\
         recordings live next to it under oneterm/.\n\
         --engine accepts only `new`: the engine that blessed the frozen files\n\
         was deleted at US-0087. The flag is kept so recorded commands still run."
    );
}

fn recordings(args: &Args) -> Result<Vec<Recording>> {
    let dir = args.dir.clone().unwrap_or_else(alacritty_ref_dir);
    let found = corpus::load_all(&dir, args.filter.as_deref())?;
    if found.is_empty() {
        bail!("no recordings under {} matched the filter", dir.display());
    }
    Ok(found)
}

/// There is one engine left, so the flag only has to reject the one that is
/// gone — with the reason, rather than "unknown value".
fn check_engine(args: &Args) -> Result<()> {
    match args.engine.as_deref() {
        None | Some("new") => Ok(()),
        Some("old") => bail!(
            "the old engine was deleted at US-0087; the frozen expectations are what it \
             left behind"
        ),
        Some(other) => bail!("unknown engine {other:?} (only `new` remains)"),
    }
}

fn check(args: &Args) -> Result<bool> {
    check_engine(args)?;
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
        // A filtered run is someone reading one recording's diff, so it prints
        // the lot; an unfiltered run is a summary and stays readable.
        let shown = if args.filter.is_some() { 2000 } else { 20 };
        for difference in report.undeclared.iter().take(shown) {
            println!("        {difference}");
        }
        if report.undeclared.len() > shown {
            println!("        ... and {} more", report.undeclared.len() - shown);
        }
    }
    println!(
        "\n{} recordings, {} passed, {failed} failed",
        recordings.len(),
        recordings.len() - failed
    );
    Ok(failed == 0)
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
            fs::write(path, &out)
                .map_err(|error| anyhow::anyhow!("writing {}: {error}", path.display()))?;
            println!("wrote {}", path.display());
        }
        None => print!("{out}"),
    }
    Ok(true)
}
