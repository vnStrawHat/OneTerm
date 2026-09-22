//! Row roles derived from OSC 133 shell-integration markers.
//!
//! Each display row is classified as `Output`, `Prompt`, or `Command` from the
//! OSC 133 regions the engine stamps on the cells themselves
//! (`OSC 133;A/B/C` -> `Semantic::Prompt`/`Input`/`Output`). A row the shell
//! said nothing about has **no** role, and the scanner falls back to the
//! [`ShellProfile`](crate::ShellProfile) prompt regex for it.
//!
//! See §4.2 and §Q1 of the design doc.

use std::ops::Range;

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

/// Per-display-row roles for one viewport, derived from the OSC 133 regions the
/// cells carry.
///
/// `None` at a row means **this row carries no mark**, which is decided per row
/// and not per session: a session that starts under a shell without integration
/// and gains it later, or that prints unmarked output between two marked
/// prompts, gets the right answer for each row. An unmarked row falls back to
/// the prompt regex; a marked one never runs it.
///
/// A row's role is the role of the **logical line** it belongs to, so every row
/// of a wrapped prompt carries `Prompt` — which is what lets the prompt-line
/// background cover the whole run (`US-0134`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RowRoles {
    /// Per display row -> its logical line's role, or `None` when unmarked.
    pub role: Vec<Option<RowRole>>,
}

impl RowRoles {
    /// Clear every row and resize to `rows` unmarked entries, keeping the
    /// allocation so a per-frame rebuild allocates nothing.
    pub fn reset(&mut self, rows: usize) {
        self.role.clear();
        self.role.resize(rows, None);
    }

    /// The role of a display row; `None` when the row carries no mark or is out
    /// of range.
    pub fn role_at(&self, row: usize) -> Option<RowRole> {
        self.role.get(row).copied().flatten()
    }

    /// Whether no row carries a mark (i.e. the shell emits no OSC 133 at all).
    pub fn is_empty(&self) -> bool {
        self.role.iter().all(Option::is_none)
    }

    /// The rows of the prompt whose command has **finished**: the last prompt
    /// run that has another prompt run below it.
    ///
    /// The engine reports `OSC 133;D` as an event with no row attached, so the
    /// only fact available is the backend's `last_exit_code` — the code of the
    /// most recently completed block. The prompt that launched it is the
    /// second-to-last prompt on screen: a newer prompt below it is exactly the
    /// evidence that its command ended. While a command is still running there
    /// is no prompt below it, so nothing is tinted with a stale code.
    ///
    /// The whole run is returned rather than one row, because the prompt sign of
    /// a wrapped prompt sits on whichever row the cwd stopped on.
    ///
    /// `wraps` is the frame's per-row `WRAPLINE` flags, and it is what separates
    /// two prompts printed on adjacent rows (a command that output nothing)
    /// from one prompt that wrapped. Without it the two are the same run and the
    /// tint lands on the wrong prompt.
    pub fn last_completed_prompt(&self, wraps: &[bool]) -> Option<Range<usize>> {
        let is_prompt = |row: usize| self.role_at(row) == Some(RowRole::Prompt);
        let mut runs: Vec<Range<usize>> = Vec::new();
        let mut row = 0;
        while row < self.role.len() {
            if !is_prompt(row) {
                row += 1;
                continue;
            }
            let start = row;
            // One logical line: extend only across a soft wrap.
            while wraps.get(row).copied().unwrap_or(false) && is_prompt(row + 1) {
                row += 1;
            }
            row += 1;
            runs.push(start..row);
        }
        (runs.len() >= 2).then(|| runs.swap_remove(runs.len() - 2))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const P: Option<RowRole> = Some(RowRole::Prompt);
    const O: Option<RowRole> = Some(RowRole::Output);
    const N: Option<RowRole> = None;

    fn roles(rows: &[Option<RowRole>]) -> RowRoles {
        RowRoles {
            role: rows.to_vec(),
        }
    }

    #[test]
    fn empty_is_absent() {
        assert!(RowRoles::default().is_empty());
    }

    #[test]
    fn reset_clears_to_unmarked() {
        let mut r = roles(&[P, O]);
        r.reset(5);
        assert!(r.is_empty());
        assert_eq!(r.role.len(), 5);
        assert_eq!(r.role_at(0), None);
    }

    #[test]
    fn role_at_out_of_range_is_unmarked() {
        assert_eq!(roles(&[P]).role_at(100), None);
    }

    #[test]
    fn absence_is_per_row_not_per_viewport() {
        // A session that starts unmarked and becomes marked mid-session.
        let r = roles(&[N, N, P, O]);
        assert!(!r.is_empty());
        assert_eq!(r.role_at(1), None);
        assert_eq!(r.role_at(2), P);
    }

    /// No soft wrap anywhere: every row is its own logical line.
    fn hard(rows: usize) -> Vec<bool> {
        vec![false; rows]
    }

    #[test]
    fn last_completed_prompt_is_the_one_above_the_newest() {
        // prompt, its output, the new prompt: the first one's command finished.
        let r = roles(&[P, O, O, P]);
        assert_eq!(r.last_completed_prompt(&hard(4)), Some(0..1));
    }

    #[test]
    fn a_wrapped_prompt_is_tinted_as_a_whole_run() {
        let r = roles(&[P, P, O, P, P]);
        let wraps = vec![true, false, false, true, false];
        assert_eq!(r.last_completed_prompt(&wraps), Some(0..2));
    }

    #[test]
    fn a_running_command_tints_the_previous_prompt_not_its_own() {
        // prompt A, output A, prompt B, output B still streaming: the newest
        // prompt is B, so A is the completed one and B stays untinted.
        let r = roles(&[P, O, P, O, O]);
        assert_eq!(r.last_completed_prompt(&hard(5)), Some(0..1));
    }

    /// A command that printed nothing leaves two prompts on adjacent rows. They
    /// are two logical lines, not one wrapped prompt, and the newer one's code
    /// must tint the older one.
    #[test]
    fn two_prompts_on_adjacent_rows_are_two_runs() {
        let r = roles(&[P, P]);
        assert_eq!(r.last_completed_prompt(&hard(2)), Some(0..1));
        // The same two rows joined by a soft wrap are one prompt, and one
        // prompt is never tinted.
        assert_eq!(r.last_completed_prompt(&[true, false]), None);
    }

    #[test]
    fn a_single_prompt_on_screen_is_not_tinted() {
        assert_eq!(roles(&[P, O, O]).last_completed_prompt(&hard(3)), None);
        assert_eq!(roles(&[N, N]).last_completed_prompt(&hard(2)), None);
    }
}
