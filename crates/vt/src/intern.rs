//! The tables a [`Cell`](crate::cell::Cell) points into.
//!
//! Design:
//! <https://github.com/vnStrawHat/OneTerm/blob/main/docs/spec-intakes/IN-0029-vt-engine/low-level-design/cell-and-style.md>
//!
//! One [`Interner`] per terminal, shared by the primary and the alternate
//! screen, so a sweep is a method on the terminal that destructures its own
//! fields and no id is ambiguous after a screen swap.
//!
//! Two different overflow strategies, deliberately:
//!
//! * **Styles and extras never move.** The ladder is reuse, insert, or fall
//!   back to id 0 with one warning — no renumbering sweep, ever, because a
//!   render copy taken under the lock holds resolved values and ids that a
//!   sweep could invalidate are the design's own worst hazard.
//! * **The grapheme arena is collected**, because unbounded growth there is
//!   attacker-reachable: a stream of unique multi-codepoint cells grows it
//!   without bound. Collection is by remap, and grapheme ids live in the cell's
//!   content bits, so a rewrite touches only cells the caller walks.

use std::fmt;
use std::hash::Hash;

use rustc_hash::FxHashMap;

use crate::cell::{CONTENT_LIMIT, Style};

/// Codepoints kept per cell. Longer clusters are truncated, not rejected.
pub(crate) const GRAPHEME_MAX_LEN: usize = 16;
// An absolute sweep trigger rather than a fraction of the id space: the id
// space is the cell's 21 content bits, and these two constants keep the arena
// inside a few megabytes while making a sweep rare.
/// Entries in the grapheme arena above which a sweep is due.
pub const GRAPHEME_SWEEP_ENTRIES: usize = 65_536;
/// Codepoints in the grapheme arena above which a sweep is due.
pub const GRAPHEME_SWEEP_CHARS: usize = 1 << 20;

/// Id 0 is reserved in both tables, so `65_535` distinct values fit.
const TABLE_LIMIT: usize = 65_535;

/// Interned style id. Id 0 is the default style: it can never be evicted and
/// never fails to resolve, which is what makes the overflow ladder safe — the
/// worst case is wrong colours, never a panic and never a lost cell.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct StyleId(pub u16);

impl StyleId {
    /// The default style. Always resolves, and is never evicted.
    pub const DEFAULT: StyleId = StyleId(0);
}

/// Interned extras id. Id 0 means "no extras".
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct ExtrasId(pub u16);

impl ExtrasId {
    /// No hyperlink and no graphic.
    pub const NONE: ExtrasId = ExtrasId(0);
}

/// Index into the [`GraphemeArena`], stored in a cell's content bits.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct GraphemeId(pub u32);

/// Identity of one OSC 8 hyperlink, per terminal.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct HyperlinkId(pub u32);

/// Identity of one image. Per terminal, starts at 1, not reset by RIS; the
/// image data and its placements belong to the graphics module.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct GraphicId(pub u64);

/// The rare per-cell attachments.
///
/// `graphic` says **which** image covers the cell, never where in it: the
/// painter derives the offset from the placement record. That is what makes a
/// whole image one extras entry instead of one per covered cell — a
/// 4096x4096 Sixel covers more cells than the entire `u16` id space.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct Extras {
    /// The `OSC 8` link this cell belongs to, if any.
    pub hyperlink: Option<HyperlinkId>,
    /// The image covering this cell, if any.
    pub graphic: Option<GraphicId>,
}

impl Extras {
    /// Neither a hyperlink nor a graphic; interned at [`ExtrasId::NONE`].
    pub const NONE: Extras = Extras {
        hyperlink: None,
        graphic: None,
    };
}

/// Content-hash interned table on the three-step no-sweep ladder.
///
/// Step 1 reuses an interned value, step 2 inserts, and step 3 — the table is
/// full — returns id 0 and warns once. An id, once issued, is valid for the
/// life of the terminal.
#[derive(Debug)]
pub struct InternTable<T> {
    entries: Vec<T>,
    index: FxHashMap<T, u16>,
    exhausted: u32,
    warned: bool,
    name: &'static str,
}

pub(crate) type StyleSet = InternTable<Style>;
pub(crate) type ExtrasTable = InternTable<Extras>;

impl<T: Copy + Eq + Hash + Default + fmt::Debug> InternTable<T> {
    fn new(name: &'static str) -> Self {
        let default = T::default();
        let mut index = FxHashMap::default();
        index.insert(default, 0);
        Self {
            entries: vec![default],
            index,
            exhausted: 0,
            warned: false,
            name,
        }
    }

