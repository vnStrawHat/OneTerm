//! `CompletionController` — the terminal-view side of auto-completion.
//!
//! Holds the per-terminal completion state (engine, resolved `ShellFamily`,
//! current suggestion list + selection) and the gating signals (alternate
//! screen, OSC 133 command-input region). It is intentionally **gpui-free** so
//! the gating + accept + capture decisions are unit-testable in isolation
//! (docs/auto-completion/06 §3, 11 §2). The `TerminalView` owns one of these and
//! feeds it the input line, cursor, and screen state; the [`overlay`] module
//! renders `suggestions()`.
//!
//! History lives in the process-global `Entity<CompletionHistory>` (docs 01 §4);
//! the controller receives a `&`/`&mut CompletionHistory` when it needs it so its
//! logic stays free of gpui.

pub mod overlay;

use oneterm_completion::{
    CompletionContext, CompletionHistory, CompletionParams, Engine, ShellFamily, Suggestion, redact,
};
use oneterm_core::ShellKind;
use oneterm_settings::CompletionSettings;

/// Per-terminal completion state + gating.
pub struct CompletionController {
    engine: Engine,
    family: ShellFamily,
    params: CompletionParams,
    // Gating flags mirrored from settings (not part of the engine params).
    enabled: bool,
    accept_tab: bool,
    disable_in_alt_screen: bool,
    require_prompt_region: bool,
    // Live gating signals fed by the view.
    on_alt_screen: bool,
    in_prompt_region: bool,
    // Current results.
    suggestions: Vec<Suggestion>,
    selected: Option<usize>,
    /// The `token`/line prefix the current suggestions were computed against —
    /// used to compute the accept remainder.
    typed_prefix: String,
    // Change-detection so `recompute` is a no-op (preserving selection) when the
    // input line + gating are unchanged since the last render.
    last_line: String,
    last_cursor: usize,
    dirty: bool,
    /// Last-applied settings snapshot — `sync_settings` is a no-op when unchanged.
    settings_snapshot: CompletionSettings,
}

impl CompletionController {
    /// Build a controller for a session whose shell is `kind`, using the live
    /// completion settings.
    pub fn new(kind: ShellKind, settings: &CompletionSettings) -> Self {
        let family = resolve_family(kind, settings);
        Self {
            engine: Engine::from_embedded(),
            family,
            params: params_from_settings(settings),
            enabled: settings.enabled,
            accept_tab: settings.accept_tab,
            disable_in_alt_screen: settings.disable_in_alt_screen,
            require_prompt_region: settings.require_prompt_region,
            on_alt_screen: false,
            in_prompt_region: true,
            suggestions: Vec::new(),
            selected: None,
            typed_prefix: String::new(),
            last_line: String::new(),
            last_cursor: usize::MAX,
            dirty: true,
            settings_snapshot: settings.clone(),
        }
    }

    /// Re-apply settings live if they changed since the last sync (docs 06 §4).
    /// A no-op when the settings are identical, so it is cheap to call per frame.
    pub fn sync_settings(&mut self, kind: ShellKind, settings: &CompletionSettings) {
        if self.settings_snapshot == *settings {
            return;
        }
        log::debug!("completion: settings changed — re-applying");
        self.family = resolve_family(kind, settings);
        self.params = params_from_settings(settings);
        self.enabled = settings.enabled;
        self.accept_tab = settings.accept_tab;
        self.disable_in_alt_screen = settings.disable_in_alt_screen;
        self.require_prompt_region = settings.require_prompt_region;
        self.settings_snapshot = settings.clone();
        self.dirty = true;
    }

    /// The resolved shell family for this session.
    pub fn family(&self) -> ShellFamily {
        self.family
    }

    /// Whether `Tab` should accept the selection (else forward to the shell).
    pub fn accept_tab(&self) -> bool {
        self.accept_tab
    }

    /// Whether auto-completion is enabled at all.
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Update the alternate-screen gating signal.
    pub fn set_alt_screen(&mut self, on: bool) {
        if self.on_alt_screen != on {
            self.on_alt_screen = on;
            self.dirty = true;
            log::debug!(
                "completion: alt-screen = {on} (overlay {})",
                if on { "suppressed" } else { "resumes" }
            );
        }
        if on {
            self.dismiss();
        }
    }

