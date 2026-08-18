//! `CompletionController` — the gpui-free per-terminal completion state machine.
//!
//! Holds the engine, resolved `ShellFamily`, current suggestion list + selection,
//! and the gating signals (alternate screen, OSC 133 command-input region). It is
//! intentionally **gpui-free** so the gating + accept + capture decisions are
//! unit-testable in isolation (docs/auto-completion/06 §3, 11 §2). The
//! `TerminalView` owns one of these and feeds it the input line, cursor, and
//! screen state; the [`super::overlay`] module renders `suggestions()`.

use oneterm_completion::{
    CompletionContext, CompletionHistory, CompletionParams, Engine, ShellFamily, Suggestion, redact,
};
use oneterm_core::ShellKind;
use oneterm_settings::CompletionConfig;

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
    /// The current token/line prefix, used to anchor the overlay under the text
    /// being edited.
    typed_prefix: String,
    // Change-detection so `recompute` is a no-op (preserving selection) when the
    // input line + gating are unchanged since the last render.
    last_line: String,
    last_cursor: usize,
    dirty: bool,
    /// Last-applied settings snapshot — `sync_settings` is a no-op when unchanged.
    settings_snapshot: CompletionConfig,
}

impl CompletionController {
    /// Build a controller for a session whose shell is `kind`, using the live
    /// completion settings.
    pub fn new(kind: ShellKind, settings: &CompletionConfig) -> Self {
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
    pub fn sync_settings(&mut self, kind: ShellKind, settings: &CompletionConfig) {
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
        if has_sole_exact_match(&self.suggestions, line, cursor) {
            self.suggestions.clear();
        }
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

    /// Select the first suggestion when the list is not yet engaged. Returns
    /// whether selection changed, allowing first Tab to select and later Tab to
    /// accept without duplicating state checks in the view.
    pub fn select_first_if_none(&mut self) -> bool {
        if self.selected.is_some() || self.suggestions.is_empty() {
            return false;
        }
        self.selected = Some(0);
        true
    }

    /// Move an existing selection down (clamped). Does nothing before selection.
    pub fn select_next(&mut self) {
        let Some(selected) = self.selected else {
            return;
        };
        self.selected = Some((selected + 1).min(self.suggestions.len().saturating_sub(1)));
    }

    /// Move an existing selection up (clamped). Does nothing before selection.
    pub fn select_prev(&mut self) {
        let Some(selected) = self.selected else {
            return;
        };
        self.selected = Some(selected.saturating_sub(1));
    }

    /// The terminal bytes that apply the selected suggestion, or `None` when no
    /// selection exists or acceptance would require a fuzzy/non-prefix replace.
    ///
    /// Unix remains exact-case and append-only. Cmd/PowerShell may erase a
    /// case-mismatched suffix with plain Backspace bytes before writing the exact
    /// suggestion casing.
    pub fn accept_bytes(&self) -> Option<Vec<u8>> {
        let idx = self.selected?;
        let suggestion = self.suggestions.get(idx)?;
        let typed = replacement_text(&self.last_line, self.last_cursor, suggestion)?;

        if suggestion.text.starts_with(typed) {
            return Some(suggestion.text.as_bytes()[typed.len()..].to_vec());
        }

        if matches!(self.family, ShellFamily::Cmd | ShellFamily::PowerShell)
            && suggestion.is_prefix_of_typed(typed)
        {
            return case_corrected_acceptance_bytes(&suggestion.text, typed);
        }

        if self.params.allow_fuzzy_accept {
            // The existing opt-in fuzzy path writes the full suggestion. Callers
            // enabling it remain responsible for replacing the typed range.
            Some(suggestion.text.as_bytes().to_vec())
        } else {
            None
        }
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

fn has_sole_exact_match(suggestions: &[Suggestion], line: &str, cursor: usize) -> bool {
    let [suggestion] = suggestions else {
        return false;
    };
    replacement_text(line, cursor, suggestion).is_some_and(|typed| typed == suggestion.text)
}

fn replacement_text<'a>(line: &'a str, cursor: usize, suggestion: &Suggestion) -> Option<&'a str> {
    let typed = line.get(suggestion.replace_from..cursor)?;
    Some(if suggestion.replace_from == 0 {
        typed.trim_start()
    } else {
        typed
    })
}

fn case_corrected_acceptance_bytes(suggestion: &str, typed: &str) -> Option<Vec<u8>> {
    let mismatch = suggestion
        .as_bytes()
        .iter()
        .zip(typed.as_bytes())
        .position(|(suggested, actual)| suggested != actual)?;
    let typed_tail = typed.get(mismatch..)?;
    let suggestion_tail = suggestion.get(mismatch..)?;

    let mut bytes = vec![0x7f; typed_tail.chars().count()];
    bytes.extend_from_slice(suggestion_tail.as_bytes());
    Some(bytes)
}

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
fn resolve_family(kind: ShellKind, settings: &CompletionConfig) -> ShellFamily {
    settings
        .force_family
        .as_deref()
        .and_then(ShellFamily::from_config_str)
        .unwrap_or_else(|| ShellFamily::from_kind(kind))
}

/// Project the live completion settings into the engine's `CompletionParams`.
pub(crate) fn params_from_settings(s: &CompletionConfig) -> CompletionParams {
    let mut p = CompletionParams::default();
    p.min_prefix_len = s.min_prefix_len;
    p.max_visible_items = s.max_visible_items.max(1);
    p.sources = oneterm_completion::SourceToggles {
        memory: s.sources.memory && s.max_history > 0,
        manual: s.sources.manual,
        external: s.sources.external,
    };
    p.fuzzy = s.fuzzy;
    p.inherit_ancestor_options = s.inherit_ancestor_options;
    p.windows_allow_coreutils = s.windows_allow_coreutils;
    p.redact_sensitive = s.redact_sensitive;
    p
}
