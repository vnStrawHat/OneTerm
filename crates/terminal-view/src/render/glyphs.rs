//! Shaped-run cache: run text → `Arc<LineLayout>`, shared across rows and frames.
//!
//! GPUI's own line-layout cache lives for two frames; this cache keeps a run
//! alive for as long as it is painted (plus two generations of grace when the
//! cap is hit), so an idle terminal never re-shapes. `TextRun.color` does not
//! take part in shaping, so one entry serves every color; colors are applied
//! at paint time from the plan's `ColorSpan`s.
//!
//! The cache holds GPUI's `Arc<LineLayout>` (the value GPUI's own cache holds),
//! not a `ShapedLine`: the painter reads only the layout, and a `ShapedLine`
//! carries an inline 32-slot decoration `SmallVec` that made every bucket
//! 3 KB (IN-0045, BUG-0078).
//!
//! **ASCII fast path (US-0146).** A miss for a run of printable ASCII
//! (0x20-0x7E) in a font with ligatures off ([`FontKey::plain`]) does not call
//! the shaper: its layout is one glyph per byte from a per-font table
//! ([`AsciiGlyphs`]), at the font's constant advance, with GPUI's force-width
//! pass applied. The table is read once per [`FontKey`] from one shaped
//! reference line holding every ordered pair of printable chars plus longer
//! ligature probes, and kept only if rebuilding that line by hand reproduces
//! GPUI's layout exactly (one run of one face, one glyph per char, the same
//! glyph in every two-char context, no offsets); otherwise that font always
//! shapes. Contexts longer than two chars are covered only by the probes. Any
//! other run (a non-ASCII or control char, ligatures on, another feature on)
//! shapes as before.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

use gpui::{
    Font, FontId, FontStyle, FontWeight, GlyphId, LineLayout, Pixels, ShapedGlyph, ShapedRun,
    SharedString, TextRun, Window, point, px,
};

use super::diagnostics::FrameStats;
use super::frame::Fnv1a;

/// The font a run was shaped with, in hashable form.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) struct FontKey {
    /// FNV of the family name.
    pub family: u32,
    pub size_bits: u32,
    pub weight_bits: u32,
    pub italic: bool,
    /// Ligatures off: the font's features list turns `calt` off and turns no
    /// feature on. Only such a font may take the ASCII fast path, and the
    /// ligature switch changes the key of every run and table (US-0146).
    pub plain: bool,
}

/// The four style variants of the terminal font plus their keys, so row
/// planning never builds a `Font` per cell.
pub(crate) struct FontSet {
    variants: [(Font, FontKey); 4],
}

impl FontSet {
    pub(crate) fn new(base: &Font, font_size: Pixels) -> Self {
        let mut family = Fnv1a::new();
        family.write(base.family.as_bytes());
        let family = family.finish() as u32;
        let features = base.features.tag_value_list();
        let plain = features.iter().any(|(tag, _)| tag == "calt")
            && features.iter().all(|&(_, value)| value == 0);
        let variant = |bold: bool, italic: bool| {
            let mut font = base.clone();
            let weight = if bold {
                FontWeight(base.weight.0.max(FontWeight::BOLD.0))
            } else {
                base.weight
            };
            font.weight = weight;
            font.style = if italic {
                FontStyle::Italic
            } else {
                base.style
            };
            let key = FontKey {
                family,
                size_bits: f32::from(font_size).to_bits(),
                weight_bits: weight.0.to_bits(),
                italic,
                plain,
            };
            (font, key)
        };
        Self {
            variants: [
                variant(false, false),
                variant(true, false),
                variant(false, true),
                variant(true, true),
            ],
        }
    }

    #[inline]
    pub(crate) fn get(&self, bold: bool, italic: bool) -> (&Font, FontKey) {
        let (font, key) = &self.variants[usize::from(bold) | (usize::from(italic) << 1)];
        (font, *key)
    }