    /// Never fails: a full table falls back to id 0.
    pub fn intern(&mut self, value: &T) -> u16 {
        if let Some(&id) = self.index.get(value) {
            return id;
        }
        if self.entries.len() < TABLE_LIMIT {
            let id = self.entries.len() as u16;
            self.entries.push(*value);
            self.index.insert(*value, id);
            self.debug_assert_integrity();
            return id;
        }
        self.exhausted = self.exhausted.saturating_add(1);
        if !self.warned {
            self.warned = true;
            log::warn!(
                "{} table is full at {} entries; further values fall back to id 0 for the \
                 life of this terminal",
                self.name,
                TABLE_LIMIT
            );
        }
        0
    }

    /// Id 0 always resolves, and so does every id this table ever issued.
    pub fn resolve(&self, id: u16) -> &T {
        match self.entries.get(id as usize) {
            Some(value) => value,
            // Unreachable while ids come from `intern`; cheaper than a panic
            // on a stream we do not control.
            None => &self.entries[0],
        }
    }

    /// Interned values, including the default at id 0.
    pub fn entries(&self) -> usize {
        self.entries.len()
    }

    /// How many values were dropped because the table was full.
    pub fn exhausted(&self) -> u32 {
        self.exhausted
    }

    fn debug_assert_integrity(&self) {
        debug_assert_eq!(
            self.entries.len(),
            self.index.len(),
            "{} table index and entries disagree",
            self.name
        );
        debug_assert_eq!(
            self.entries.first(),
            Some(&T::default()),
            "{} table id 0 was overwritten",
            self.name
        );
    }
}

impl Default for StyleSet {
    fn default() -> Self {
        InternTable::new("style")
    }
}

impl Default for ExtrasTable {
    fn default() -> Self {
        InternTable::new("extras")
    }
}

/// One flat arena of codepoints plus a span per interned cluster.
///
/// Identical sequences deduplicate for free, which is the reason to intern
/// rather than to box per cell.
#[derive(Debug)]
pub struct GraphemeArena {
    chars: Vec<char>,
    spans: Vec<(u32, u8)>,
    index: FxHashMap<Box<[char]>, u32>,
    id_limit: u32,
    truncated: u32,
    exhausted: u32,
    truncation_logged: bool,
    exhaustion_logged: bool,
}

impl Default for GraphemeArena {
    fn default() -> Self {
        GraphemeArena::with_id_limit(CONTENT_LIMIT)
    }
}

impl GraphemeArena {
    /// `id_limit` is the size of the cell's content-bit id space. It is a
    /// parameter only so the exhaustion ladder can be driven in a test without
    /// interning two million clusters.
    fn with_id_limit(id_limit: u32) -> Self {
        let mut arena = Self {
            chars: Vec::new(),
            spans: Vec::new(),
            index: FxHashMap::default(),
            id_limit,
            truncated: 0,
            exhausted: 0,
            truncation_logged: false,
            exhaustion_logged: false,
        };
        // Id 0 is the fallback cluster: a full arena degrades a cell to a
        // space rather than losing the grid's consistency.
        arena.insert(&[' ']);
        arena
    }

    /// Never fails. A cluster longer than `GRAPHEME_MAX_LEN` is truncated and
    /// counted; a full arena returns id 0, which resolves to a single space.
    pub fn intern(&mut self, cluster: &[char]) -> GraphemeId {
        let cluster = if cluster.len() > GRAPHEME_MAX_LEN {
            self.truncated = self.truncated.saturating_add(1);
            if !self.truncation_logged {
                self.truncation_logged = true;
                log::debug!("grapheme cluster longer than {GRAPHEME_MAX_LEN} codepoints truncated");
            }
            &cluster[..GRAPHEME_MAX_LEN]
        } else {
            cluster
        };
        if cluster.is_empty() {
            return GraphemeId(0);
        }
        if let Some(&id) = self.index.get(cluster) {
            return GraphemeId(id);
        }
        if self.spans.len() as u32 >= self.id_limit {
            self.exhausted = self.exhausted.saturating_add(1);
            if !self.exhaustion_logged {
                self.exhaustion_logged = true;
                log::warn!(
                    "grapheme arena is full at {} entries; further clusters render as a space",
                    self.id_limit
                );
            }
            return GraphemeId(0);
        }
        GraphemeId(self.insert(cluster))
    }

