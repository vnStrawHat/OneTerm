//! Per-view semantic overlay — produces per-cell `Class` for the visible
//! viewport.
//!
//! Holds the [`ShellProfile`], optional [`RowRoles`] (from OSC 133, Phase 2),
//! and the enabled flag. The shared [`RuleSet`] is global (built once via
//! `LazyLock`).

use oneterm_highlight::{
    Class, RowRole, RowRoles, RuleSet, ShellProfile, scan_line, scan_line_into,
};

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
            row_roles: RowRoles::default(),
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

    /// Scan one line of display text → `Vec<u8>` of `Class` (one per char).
    ///
    /// When disabled, returns an all-`Default` vec (the caller skips the merge).
    pub fn scan(&self, line: &str, display_row: usize) -> Vec<u8> {
        if !self.enabled {
            return vec![Class::Default as u8; line.chars().count()];
        }
        let rules = RuleSet::global();
        let role = if !self.row_roles.role.is_empty() {
            self.row_roles.role_at(display_row)
        } else {
            RowRole::Output
        };
        scan_line(line, rules, &self.profile, role)
    }

    /// [`Self::scan`] into a caller-owned buffer (cleared and refilled), so a
    /// per-row scan reuses one allocation across frames.
    pub fn scan_into(&self, line: &str, display_row: usize, out: &mut Vec<u8>) {
        out.clear();
        if !self.enabled {
            out.resize(line.chars().count(), Class::Default as u8);
            return;
        }
        let rules = RuleSet::global();
        let role = if !self.row_roles.role.is_empty() {
            self.row_roles.role_at(display_row)
        } else {
            RowRole::Output
        };
        scan_line_into(line, rules, &self.profile, role, out);
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
    fn scan_into_matches_scan_and_reuses_buffer() {
        let o = SemanticOverlay::new(ShellProfile::Unix, true);
        let mut buf = Vec::with_capacity(64);
        let ptr = buf.as_ptr();
        o.scan_into("error: failed", 0, &mut buf);
        assert_eq!(buf, o.scan("error: failed", 0));
        assert_eq!(buf.as_ptr(), ptr, "buffer must be reused");
        let off = SemanticOverlay::new(ShellProfile::Unix, false);
        off.scan_into("error", 0, &mut buf);
        assert_eq!(buf, vec![Class::Default as u8; 5]);
    }

    #[test]
    fn default_overlay_scans() {
        let o = SemanticOverlay::default();
        assert!(o.is_enabled());
        let c = o.scan("$ ls", 0);
        assert_eq!(c[0], Class::PromptSign as u8);
    }
}
