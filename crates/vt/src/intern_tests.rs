use std::sync::{Mutex, Once};

use super::*;
use crate::cell::{Attrs, Cell, CellContent, Color, Rgb};

/// Captures the engine's own log records so "warns once" can be an assertion
/// rather than a claim. Installed by whichever test needs it first.
static RECORDS: Mutex<Vec<String>> = Mutex::new(Vec::new());
static INSTALL: Once = Once::new();
/// Held by every test that can fill a table, so a "warned once" count cannot
/// pick up a concurrent test's warning.
static EXHAUSTION: Mutex<()> = Mutex::new(());

struct CaptureLogger;

impl log::Log for CaptureLogger {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &log::Record<'_>) {
        if let Ok(mut records) = RECORDS.lock() {
            records.push(record.args().to_string());
        }
    }

    fn flush(&self) {}
}

fn capture_logs() {
    INSTALL.call_once(|| {
        static LOGGER: CaptureLogger = CaptureLogger;
        let _ = log::set_logger(&LOGGER);
        log::set_max_level(log::LevelFilter::Trace);
    });
}

fn exhaustion_guard() -> std::sync::MutexGuard<'static, ()> {
    EXHAUSTION
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn logged(needle: &str) -> usize {
    RECORDS
        .lock()
        .map(|records| records.iter().filter(|line| line.contains(needle)).count())
        .unwrap_or(0)
}

fn rgb_style(n: u32) -> Style {
    Style {
        fg: Color::Rgb(Rgb {
            r: (n >> 16) as u8,
            g: (n >> 8) as u8,
            b: n as u8,
        }),
        ..Style::DEFAULT
    }
}

#[test]
fn identical_styles_intern_to_one_id() {
    let mut interner = Interner::default();
    let style = Style {
        attrs: Attrs::BOLD | Attrs::ITALIC,
        ..Style::DEFAULT
    };
    let first = interner.style(&style);
    let second = interner.style(&style);
    assert_eq!(first, second);
    assert_ne!(first, StyleId::DEFAULT);
    assert_eq!(interner.styles.entries(), 2);
    assert_eq!(interner.resolve_style(first), &style);
}

#[test]
fn default_style_is_id_zero() {
    let mut interner = Interner::default();
    assert_eq!(interner.style(&Style::DEFAULT), StyleId::DEFAULT);
    assert_eq!(interner.resolve_style(StyleId::DEFAULT), &Style::DEFAULT);
    // An id no table ever issued still resolves, because the render hand-off
    // must never be able to panic the engine.
    assert_eq!(interner.resolve_style(StyleId(u16::MAX)), &Style::DEFAULT);
}

#[test]
fn style_ids_never_change_once_assigned() {
    let _guard = exhaustion_guard();
    let mut interner = Interner::default();
    let mut issued = Vec::with_capacity(70_000);
    for n in 0..70_000u32 {
        let style = rgb_style(n);
        issued.push((interner.style(&style), style));
    }

    // 65_535 distinct values fit; the rest fall back to the default.
    assert_eq!(interner.styles.entries(), TABLE_LIMIT);
    let fell_back = issued
        .iter()
        .filter(|(id, _)| *id == StyleId::DEFAULT)
        .count();
    assert_eq!(fell_back, 70_000 - (TABLE_LIMIT - 1));

    // Every id that was ever issued still resolves to the value it was issued
    // for. This is the property the render hand-off depends on.
    for (id, style) in &issued {
        if *id == StyleId::DEFAULT {
            continue;
        }
        assert_eq!(interner.resolve_style(*id), style);
    }

    // And interning more afterwards moves nothing.
    let snapshot: Vec<Style> = issued
        .iter()
        .filter(|(id, _)| *id != StyleId::DEFAULT)
        .map(|(_, style)| *style)
        .collect();
    for n in 70_000..70_100u32 {
        interner.style(&rgb_style(n));
    }
    for (index, style) in snapshot.iter().enumerate() {
        assert_eq!(interner.resolve_style(StyleId(index as u16 + 1)), style);
    }
}

#[test]
fn style_table_exhaustion_falls_back_to_default_and_logs_once() {
    let _guard = exhaustion_guard();
    capture_logs();
    let before = logged("style table is full");

    let mut styles = StyleSet::default();
    for n in 0..(TABLE_LIMIT - 1) as u32 {
        styles.intern(&rgb_style(n));
    }
    assert_eq!(styles.entries(), TABLE_LIMIT);
    assert_eq!(styles.exhausted(), 0);

    for n in 0..10u32 {
        assert_eq!(styles.intern(&rgb_style(1_000_000 + n)), 0);
    }
    assert_eq!(styles.exhausted(), 10);
    // Ten fallbacks, one warning.
    assert_eq!(logged("style table is full") - before, 1);

    // A style already interned is still reused after exhaustion.
    assert_eq!(styles.intern(&rgb_style(0)), 1);
}