    fn insert(&mut self, cluster: &[char]) -> u32 {
        let offset = self.chars.len() as u32;
        self.chars.extend_from_slice(cluster);
        self.spans.push((offset, cluster.len() as u8));
        let id = (self.spans.len() - 1) as u32;
        self.index.insert(cluster.into(), id);
        debug_assert_eq!(
            self.spans.len(),
            self.index.len(),
            "grapheme index and spans disagree"
        );
        debug_assert!(
            offset as usize + cluster.len() <= self.chars.len(),
            "grapheme span outside the arena"
        );
        id
    }

    /// An unknown id resolves to an empty cluster rather than panicking.
    pub fn resolve(&self, id: GraphemeId) -> &[char] {
        let Some(&(offset, len)) = self.spans.get(id.0 as usize) else {
            return &[];
        };
        let start = offset as usize;
        self.chars.get(start..start + len as usize).unwrap_or(&[])
    }

    /// Whether the arena has grown past the documented collection trigger.
    pub fn needs_sweep(&self) -> bool {
        self.spans.len() >= GRAPHEME_SWEEP_ENTRIES || self.chars.len() >= GRAPHEME_SWEEP_CHARS
    }

    /// Collect by remap: steal the arena, re-intern only the ids handed in, and
    /// return the old-to-new map the caller rewrites its cells with.
    ///
    /// The caller runs this only at the end of `feed`, never mid-sequence, so
    /// no borrowed id is live across it.
    pub fn sweep(&mut self, live: impl IntoIterator<Item = GraphemeId>) -> GraphemeRemap {
        let old = std::mem::replace(self, GraphemeArena::with_id_limit(self.id_limit));
        // The counters are session telemetry, not arena state.
        self.truncated = old.truncated;
        self.exhausted = old.exhausted;
        self.truncation_logged = old.truncation_logged;
        self.exhaustion_logged = old.exhaustion_logged;

        let mut map = FxHashMap::default();
        for id in live {
            if map.contains_key(&id.0) {
                continue;
            }
            let new = self.intern(old.resolve(id));
            map.insert(id.0, new.0);
        }
        GraphemeRemap(map)
    }

    /// Clusters truncated at `GRAPHEME_MAX_LEN`.
    pub fn truncated(&self) -> u32 {
        self.truncated
    }

    /// Clusters dropped because the id space was full.
    pub fn exhausted(&self) -> u32 {
        self.exhausted
    }

    /// Interned clusters, including the reserved id 0.
    pub fn entries(&self) -> usize {
        self.spans.len()
    }

    /// Codepoints held by the arena.
    pub fn chars(&self) -> usize {
        self.chars.len()
    }
}

/// Old grapheme id to new, produced by [`GraphemeArena::sweep`].
#[derive(Debug, Default)]
pub struct GraphemeRemap(FxHashMap<u32, u32>);

impl GraphemeRemap {
    /// An id the sweep was not told about resolves to id 0 (a space), which is
    /// the same degradation a full arena produces.
    pub fn get(&self, id: GraphemeId) -> GraphemeId {
        GraphemeId(self.0.get(&id.0).copied().unwrap_or(0))
    }

    /// How many ids the sweep kept.
    pub fn kept(&self) -> usize {
        self.0.len()
    }
}

/// Links kept per terminal, matching the other interned tables' bound.
///
/// A link **without** an explicit `id=` gets a fresh implicit id on every
/// occurrence, so a stream of un-`id=`-ed `OSC 8` links would otherwise grow the
/// table and its index map without limit, which any hostile stream can reach.
pub(crate) const HYPERLINK_TABLE_LIMIT: usize = 65_535;

/// One OSC 8 hyperlink.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Hyperlink {
    /// The stream's `id=` parameter, or a per-terminal counter rendered as
    /// decimal when the stream gave none.
    pub id: Box<str>,
    /// The link target, as the stream gave it. Never resolved or validated
    /// here.
    pub uri: Box<str>,
    /// Whether `id` was generated rather than given. The two spaces overlap:
    /// `OSC 8 ; id=42 ; ...` and the forty-second implicit link both spell
    /// their id `42`, so only this flag distinguishes them.
    pub implicit: bool,
}

/// Interned hyperlinks.
///
/// The implicit id counter is **per terminal**, not process-global, so two
/// sessions cannot collide and a test is deterministic.
#[derive(Debug, Default)]
pub struct HyperlinkTable {
    entries: Vec<Hyperlink>,
    index: FxHashMap<(Box<str>, Box<str>), u32>,
    next_implicit: u32,
    exhausted: u32,
}

