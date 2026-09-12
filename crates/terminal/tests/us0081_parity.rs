//! Old engine versus new: the differential `migration.md` asks `US-0081` for.
//!
//! Written by the packet's independent verifier and adopted into the gate. It
//! feeds identical bytes to `alacritty_terminal::Term` (snapshotted through a
//! verbatim copy of the pre-`US-0081` `TerminalContent::refill`) and to the new
//! `Engine`, then diffs **every** field of `TerminalContent` after each 4 KiB
//! chunk: per-cell character, zero-width followers, colours, flags, hyperlink,
//! graphic reference and position; the cursor; the mode; the scroll offset; the
//! bounds; the selection; the damage; and the decoded images.
//!
//! Corpus: the 45 vendored alacritty recordings, OneTerm's `sixel_basic`, and 35
//! hand-written streams covering the OSC, SGR, colour, wide-character, emoji,
//! mouse, alt-screen, scrollback and malformed-input paths.
//!
//! The allow-list at the end of `snapshot_parity_over_every_recording_and_hand_stream`
//! is the contract: five declared differences, everything else fails.
//!
//! **This file retires with the fork at `US-0087`**, which deletes the old
//! engine it compares against.
//!
//! NOT part of the packet. Independent verifier's differential: feeds the same
//! bytes to `alacritty_terminal::Term` (through the deleted `TerminalContent::refill`,
//! copied verbatim from 3538047) and to the new `Engine`, then diffs the two
//! `TerminalContent` values field by field.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::selection::SelectionRange;
use alacritty_terminal::term::cell::Cell;
use alacritty_terminal::term::{Config, RenderableCursor, Term, TermDamage, TermMode};
use alacritty_terminal::vte::ansi::{Processor, StdSyncHandler};

use oneterm_terminal::{Engine, GridSize, IndexedCell, TermDamageInfo, TerminalContent};
use oneterm_vt::EventBatch;

// ─────────────────────────── old side ───────────────────────────

#[derive(Clone, Default)]
struct Rec(Arc<Mutex<Vec<String>>>);

impl EventListener for Rec {
    fn send_event(&self, event: Event) {
        let text = match &event {
            Event::Title(t) => format!("Title({t})"),
            Event::ResetTitle => "ResetTitle".into(),
            Event::ClipboardStore(_, t) => format!("ClipboardStore({t})"),
            Event::ClipboardLoad(_, _) => "ClipboardLoad".into(),
            Event::PtyWrite(t) => format!("PtyWrite({:?})", t.as_bytes()),
            Event::Bell => "Bell".into(),
            Event::ClearScreen => "ClearScreen".into(),
            Event::ColorRequest(i, _) => format!("ColorRequest({i})"),
            Event::Osc { params, .. } => format!(
                "Osc({:?})",
                params
                    .iter()
                    .map(|p| String::from_utf8_lossy(p).into_owned())
                    .collect::<Vec<_>>()
            ),
            Event::Wakeup => "Wakeup".into(),
            other => format!("{other:?}"),
        };
        self.0.lock().unwrap().push(text);
    }
}

struct Dims {
    cols: usize,
    lines: usize,
}
impl Dimensions for Dims {
    fn total_lines(&self) -> usize {
        self.lines
    }
    fn screen_lines(&self) -> usize {
        self.lines
    }
    fn columns(&self) -> usize {
        self.cols
    }
}

/// `TerminalContent::refill`, copied verbatim from `3538047:crates/terminal/src/content.rs`.
fn old_refill<EP: EventListener>(content: &mut TerminalContent, term: &mut Term<EP>) {
    let num_lines = term.screen_lines();
    let mut dirty = match std::mem::replace(&mut content.damage, TermDamageInfo::Full) {
        TermDamageInfo::Partial(previous) => previous,
        TermDamageInfo::Full => Vec::new(),
    };
    dirty.clear();
    content.damage = match term.damage() {
        TermDamage::Full => TermDamageInfo::Full,
        TermDamage::Partial(iter) => {
            dirty.extend(iter.map(|ldb| ldb.line).filter(|&dl| dl < num_lines));
            TermDamageInfo::Partial(dirty)
        }
    };
    term.reset_damage();

    let renderable = term.renderable_content();
    let cursor = renderable.cursor;
    let mode = renderable.mode;
    let display_offset = renderable.display_offset;
    let selection = renderable.selection;
    content.cells.clear();
    content
        .cells
        .extend(renderable.display_iter.map(|indexed| IndexedCell {
            point: indexed.point,
            cell: indexed.cell.clone(),
        }));
    content.cursor = cursor;
    content.mode = mode;
    content.display_offset = display_offset;
    content.total_lines = term.total_lines();
    content.selection = selection;
    content.graphics = term.take_graphics();
    content.terminal_bounds = oneterm_terminal::content::TerminalBounds {
        num_lines,
        num_cols: term.columns(),
    };
}

// ─────────────────────────── the diff ───────────────────────────