#[test]
fn one_extras_entry_per_image_not_per_cell() {
    let mut interner = Interner::default();
    let before = interner.extras.entries();
    let image = Extras {
        graphic: Some(GraphicId(7)),
        ..Extras::NONE
    };

    let mut grid = vec![Cell::EMPTY; 400 * 200];
    for cell in &mut grid {
        let id = interner.extras(&image);
        *cell = cell.with_extras(id);
    }

    assert_eq!(interner.extras.entries(), before + 1);
    assert_eq!(interner.resolve_extras(grid[0].extras_id()), &image);

    // A cell carrying both a hyperlink and that image is one more entry, not
    // one per cell.
    let both = Extras {
        hyperlink: Some(HyperlinkId(3)),
        graphic: Some(GraphicId(7)),
    };
    for _ in 0..1_000 {
        interner.extras(&both);
    }
    assert_eq!(interner.extras.entries(), before + 2);
}

#[test]
fn extras_table_exhaustion_falls_back_to_no_extras() {
    let _guard = exhaustion_guard();
    let mut extras = ExtrasTable::default();
    for n in 0..(TABLE_LIMIT - 1) as u64 {
        extras.intern(&Extras {
            graphic: Some(GraphicId(n)),
            ..Extras::NONE
        });
    }
    assert_eq!(extras.entries(), TABLE_LIMIT);
    let overflow = extras.intern(&Extras {
        graphic: Some(GraphicId(u64::MAX)),
        ..Extras::NONE
    });
    assert_eq!(ExtrasId(overflow), ExtrasId::NONE);
    assert_eq!(extras.resolve(overflow), &Extras::NONE);
    assert_eq!(extras.exhausted(), 1);
}

#[test]
fn hyperlink_ids_are_per_terminal_not_global() {
    let mut first = HyperlinkTable::default();
    let mut second = HyperlinkTable::default();

    // Two terminals that see the same stream issue the same ids.
    for table in [&mut first, &mut second] {
        assert_eq!(
            table.intern(None, "https://example.com"),
            Some(HyperlinkId(0))
        );
        assert_eq!(
            table.intern(None, "https://example.com"),
            Some(HyperlinkId(1))
        );
        assert_eq!(
            table.intern(Some("a"), "https://example.com"),
            Some(HyperlinkId(2))
        );
        // An explicit id identifies a link run, so it deduplicates.
        assert_eq!(
            table.intern(Some("a"), "https://example.com"),
            Some(HyperlinkId(2))
        );
        // The same id with a different URI is a different link.
        assert_eq!(
            table.intern(Some("a"), "https://other.example"),
            Some(HyperlinkId(3))
        );
    }

    let link = first.resolve(HyperlinkId(0)).expect("issued id resolves");
    assert_eq!(&*link.id, "1");
    assert_eq!(&*link.uri, "https://example.com");
    assert_eq!(first.resolve(HyperlinkId(99)), None);
    assert_eq!(first.len(), second.len());
}

#[test]
fn identical_clusters_dedupe() {
    let mut interner = Interner::default();
    let family: Vec<char> = "👨\u{200D}👩\u{200D}👧\u{200D}👦".chars().collect();
    let first = interner.grapheme(&family);
    let second = interner.grapheme(&family);
    assert_eq!(first, second);
    assert_eq!(interner.resolve_grapheme(first), &family[..]);
    // Id 0 is the reserved fallback cluster, so a real cluster never gets it.
    assert_ne!(first, GraphemeId(0));
    assert_eq!(interner.graphemes.entries(), 2);
}

#[test]
fn cluster_longer_than_cap_is_truncated_and_counted() {
    let mut arena = GraphemeArena::default();
    let long: Vec<char> = (0..40u8).map(|n| char::from(b'a' + n % 26)).collect();
    let id = arena.intern(&long);
    assert_eq!(arena.resolve(id).len(), GRAPHEME_MAX_LEN);
    assert_eq!(arena.resolve(id), &long[..GRAPHEME_MAX_LEN]);
    assert_eq!(arena.truncated(), 1);

    // Two over-long clusters sharing a prefix collapse to one entry.
    let mut other = long.clone();
    other[30] = 'Z';
    assert_eq!(arena.intern(&other), id);
    assert_eq!(arena.truncated(), 2);
    assert_eq!(arena.entries(), 2);

    // An empty cluster is the fallback, never a new entry.
    assert_eq!(arena.intern(&[]), GraphemeId(0));
    assert_eq!(arena.entries(), 2);
}

