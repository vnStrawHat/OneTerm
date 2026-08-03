//! `CompletionParams` — the engine-side projection of the user configuration.
//!
//! A plain struct the `terminal-view` layer fills from `TerminalSettings`, so the
//! engine never depends on `settings` (docs/auto-completion/01 §5). Ranking
//! weights live here with defaults; they are not user-exposed in Phase 1.

/// Per-source enable flags (docs/auto-completion/06 §2).
#[derive(Debug, Clone, Copy)]
pub struct SourceToggles {
    pub memory: bool,
    pub manual: bool,
    pub external: bool,
}

impl Default for SourceToggles {
    fn default() -> Self {
        Self {
            memory: true,
            manual: true,
            external: true,
        }
    }
}

/// The engine-side view of the user config.
#[derive(Debug, Clone, Copy)]
pub struct CompletionParams {
    /// Minimum token length before command suggestions appear (option context
    /// ignores this — a lone trigger always lists options).
    pub min_prefix_len: usize,
    /// Visible overlay rows; the engine caps the returned vector to a bounded
    /// multiple of this to keep render cost predictable.
    pub max_visible_items: usize,
    /// Per-source toggles.
    pub sources: SourceToggles,
    /// Allow fuzzy (subsequence) matches as a secondary pass.
    pub fuzzy: bool,
    /// In subcommand trees, also offer ancestor options (ranked lower).
    pub inherit_ancestor_options: bool,
    /// Let the `Cmd`/`PowerShell` families also search coreutils+linux.
    pub windows_allow_coreutils: bool,
    /// Allow accepting a fuzzy (non-prefix) match (Q3, default off).
    pub allow_fuzzy_accept: bool,
    /// Suggest commands on an empty command token (default off to avoid noise).
    pub suggest_on_empty: bool,
    /// Redaction guard at suggestion time (defense in depth, docs 08 §5).
    pub redact_sensitive: bool,

    // ── ranking weights (not user-exposed in Phase 1) ─────────────────────
    pub w_kind: f32,
    pub w_frec: f32,
    pub w_prefix: f32,
    pub w_len: f32,
}

impl Default for CompletionParams {
    fn default() -> Self {
        Self {
            min_prefix_len: 1,
            max_visible_items: 8,
            sources: SourceToggles::default(),
            fuzzy: true,
            inherit_ancestor_options: true,
            windows_allow_coreutils: false,
            allow_fuzzy_accept: false,
            suggest_on_empty: false,
            redact_sensitive: true,
            w_kind: 10.0,
            w_frec: 6.0,
            w_prefix: 4.0,
            w_len: 1.0,
        }
    }
}

impl CompletionParams {
    /// The hard cap on the returned vector: a bounded multiple of the visible
    /// window so paging works without unbounded work (docs 04 §4.4).
    pub fn hard_cap(&self) -> usize {
        (self.max_visible_items.max(1)).saturating_mul(4)
    }
}