    pub(crate) fn regular(&self) -> (&Font, FontKey) {
        self.get(false, false)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct RunKey {
    text_hash: u64,
    len: u32,
    font: FontKey,
    forced: bool,
}

struct Entry {
    line: Arc<LineLayout>,
    used: u32,
}

pub(crate) struct GlyphCache {
    map: HashMap<RunKey, Entry>,
    /// The ASCII table per font variant; `None` = the font failed the check
    /// and always shapes.
    ascii: HashMap<FontKey, Option<AsciiGlyphs>>,
    generation: u32,
    capacity: usize,
}

impl Default for GlyphCache {
    fn default() -> Self {
        Self::new()
    }
}

impl GlyphCache {
    const CAPACITY: usize = 4096;
    /// Entries unused for this many generations are dropped when the cap hits.
    const GRACE: u32 = 2;

    pub(crate) fn new() -> Self {
        Self {
            // Grows on demand: an up-front table is paid by every view (BUG-0078).
            map: HashMap::new(),
            ascii: HashMap::new(),
            generation: 0,
            capacity: Self::CAPACITY,
        }
    }

    pub(crate) fn begin_frame(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }

    /// Drop every shaped run. [`FontKey`] cannot see `Font::features` or
    /// `Font::fallbacks` (both change how a run is shaped without changing the
    /// family, size, weight or slant), so the whole cache goes when the font
    /// value changes.
    pub(crate) fn clear(&mut self) {
        self.map.clear();
        self.ascii.clear();
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.map.len()
    }

    /// FNV-1a over the UTF-8 bytes, the hash `shape_line_by_hash` keys on.
    pub(crate) fn text_hash(text: &str) -> u64 {
        let mut h = Fnv1a::new();
        h.write(text.as_bytes());
        h.finish()
    }

    /// The shaped line for `text` in `font`; a hit costs one hash lookup and
    /// an `Arc` bump.
    #[allow(clippy::too_many_arguments)]
    #[cfg_attr(
        feature = "hotpath-profiling",
        hotpath::measure(impl_type = "GlyphCache")
    )]
    pub(crate) fn shape(
        &mut self,
        text: &str,
        font: &Font,
        key: FontKey,
        font_size: Pixels,
        force_width: Option<Pixels>,
        window: &Window,
        stats: &mut FrameStats,
    ) -> Arc<LineLayout> {
        let text_hash = Self::text_hash(text);
        let run_key = RunKey {
            text_hash,
            len: text.len() as u32,
            font: key,
            forced: force_width.is_some(),
        };
        if let Some(entry) = self.map.get_mut(&run_key) {
            entry.used = self.generation;
            stats.glyph_hits += 1;
            return Arc::clone(&entry.line);
        }
        let line = match self.ascii_layout(text, font, key, font_size, force_width, window) {
            Some(layout) => {
                stats.ascii_layouts += 1;
                Arc::new(layout)
            }
            None => window.text_system().layout_line_by_hash(
                text_hash,
                text.len(),
                font_size,
                &[text_run(text.len(), font)],
                force_width,
                || SharedString::from(text.to_string()),
            ),
        };
        stats.shape_calls += 1;
        if self.map.len() >= self.capacity {
            let cutoff = self.generation.wrapping_sub(Self::GRACE);
            self.map
                .retain(|_, entry| entry.used.wrapping_sub(cutoff) as i32 >= 0);
        }
        self.map.insert(
            run_key,
            Entry {
                line: Arc::clone(&line),
                used: self.generation,
            },
        );
        line
    }

    /// The fast-path layout for `text`, or `None` when the run must be shaped.
    fn ascii_layout(
        &mut self,
        text: &str,
        font: &Font,
        key: FontKey,
        font_size: Pixels,
        force_width: Option<Pixels>,
        window: &Window,
    ) -> Option<LineLayout> {
        if !key.plain || text.is_empty() || !text.bytes().all(AsciiGlyphs::covers) {
            return None;
        }
        self.ascii
            .entry(key)
            .or_insert_with(|| AsciiGlyphs::read(font, font_size, window))
            .as_ref()
            .map(|table| table.layout(text, font_size, force_width))
    }

    #[cfg(test)]
    pub(crate) fn with_capacity_for_test(capacity: usize) -> Self {
        Self {
            map: HashMap::with_capacity(capacity),
            ascii: HashMap::new(),
            generation: 0,
            capacity,
        }
    }
}

fn text_run(len: usize, font: &Font) -> TextRun {
    TextRun {
        len,
        font: font.clone(),
        color: gpui::black(),
        background_color: None,
        underline: None,
        strikethrough: None,
    }
}