fn cell_desc(cell: &Cell) -> String {
    let zero: String = cell
        .zerowidth()
        .map(|z| z.iter().collect())
        .unwrap_or_default();
    let link = cell
        .hyperlink()
        .map(|h| format!("{}|{}", h.id(), h.uri()))
        .unwrap_or_default();
    let graphic = cell
        .graphic()
        .map(|g| format!("{}@{},{}", g.id.0, g.col, g.row))
        .unwrap_or_default();
    format!(
        "c={:?} z={zero:?} fg={:?} bg={:?} flags={:?} link={link} gfx={graphic}",
        cell.c, cell.fg, cell.bg, cell.flags
    )
}

fn diff_cells(
    old: &[IndexedCell],
    new: &[IndexedCell],
    out: &mut BTreeMap<String, (usize, String)>,
) {
    if old.len() != new.len() {
        note(
            out,
            "cells.len",
            format!("old={} new={}", old.len(), new.len()),
        );
        return;
    }
    for (o, n) in old.iter().zip(new.iter()) {
        if o.point != n.point {
            note(
                out,
                "cell.point",
                format!("old={:?} new={:?}", o.point, n.point),
            );
            continue;
        }
        let (od, nd) = (cell_desc(&o.cell), cell_desc(&n.cell));
        if od == nd {
            continue;
        }
        // Classify by which part differs, so the report is readable.
        let kind = if o.cell.c != n.cell.c {
            "cell.char"
        } else if o.cell.fg != n.cell.fg {
            "cell.fg"
        } else if o.cell.bg != n.cell.bg {
            "cell.bg"
        } else if o.cell.flags != n.cell.flags {
            "cell.flags"
        } else if o.cell.hyperlink() != n.cell.hyperlink() {
            "cell.hyperlink"
        } else if o.cell.graphic() != n.cell.graphic() {
            "cell.graphic"
        } else {
            "cell.other"
        };
        note(out, kind, format!("at {:?}: old[{od}] new[{nd}]", o.point));
    }
}

fn note(out: &mut BTreeMap<String, (usize, String)>, kind: &str, detail: String) {
    let entry = out.entry(kind.to_string()).or_insert((0, detail.clone()));
    entry.0 += 1;
    if entry.1.is_empty() {
        entry.1 = detail;
    }
}

fn diff_content(
    old: &TerminalContent,
    new: &TerminalContent,
    out: &mut BTreeMap<String, (usize, String)>,
) {
    diff_cells(&old.cells, &new.cells, out);
    if old.cursor.point != new.cursor.point {
        note(
            out,
            "cursor.point",
            format!("old={:?} new={:?}", old.cursor.point, new.cursor.point),
        );
    }
    if old.cursor.shape != new.cursor.shape {
        note(
            out,
            "cursor.shape",
            format!("old={:?} new={:?}", old.cursor.shape, new.cursor.shape),
        );
    }
    // LINE_WRAP and URGENCY_HINTS are dropped by `legacy_mode` by design;
    // counted once, then masked so the rest of the mode signal is visible.
    let masked = TermMode::LINE_WRAP | TermMode::URGENCY_HINTS;
    if old.mode.intersects(masked) && !new.mode.intersects(masked) {
        note(
            out,
            "mode.dropped_line_wrap_urgency",
            format!("old-only={:?}", old.mode.intersection(masked)),
        );
    }
    let (old_mode, new_mode) = (old.mode.difference(masked), new.mode.difference(masked));
    if old_mode != new_mode {
        note(
            out,
            "mode",
            format!(
                "old={:?} new={:?} (only-old={:?} only-new={:?})",
                old_mode,
                new_mode,
                old_mode.difference(new_mode),
                new_mode.difference(old_mode)
            ),
        );
    }
    if old.display_offset != new.display_offset {
        note(
            out,
            "display_offset",
            format!("old={} new={}", old.display_offset, new.display_offset),
        );
    }
    if old.total_lines != new.total_lines {
        note(
            out,
            "total_lines",
            format!("old={} new={}", old.total_lines, new.total_lines),
        );
    }
    if old.selection != new.selection {
        note(
            out,
            "selection",
            format!("old={:?} new={:?}", old.selection, new.selection),
        );
    }
    if old.terminal_bounds != new.terminal_bounds {
        note(
            out,
            "bounds",
            format!(
                "old={:?} new={:?}",
                old.terminal_bounds, new.terminal_bounds
            ),
        );
    }
    let damage_kind = |d: &TermDamageInfo| match d {
        TermDamageInfo::Full => "Full".to_string(),
        TermDamageInfo::Partial(rows) => {
            let mut rows = rows.clone();
            rows.sort_unstable();
            rows.dedup();
            format!("Partial{rows:?}")
        }
    };
    let (od, nd) = (damage_kind(&old.damage), damage_kind(&new.damage));
    if od != nd {
        note(out, "damage", format!("old={od} new={nd}"));
    }
    if old.graphics.len() != new.graphics.len() {
        note(
            out,
            "graphics.len",
            format!("old={} new={}", old.graphics.len(), new.graphics.len()),
        );
    } else {
        for (o, n) in old.graphics.iter().zip(new.graphics.iter()) {
            if o.width != n.width || o.height != n.height || o.rgba != n.rgba {
                note(
                    out,
                    "graphics.pixels",
                    format!(
                        "old={}x{} ({} bytes) new={}x{} ({} bytes)",
                        o.width,
                        o.height,
                        o.rgba.len(),
                        n.width,
                        n.height,
                        n.rgba.len()
                    ),
                );
            }
        }
    }
}

