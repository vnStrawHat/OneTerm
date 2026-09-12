//! The deviation grep: what the 45 recordings actually contain.
//!
//! Every correction in the two low-level designs (`C1`-`C11`) and the deviations
//! still marked "measure in `US-0072`" name a sequence. This module scans each
//! recording for those sequences and reports which recordings carry them, so the
//! "Affected recordings" columns are filled with a measured answer instead of a
//! guess.
//!
//! The scan runs at the **parser** level (`vte::Parser` plus a recording
//! `Perform`), not over raw bytes: a byte search for `\x1b[1J` misses
//! `\x1b[?25l\x1b[1J` split across a chunk boundary, counts `1J` inside an OSC
//! payload, and cannot tell `SGR 5` from the `5` in `SGR 38;5;n`. The state
//! machine gets all three right for free.
//!
//! What it deliberately does **not** do is emulate the terminal. Several
//! corrections only bite under a runtime condition — `C1` needs a `DCH` count
//! that reaches `cols - col`, `C2` needs the cursor on row 1 — and answering
//! those would mean reimplementing the engine this packet exists to measure.
//! The report names the recordings that *can* trigger each correction; the
//! packet that implements the correction writes the exact cells into that
//! recording's `expected-diffs.json`.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use alacritty_terminal::vte::{Params, Parser, Perform};

use crate::corpus::Recording;

/// Everything the corrections and the remaining "measure in `US-0072`"
/// deviations need to know about one recording.
#[derive(Debug, Default)]
pub struct Trace {
    /// `DCH` (`CSI Ps P`) counts, in order.
    pub delete_chars: Vec<u16>,
    /// `ED 1` (`CSI 1 J`) dispatches.
    pub erase_above: usize,
    /// `DECSTBM` (`CSI t;b r`) regions.
    pub scroll_regions: Vec<(u16, u16)>,
    /// `SU` / `SD` / `IL` / `DL` counts, in order.
    pub region_scrolls: Vec<(char, u16)>,
    /// `IRM` (`CSI 4 h`) set.
    pub insert_mode_set: usize,
    /// Printed characters outside the Basic Multilingual ASCII range, a cheap
    /// screen for the wide characters `C4` needs.
    pub non_ascii_printed: usize,
    /// `DSR` cursor-position reports (`CSI 6 n`).
    pub cursor_position_reports: usize,
    /// `RIS` (`ESC c`).
    pub full_resets: usize,
    /// Palette OSCs seen, by numeric code.
    pub palette_oscs: BTreeMap<u16, usize>,
    /// `OSC 4` dispatches whose colour-argument count is even (rejected by the
    /// engine being replaced, applied pair-by-pair after `C7`).
    pub osc4_even_arguments: usize,
    /// Private modes set (`CSI ? Ps h`).
    pub private_set: BTreeMap<u16, usize>,
    /// Private modes reset (`CSI ? Ps l`).
    pub private_reset: BTreeMap<u16, usize>,
    /// `DECSTR` (`CSI ! p`).
    pub soft_resets: usize,
    /// `CSI ? 5 W` (reset tab stops).
    pub tab_stop_resets: usize,
    /// Top-level `SGR` attributes 5, 6, 53 and 55, by parameter.
    pub blink_overline_sgr: BTreeMap<u16, usize>,
}