/// Longer contexts a font may rewrite with `calt` off (`liga`, `clig`, and
/// the required `ccmp` / `rlig` / `rclt`, which cannot be turned off).
const PROBES: &str = " fi fl ff ffi ffl -> => != == <= >= === !== <=> ==> <!-- --> ::= ... www";

/// Chars of the pair walk: every ordered pair of printable ASCII once.
const WALK_LEN: usize = AsciiGlyphs::LEN * AsciiGlyphs::LEN + 1;

/// The check's reference line: a walk through every ordered pair of printable
/// ASCII (an order-2 de Bruijn sequence, 9,026 chars), then [`PROBES`]. Any
/// substitution, kerning or offset a font applies in a two-char context shows
/// up here and fails the check.
static REFERENCE: LazyLock<String> = LazyLock::new(|| {
    let char_at = |i: usize| char::from(AsciiGlyphs::FIRST + i as u8);
    let mut text = String::with_capacity(WALK_LEN + PROBES.len());
    // Lyndon words of length 1 and 2 in order: `i`, then `i j` for every
    // `j > i`; their concatenation is cyclic, so the first char closes it.
    for i in 0..AsciiGlyphs::LEN {
        text.push(char_at(i));
        for j in i + 1..AsciiGlyphs::LEN {
            text.push(char_at(i));
            text.push(char_at(j));
        }
    }
    text.push(char_at(0));
    text.push_str(PROBES);
    text
});

/// Glyph ids of printable ASCII in one font variant, and the one advance
/// they share (US-0146).
struct AsciiGlyphs {
    font_id: FontId,
    ids: [GlyphId; AsciiGlyphs::LEN],
    advance: f32,
    ascent: Pixels,
    descent: Pixels,
}

impl AsciiGlyphs {
    const FIRST: u8 = 0x20;
    const LEN: usize = 95;

    fn covers(byte: u8) -> bool {
        byte.wrapping_sub(Self::FIRST) < Self::LEN as u8
    }

    /// Shape [`REFERENCE`] once (1.5-3 ms) and keep the table only if it
    /// reproduces that layout; see [`Self::check`].
    fn read(font: &Font, font_size: Pixels, window: &Window) -> Option<Self> {
        let shaped = window.text_system().layout_line(
            &REFERENCE,
            font_size,
            &[text_run(REFERENCE.len(), font)],
            None,
        );
        Self::check(&REFERENCE, &shaped)
    }

    /// The table read from `shaped` (the shaper's layout of `reference`), if
    /// [`Self::layout`] rebuilds that layout exactly: one run, one glyph per
    /// byte at one advance, no offsets, no emoji, and every char's glyph the
    /// same in every context. Anything else keeps the font on the shaper: a
    /// fallback face, a contextual substitution, kerning or a proportional
    /// advance **that the reference exhibits**.
    fn check(reference: &str, shaped: &LineLayout) -> Option<Self> {
        let [run] = shaped.runs.as_slice() else {
            return None;
        };
        let mut ids = [GlyphId(0); Self::LEN];
        for (byte, glyph) in reference.bytes().zip(&run.glyphs) {
            ids[usize::from(byte - Self::FIRST)] = glyph.id;
        }
        let table = Self {
            font_id: run.font_id,
            ids,
            // The pen starts at 0, so the second glyph's `x` is the advance
            // exactly (the check rejects an offset on the first glyph).
            advance: f32::from(run.glyphs.get(1)?.position.x),
            ascent: shaped.ascent,
            descent: shaped.descent,
        };
        same_layout(&table.layout(reference, shaped.font_size, None), shaped).then_some(table)
    }

    /// What the platform shaper returns for `text` (every byte covered): the
    /// pen moves by `advance` per glyph, then GPUI's force-width pass.
    fn layout(&self, text: &str, font_size: Pixels, force_width: Option<Pixels>) -> LineLayout {
        let mut pen = 0.0f32;
        let glyphs = text
            .bytes()
            .enumerate()
            .map(|(index, byte)| {
                let glyph = ShapedGlyph {
                    id: self.ids[usize::from(byte - Self::FIRST)],
                    position: point(px(pen), px(0.0)),
                    index,
                    is_emoji: false,
                };
                pen += self.advance;
                glyph
            })
            .collect();
        let mut layout = LineLayout {
            font_size,
            width: px(pen),
            ascent: self.ascent,
            descent: self.descent,
            runs: vec![ShapedRun {
                font_id: self.font_id,
                glyphs,
            }],
            len: text.len(),
        };
        if let Some(width) = force_width {
            apply_force_width(&mut layout, width);
        }
        layout
    }
}