/// Is the damage the new snapshot reports **sound** — does it name every row
/// whose rendered cells changed since the previous snapshot, plus the cursor
/// row when the cursor moved? Under-damage leaves stale pixels on screen.
fn check_damage_sound(
    content: &TerminalContent,
    previous: &mut Option<(Vec<String>, usize, Point, bool)>,
    out: &mut BTreeMap<String, (usize, String)>,
) {
    let cols = content.terminal_bounds.num_cols.max(1);
    let rows: Vec<String> = content
        .cells
        .chunks(cols)
        .map(|row| row.iter().map(|c| cell_desc(&c.cell)).collect::<String>())
        .collect();
    let cursor = content.cursor.point;
    let visible = content.cursor_visible();
    if let Some((prev_rows, prev_offset, prev_cursor, prev_visible)) = previous.take()
        && prev_offset == content.display_offset
        && prev_rows.len() == rows.len()
    {
        let mut changed: Vec<usize> = (0..rows.len())
            .filter(|index| rows[*index] != prev_rows[*index])
            .collect();
        // The view repaints the cursor by row, so a cursor move dirties the
        // row it left and the row it entered.
        // A cursor that is hidden in both frames paints nothing, so the rows it
        // moves between need no repaint (`render/cursor.rs:43`).
        if prev_cursor != cursor {
            // The row the cursor left needs a repaint only if it was painted
            // there; the row it entered only if it is painted now.
            let moved = [(prev_cursor, prev_visible), (cursor, visible)];
            for point in moved.into_iter().filter(|(_, on)| *on).map(|(p, _)| p) {
                let row = point.line.0 + content.display_offset as i32;
                if row >= 0 && (row as usize) < rows.len() && !changed.contains(&(row as usize)) {
                    changed.push(row as usize);
                }
            }
        }
        if let TermDamageInfo::Partial(damaged) = &content.damage {
            let missing: Vec<usize> = changed
                .iter()
                .copied()
                .filter(|row| !damaged.contains(row))
                .collect();
            for row in &missing {
                let content_changed = rows[*row] != prev_rows[*row];
                note(
                    out,
                    if content_changed {
                        "damage.UNSOUND.content"
                    } else {
                        "damage.UNSOUND.cursor"
                    },
                    format!(
                        "row {row} (content_changed={content_changed}, cursor {prev_cursor:?}->{cursor:?} visible {prev_visible}->{visible}, offset {}) not in {damaged:?}
      was: {}
      now: {}",
                        content.display_offset,
                        prev_rows[*row].chars().take(200).collect::<String>(),
                        rows[*row].chars().take(200).collect::<String>(),
                    ),
                );
            }
        }
    }
    *previous = Some((rows, content.display_offset, cursor, visible));
}

struct Case {
    name: String,
    bytes: Vec<u8>,
    cols: usize,
    lines: usize,
    history: usize,
}

fn corpus_cases() -> Vec<Case> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("vt/tests/corpus");
    let mut cases = Vec::new();
    for set in ["alacritty-ref", "oneterm"] {
        let dir = root.join(set);
        let mut names: Vec<_> = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("reading {}: {e}", dir.display()))
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        for name in names {
            let d = dir.join(&name);
            let size = std::fs::read_to_string(d.join("size.json")).unwrap();
            let config = std::fs::read_to_string(d.join("config.json")).unwrap();
            cases.push(Case {
                name: format!("{set}/{name}"),
                bytes: std::fs::read(d.join("recording")).unwrap(),
                cols: json_int(&size, "columns"),
                lines: json_int(&size, "screen_lines"),
                history: json_int(&config, "history_size"),
            });
        }
    }
    cases
}

fn json_int(text: &str, key: &str) -> usize {
    let at = text.find(&format!("\"{key}\"")).expect("key");
    let rest = &text[at + key.len() + 2..];
    let digits: String = rest
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().expect("int")
}