#[test]
fn grapheme_arena_exhaustion_falls_back_to_a_blank_and_logs_once() {
    let _guard = exhaustion_guard();
    capture_logs();
    let before = logged("grapheme arena is full");

    // The real id space is 2^21 entries; driving it would cost seconds and
    // hundreds of megabytes in the CI gate, so the limit is injected instead.
    let mut arena = GraphemeArena::with_id_limit(4);
    for n in 0..3u32 {
        let id = arena.intern(&['x', char::from_u32(n + 0x300).unwrap_or('y')]);
        assert_ne!(id, GraphemeId(0));
    }
    assert_eq!(arena.entries(), 4);

    for n in 0..5u32 {
        let id = arena.intern(&['z', char::from_u32(n + 0x300).unwrap_or('y')]);
        assert_eq!(id, GraphemeId(0), "a full arena degrades to a space");
    }
    assert_eq!(arena.exhausted(), 5);
    assert_eq!(arena.resolve(GraphemeId(0)), &[' '][..]);
    assert_eq!(logged("grapheme arena is full") - before, 1);
}

#[test]
fn grapheme_sweep_trigger_is_the_documented_constant() {
    assert_eq!(GRAPHEME_MAX_LEN, 16);
    assert_eq!(GRAPHEME_SWEEP_ENTRIES, 65_536);
    assert_eq!(GRAPHEME_SWEEP_CHARS, 1 << 20);
    // The id space is the cell's 21 content bits, not the u16 the earlier
    // design assumed (R-27).
    assert_eq!(
        GraphemeArena::default().id_limit,
        crate::cell::CONTENT_LIMIT
    );
    assert_eq!(crate::cell::CONTENT_LIMIT, 2_097_152);

    let mut arena = GraphemeArena::default();
    // The reserved cluster already occupies one entry.
    for n in 0..(GRAPHEME_SWEEP_ENTRIES - 2) as u32 {
        arena.intern(&unique_cluster(n));
    }
    assert!(!arena.needs_sweep());
    arena.intern(&unique_cluster(999_999));
    assert!(arena.needs_sweep());
}

/// A two-codepoint cluster unique to `n`: a base letter plus a combining mark.
fn unique_cluster(n: u32) -> Vec<char> {
    let mark = char::from_u32(0x10000 + n).unwrap_or('\u{301}');
    vec!['a', mark]
}

#[test]
fn grapheme_gc_preserves_every_live_cell() {
    let mut arena = GraphemeArena::default();

    // A stream of unique clusters, half of which the grid still holds.
    let mut grid: Vec<Cell> = Vec::new();
    for n in 0..2_000u32 {
        let id = arena.intern(&unique_cluster(n));
        if n % 2 == 0 {
            grid.push(Cell::EMPTY.with_content(CellContent::Grapheme(id)));
        }
    }
    let before: Vec<Vec<char>> = grid
        .iter()
        .map(|cell| match cell.content() {
            CellContent::Grapheme(id) => arena.resolve(id).to_vec(),
            CellContent::Scalar(c) => vec![c],
        })
        .collect();
    assert_eq!(arena.entries(), 2_001);

    let live: Vec<GraphemeId> = grid
        .iter()
        .filter_map(|cell| match cell.content() {
            CellContent::Grapheme(id) => Some(id),
            CellContent::Scalar(_) => None,
        })
        .collect();
    let remap = arena.sweep(live);
    assert_eq!(remap.kept(), 1_000);

    for cell in &mut grid {
        if let CellContent::Grapheme(id) = cell.content() {
            *cell = cell.with_content(CellContent::Grapheme(remap.get(id)));
        }
    }

    // The arena shrank to the live set plus the reserved cluster, and not one
    // cell's text changed.
    assert_eq!(arena.entries(), 1_001);
    assert!(!arena.needs_sweep());
    let after: Vec<Vec<char>> = grid
        .iter()
        .map(|cell| match cell.content() {
            CellContent::Grapheme(id) => arena.resolve(id).to_vec(),
            CellContent::Scalar(c) => vec![c],
        })
        .collect();
    assert_eq!(before, after);

    // An id the sweep was not told about degrades to the reserved blank rather
    // than resolving to someone else's cluster.
    assert_eq!(remap.get(GraphemeId(12_345)), GraphemeId(0));
}

#[test]
fn a_sweep_keeps_the_session_counters() {
    let mut arena = GraphemeArena::default();
    let long: Vec<char> = std::iter::repeat_n('x', 40).collect();
    arena.intern(&long);
    assert_eq!(arena.truncated(), 1);
    arena.sweep(std::iter::empty());
    assert_eq!(arena.truncated(), 1);
    assert_eq!(arena.entries(), 1);
}