    /// Update the OSC 133 command-input-region gating signal.
    pub fn set_in_prompt_region(&mut self, in_region: bool) {
        if self.in_prompt_region != in_region {
            self.in_prompt_region = in_region;
            self.dirty = true;
        }
    }

    /// Whether gating currently permits showing suggestions (docs 06 §3.3).
    pub fn gating_allows(&self) -> bool {
        self.enabled
            && !(self.disable_in_alt_screen && self.on_alt_screen)
            && (!self.require_prompt_region || self.in_prompt_region)
    }

    /// The cheap pre-grid gate: `enabled` + alt-screen only. Used before the
    /// grid snapshot so we can decide whether to read the input line at all
    /// **without** depending on `in_prompt_region` (which is only known *after*
    /// reading the line — the full [`Self::gating_allows`] is checked then).
    pub fn pre_gate_ok(&self) -> bool {
        self.enabled && !(self.disable_in_alt_screen && self.on_alt_screen)
    }

    /// Whether the overlay currently has anything to show.
    pub fn is_visible(&self) -> bool {
        !self.suggestions.is_empty()
    }

    /// The current suggestion list.
    pub fn suggestions(&self) -> &[Suggestion] {
        &self.suggestions
    }

    /// The selected index, if any (run-first: `None` until the user navigates).
    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    /// Maximum number of rows the overlay should render at once (the scroll
    /// window size). The result list may be longer; the view windows it.
    pub fn max_visible(&self) -> usize {
        self.params.max_visible_items.max(1)
    }

    /// Number of characters of the token/prefix the current suggestions extend
    /// (for anchoring the overlay under what the user typed).
    pub fn typed_len(&self) -> usize {
        self.typed_prefix.chars().count()
    }

    /// Recompute suggestions from the current input line + cursor. `force`
    /// bypasses the `min_prefix_len` gate (the `TriggerCompletion` action).
    /// Clears results if gating forbids them.
    pub fn recompute(
        &mut self,
        line: &str,
        cursor_col: usize,
        now_ms: u64,
        history: &CompletionHistory,
        force: bool,
    ) {
        if !self.gating_allows() {
            self.dismiss();
            return;
        }
        let cursor = cursor_col.min(line.len());
        // No-op when nothing changed since the last render — this preserves the
        // user's selection across frames and avoids recompute/log spam.
        if !force && !self.dirty && line == self.last_line && cursor == self.last_cursor {
            return;
        }
        self.last_line = line.to_string();
        self.last_cursor = cursor;
        self.dirty = false;

        self.typed_prefix = current_typed_prefix(line, cursor);

        let mut params = self.params;
        if force {
            params.min_prefix_len = 0;
            params.suggest_on_empty = true;
        }
        let ctx = CompletionContext {
            family: self.family,
            line,
            cursor_col: cursor,
            now_ms,
        };
        let was_visible = !self.suggestions.is_empty();
        self.suggestions = self.engine.suggest(history, &ctx, &params);
        // Truncate to the visible window for the overlay; engine already caps to
        // a bounded multiple.
        self.suggestions
            .truncate(self.params.max_visible_items.max(1) * 4);
        // Run-first: no selection until the user navigates (docs 09 §4 Q1).
        self.selected = None;

        let now_visible = !self.suggestions.is_empty();
        if now_visible {
            log::debug!(
                "completion: {} suggestion(s) for {:?} (token {:?})",
                self.suggestions.len(),
                line,
                self.typed_prefix
            );
        } else if was_visible {
            log::debug!("completion: overlay hidden for {line:?}");
        }
    }

    /// Whether a recompute is warranted this frame: either the input changed
    /// (proxied by cursor movement) or settings/gating changed. Lets the view
    /// skip the expensive grid snapshot on idle frames (e.g. cursor blink).
    pub fn wants_recompute(&self, cursor_moved: bool) -> bool {
        self.dirty || cursor_moved
    }

    /// Dismiss the overlay (Esc / accept / leaving the prompt region).
    pub fn dismiss(&mut self) {
        self.suggestions.clear();
        self.selected = None;
    }