/// GPUI's `apply_force_width_to_layout` (`gpui-pre` 0.3.7,
/// `text_system/line_layout.rs`, private) line for line, so a hand-built
/// layout lands where a shaped one does.
fn apply_force_width(layout: &mut LineLayout, force_width: Pixels) {
    let mut glyph_pos: usize = 0;
    let mut last_base_shaped_x = px(f32::NEG_INFINITY);
    let mut last_base_actual_x = px(0.);
    for run in layout.runs.iter_mut() {
        for glyph in run.glyphs.iter_mut() {
            let shaped_x = glyph.position.x;
            if shaped_x > last_base_shaped_x + force_width * 0.5 {
                let forced_x = glyph_pos * force_width;
                if (shaped_x - forced_x).abs() > px(1.) {
                    glyph.position.x = forced_x;
                }
                last_base_shaped_x = shaped_x;
                last_base_actual_x = glyph.position.x;
                glyph_pos += 1;
            } else {
                glyph.position.x = last_base_actual_x + (shaped_x - last_base_shaped_x);
            }
        }
    }
}

/// Field-by-field equality of two layouts (`LineLayout` has no `PartialEq`).
fn same_layout(a: &LineLayout, b: &LineLayout) -> bool {
    a.font_size == b.font_size
        && a.width == b.width
        && a.ascent == b.ascent
        && a.descent == b.descent
        && a.len == b.len
        && a.runs.len() == b.runs.len()
        && a.runs.iter().zip(&b.runs).all(|(x, y)| {
            x.font_id == y.font_id
                && x.glyphs.len() == y.glyphs.len()
                && x.glyphs.iter().zip(&y.glyphs).all(|(g, h)| {
                    g.id == h.id
                        && g.position == h.position
                        && g.index == h.index
                        && g.is_emoji == h.is_emoji
                })
        })
}

#[cfg(test)]
mod tests {
    use gpui::{Font, FontFeatures, FontStyle, FontWeight};

    use super::*;

    fn font() -> Font {
        Font {
            family: "Test Mono".into(),
            features: FontFeatures::default(),
            fallbacks: None,
            weight: FontWeight::NORMAL,
            style: FontStyle::Normal,
        }
    }

    #[test]
    fn font_set_keys_differ_per_variant_and_size() {
        let set = FontSet::new(&font(), gpui::px(13.0));
        let (_, regular) = set.regular();
        let (bold_font, bold) = set.get(true, false);
        let (italic_font, italic) = set.get(false, true);
        let (_, both) = set.get(true, true);
        assert_ne!(regular, bold);
        assert_ne!(regular, italic);
        assert_ne!(bold, both);
        assert_eq!(bold_font.weight, FontWeight::BOLD);
        assert_eq!(italic_font.style, FontStyle::Italic);
        let bigger = FontSet::new(&font(), gpui::px(14.0));
        assert_ne!(bigger.regular().1, regular);
        assert_eq!(bigger.regular().1.family, regular.family);
    }

    #[gpui::test]
    fn glyph_cache_evicts_stale_generation(cx: &mut gpui::TestAppContext) {
        let cx = cx.add_empty_window();
        cx.update(|window, _| {
            let mut cache = GlyphCache::with_capacity_for_test(2);
            let mut stats = FrameStats::default();
            let set = FontSet::new(&font(), gpui::px(13.0));
            let (f, key) = set.regular();
            let size = gpui::px(13.0);
            cache.begin_frame();
            cache.shape("a", f, key, size, None, window, &mut stats);
            cache.shape("a", f, key, size, None, window, &mut stats);
            assert_eq!((stats.shape_calls, stats.glyph_hits), (1, 1));
            for _ in 0..3 {
                cache.begin_frame();
            }
            cache.shape("b", f, key, size, None, window, &mut stats);
            cache.shape("c", f, key, size, None, window, &mut stats);
            assert_eq!(cache.len(), 2, "'a' aged out when the cap was hit");
            cache.shape("a", f, key, size, None, window, &mut stats);
            assert_eq!(stats.shape_calls, 4, "'a' had to be shaped again");
            cache.shape("b", f, key, size, None, window, &mut stats);
            assert_eq!(stats.glyph_hits, 2, "'b' survived: used this generation");
        });
    }

