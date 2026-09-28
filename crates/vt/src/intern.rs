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
//!   sweep could invalidate are the design's own worst hazard. An id an
//!   [`InternTable`] hands out still resolves to the same value for as long
//!   as anything can reach it. The extras table alone also frees ids, through
//!   `InternTable::sweep_unreferenced` — a mark phase over the *whole grid*
//!   supplies the proof (nothing else can), so freeing an id one at a time
//!   from a single call site, trusting that call site's own bookkeeping to
//!   know when nothing references it, was tried and rejected: a placement's
//!   release does not mean its cells are gone (eviction, `IL`/`SD` splitting
//!   a placement outside its tracked extent, and a history trim all leave
//!   live cells behind), so nothing short of reading every cell proves an id
//!   is free. Freeing still moves nothing else: a swept id's slot is the
//!   first one [`intern`](InternTable::intern) reuses, and every other id
//!   keeps its value.
//! * **The grapheme arena is collected**, because unbounded growth there is
//!   attacker-reachable: a stream of unique multi-codepoint cells grows it
//!   without bound. Collection is by remap, and grapheme ids live in the cell's
//!   content bits, so a rewrite touches only cells the caller walks.

use std::fmt;
use std::hash::Hash;

use rustc_hash::{FxHashMap, FxHashSet};

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

/// New entries since the last [`InternTable::sweep_unreferenced`] above which
/// another sweep is due (`needs_sweep`). An absolute count, not a fraction of
/// [`TABLE_LIMIT`], for the same reason [`GRAPHEME_SWEEP_ENTRIES`] is one: it
/// keeps a sweep rare for an ordinary session and still guarantees one long
/// before a hostile stream's distinct values could fill the table. Unused by
/// a table nothing ever calls [`InternTable::sweep_unreferenced`] on (styles,
/// today).
const TABLE_SWEEP_INTERVAL: usize = 4_096;

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