    /// Move the selection down (clamped). Begins selection at row 0.
    pub fn select_next(&mut self) {
        if self.suggestions.is_empty() {
            return;
        }
        self.selected = Some(match self.selected {
            None => 0,
            Some(i) => (i + 1).min(self.suggestions.len() - 1),
        });
    }

    /// Move the selection up (clamped). Begins selection at the last row.
    pub fn select_prev(&mut self) {
        if self.suggestions.is_empty() {
            return;
        }
        self.selected = Some(match self.selected {
            None => self.suggestions.len() - 1,
            Some(i) => i.saturating_sub(1),
        });
    }

    /// The bytes to append to the PTY to accept the selected suggestion, or
    /// `None` if there is no selection or the accept is not a safe prefix
    /// extension (append-only guarantee, docs 04 §5 / 09 Q3).
    pub fn accept_bytes(&self) -> Option<String> {
        let idx = self.selected?;
        let s = self.suggestions.get(idx)?;
        if s.is_prefix_of_typed(&self.typed_prefix) {
            Some(s.remainder(&self.typed_prefix).to_string())
        } else if self.params.allow_fuzzy_accept {
            // Replace form: caller sends backspaces for typed then full text.
            // Phase 1 keeps this off by default.
            Some(s.text.clone())
        } else {
            None
        }
    }

    /// Whether the currently selected suggestion (if any) accepts via a pure
    /// prefix append.
    pub fn selected_is_prefix_accept(&self) -> bool {
        self.selected
            .and_then(|i| self.suggestions.get(i))
            .map(|s| s.is_prefix_of_typed(&self.typed_prefix))
            .unwrap_or(false)
    }

    /// Capture a just-run command line into history (redacting first, docs 08).
    pub fn capture(&self, raw_command: &str, now_ms: u64, history: &mut CompletionHistory) {
        if !self.enabled || !self.params.sources.memory {
            return;
        }
        // Do not capture while a TUI is running.
        if self.disable_in_alt_screen && self.on_alt_screen {
            return;
        }
        let line = if self.params.redact_sensitive {
            redact::redact(raw_command)
        } else {
            raw_command.trim().to_string()
        };
        if line.is_empty() {
            return;
        }
        log::debug!(
            "completion: capture → history ({:?}): {line:?}",
            self.family
        );
        history.record(self.family, &line, now_ms);
    }
}

/// The text the accept-remainder is computed against: the whole-line prefix for
/// history recall, or just the current token. We use the current token when the
/// cursor is mid/He end of a token, else the whole line prefix. The engine's
/// `replace_from` already encodes which; here we mirror it with the token when
/// present, falling back to the line prefix.
fn current_typed_prefix(line: &str, cursor: usize) -> String {
    let p = oneterm_completion::ParsedLine::parse(line, cursor);
    if p.token.is_empty() {
        // Whole-line recall / subcommand-on-space: use the trimmed line prefix.
        line[..cursor].trim_start().to_string()
    } else {
        p.token
    }
}

/// Resolve the completion family, honoring `force_family` (docs 03 §5, 06).
fn resolve_family(kind: ShellKind, settings: &CompletionSettings) -> ShellFamily {
    settings
        .force_family
        .as_deref()
        .and_then(ShellFamily::from_config_str)
        .unwrap_or_else(|| ShellFamily::from_kind(kind))
}

/// Project the live completion settings into the engine's `CompletionParams`.
pub fn params_from_settings(s: &CompletionSettings) -> CompletionParams {
    let mut p = CompletionParams::default();
    p.min_prefix_len = s.min_prefix_len;
    p.max_visible_items = s.max_visible_items.max(1);
    p.sources = oneterm_completion::SourceToggles {
        memory: s.source_memory && s.max_history > 0,
        manual: s.source_manual,
        external: s.source_external,
    };
    p.fuzzy = s.fuzzy;
    p.inherit_ancestor_options = s.inherit_ancestor_options;
    p.windows_allow_coreutils = s.windows_allow_coreutils;
    p.redact_sensitive = s.redact_sensitive;
    p
}

#[cfg(test)]
mod tests;