    /// BUG-0078: a new cache allocates nothing, a hit returns the very layout
    /// the miss stored, and a cache filled to its cap stays under 1 MiB
    /// (table plus the per-entry `LineLayout` headers; the glyph vectors are
    /// GPUI's shaped data and are not counted). Before the fix one bucket was
    /// 3,024 bytes and the table 24.8 MB from the first frame.
    #[gpui::test]
    fn glyph_cache_full_table_stays_small_and_hits_share_the_layout(cx: &mut gpui::TestAppContext) {
        use std::fmt::Write as _;
        use std::mem::size_of;

        let bucket = size_of::<(RunKey, Entry)>();
        assert!(bucket <= 64, "bucket is {bucket} bytes");
        let cx = cx.add_empty_window();
        cx.update(|window, _| {
            let mut cache = GlyphCache::new();
            assert_eq!(cache.map.capacity(), 0, "nothing is allocated up front");
            let mut stats = FrameStats::default();
            let set = FontSet::new(&font(), gpui::px(13.0));
            let (f, key) = set.regular();
            let size = gpui::px(13.0);
            cache.begin_frame();
            let first = cache.shape("same", f, key, size, None, window, &mut stats);
            let again = cache.shape("same", f, key, size, None, window, &mut stats);
            assert!(Arc::ptr_eq(&first, &again), "a hit returns the stored layout");
            assert_eq!((stats.shape_calls, stats.glyph_hits), (1, 1));

            let mut text = String::new();
            for i in 1..GlyphCache::CAPACITY {
                text.clear();
                write!(text, "w{i}").unwrap();
                cache.shape(&text, f, key, size, None, window, &mut stats);
            }
            assert_eq!(cache.len(), GlyphCache::CAPACITY);
            // hashbrown fills at most 7/8 of a power-of-two bucket count and
            // keeps one control byte per bucket.
            let buckets = (cache.map.capacity() * 8 / 7).next_power_of_two();
            let table = buckets * (bucket + 1);
            // `ArcInner` = two counters + the value.
            let headers = cache.len() * (size_of::<LineLayout>() + 2 * size_of::<usize>());
            eprintln!("full glyph cache: {bucket} B/bucket, {buckets} buckets, {table} B table + {headers} B layouts");
            assert!(
                table + headers < 1 << 20,
                "full cache is {table} B table + {headers} B layouts ({buckets} buckets)"
            );
        });
    }

    fn with_features(features: &[(&str, u32)]) -> Font {
        Font {
            features: FontFeatures(Arc::new(
                features.iter().map(|&(t, v)| (t.to_string(), v)).collect(),
            )),
            ..font()
        }
    }

    /// US-0146: the ligature flag is part of the key, so a run or a table built
    /// with ligatures off is never handed out with them on.
    #[test]
    fn font_key_plain_needs_calt_off_and_nothing_on() {
        let key = |features: &[(&str, u32)]| {
            FontSet::new(&with_features(features), gpui::px(13.0))
                .regular()
                .1
        };
        assert!(!key(&[]).plain, "no list: DirectWrite's default calt is on");
        assert!(key(&[("calt", 0)]).plain);
        assert!(key(&[("calt", 0), ("liga", 0)]).plain);
        assert!(!key(&[("calt", 1)]).plain);
        assert!(!key(&[("calt", 0), ("ss01", 1)]).plain);
        assert_ne!(key(&[("calt", 0)]), key(&[("calt", 1)]));
        let set = FontSet::new(&with_features(&[("calt", 0)]), gpui::px(13.0));
        assert!(
            set.get(true, true).1.plain,
            "every variant carries the flag"
        );
    }