impl HyperlinkTable {
    /// The three-step ladder: reuse an interned link, insert a new one, or —
    /// when the table is full — return `None` so the caller drops the hyperlink
    /// attribute for that cell. The text still renders; the link is simply not
    /// clickable.
    pub fn intern(&mut self, id: Option<&str>, uri: &str) -> Option<HyperlinkId> {
        let (id, implicit): (Box<str>, bool) = match id {
            // Explicit ids identify a link run across cells and rows, so they
            // deduplicate; an implicit link is one occurrence and gets a fresh
            // identity.
            Some(id) => {
                let key = (Box::from(id), Box::from(uri));
                if let Some(&existing) = self.index.get(&key) {
                    return Some(HyperlinkId(existing));
                }
                (key.0, false)
            }
            None => {
                self.next_implicit = self.next_implicit.saturating_add(1);
                (self.next_implicit.to_string().into_boxed_str(), true)
            }
        };
        if self.entries.len() >= HYPERLINK_TABLE_LIMIT {
            self.exhausted = self.exhausted.saturating_add(1);
            return None;
        }
        let uri: Box<str> = Box::from(uri);
        let key = (id.clone(), uri.clone());
        let new_id = self.entries.len() as u32;
        self.entries.push(Hyperlink { id, uri, implicit });
        self.index.insert(key, new_id);
        Some(HyperlinkId(new_id))
    }

    /// The link behind an id, or `None` if the id is not from this terminal.
    pub fn resolve(&self, id: HyperlinkId) -> Option<&Hyperlink> {
        self.entries.get(id.0 as usize)
    }

    /// Links interned so far.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether no link has been interned yet.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Links dropped because the table was full.
    pub fn exhausted(&self) -> u32 {
        self.exhausted
    }

    /// `RIS` and a full reset recycle the implicit ids, so a long session that
    /// never repeats a URI cannot accumulate across a clear-and-restart cycle.
    /// Every live cell is blanked by the same reset, so no id is orphaned.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.index.clear();
        self.next_implicit = 0;
    }
}

/// Every table a cell points into, owned by the terminal rather than by a
/// screen: one pair shared by the primary and the alternate screen.
#[derive(Debug, Default)]
pub struct Interner {
    /// Distinct [`Style`] values, addressed by [`StyleId`].
    pub styles: StyleSet,
    /// Distinct [`Extras`] values, addressed by [`ExtrasId`].
    pub extras: ExtrasTable,
    /// Grapheme clusters, addressed by [`GraphemeId`].
    pub graphemes: GraphemeArena,
    /// `OSC 8` links, addressed by [`HyperlinkId`].
    pub hyperlinks: HyperlinkTable,
}

impl Interner {
    /// Never fails; falls back to [`StyleId::DEFAULT`].
    pub fn style(&mut self, style: &Style) -> StyleId {
        StyleId(self.styles.intern(style))
    }

    /// Id 0 always resolves.
    pub fn resolve_style(&self, id: StyleId) -> &Style {
        self.styles.resolve(id.0)
    }

    /// Never fails; falls back to [`ExtrasId::NONE`].
    pub fn extras(&mut self, extras: &Extras) -> ExtrasId {
        ExtrasId(self.extras.intern(extras))
    }

    /// [`ExtrasId::NONE`] always resolves.
    pub fn resolve_extras(&self, id: ExtrasId) -> &Extras {
        self.extras.resolve(id.0)
    }

    /// Interns a cluster, truncated to 16 codepoints.
    pub fn grapheme(&mut self, cluster: &[char]) -> GraphemeId {
        self.graphemes.intern(cluster)
    }

    /// The codepoints behind a grapheme id. Unknown ids resolve to a space.
    pub fn resolve_grapheme(&self, id: GraphemeId) -> &[char] {
        self.graphemes.resolve(id)
    }

    /// Whether the grapheme arena has passed [`GRAPHEME_SWEEP_ENTRIES`] or
    /// [`GRAPHEME_SWEEP_CHARS`].
    pub fn needs_grapheme_sweep(&self) -> bool {
        self.graphemes.needs_sweep()
    }
}

// Kept in a sibling file (the suite is substantial) while staying
// `intern::tests`.
#[cfg(test)]
#[path = "intern_tests.rs"]
mod tests;