/// Content-hash interned table on the three-step no-sweep-by-default ladder.
///
/// Step 1 reuses an interned value, step 2 inserts, and step 3 — the table is
/// full — returns id 0 and warns once. An id, once issued, resolves to the
/// same value for as long as anything can still reach it. Three things end
/// that: `RIS`, which empties the extras table wholesale because it first
/// blanks every cell that could hold one of its ids; `free`, called once an
/// id is individually proven unreferenced; and `sweep_unreferenced`, which
/// proves a whole batch of ids unreferenced at once by reading every cell
/// that can name one (its caller's job — this type has no access to a grid)
/// and frees everything the read did not find live. A freed id's slot is the
/// first one [`intern`](Self::intern) reuses, so a table an owner keeps
/// sweeping stays bounded by what is live, not by how many values ever
/// passed through it.
#[derive(Debug)]
pub struct InternTable<T> {
    entries: Vec<T>,
    index: FxHashMap<T, u16>,
    /// Ids [`free`](Self::free) released, waiting for
    /// [`intern`](Self::intern) to hand them back out. Empty for a table
    /// nothing ever frees from (styles, today).
    free: FxHashSet<u16>,
    /// New entries pushed since the last [`sweep_unreferenced`](Self::sweep_unreferenced).
    since_sweep: u32,
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
            free: FxHashSet::default(),
            since_sweep: 0,
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
        // `since_sweep` counts every index miss below -- a free-list reuse
        // as much as a push -- not only growth. A sweep frees `F` ids, and
        // the next `F` calls all land here on the free-list branch; if only
        // pushes counted, those `F` calls would count for nothing, so the
        // table would have to grow by a full `TABLE_SWEEP_INTERVAL` every
        // cycle before the next sweep could ever run, without bound, until
        // it filled `TABLE_LIMIT` and then, past the last push, could never
        // trigger another sweep at all. Counting every miss instead bounds a
        // sweep to one per `TABLE_SWEEP_INTERVAL` *new values*, whichever
        // slot they land in, which is the cost the table's owner already
        // accepted, and it never lets the table both fill and go silent.
        if let Some(&id) = self.free.iter().next() {
            self.free.remove(&id);
            self.entries[id as usize] = *value;
            self.index.insert(*value, id);
            self.since_sweep = self.since_sweep.saturating_add(1);
            self.debug_assert_integrity();
            return id;
        }
        if self.entries.len() < TABLE_LIMIT {
            let id = self.entries.len() as u16;
            self.entries.push(*value);
            self.index.insert(*value, id);
            self.since_sweep = self.since_sweep.saturating_add(1);
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

    /// Back to the default value alone, for `RIS`, which blanks every cell and
    /// pen that could hold an id. The exhaustion count and the warn-once flag
    /// are session telemetry and survive.
    pub(crate) fn clear(&mut self) {
        let default = T::default();
        self.entries.clear();
        self.entries.push(default);
        self.index.clear();
        self.index.insert(default, 0);
        self.free.clear();
        self.since_sweep = 0;
    }

    /// Give back one id: the caller has proven nothing resolves to it any
    /// more, so the next [`intern`](Self::intern) for a new value can reuse
    /// its slot instead of growing the table. Id 0, the default, is not
    /// owned by any one value and can never be freed. Idempotent in every
    /// build, not only under `debug_assert!`: a caller (or
    /// [`sweep_unreferenced`](Self::sweep_unreferenced)) that frees the same
    /// id twice does nothing the second time, rather than handing it out
    /// twice from the free list.
    ///
    /// Until reused, the slot reads back as the default value rather than
    /// keeping the stale one: a caller that still held this id past the
    /// point it proved unreachable gets "nothing" instead of silently
    /// resolving to whatever the next occupant turns out to be.
    pub(crate) fn free(&mut self, id: u16) {
        if id == 0 || self.free.contains(&id) {
            return;
        }
        let Some(&value) = self.entries.get(id as usize) else {
            return;
        };
        if self.index.get(&value) == Some(&id) {
            self.index.remove(&value);
        }
        self.entries[id as usize] = T::default();
        self.free.insert(id);
        self.debug_assert_integrity();
    }

    /// Whether `id` is on the free list: allocated once, and proven
    /// unreferenced by the last [`sweep_unreferenced`](Self::sweep_unreferenced)
    /// or [`free`](Self::free) call, waiting for [`intern`](Self::intern) to
    /// hand it back out.
    pub(crate) fn is_free(&self, id: u16) -> bool {
        self.free.contains(&id)
    }

    /// Interned values, including the default at id 0. Never shrinks: a
    /// freed slot stays counted here until it is reused, because freeing
    /// hands a slot back for reuse without renumbering anything, and there
    /// is no lower id to renumber it to.
    pub fn entries(&self) -> usize {
        self.entries.len()
    }

    /// How many values were dropped because the table was full.
    pub fn exhausted(&self) -> u32 {
        self.exhausted
    }

    /// Whether [`TABLE_SWEEP_INTERVAL`] new entries have been handed out
    /// since the last sweep — a free-list reuse counts exactly like table
    /// growth, or a sweep that only ever counted growth could fill the table
    /// once and then never trigger again. The owner is expected to check
    /// this **before** interning a value it is about to hand out — sweeping
    /// after would find that fresh value unreferenced and free it before its
    /// caller can write it anywhere — and, when true, build the live set by
    /// reading every cell that can carry this table's ids and call
    /// [`sweep_unreferenced`](Self::sweep_unreferenced).
    pub(crate) fn needs_sweep(&self) -> bool {
        self.since_sweep as usize >= TABLE_SWEEP_INTERVAL
    }

    /// Free every allocated id **not** in `live`, and reset the sweep
    /// trigger. `live` must be exactly the ids something can still resolve
    /// through — every cell in both screens' full history, the pen and erase
    /// cell of both the active and the saved cursor, on both screens. A
    /// short count is always safe (it only frees less), a long one is not: an
    /// id missing from `live` that a cell still carries is exactly the hazard
    /// this table exists to prevent, so the caller's scan must be complete,
    /// never a `feed`-local approximation.
    ///
    /// Nothing here is renumbered: a kept id keeps its value and its slot: a
    /// freed one is cleared to the default and pushed onto the free list,
    /// same as [`free`](Self::free) called once per id. `O(entries)`, which
    /// is why [`needs_sweep`](Self::needs_sweep) exists — the caller decides
    /// how rare that cost should be, this method does not.
    pub(crate) fn sweep_unreferenced(&mut self, live: &FxHashSet<u16>) {
        let len = self.entries.len() as u16;
        for id in 1..len {
            if !self.free.contains(&id) && !live.contains(&id) {
                self.free(id);
            }
        }
        self.since_sweep = 0;
    }

    fn debug_assert_integrity(&self) {
        debug_assert_eq!(
            self.entries.len(),
            self.index.len() + self.free.len(),
            "{} table entries, index and free list disagree",
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
/// A link without an explicit `id=` is keyed by its URI, so repainting the same
/// link reuses its entry; the table still grows with every distinct URI and
/// every distinct explicit id, which any hostile stream can reach.
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
    /// Implicit links by URI. Kept apart from `index` because the two id
    /// spaces overlap: explicit `id=1` must not alias implicit link `1`.
    implicit: FxHashMap<Box<str>, u32>,
    next_implicit: u32,
    exhausted: u32,
}

impl HyperlinkTable {
    /// The three-step ladder: reuse an interned link, insert a new one, or —
    /// when the table is full — return `None` so the caller drops the hyperlink
    /// attribute for that cell. The text still renders; the link is simply not
    /// clickable.
    ///
    /// Identity: an explicit link is its `(id, uri)` pair; an implicit link
    /// (no `id=`) is its URI alone, so every occurrence of one URI without an
    /// `id=` shares one id. That makes repainting a link idempotent — a TUI
    /// that redraws the same link every frame holds one entry, not one per
    /// frame. Ids are never renumbered.
    pub fn intern(&mut self, id: Option<&str>, uri: &str) -> Option<HyperlinkId> {
        let existing = match id {
            Some(id) => self.index.get(&(Box::from(id), Box::from(uri))),
            None => self.implicit.get(uri),
        };
        if let Some(&existing) = existing {
            return Some(HyperlinkId(existing));
        }
        if self.entries.len() >= HYPERLINK_TABLE_LIMIT {
            self.exhausted = self.exhausted.saturating_add(1);
            return None;
        }
        let new_id = self.entries.len() as u32;
        let implicit = id.is_none();
        let uri: Box<str> = Box::from(uri);
        let id: Box<str> = match id {
            Some(id) => {
                self.index.insert((Box::from(id), uri.clone()), new_id);
                Box::from(id)
            }
            None => {
                self.next_implicit = self.next_implicit.saturating_add(1);
                self.implicit.insert(uri.clone(), new_id);
                self.next_implicit.to_string().into_boxed_str()
            }
        };
        self.entries.push(Hyperlink { id, uri, implicit });
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
        self.implicit.clear();
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