    /// US-0146 F1: the walk holds every ordered printable pair (the Fira Code
    /// `ccmp` rule fires on `` X` `` and ```` `` ````, which one fixed context
    /// per char missed), and the line does not end in a space (DirectWrite
    /// splits trailing whitespace into a run of its own).
    #[test]
    fn reference_walks_every_ordered_printable_pair() {
        let walk = &REFERENCE.as_bytes()[..WALK_LEN];
        let pairs: std::collections::HashSet<[u8; 2]> =
            walk.windows(2).map(|w| [w[0], w[1]]).collect();
        assert_eq!(pairs.len(), AsciiGlyphs::LEN * AsciiGlyphs::LEN);
        assert!(REFERENCE.ends_with(PROBES) && !REFERENCE.ends_with(' '));
        assert!(REFERENCE.bytes().all(AsciiGlyphs::covers));
        assert!(!AsciiGlyphs::covers(0x1f) && !AsciiGlyphs::covers(0x7f));
        assert!(!AsciiGlyphs::covers(0xc3), "a UTF-8 lead byte");
    }

    fn copy(layout: &LineLayout) -> LineLayout {
        LineLayout {
            runs: layout.runs.clone(),
            ..*layout
        }
    }

    /// US-0146 F4: the check rejects a font whose layout of the reference
    /// differs anywhere a contextual rule would show: one glyph swapped in a
    /// single pair (`` A` ``, the Fira Code case) or a probe ligated into one
    /// glyph. Synthetic: the stub text system is the "font", and the shaped
    /// layout is edited the way such a font would shape it.
    #[gpui::test]
    fn ascii_check_rejects_a_contextual_rule(cx: &mut gpui::TestAppContext) {
        let cx = cx.add_empty_window();
        cx.update(|window, _| {
            let f = with_features(&[("calt", 0)]);
            let shaped = window.text_system().layout_line(
                &REFERENCE,
                gpui::px(13.0),
                &[text_run(REFERENCE.len(), &f)],
                None,
            );
            assert!(
                AsciiGlyphs::check(&REFERENCE, &shaped).is_some(),
                "a plain font passes"
            );

            let at = REFERENCE[..WALK_LEN].find("A`").unwrap() + 1;
            let mut swapped = copy(&shaped);
            swapped.runs[0].glyphs[at].id = GlyphId(999);
            assert!(
                AsciiGlyphs::check(&REFERENCE, &swapped).is_none(),
                "`A`` pair"
            );

            // Contexts longer than a pair: only the probes hold them. Derived from
            // `PROBES`, so a new probe is tested; the count pins against deletion.
            let long: Vec<&str> = PROBES.split(' ').filter(|p| p.len() >= 3).collect();
            assert_eq!(long.len(), 11, "{long:?}");
            for probe in long {
                let at = WALK_LEN + REFERENCE[WALK_LEN..].find(probe).unwrap();
                let mut ligated = copy(&shaped);
                ligated.runs[0].glyphs.drain(at + 1..at + probe.len());
                assert!(
                    AsciiGlyphs::check(&REFERENCE, &ligated).is_none(),
                    "{probe}"
                );
            }
        });
    }

    /// US-0146 F6: `apply_force_width` is a copy of GPUI's private pass; run
    /// both on the stub shaper's layout and compare, so a `gpui-pre` bump that
    /// changes GPUI's pass fails here. Widths at and above twice the stub's
    /// 7.8 px advance make every other glyph a non-base (the combining-mark
    /// branch); a non-BMP char has a double advance.
    #[gpui::test]
    fn force_width_copy_matches_gpui(cx: &mut gpui::TestAppContext) {
        let cx = cx.add_empty_window();
        cx.update(|window, _| {
            let f = font();
            let size = gpui::px(13.0);
            for text in ["abc", "a\u{1f600}b", "abcdefghij", "x"] {
                let runs = [text_run(text.len(), &f)];
                let unforced = window.text_system().layout_line(text, size, &runs, None);
                for width in [4.0, 7.8, 8.0, 9.5, 15.6, 16.0, 20.0] {
                    let width = gpui::px(width);
                    let gpui_forced =
                        window
                            .text_system()
                            .layout_line(text, size, &runs, Some(width));
                    let mut ours = copy(&unforced);
                    apply_force_width(&mut ours, width);
                    assert!(
                        same_layout(&ours, &gpui_forced),
                        "{text:?} {width:?}\n{ours:?}\n{gpui_forced:?}"
                    );
                }
            }
        });
    }

