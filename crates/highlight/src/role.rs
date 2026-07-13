//! Row roles derived from OSC 133 shell-integration markers.
//!
//! Each display row is classified as `Output`, `Prompt`, or `Command` from the
//! OSC 133 stream (`PromptStart`/`PromptEnd`/`OutputStart`/`OutputEnd`). When
//! shell integration is absent, the scanner falls back to the `ShellProfile`
//! prompt regex (see [`crate::profile`]).
//!
//! See §4.2 and §Q1 of the design doc.

/// The role of one display row — drives the scanner's starting state.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RowRole {
    /// Normal command output (the default — no prompt, no command input).
    #[default]
    Output = 0,
    /// A prompt row (between `PromptStart` and `PromptEnd`).
    Prompt = 1,
    /// A command-input row (between `PromptEnd` and `OutputStart`).
    Command = 2,
}

impl RowRole {
    /// Convert a raw `u8` to a `RowRole`, defaulting to `Output`.
    pub const fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Prompt,
            2 => Self::Command,
            _ => Self::Output,
        }
    }
}

/// Per-display-row roles + exit codes, rebuilt from the OSC 133 stream.
///
/// When `RowRoles` is present (shell emits OSC 133), the scanner uses
/// authoritative row roles instead of prompt-regex guessing. When absent, the
/// caller passes [`RowRole::Output`] to every row and the scanner's prompt
/// regex fallback determines the state.
#[derive(Clone, Debug, Default)]
pub struct RowRoles {
    /// Per display row → `RowRole` (as `u8`).
    pub role: Box<[u8]>,
    /// Exit code of the command that produced each `Output` row.
    /// `None` for prompt/command rows or when no `OutputEnd` was received.
    pub exit_code: Box<[Option<i32>]>,
}

impl RowRoles {
    /// Create an empty `RowRoles` (no shell integration — all rows are `Output`).
    pub fn empty() -> Self {
        Self::default()
    }

    /// Create a `RowRoles` with all rows set to `Output` and no exit codes.
    pub fn new_output(num_rows: usize) -> Self {
        Self {
            role: vec![RowRole::Output as u8; num_rows].into_boxed_slice(),
            exit_code: vec![None; num_rows].into_boxed_slice(),
        }
    }

    /// Get the role for a display row (clamped to `Output` if out of range).
    pub fn role_at(&self, row: usize) -> RowRole {
        self.role
            .get(row)
            .copied()
            .map(RowRole::from_u8)
            .unwrap_or_default()
    }

    /// Get the exit code for a display row.
    pub fn exit_code_at(&self, row: usize) -> Option<i32> {
        self.exit_code.get(row).copied().flatten()
    }

    /// Whether any shell-integration data is present (non-empty role array).
    pub fn is_present(&self) -> bool {
        !self.role.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_is_absent() {
        assert!(!RowRoles::empty().is_present());
    }

    #[test]
    fn new_output_all_output() {
        let r = RowRoles::new_output(5);
        assert!(r.is_present());
        for i in 0..5 {
            assert_eq!(r.role_at(i), RowRole::Output);
            assert_eq!(r.exit_code_at(i), None);
        }
    }

    #[test]
    fn role_at_out_of_range_defaults() {
        let r = RowRoles::new_output(2);
        assert_eq!(r.role_at(100), RowRole::Output);
    }
}
