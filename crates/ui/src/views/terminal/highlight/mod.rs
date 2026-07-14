//! Semantic overlay — produces per-cell `Class` for the visible viewport.
//!
//! Holds the [`ShellProfile`], a reference to the shared [`RuleSet`], and
//! optional [`RowRoles`] (from OSC 133, Phase 2). The overlay is per-view; the
//! `RuleSet` is global (built once via `LazyLock`).

pub mod bridge;

use oneterm_highlight::{Class, RowRole, RowRoles, RuleSet, ShellProfile, scan_line};

pub use bridge::{load_default_styles, parse_semantic_json, to_gpui, to_gpui_hsla};

/// Per-view semantic overlay — produces `cell_class` for one display row.
///
/// Phase 0/1: `RowRoles` is absent → the scanner uses the `ShellProfile` prompt
/// regex fallback to detect prompt lines. Phase 2 will populate `row_roles`
/// from the OSC 133 stream for authoritative row roles.
#[derive(Clone)]
pub struct SemanticOverlay {
    profile: ShellProfile,
    row_roles: RowRoles,
    /// Whether semantic highlighting is enabled (gated by the setting).
    enabled: bool,
}

impl Default for SemanticOverlay {
    fn default() -> Self {
        Self::new(ShellProfile::default(), true)
    }
}

impl SemanticOverlay {
    /// Create a new overlay with the given shell profile and enabled flag.
    pub fn new(profile: ShellProfile, enabled: bool) -> Self {
        Self {
            profile,
            row_roles: RowRoles::empty(),
            enabled,
        }
    }

    /// Whether the overlay is active (enabled + scanning).
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Get the current shell profile.
    pub fn profile(&self) -> oneterm_highlight::ShellProfile {
        self.profile
    }

    /// Enable or disable the overlay (from the `semantic_highlighting` setting).
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Update the shell profile (e.g. when the session shell kind changes).
    pub fn set_profile(&mut self, profile: ShellProfile) {
        self.profile = profile;
    }

    /// Update the row roles (Phase 2: from OSC 133).
    pub fn set_row_roles(&mut self, roles: RowRoles) {
        self.row_roles = roles;
    }

    /// Scan one line of display text → `Vec<u8>` of `Class` (one per char).
    ///
    /// When disabled, returns an all-`Default` vec (the caller skips the merge).
    pub fn scan(&self, line: &str, display_row: usize) -> Vec<u8> {
        if !self.enabled {
            return vec![Class::Default as u8; line.chars().count()];
        }
        let rules = RuleSet::global();
        let role = if self.row_roles.is_present() {
            self.row_roles.role_at(display_row)
        } else {
            RowRole::Output
        };
        scan_line(line, rules, &self.profile, role)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_returns_all_default() {
        let o = SemanticOverlay::new(ShellProfile::Unix, false);
        let c = o.scan("error: failed", 0);
        assert!(c.iter().all(|&v| v == Class::Default as u8));
    }

    #[test]
    fn enabled_tags_keywords() {
        let o = SemanticOverlay::new(ShellProfile::Unix, true);
        let c = o.scan("error: failed", 0);
        assert_eq!(c[0], Class::Error as u8);
    }

    #[test]
    fn default_overlay_scans() {
        let o = SemanticOverlay::default();
        assert!(o.is_enabled());
        let c = o.scan("$ ls", 0);
        assert_eq!(c[0], Class::PromptSign as u8);
    }
}