    /// US-0146: every run the fast path must not take reaches the shaper
    /// (`shape_calls` without `ascii_layouts`); a plain ASCII run does not.
    #[gpui::test]
    fn ascii_fast_path_falls_through_to_the_shaper(cx: &mut gpui::TestAppContext) {
        let cx = cx.add_empty_window();
        cx.update(|window, _| {
            let size = gpui::px(13.0);
            let plain = FontSet::new(&with_features(&[("calt", 0)]), size);
            let mut cache = GlyphCache::new();
            let mut stats = FrameStats::default();
            let mut shape = |cache: &mut GlyphCache, text: &str, set: &FontSet| {
                let (f, key) = set.regular();
                let before = stats;
                cache.shape(text, f, key, size, Some(size), window, &mut stats);
                (
                    stats.shape_calls - before.shape_calls,
                    stats.ascii_layouts - before.ascii_layouts,
                )
            };
            assert_eq!(shape(&mut cache, "abc", &plain), (1, 1), "plain ASCII");
            assert_eq!(shape(&mut cache, "abc", &plain), (0, 0), "then a hit");
            assert_eq!(shape(&mut cache, "ab\u{e9}", &plain), (1, 0), "non-ASCII");
            assert_eq!(shape(&mut cache, "e\u{301}", &plain), (1, 0), "combining");
            assert_eq!(shape(&mut cache, "a\tb", &plain), (1, 0), "control char");
            assert_eq!(shape(&mut cache, "a\u{7f}", &plain), (1, 0), "DEL");
            let not_plain: [&[(&str, u32)]; 3] = [&[], &[("calt", 1)], &[("calt", 0), ("ss01", 1)]];
            // Distinct texts: the non-plain keys are equal (features beyond the
            // ligature flag are covered by `clear`, not by the key).
            for (text, features) in ["abc", "abd", "abe"].into_iter().zip(not_plain) {
                let set = FontSet::new(&with_features(features), size);
                assert_eq!(shape(&mut cache, text, &set), (1, 0), "{features:?}");
            }
            // A font whose check failed keeps shaping.
            let other = FontSet::new(&with_features(&[("calt", 0)]), gpui::px(14.0));
            cache.ascii.insert(other.regular().1, None);
            assert_eq!(shape(&mut cache, "abc", &other), (1, 0), "failed check");
            assert_eq!(cache.ascii.len(), 2, "one table per font key, 13 and 14 px");
            cache.clear();
            assert!(cache.ascii.is_empty(), "a font change drops the tables");
        });
    }

