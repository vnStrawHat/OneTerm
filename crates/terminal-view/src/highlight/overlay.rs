//! Per-view semantic overlay — produces per-cell `Class` for the visible
//! viewport.
//!
//! Holds the [`ShellProfile`], the exit code of the most recently completed
//! command block, and the enabled flag. The shared [`RuleSet`] is global (built
//! once via `LazyLock`).
//!
//! The per-row roles are **not** here: they are derived from the frame's own
//! cells once per rescan and owned by the plan cache, next to the URL masks and
//! the classes that share their lifetime (`US-0133`).

use oneterm_highlight::{Class, RowRole, RuleSet, ShellProfile, scan_line_into};

/// Per-view semantic overlay — produces `cell_class` for one logical line.
#[derive(Clone)]
pub struct SemanticOverlay {
    profile: ShellProfile,
    /// `OSC 133;D` of the most recently completed command block, when the
    /// viewport is at the bottom. The engine attaches no row to it, so which
    /// prompt it belongs to is decided from the roles (`RowRoles`), not here.
    exit_code: Option<i32>,
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
            exit_code: None,
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

    /// The exit code that tints the most recently completed block's prompt
    /// sign, or `None` when there is none to show.
    pub fn exit_code(&self) -> Option<i32> {
        self.exit_code
    }

    /// Set the exit code of the most recently completed command block.
    pub fn set_exit_code(&mut self, exit_code: Option<i32>) {
        self.exit_code = exit_code;
    }

    /// Scan one **logical** line of display text into a caller-owned buffer of
    /// `Class` bytes (one per char, cleared and refilled), so a per-frame scan
    /// reuses one allocation. When disabled the buffer is all `Default`.
    ///
    /// `line` is the whole logical line — a wrap-connected run of display rows
    /// joined into one string. `role` is what the shell's OSC 133 marks say
    /// about it, and `None` means the shell said nothing, which is the only
    /// case where the prompt regex runs. `input_at` is the char index where the
    /// typed command starts (`OSC 133;B`).
    pub fn scan_into(
        &self,
        line: &str,
        role: Option<RowRole>,
        input_at: Option<usize>,
        out: &mut Vec<u8>,
    ) {
        out.clear();
        if !self.enabled {
            out.resize(line.chars().count(), Class::Default as u8);
            return;
        }
        let rules = RuleSet::global();
        scan_line_into(line, rules, &self.profile, role, input_at, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan(o: &SemanticOverlay, line: &str) -> Vec<u8> {
        let mut out = Vec::new();
        o.scan_into(line, None, None, &mut out);
        out
    }

    #[test]
    fn disabled_returns_all_default() {
        let o = SemanticOverlay::new(ShellProfile::Unix, false);
        let c = scan(&o, "error: failed");
        assert_eq!(c.len(), "error: failed".chars().count());
        assert!(c.iter().all(|&v| v == Class::Default as u8));
    }

    #[test]
    fn enabled_tags_keywords() {
        let o = SemanticOverlay::new(ShellProfile::Unix, true);
        let c = scan(&o, "error: failed");
        assert_eq!(c[0], Class::Error as u8);
    }

    #[test]
    fn scan_into_reuses_the_buffer() {
        let o = SemanticOverlay::new(ShellProfile::Unix, true);
        let mut buf = Vec::with_capacity(64);
        let ptr = buf.as_ptr();
        o.scan_into("error: failed", None, None, &mut buf);
        assert_eq!(buf, scan(&o, "error: failed"));
        assert_eq!(buf.as_ptr(), ptr, "buffer must be reused");
        let off = SemanticOverlay::new(ShellProfile::Unix, false);
        off.scan_into("error", None, None, &mut buf);
        assert_eq!(buf, vec![Class::Default as u8; 5]);
    }

    #[test]
    fn default_overlay_scans() {
        let o = SemanticOverlay::default();
        assert!(o.is_enabled());
        let c = scan(&o, "$ ls");
        assert_eq!(c[0], Class::PromptSign as u8);
    }

    /// A marked row never reaches the prompt regex: the same text that the
    /// fallback reads as a prompt is left to the output matchers.
    #[test]
    fn a_marked_output_row_skips_the_prompt_regex() {
        let o = SemanticOverlay::new(ShellProfile::Unix, true);
        let mut marked = Vec::new();
        o.scan_into("$ ls", Some(RowRole::Output), None, &mut marked);
        assert_ne!(marked[0], Class::PromptSign as u8, "{marked:?}");
        assert_eq!(scan(&o, "$ ls")[0], Class::PromptSign as u8);
    }

    #[test]
    fn a_marked_prompt_uses_the_input_boundary() {
        let o = SemanticOverlay::new(ShellProfile::PowerShell, true);
        let line = r"PS C:\Program Files> dir";
        let sign = line.find('>').unwrap();
        let mut out = Vec::new();
        o.scan_into(line, Some(RowRole::Prompt), Some(sign + 2), &mut out);
        assert_eq!(out[sign], Class::PromptSign as u8, "{out:?}");
        assert_eq!(out[sign + 2], Class::Command as u8, "{out:?}");
    }

    #[test]
    fn the_exit_code_round_trips() {
        let mut o = SemanticOverlay::default();
        assert_eq!(o.exit_code(), None);
        o.set_exit_code(Some(3));
        assert_eq!(o.exit_code(), Some(3));
    }
}