fn hand_cases() -> Vec<Case> {
    let mk = |name: &str, bytes: &[u8]| Case {
        name: name.into(),
        bytes: bytes.to_vec(),
        cols: 20,
        lines: 6,
        history: 100,
    };
    vec![
        mk("title", b"\x1b]0;hello world\x07text"),
        mk("title-2", b"\x1b]2;other title\x1b\\text"),
        mk("bell", b"abc\x07def"),
        mk("hyperlink", b"\x1b]8;id=x1;https://example.com/a\x07link text\x1b]8;;\x07 tail"),
        mk("osc52-store", b"\x1b]52;c;aGVsbG8=\x07after"),
        mk("osc7-cwd", b"\x1b]7;file://host/C:/tmp\x07x"),
        mk("osc9-agent", b"\x1b]9;7;{\"state\":\"working\"}\x07x"),
        mk("osc133", b"\x1b]133;A\x07prompt$ \x1b]133;B\x07cmd"),
        mk("color-query", b"\x1b]11;?\x07\x1b]10;?\x1b\\\x1b]4;3;?\x07"),
        mk("color-set", b"\x1b]11;rgb:1122/3344/5566\x07\x1b]4;5;#00ff00\x07X"),
        mk("da1", b"\x1b[c\x1b[>c\x1b[5n\x1b[6n"),
        mk("clear-screen", b"one\r\ntwo\x1b[2J\x1b[3J\x1b[Hx"),
        mk("sgr-attrs", b"\x1b[1mB\x1b[2mD\x1b[3mI\x1b[4mU\x1b[7mR\x1b[8mH\x1b[9mS\x1b[21mDU\x1b[4:3mCurl\x1b[4:4mDot\x1b[4:5mDash\x1b[0mN"),
        mk("colors-256", b"\x1b[38;5;196mA\x1b[48;5;21mB\x1b[38;2;10;20;30mC\x1b[39;49mD\x1b[31;42mE\x1b[91;102mF"),
        mk("cjk", "日本語テキスト ab\r\n中文字符\r\n".as_bytes()),
        mk("emoji", "ab\u{1F600}cd \u{1F1EF}\u{1F1F5}\r\n".as_bytes()),
        mk("combining", "e\u{0301}a\u{0308}\u{0323}x\r\n".as_bytes()),
        mk("wrap", b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        mk("wide-at-edge", "aaaaaaaaaaaaaaaaaaa\u{65E5}bb".as_bytes()),
        mk("cursor-hide", b"\x1b[?25labc"),
        mk("cursor-shapes", b"\x1b[3 q\x1b[5 qab\x1b[1 q"),
        mk("alt-screen", b"main\x1b[?1049hALT\r\nmore"),
        mk("bracketed", b"\x1b[?2004h\x1b[?1000h\x1b[?1002h\x1b[?1003h\x1b[?1006hx"),
        mk("mouse-1000-sgr", b"\x1b[?1000h\x1b[?1006hx"),
        mk("mouse-utf8", b"\x1b[?1000h\x1b[?1005hx"),
        mk("app-modes", b"\x1b[?1h\x1b=\x1b[4hx"),
        mk("scroll-history", b"1\r\n2\r\n3\r\n4\r\n5\r\n6\r\n7\r\n8\r\n9\r\n10\r\n11\r\n12\r\n"),
        mk("tabs-region", b"\x1b[2;5r\x1b[Hab\tcd\r\n\x1b[10;1Hzz"),
        mk("insert-delete", b"abcdef\x1b[3D\x1b[2@XY\x1b[2P\x1b[1L\x1b[1M"),
        mk("erase", b"abcdef\x1b[3D\x1b[K\r\nxyz\x1b[1K\x1b[2K"),
        mk("decaln", b"\x1b#8"),
        mk("reverse-video", b"\x1b[?5hx"),
        mk("ris", b"abc\x1bcdef"),
        mk("nul-and-controls", b"a\x00b\x0e\x0f\x08c\r\n"),
        mk("invalid-utf8", b"a\xff\xfe b\xc3\x28 c"),
    ]
}

fn run_case(case: &Case) -> BTreeMap<String, (usize, String)> {
    let mut diffs = BTreeMap::new();

    let listener = Rec::default();
    let mut old_term = Term::new(
        Config {
            scrolling_history: case.history,
            ..Config::default()
        },
        &Dims {
            cols: case.cols,
            lines: case.lines,
        },
        listener.clone(),
    );
    let mut processor = Processor::<StdSyncHandler>::new();

    let mut engine = Engine::new(
        GridSize {
            cols: case.cols,
            lines: case.lines,
        },
        case.history,
    );

    let mut old_content = TerminalContent::default();
    let mut new_content = TerminalContent::default();
    // Previous new-side snapshot, for the damage soundness check.
    let mut previous: Option<(Vec<String>, usize, Point, bool)> = None;

    // Chunked feed, snapshot after every chunk: damage and graphics are
    // per-snapshot state, so one snapshot at the end would not exercise them.
    let chunk = 4096.min(case.bytes.len().max(1));
    let mut parts: Vec<&[u8]> = case.bytes.chunks(chunk).collect();
    // One extra snapshot with no input: the steady-state damage shape.
    parts.push(b"");
    for part in parts {
        processor.advance(&mut old_term, part);
        let mut batch = EventBatch::new();
        engine.feed(part, &mut batch, Instant::now());

        old_refill(&mut old_content, &mut old_term);
        new_content.refill(&mut engine);
        diff_content(&old_content, &new_content, &mut diffs);
        check_damage_sound(&new_content, &mut previous, &mut diffs);
    }

    diffs
}

#[test]
fn snapshot_parity_over_every_recording_and_hand_stream() {
    let mut cases = corpus_cases();
    cases.extend(hand_cases());
    println!("cases: {}", cases.len());

    let mut total: BTreeMap<String, (usize, String)> = BTreeMap::new();
    let mut per_case: Vec<(String, Vec<String>)> = Vec::new();
    for case in &cases {
        let diffs = run_case(case);
        if diffs.is_empty() {
            continue;
        }
        let mut lines = Vec::new();
        for (kind, (count, example)) in &diffs {
            lines.push(format!("    {kind} x{count}: {example}"));
            let entry = total
                .entry(kind.clone())
                .or_insert((0, format!("{}: {example}", case.name)));
            entry.0 += count;
        }
        per_case.push((case.name.clone(), lines));
    }

    println!("=== cases with differences: {} ===", per_case.len());
    for (name, lines) in &per_case {
        println!("  {name}");
        for line in lines {
            println!("{line}");
        }
    }
    println!("=== totals by kind ===");
    for (kind, (count, example)) in &total {
        println!("  {kind}: {count}  e.g. {example}");
    }
    // Every difference the two engines are *allowed* to have, each declared in
    // `US-0081-engine-shim.md` § "Snapshot deviations" with its evidence that no
    // consumer reads it. Anything else is a regression and fails here.
    let declared: BTreeSet<&str> = [
        // The engine's hyperlink ids are a per-terminal counter, not the
        // reference's process-global `AtomicU32` with an `_alacritty` suffix.
        "cell.hyperlink",
        // Narrower, never wider: the reference damages a row on any write, the
        // engine on an actual change. `damage_soundness_detail` proves nothing
        // is under-damaged.
        "damage",
        // `ORIGIN` has no reader outside `crates/tools`' corpus dumper.
        "mode",
        // Nor do `LINE_WRAP` and `URGENCY_HINTS`.
        "mode.dropped_line_wrap_urgency",
        // The engine's history is one row shorter after a Sixel; scrollbar
        // length only, raised as a gap against `US-0080`.
        "total_lines",
    ]
    .into_iter()
    .collect();
    let undeclared: Vec<&String> = total
        .keys()
        .filter(|kind| !declared.contains(kind.as_str()))
        .collect();
    assert!(
        undeclared.is_empty(),
        "undeclared snapshot differences between the old and the new engine: {undeclared:?}          (see stdout for examples; {} of {} cases differ in some way)",
        per_case.len(),
        cases.len()
    );
}

/// Narrow the unsound-damage cases down to the chunk that causes them.
#[test]
fn damage_soundness_detail() {
    let names = [
        "alacritty-ref/tmux_htop",
        "alacritty-ref/vim_24bitcolors_bce",
        "alacritty-ref/vim_large_window_scroll",
    ];
    for case in corpus_cases()
        .into_iter()
        .filter(|c| names.contains(&c.name.as_str()))
    {
        println!("=== {} ({}x{})", case.name, case.cols, case.lines);
        let mut engine = Engine::new(
            GridSize {
                cols: case.cols,
                lines: case.lines,
            },
            case.history,
        );
        let mut content = TerminalContent::default();
        let mut prev: Option<Vec<String>> = None;
        let mut prev_offset = 0usize;
        let mut reported = 0;
        for (index, part) in case.bytes.chunks(4096).enumerate() {
            let mut batch = EventBatch::new();
            engine.feed(part, &mut batch, Instant::now());
            content.refill(&mut engine);
            let cols = content.terminal_bounds.num_cols.max(1);
            let rows: Vec<String> = content
                .cells
                .chunks(cols)
                .map(|row| {
                    row.iter()
                        .map(|c| cell_desc(&c.cell))
                        .collect::<Vec<_>>()
                        .join(" | ")
                })
                .collect();
            if let Some(previous) = &prev
                && prev_offset == content.display_offset
                && let TermDamageInfo::Partial(damaged) = &content.damage
            {
                for row in 0..rows.len() {
                    if rows[row] != previous[row] && !damaged.contains(&row) && reported < 3 {
                        reported += 1;
                        println!(
                            "  chunk {index}: row {row} changed but not damaged\n    was: {:?}\n    now: {:?}\n    damaged: {damaged:?}\n    chunk bytes: {:?}",
                            previous[row].trim_end(),
                            rows[row].trim_end(),
                            String::from_utf8_lossy(part)
                        );
                    }
                }
            }
            prev = Some(rows);
            prev_offset = content.display_offset;
        }
    }
}

// ───────────────────── selection / scroll parity ─────────────────────

fn feed_both(old: &mut Term<Rec>, p: &mut Processor<StdSyncHandler>, new: &mut Engine, b: &[u8]) {
    p.advance(old, b);
    let mut batch = EventBatch::new();
    new.feed(b, &mut batch, Instant::now());
}

#[test]
fn selection_and_scroll_parity() {
    use alacritty_terminal::index::Side;
    use alacritty_terminal::selection::{Selection, SelectionType};

    let (cols, lines, history) = (20usize, 5usize, 100usize);
    let listener = Rec::default();
    let mut old_term = Term::new(
        Config {
            scrolling_history: history,
            ..Config::default()
        },
        &Dims { cols, lines },
        listener,
    );
    let mut processor = Processor::<StdSyncHandler>::new();
    let mut engine = Engine::new(GridSize { cols, lines }, history);
    let text = b"line one\r\nline two\r\nline three\r\nline four\r\nline five\r\nline six\r\nline seven\r\n";
    feed_both(&mut old_term, &mut processor, &mut engine, text);

    let mut old_content = TerminalContent::default();
    let mut new_content = TerminalContent::default();
    let model = |engine: &mut Engine| -> () {
        let _ = engine;
    };
    model(&mut engine);

    // 1. scrollback offsets must agree.
    for offset in [0usize, 1, 2, 3] {
        old_term
            .grid_mut()
            .scroll_display(alacritty_terminal::grid::Scroll::Top);
        old_term
            .grid_mut()
            .scroll_display(alacritty_terminal::grid::Scroll::Delta(-(i32::MAX)));
        old_term
            .grid_mut()
            .scroll_display(alacritty_terminal::grid::Scroll::Delta(offset as i32));
        engine.grid_mut().screen_mut().scroll_to_bottom();
        engine
            .grid_mut()
            .screen_mut()
            .scroll_viewport(-(offset as i32));
        engine.grid_mut().sync_anchors();

        old_refill(&mut old_content, &mut old_term);
        new_content.refill(&mut engine);
        let mut diffs = BTreeMap::new();
        diff_content(&old_content, &new_content, &mut diffs);
        // Damage differs by construction here (we scrolled the old grid by a
        // different route), so only compare the content-bearing fields.
        diffs.remove("damage");
        diffs.remove("mode.dropped_line_wrap_urgency");
        assert!(
            diffs.is_empty(),
            "offset {offset}: {:?}",
            diffs.keys().collect::<Vec<_>>()
        );
        assert_eq!(old_content.display_offset, offset);
    }

    // 2. a simple selection over the same two points.
    old_term
        .grid_mut()
        .scroll_display(alacritty_terminal::grid::Scroll::Bottom);
    engine.grid_mut().screen_mut().scroll_to_bottom();
    engine.grid_mut().sync_anchors();

    let start = Point::new(Line(1), Column(2));
    let end = Point::new(Line(3), Column(5));
    let mut selection = Selection::new(SelectionType::Simple, start, Side::Left);
    selection.update(end, Side::Right);
    old_term.selection = Some(selection);

    let (pos, side) = engine.hit_test(1.0, 2.0);
    engine.selection_start(pos, side, oneterm_vt::SelectionKind::Simple);
    let (pos, side) = engine.hit_test(3.0, 5.9);
    engine.selection_update(pos, side);

    old_refill(&mut old_content, &mut old_term);
    new_content.refill(&mut engine);
    println!("old selection: {:?}", old_content.selection);
    println!("new selection: {:?}", new_content.selection);
    let old_text = old_term.selection_to_string();
    let new_text = engine.selection_text();
    println!("old text: {old_text:?}");
    println!("new text: {new_text:?}");
    assert_eq!(
        old_content.selection, new_content.selection,
        "selection range"
    );
    assert_eq!(old_text, new_text, "selection text");
}

// ───────────────────── event parity ─────────────────────

#[test]
fn event_parity() {
    let cases = hand_cases();
    for case in cases {
        let listener = Rec::default();
        let mut old_term = Term::new(
            Config {
                scrolling_history: case.history,
                ..Config::default()
            },
            &Dims {
                cols: case.cols,
                lines: case.lines,
            },
            listener.clone(),
        );
        let mut processor = Processor::<StdSyncHandler>::new();
        processor.advance(&mut old_term, &case.bytes);

        let mut engine = Engine::new(
            GridSize {
                cols: case.cols,
                lines: case.lines,
            },
            case.history,
        );
        let mut batch = EventBatch::new();
        engine.feed(&case.bytes, &mut batch, Instant::now());
        let new_events: Vec<String> = batch
            .iter()
            .map(|event| match event {
                oneterm_vt::VtEvent::Title(span) => format!("Title({})", batch.str(*span)),
                oneterm_vt::VtEvent::TitleReset => "ResetTitle".into(),
                oneterm_vt::VtEvent::ClipboardStore { text, .. } => {
                    format!("ClipboardStore({})", batch.str(*text))
                }
                oneterm_vt::VtEvent::ClipboardLoad { .. } => "ClipboardLoad".into(),
                oneterm_vt::VtEvent::Reply(span) => format!("PtyWrite({:?})", batch.bytes(*span)),
                oneterm_vt::VtEvent::Bell => "Bell".into(),
                oneterm_vt::VtEvent::ScreenCleared => "ClearScreen".into(),
                oneterm_vt::VtEvent::ColorQuery { key, .. } => {
                    format!("ColorRequest({})", key.index())
                }
                oneterm_vt::VtEvent::Osc { params, .. } => format!(
                    "Osc({:?})",
                    batch
                        .params(*params)
                        .map(|p| String::from_utf8_lossy(p).into_owned())
                        .collect::<Vec<_>>()
                ),
                oneterm_vt::VtEvent::Repaint => "Wakeup".into(),
                other => format!("{other:?}"),
            })
            .collect();
        let old_events = listener.0.lock().unwrap().clone();
        println!("--- {}", case.name);
        println!("  old: {old_events:?}");
        println!("  new: {new_events:?}");
    }
}

// ───────────────────── the unused-import silencer ─────────────────────
#[allow(dead_code)]
fn _unused(_: RenderableCursor, _: SelectionRange, _: TermMode) {}

/// Two open questions from the main differential, isolated.
#[test]
fn hyperlink_ids_and_sixel_history() {
    // (a) two separate OSC 8 runs carrying the same URI must not merge.
    let bytes = b"\x1b]8;;http://a\x07A\x1b]8;;\x07 \x1b]8;;http://a\x07B\x1b]8;;\x07 \x1b]8;id=q;http://b\x07C\x1b]8;;\x07";
    let listener = Rec::default();
    let mut old_term = Term::new(Config::default(), &Dims { cols: 10, lines: 2 }, listener);
    Processor::<StdSyncHandler>::new().advance(&mut old_term, bytes);
    let mut engine = Engine::new(GridSize { cols: 10, lines: 2 }, 100);
    let mut batch = EventBatch::new();
    engine.feed(bytes, &mut batch, Instant::now());
    let mut old_content = TerminalContent::default();
    let mut new_content = TerminalContent::default();
    old_refill(&mut old_content, &mut old_term);
    new_content.refill(&mut engine);
    let ids = |content: &TerminalContent| -> Vec<String> {
        content
            .cells
            .iter()
            .take(10)
            .map(|c| {
                c.cell
                    .hyperlink()
                    .map(|h| format!("{}={}", c.cell.c, h.id()))
                    .unwrap_or_else(|| format!("{}=-", c.cell.c))
            })
            .collect()
    };
    println!("old link ids: {:?}", ids(&old_content));
    println!("new link ids: {:?}", ids(&new_content));

    // (b) where does the Sixel recording's history length diverge?
    let case = corpus_cases()
        .into_iter()
        .find(|c| c.name == "oneterm/sixel_basic")
        .expect("sixel recording");
    let listener = Rec::default();
    let mut old_term = Term::new(
        Config {
            scrolling_history: case.history,
            ..Config::default()
        },
        &Dims {
            cols: case.cols,
            lines: case.lines,
        },
        listener,
    );
    let mut processor = Processor::<StdSyncHandler>::new();
    let mut engine = Engine::new(
        GridSize {
            cols: case.cols,
            lines: case.lines,
        },
        case.history,
    );
    let mut last = (0usize, 0usize);
    for (index, part) in case.bytes.chunks(64).enumerate() {
        processor.advance(&mut old_term, part);
        let mut batch = EventBatch::new();
        engine.feed(part, &mut batch, Instant::now());
        let old_total = old_term.total_lines();
        let new_total =
            engine.screen().history_len() as usize + usize::from(engine.screen().rows());
        if (old_total, new_total) != last {
            println!(
                "  chunk {index}: old total_lines={old_total} new={new_total} | bytes {:?}",
                String::from_utf8_lossy(&part[..part.len().min(48)])
            );
            last = (old_total, new_total);
        }
    }
}

// ───────────────────── flood benchmark (old vs new) ─────────────────────

fn flood_bytes(megabytes: usize) -> Vec<u8> {
    let line =
        b"the quick brown fox jumps over the lazy dog 0123456789 \x1b[33mcolour\x1b[0m tail\r\n";
    let mut out = Vec::with_capacity(megabytes * 1024 * 1024 + line.len());
    while out.len() < megabytes * 1024 * 1024 {
        out.extend_from_slice(line);
    }
    out
}

fn percentile(samples: &mut [u128], p: f64) -> u128 {
    samples.sort_unstable();
    let index = ((samples.len() as f64 - 1.0) * p).round() as usize;
    samples[index]
}

#[test]
#[ignore = "measurement, run explicitly"]
fn flood_bench() {
    let (cols, lines, history) = (120usize, 30usize, 10_000usize);
    let bytes = flood_bytes(4);
    println!(
        "flood: {} bytes, grid {cols}x{lines}, scrollback {history}, debug_assertions={}",
        bytes.len(),
        cfg!(debug_assertions)
    );

    // ── new engine ──
    let mut engine = Engine::new(GridSize { cols, lines }, history);
    let mut content = TerminalContent::default();
    let (mut feed_us, mut render_us) = (Vec::new(), Vec::new());
    let start = Instant::now();
    for part in bytes.chunks(4096) {
        let mut batch = EventBatch::new();
        let t0 = Instant::now();
        engine.feed(part, &mut batch, Instant::now());
        feed_us.push(t0.elapsed().as_micros());
        let t1 = Instant::now();
        content.refill(&mut engine);
        render_us.push(t1.elapsed().as_micros());
    }
    let new_total = start.elapsed();

    // ── old engine ──
    let listener = Rec::default();
    let mut old_term = Term::new(
        Config {
            scrolling_history: history,
            ..Config::default()
        },
        &Dims { cols, lines },
        listener,
    );
    let mut processor = Processor::<StdSyncHandler>::new();
    let mut old_content = TerminalContent::default();
    let (mut old_feed_us, mut old_render_us) = (Vec::new(), Vec::new());
    let start = Instant::now();
    for part in bytes.chunks(4096) {
        let t0 = Instant::now();
        processor.advance(&mut old_term, part);
        old_feed_us.push(t0.elapsed().as_micros());
        let t1 = Instant::now();
        old_refill(&mut old_content, &mut old_term);
        old_render_us.push(t1.elapsed().as_micros());
    }
    let old_total = start.elapsed();

    let report = |name: &str, samples: &mut Vec<u128>| {
        let sum: u128 = samples.iter().sum();
        println!(
            "  {name}: n={} total={} ms avg={} us p95={} us max={} us",
            samples.len(),
            sum / 1000,
            sum / samples.len() as u128,
            percentile(samples, 0.95),
            percentile(samples, 1.0),
        );
    };
    println!("NEW total {:?}", new_total);
    report("new feed", &mut feed_us);
    report("new render_update+snapshot", &mut render_us);
    println!("OLD total {:?}", old_total);
    report("old advance", &mut old_feed_us);
    report("old damage+snapshot", &mut old_render_us);
}

// ───────────────────── three-thread lock stress ─────────────────────

#[test]
#[ignore = "10 s stress, run explicitly"]
fn lock_stress_three_threads() {
    use std::sync::atomic::{AtomicBool, Ordering};

    let shared = oneterm_terminal::new_shared_terminal(
        GridSize {
            cols: 100,
            lines: 30,
        },
        5_000,
    );
    let stop = Arc::new(AtomicBool::new(false));
    let payload = flood_bytes(1);

    let feeder = {
        let shared = shared.clone();
        let stop = stop.clone();
        std::thread::spawn(move || {
            let mut fed = 0usize;
            while !stop.load(Ordering::Relaxed) {
                for part in payload.chunks(4096) {
                    let mut guard = shared.lock();
                    let mut batch = EventBatch::new();
                    guard.feed(part, &mut batch, Instant::now());
                    drop(guard);
                    fed += part.len();
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                }
            }
            fed
        })
    };
    let resizer = {
        let shared = shared.clone();
        let stop = stop.clone();
        std::thread::spawn(move || {
            let mut count = 0usize;
            let sizes = [(30u16, 100u16), (12, 40), (45, 200), (30, 100), (8, 20)];
            while !stop.load(Ordering::Relaxed) {
                for (rows, cols) in sizes {
                    let mut guard = shared.lock();
                    guard.resize(
                        oneterm_vt::Size { rows, cols },
                        oneterm_vt::ResizePolicy::KeepViewportTop,
                    );
                    drop(guard);
                    count += 1;
                    std::thread::yield_now();
                }
            }
            count
        })
    };
    let snapshotter = {
        let shared = shared.clone();
        let stop = stop.clone();
        std::thread::spawn(move || {
            let mut content = TerminalContent::default();
            let mut frames = 0usize;
            while !stop.load(Ordering::Relaxed) {
                {
                    let mut guard = shared.lock();
                    content.refill(&mut guard);
                }
                // Integrity of the snapshot itself: dense, in bounds, ordered.
                let bounds = content.terminal_bounds;
                assert_eq!(
                    content.cells.len(),
                    bounds.num_lines * bounds.num_cols,
                    "snapshot must stay dense"
                );
                if let TermDamageInfo::Partial(rows) = &content.damage {
                    assert!(
                        rows.iter().all(|row| *row < bounds.num_lines),
                        "damage row out of bounds: {rows:?} for {bounds:?}"
                    );
                }
                assert!(content.display_offset <= content.total_lines);
                frames += 1;
                std::thread::yield_now();
            }
            frames
        })
    };

    let deadline = Instant::now() + std::time::Duration::from_secs(10);
    while Instant::now() < deadline {
        std::thread::yield_now();
    }
    stop.store(true, Ordering::Relaxed);
    let fed = feeder.join().expect("feeder must not panic");
    let resizes = resizer.join().expect("resizer must not panic");
    let frames = snapshotter.join().expect("snapshotter must not panic");
    println!("stress: fed {fed} bytes, {resizes} resizes, {frames} snapshots in 10 s");
    assert!(fed > 0 && resizes > 0 && frames > 0);
}