    /// US-0146: on the platform shaper (DirectWrite) the hand-built layout is
    /// the shaped one, field for field, for the app's default font and two
    /// system monospace fonts in every variant tried, forced and unforced; a
    /// proportional font fails the check and keeps shaping.
    #[cfg(windows)]
    #[test]
    fn ascii_fast_path_matches_directwrite() {
        use std::borrow::Cow;

        // Not headless: a headless Windows platform carries GPUI's no-op text system.
        let platform = gpui_platform::current_platform(false);
        let mut cx = gpui::TestAppContext::build_with_text_system(
            gpui::TestDispatcher::new(0),
            None,
            platform.text_system(),
        );
        cx.update(|cx| {
            cx.text_system()
                .add_fonts(vec![
                    Cow::Borrowed(
                        include_bytes!("../../../app/fonts/Lilex-Regular.ttf").as_slice(),
                    ),
                    Cow::Borrowed(include_bytes!("../../../app/fonts/Lilex-Bold.ttf").as_slice()),
                ])
                .unwrap();
        });
        let cx = cx.add_empty_window();
        cx.update(|window, _| {
            let printable: String = (0x21u8..=0x7e).map(char::from).collect();
            let lines = [
                printable.as_str(),
                "```md``` A`B`",
                "'node_modules'",
                "hello",
                "a",
                "fn(x)->y;",
                "0x7fff_ffff",
                "[INFO]",
                "https://example.com/a?b=c&d=%20",
            ];
            let fonts = [
                ("Lilex", false, false),
                ("Lilex", true, false),
                ("Consolas", false, false),
                ("Consolas", true, true),
                ("Courier New", false, false),
            ];
            for (family, bold, italic) in fonts {
                let base = Font {
                    family: family.into(),
                    ..with_features(&[("calt", 0)])
                };
                for (size, force) in [13.0, 15.0]
                    .into_iter()
                    .flat_map(|s| [None, Some(8.0), Some(9.5)].map(|w| (s, w)))
                {
                    let (size, force) = (gpui::px(size), force.map(gpui::px));
                    let set = FontSet::new(&base, size);
                    let (f, key) = set.get(bold, italic);
                    if force.is_none() {
                        let started = std::time::Instant::now();
                        let table = AsciiGlyphs::read(f, size, window);
                        eprintln!(
                            "{family} {bold} {italic} {size:?}: check {:?}, pass {}",
                            started.elapsed(),
                            table.is_some()
                        );
                    }
                    // One cache per forced width: `RunKey` keys `forced`, not the width.
                    let mut cache = GlyphCache::new();
                    let mut stats = FrameStats::default();
                    for text in lines {
                        let fast = cache.shape(text, f, key, size, force, window, &mut stats);
                        let shaped = window.text_system().layout_line_by_hash(
                            GlyphCache::text_hash(text),
                            text.len(),
                            size,
                            &[text_run(text.len(), f)],
                            force,
                            || SharedString::from(text.to_string()),
                        );
                        assert!(
                            same_layout(&fast, &shaped),
                            "{family} {size:?} {text:?} {force:?}\n{fast:?}\n{shaped:?}"
                        );
                    }
                    assert_eq!(
                        (stats.ascii_layouts, stats.shape_calls),
                        (lines.len() as u32, lines.len() as u32),
                        "{family}: every ASCII run took the fast path"
                    );
                    // A real shaper: 94 distinct glyphs, not a stub's one id.
                    let table = cache.ascii[&key].as_ref().unwrap();
                    let mut ids: Vec<u32> = table.ids[1..].iter().map(|id| id.0).collect();
                    ids.sort_unstable();
                    ids.dedup();
                    assert_eq!(ids.len(), AsciiGlyphs::LEN - 1, "{family}");
                }
            }
            let proportional = FontSet::new(
                &Font {
                    family: "Segoe UI".into(),
                    ..with_features(&[("calt", 0)])
                },
                gpui::px(13.0),
            );
            let (f, key) = proportional.regular();
            let mut cache = GlyphCache::new();
            let mut stats = FrameStats::default();
            cache.shape("Wil", f, key, gpui::px(13.0), None, window, &mut stats);
            assert_eq!((stats.ascii_layouts, stats.shape_calls), (0, 1));
            assert!(cache.ascii[&key].is_none(), "Segoe UI fails the check");

            // US-0146 F1: Fira Code's `ccmp` turns a backtick after A-Z or a
            // backtick into `grave.case`, with ligatures off; the pair walk sees
            // it, so every variant keeps shaping. Only where it is installed.
            if window
                .text_system()
                .all_font_names()
                .iter()
                .any(|n| n == "Fira Code")
            {
                let fira = FontSet::new(
                    &Font {
                        family: "Fira Code".into(),
                        ..with_features(&[("calt", 0)])
                    },
                    gpui::px(13.0),
                );
                for (bold, italic) in [(false, false), (true, false), (false, true)] {
                    let (f, key) = fira.get(bold, italic);
                    let mut cache = GlyphCache::new();
                    let mut stats = FrameStats::default();
                    let started = std::time::Instant::now();
                    cache.shape("A`B`", f, key, gpui::px(13.0), None, window, &mut stats);
                    eprintln!("Fira Code check: {:?}", started.elapsed());
                    assert_eq!((stats.ascii_layouts, stats.shape_calls), (0, 1));
                    assert!(cache.ascii[&key].is_none(), "Fira Code {bold} {italic}");
                }
            } else {
                eprintln!("Fira Code not installed: its rejection is not exercised");
            }
        });
    }

    #[test]
    fn text_hash_is_stable_and_content_sensitive() {
        assert_eq!(GlyphCache::text_hash("abc"), GlyphCache::text_hash("abc"));
        assert_ne!(GlyphCache::text_hash("abc"), GlyphCache::text_hash("abd"));
    }
}