impl Perform for Trace {
    fn print(&mut self, c: char) {
        if !c.is_ascii() {
            self.non_ascii_printed += 1;
        }
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: char) {
        if ignore {
            return;
        }
        let private = intermediates.first() == Some(&b'?');
        let first = params.iter().next().and_then(|p| p.first().copied());

        match (private, intermediates.first().copied(), action) {
            (true, _, 'h') => {
                for param in params.iter().filter_map(|p| p.first().copied()) {
                    *self.private_set.entry(param).or_default() += 1;
                }
            }
            (true, _, 'l') => {
                for param in params.iter().filter_map(|p| p.first().copied()) {
                    *self.private_reset.entry(param).or_default() += 1;
                }
            }
            (true, _, 'W') if first == Some(5) => self.tab_stop_resets += 1,
            (false, Some(b'!'), 'p') => self.soft_resets += 1,
            (false, None, 'P') => self.delete_chars.push(first.unwrap_or(1).max(1)),
            (false, None, 'J') if first == Some(1) => self.erase_above += 1,
            (false, None, 'r') => {
                let mut values = params.iter().filter_map(|p| p.first().copied());
                let top = values.next().unwrap_or(0);
                let bottom = values.next().unwrap_or(0);
                self.scroll_regions.push((top, bottom));
            }
            (false, None, action @ ('S' | 'T' | 'L' | 'M')) => {
                self.region_scrolls
                    .push((action, first.unwrap_or(1).max(1)));
            }
            (false, None, 'h') if first == Some(4) => self.insert_mode_set += 1,
            (false, None, 'n') if first == Some(6) => self.cursor_position_reports += 1,
            (false, None, 'm') => self.trace_sgr(params),
            _ => {}
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        if !ignore && intermediates.is_empty() && byte == b'c' {
            self.full_resets += 1;
        }
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        let Some(code) = params.first().and_then(|raw| std::str::from_utf8(raw).ok()) else {
            return;
        };
        let Ok(code) = code.parse::<u16>() else {
            return;
        };
        if !matches!(code, 4 | 5 | 10..=19 | 104 | 105 | 110..=119) {
            return;
        }
        *self.palette_oscs.entry(code).or_default() += 1;
        // `OSC 4` carries index/colour pairs after the code, so the engine
        // being replaced requires an odd *total* parameter count and drops the
        // whole sequence otherwise (trap 26). An even total — a trailing
        // index with no colour — is the case `C7` changes.
        if code == 4 && params.len().is_multiple_of(2) {
            self.osc4_even_arguments += 1;
        }
    }
}

impl Trace {
    /// Walk `SGR` parameters, stepping over the arguments of an extended colour
    /// so `38;5;5` is not read as the blink attribute.
    fn trace_sgr(&mut self, params: &Params) {
        let flat: Vec<&[u16]> = params.iter().collect();
        let mut index = 0;
        while index < flat.len() {
            let group = flat[index];
            let Some(&value) = group.first() else {
                index += 1;
                continue;
            };
            if matches!(value, 38 | 48 | 58) {
                if group.len() > 1 {
                    // Colon form: the arguments live inside this group.
                    index += 1;
                    continue;
                }
                // Semicolon form: `5;<n>` is indexed, `2;<r>;<g>;<b>` is direct.
                let selector = flat.get(index + 1).and_then(|g| g.first().copied());
                index += match selector {
                    Some(5) => 3,
                    Some(2) => 5,
                    _ => 1,
                };
                continue;
            }
            if matches!(value, 5 | 6 | 53 | 55) {
                *self.blink_overline_sgr.entry(value).or_default() += 1;
            }
            index += 1;
        }
    }
}

/// Trace one recording.
pub fn trace(recording: &Recording) -> Trace {
    let mut trace = Trace::default();
    let mut parser = Parser::new();
    parser.advance(&mut trace, &recording.bytes);
    trace
}

/// One row of the recording-risk report.
pub struct RiskRow {
    /// Deviation or correction id.
    pub id: &'static str,
    /// What the id changes.
    pub what: &'static str,
    /// The sequences searched for.
    pub sequences: &'static str,
    /// Recording name plus the evidence found there.
    pub hits: Vec<(String, String)>,
}

/// Build the recording-risk report from a trace of every recording.
pub fn risk_report(traces: &[(String, Trace)]) -> Vec<RiskRow> {
    let collect = |what, sequences, id, probe: &dyn Fn(&Trace) -> Option<String>| RiskRow {
        id,
        what,
        sequences,
        hits: traces
            .iter()
            .filter_map(|(name, trace)| probe(trace).map(|evidence| (name.clone(), evidence)))
            .collect(),
    };

    vec![
        collect(
            "`DCH` is a plain shift left by `n`",
            "`CSI Ps P`",
            "C1",
            &|t| {
                (!t.delete_chars.is_empty()).then(|| {
                    format!(
                        "{} DCH, max count {}",
                        t.delete_chars.len(),
                        t.delete_chars.iter().max().copied().unwrap_or(0)
                    )
                })
            },
        ),
        collect("`ED 1` clears row 0", "`CSI 1 J`", "C2", &|t| {
            (t.erase_above > 0).then(|| format!("{} ED 1", t.erase_above))
        }),
        collect(
            "A short region scroll rotates, then blanks",
            "`CSI r` with `CSI S` / `T` / `L` / `M`",
            "C3",
            &|t| {
                (!t.scroll_regions.is_empty() && !t.region_scrolls.is_empty()).then(|| {
                    format!(
                        "regions {}, scrolls {}",
                        summarize_regions(&t.scroll_regions),
                        summarize_scrolls(&t.region_scrolls)
                    )
                })
            },
        ),
        collect(
            "Insert mode repairs wide pairs",
            "`CSI 4 h` plus a non-ASCII print",
            "C4",
            &|t| {
                (t.insert_mode_set > 0).then(|| {
                    format!(
                        "IRM set {}x, non-ASCII printed {}x",
                        t.insert_mode_set, t.non_ascii_printed
                    )
                })
            },
        ),
        collect(
            "`CPR` is region-relative under `DECOM`",
            "`CSI ? 6 h` plus `CSI 6 n`",
            "C5",
            &|t| {
                let origin = t.private_set.get(&6).copied().unwrap_or(0);
                (origin > 0 || t.cursor_position_reports > 0)
                    .then(|| format!("DECOM set {origin}x, CPR {}x", t.cursor_position_reports))
            },
        ),
        collect(
            "`RIS` resets the colour overrides",
            "`ESC c` plus a palette OSC",
            "C6",
            &|t| {
                (t.full_resets > 0 && !t.palette_oscs.is_empty())
                    .then(|| format!("RIS {}x, palette OSC {:?}", t.full_resets, t.palette_oscs))
            },
        ),
        collect(
            "`OSC 4` applies complete pairs",
            "`OSC 4` with an even colour-argument count",
            "C7",
            &|t| {
                (t.osc4_even_arguments > 0).then(|| {
                    format!(
                        "{} OSC 4 with an even argument count",
                        t.osc4_even_arguments
                    )
                })
            },
        ),
        collect(
            "`? 47` / `? 1047` / `? 1048` implemented",
            "`CSI ? 47 h/l`, `? 1047`, `? 1048`",
            "C8",
            &|t| private_hits(t, &[47, 1047, 1048]),
        ),
        collect("`DECSTR` implemented", "`CSI ! p`", "C9", &|t| {
            (t.soft_resets > 0).then(|| format!("{} DECSTR", t.soft_resets))
        }),
        collect(
            "`CSI ? 5 W` restores the default tab stops",
            "`CSI ? 5 W`",
            "C10",
            &|t| (t.tab_stop_resets > 0).then(|| format!("{} CSI ? 5 W", t.tab_stop_resets)),
        ),
        collect(
            "Blink and overline attributes stored",
            "`SGR 5`, `6`, `53`, `55`",
            "C11",
            &|t| {
                (!t.blink_overline_sgr.is_empty())
                    .then(|| format!("SGR {:?}", t.blink_overline_sgr))
            },
        ),
        collect(
            "Pending wrap is not armed while `DECAWM` is off",
            "`CSI ? 7 l`",
            "G3",
            &|t| private_hits(t, &[7]).filter(|_| t.private_reset.contains_key(&7)),
        ),
        collect("Reverse wrap implemented", "`CSI ? 45 h/l`", "D12", &|t| {
            private_hits(t, &[45])
        }),
    ]
}

fn private_hits(trace: &Trace, modes: &[u16]) -> Option<String> {
    let mut parts = Vec::new();
    for mode in modes {
        let set = trace.private_set.get(mode).copied().unwrap_or(0);
        let reset = trace.private_reset.get(mode).copied().unwrap_or(0);
        if set + reset > 0 {
            parts.push(format!("?{mode}: {set} set, {reset} reset"));
        }
    }
    (!parts.is_empty()).then(|| parts.join("; "))
}

fn summarize_regions(regions: &[(u16, u16)]) -> String {
    let mut counts: BTreeMap<(u16, u16), usize> = BTreeMap::new();
    for region in regions {
        *counts.entry(*region).or_default() += 1;
    }
    counts
        .into_iter()
        .map(|((top, bottom), times)| format!("{top}..{bottom} x{times}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn summarize_scrolls(scrolls: &[(char, u16)]) -> String {
    let mut counts: BTreeMap<char, (usize, u16)> = BTreeMap::new();
    for (action, count) in scrolls {
        let entry = counts.entry(*action).or_insert((0, 0));
        entry.0 += 1;
        entry.1 = entry.1.max(*count);
    }
    let mut out = String::new();
    for (action, (times, max)) in counts {
        if !out.is_empty() {
            out.push_str(", ");
        }
        let _ = write!(out, "{action}x{times} (max {max})");
    }
    out
}
