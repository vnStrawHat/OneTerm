//! Shaped-run cache: run text → `ShapedLine`, shared across rows and frames.
//!
//! GPUI's own line-layout cache lives for two frames; this cache keeps a run
//! alive for as long as it is painted (plus two generations of grace when the
//! cap is hit), so an idle terminal never re-shapes. `TextRun.color` does not
//! take part in shaping, so one entry serves every color; colors are applied
//! at paint time from the plan's `ColorSpan`s.

use std::collections::HashMap;

use gpui::{Font, FontStyle, FontWeight, Pixels, ShapedLine, SharedString, TextRun, Window};

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
    line: ShapedLine,
    used: u32,
}

pub(crate) struct GlyphCache {
    map: HashMap<RunKey, Entry>,
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
            map: HashMap::with_capacity(Self::CAPACITY),
            generation: 0,
            capacity: Self::CAPACITY,
        }
    }

    pub(crate) fn begin_frame(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }

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
    pub(crate) fn shape(
        &mut self,
        text: &str,
        font: &Font,
        key: FontKey,
        font_size: Pixels,
        force_width: Option<Pixels>,
        window: &Window,
        stats: &mut FrameStats,
    ) -> ShapedLine {
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
            return entry.line.clone();
        }
        let runs = [TextRun {
            len: text.len(),
            font: font.clone(),
            color: gpui::black(),
            background_color: None,
            underline: None,
            strikethrough: None,
        }];
        let line = window.text_system().shape_line_by_hash(
            text_hash,
            text.len(),
            font_size,
            &runs,
            force_width,
            || SharedString::from(text.to_string()),
        );
        stats.shape_calls += 1;
        if self.map.len() >= self.capacity {
            let cutoff = self.generation.wrapping_sub(Self::GRACE);
            self.map
                .retain(|_, entry| entry.used.wrapping_sub(cutoff) as i32 >= 0);
        }
        self.map.insert(
            run_key,
            Entry {
                line: line.clone(),
                used: self.generation,
            },
        );
        line
    }

    #[cfg(test)]
    pub(crate) fn with_capacity_for_test(capacity: usize) -> Self {
        Self {
            map: HashMap::with_capacity(capacity),
            generation: 0,
            capacity,
        }
    }
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

    #[test]
    fn text_hash_is_stable_and_content_sensitive() {
        assert_eq!(GlyphCache::text_hash("abc"), GlyphCache::text_hash("abc"));
        assert_ne!(GlyphCache::text_hash("abc"), GlyphCache::text_hash("abd"));
    }
}
