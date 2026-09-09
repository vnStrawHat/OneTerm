//! `TerminalView` ↔ completion wiring.
//!
//! Reads the live input line from the grid each render, feeds the gpui-free
//! [`CompletionController`](crate::completion::CompletionController), renders
//! the cursor-anchored overlay, and applies the navigation/accept keys the
//! keyboard layer intercepted. History is the cross-tab `CompletionHistory`
//! entity the view receives through its `TerminalDeps`.

use std::time::{SystemTime, UNIX_EPOCH};

use gpui::{Anchor, App, Context, IntoElement, ParentElement as _, anchored, deferred, point, px};

use oneterm_core::config::ShellKind;
use oneterm_terminal::{IndexedCell, SessionKind};

use super::TerminalView;
use crate::completion::{CompletionController, overlay::CompletionOverlay};
use crate::input::CompletionKey;

/// Milliseconds since the Unix epoch — the caller-supplied clock for frecency.
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Compute the scroll window `(offset, count)` into a suggestion list of length
/// `n`, showing at most `max_visible` rows and keeping `selected` (if any) in
/// view. When nothing is selected the window starts at the top.
fn visible_window(n: usize, selected: Option<usize>, max_visible: usize) -> (usize, usize) {
    let max_v = max_visible.max(1);
    if n <= max_v {
        return (0, n);
    }
    let offset = match selected {
        None => 0,
        Some(i) if i < max_v => 0,
        // Keep the selected row at the bottom edge of the window as it scrolls.
        Some(i) => (i + 1 - max_v).min(n - max_v),
    };
    (offset, max_v)
}

/// Auto-completion state owned by the view.
#[derive(Default)]
pub(super) struct CompletionState {
    /// Controller + overlay state. Lazily created on the first render (needs
    /// `cx` to read settings + session kind). `None` until then.
    pub(super) controller: Option<CompletionController>,
    /// Anchor for the overlay: (display line, token-start column) in the grid,
    /// computed during `update_completion`. `None` when hidden.
    anchor: Option<(i32, usize)>,
    /// Last cursor (line, col) seen by `update_completion` — skips the grid
    /// read on frames where the cursor did not move (blink ticks).
    last_cursor: Option<(i32, usize)>,
}

impl CompletionState {
    /// Whether the overlay is showing.
    pub(super) fn is_visible(&self) -> bool {
        self.controller.as_ref().is_some_and(|c| c.is_visible())
    }

    /// Whether a suggestion is highlighted (navigation and Enter bind then).
    pub(super) fn has_selection(&self) -> bool {
        self.controller
            .as_ref()
            .is_some_and(|c| c.selected().is_some())
    }

    /// The `completion.accept_tab` setting as the controller sees it.
    pub(super) fn accept_tab(&self) -> bool {
        self.controller.as_ref().is_some_and(|c| c.accept_tab())
    }

    /// Hide the overlay (controller dismissed, anchor cleared).
    pub(super) fn dismiss(&mut self) {
        if let Some(c) = self.controller.as_mut() {
            c.dismiss();
        }
        self.anchor = None;
    }

    /// Anchor the overlay under the start of the token the user is editing
    /// (when visible), otherwise clear the anchor.
    fn anchor_at(&mut self, cursor: (i32, usize)) {
        let Some(c) = self.controller.as_ref() else {
            self.anchor = None;
            return;
        };
        self.anchor = c.is_visible().then(|| {
            let (line, col) = cursor;
            (line, col.saturating_sub(c.typed_len()))
        });
    }
}

/// The command line read from under the cursor: the text after the prompt, plus
/// whether a prompt prefix was found and where the cursor sits.
struct CursorCommand {
    /// Command text after the prompt prefix, up to the cursor column.
    line: String,
    /// Whether a shell prompt prefix was detected on the row.
    prompt_found: bool,
    /// Cursor position `(display_line, column)` the command was read at.
    anchor: (i32, usize),
}

/// Extract the command-input text on the cursor's row (up to the cursor column),
/// stripped of the shell prompt prefix. `cells` are the cells of the cursor's
/// display row (any other rows are ignored); `cursor` is the cursor's grid
/// `(line, column)` as reported by `TerminalQueryState`.
fn extract_cursor_command(cells: &[IndexedCell], cursor: (i32, usize)) -> CursorCommand {
    let (cursor_line, cursor_col) = cursor;

    let mut row: Vec<char> = Vec::new();
    for ic in cells {
        if ic.point.line.0 != cursor_line {
            continue;
        }
        let c = ic.point.column.0;
        if c >= cursor_col {
            continue;
        }
        while row.len() <= c {
            row.push(' ');
        }
        row[c] = ic.cell.c;
    }
    let row_str: String = row.into_iter().collect();
    let (command, found) = strip_prompt(&row_str);
    CursorCommand {
        line: command,
        prompt_found: found,
        anchor: (cursor_line, cursor_col),
    }
}

/// Strip a shell prompt prefix from a row. Returns `(command_after_prompt,
/// found)`. Best-effort: matches the first `>` (cmd/PowerShell) or
/// `$`/`#`/`❯`/`➜`/`λ` sign followed by a space (POSIX), skipping trailing
/// prompt spaces.
fn strip_prompt(row: &str) -> (String, bool) {
    let chars: Vec<char> = row.chars().collect();
    for (i, &ch) in chars.iter().enumerate() {
        let is_sign = ch == '>'
            || (matches!(ch, '$' | '#' | '❯' | '➜' | 'λ')
                && chars.get(i + 1).is_none_or(|n| *n == ' '));
        if is_sign {
            let mut j = i + 1;
            while chars.get(j) == Some(&' ') {
                j += 1;
            }
            let command: String = chars[j..].iter().collect();
            return (command, true);
        }
    }
    (row.trim_start().to_string(), false)
}

impl TerminalView {
    /// Read the cursor's row from the grid (O(cols) under the lock, no
    /// full-grid clone) together with the cursor position.
    fn cursor_row(&self, cx: &App) -> CursorCommand {
        let session = self.session.read(cx);
        let query = session.query_state();
        // `query_line_range_cells` addresses display rows; the cursor's grid
        // line is offset by the scroll position.
        let display_row = (query.cursor_line + query.display_offset as i32).max(0) as usize;
        let cells = session.query_line_range_cells(display_row, 1).cells;
        extract_cursor_command(&cells, (query.cursor_line, query.cursor_col))
    }

    /// The shell kind the controller completes for: the configured local
    /// shell, or Bash for SSH (remote hosts are virtually always Unix).
    fn completion_shell_kind(&self, cx: &App) -> ShellKind {
        match self.session.read(cx).kind() {
            SessionKind::Local => self.deps.settings.read(cx).shell.kind,
            SessionKind::Ssh => ShellKind::Bash,
        }
    }

    /// Lazily create the completion controller (needs `cx` for settings + kind).
    fn ensure_completion(&mut self, cx: &App) {
        if self.completion.controller.is_some() {
            return;
        }
        let kind = self.completion_shell_kind(cx);
        let settings_entity = self.deps.settings.clone();
        let settings = settings_entity.read(cx);
        let controller = CompletionController::new(kind, &settings.completion);
        log::info!(
            "completion: controller initialized (kind={kind:?}, family={:?}, enabled={})",
            controller.family(),
            controller.enabled()
        );
        self.completion.controller = Some(controller);
    }

    /// Per-render update: sync settings, feed gating signals + the live input
    /// line, and recompute suggestions.
    pub(super) fn update_completion(&mut self, cx: &mut Context<Self>) {
        if self.ssh_closed {
            self.completion.dismiss();
            return;
        }
        self.ensure_completion(cx);

        // Sync settings + master-enable gate, borrowing the settings instead
        // of cloning `CompletionConfig` per frame.
        {
            let kind = self.completion_shell_kind(cx);
            let settings_entity = self.deps.settings.clone();
            let settings = settings_entity.read(cx);
            let Some(c) = self.completion.controller.as_mut() else {
                return;
            };
            c.sync_settings(kind, &settings.completion);
            if !c.enabled() {
                self.completion.dismiss();
                return;
            }
        }

        // Cheap pre-grid gate (enabled + alt screen); the prompt-region gate
        // needs the line and is applied after reading it.
        let (on_alt, cursor_pos) = {
            let session = self.session.read(cx);
            let query = session.query_state();
            (
                session.is_alt_screen(),
                (query.cursor_line, query.cursor_col),
            )
        };
        {
            let Some(c) = self.completion.controller.as_mut() else {
                return;
            };
            c.set_alt_screen(on_alt);
            if !c.pre_gate_ok() {
                self.completion.dismiss();
                return;
            }
        }

        // Skip the grid read when the cursor has not moved and no settings /
        // gating change asked for a recompute (idle blink frames, fast output).
        let cursor_moved = self.completion.last_cursor != Some(cursor_pos);
        let wants = self
            .completion
            .controller
            .as_ref()
            .is_some_and(|c| c.wants_recompute(cursor_moved));
        if !wants {
            return;
        }
        self.completion.last_cursor = Some(cursor_pos);

        let CursorCommand {
            line,
            prompt_found,
            anchor,
        } = self.cursor_row(cx);

        let Some(history_entity) = self.deps.completion_history.clone() else {
            log::warn!("completion: history not initialized — completion disabled");
            self.completion.anchor = None;
            return;
        };
        let now = now_ms();
        {
            let history = history_entity.read(cx);
            let Some(c) = self.completion.controller.as_mut() else {
                return;
            };
            c.set_in_prompt_region(prompt_found);
            if !c.gating_allows() {
                log::debug!("completion: line={line:?} gating=false (hidden)");
                self.completion.dismiss();
                return;
            }
            c.recompute(&line, line.len(), now, history, false);
            log::debug!(
                "completion: line={line:?} prompt_found={prompt_found} visible={} n={}",
                c.is_visible(),
                c.suggestions().len()
            );
        }
        self.completion.anchor_at(anchor);
    }

    /// Force-open the overlay at the cursor (Ctrl+Shift+Space), bypassing
    /// `min_prefix_len`.
    pub(super) fn trigger_completion(&mut self, cx: &mut Context<Self>) {
        self.ensure_completion(cx);
        if self.session.read(cx).is_alt_screen() {
            return;
        }
        let CursorCommand {
            line,
            prompt_found,
            anchor,
        } = self.cursor_row(cx);
        let Some(history_entity) = self.deps.completion_history.clone() else {
            return;
        };
        let now = now_ms();
        {
            let history = history_entity.read(cx);
            let Some(c) = self.completion.controller.as_mut() else {
                return;
            };
            c.set_in_prompt_region(prompt_found);
            c.recompute(&line, line.len(), now, history, true);
        }
        if self.completion.is_visible() {
            self.completion.anchor_at(anchor);
        }
        cx.notify();
    }

    /// The positioned completion overlay element, if visible.
    pub(super) fn completion_overlay_element(&self) -> Option<impl IntoElement> {
        let c = self.completion.controller.as_ref()?;
        if !c.is_visible() {
            return None;
        }
        let (line, col) = self.completion.anchor?;
        let geometry = self.render_state.borrow().geometry?;

        // Only a window of `max_visible` rows, scrolled to keep the selected
        // row in view; the engine keeps more candidates than we show.
        let all = c.suggestions();
        let (offset, count) = visible_window(all.len(), c.selected(), c.max_visible());
        let slice = &all[offset..offset + count];
        let local_selected = c
            .selected()
            .filter(|&i| i >= offset && i < offset + count)
            .map(|i| i - offset);
        let hidden_above = offset;
        let hidden_below = all.len() - (offset + count);

        // Flip the list above the input row when it does not fit below, so it
        // never covers what the user is typing. Rows × line height estimates
        // the overlay height closely enough for the flip decision.
        let line_height = geometry.metrics.line_height;
        let row_count = count + usize::from(hidden_above > 0) + usize::from(hidden_below > 0);
        let est_height = line_height * (row_count as f32) + px(10.0);
        let row = line.max(0) as usize;
        let cell = geometry.cell_origin(row, col);
        let row_bottom = cell.y + line_height;
        let viewport_bottom = geometry.bounds.bottom();
        // `snap_to_window_with_margin` still clamps horizontally on-screen;
        // the explicit corner flip handles vertical placement.
        let (anchor, pos_y) = if est_height <= (viewport_bottom - row_bottom) {
            (Anchor::TopLeft, row_bottom)
        } else {
            (Anchor::BottomLeft, cell.y)
        };

        let overlay =
            CompletionOverlay::new(slice, local_selected, None, hidden_above, hidden_below);
        Some(
            deferred(
                anchored()
                    .snap_to_window_with_margin(px(8.0))
                    .anchor(anchor)
                    .position(point(cell.x, pos_y))
                    .child(overlay),
            )
            .with_priority(1),
        )
    }

    /// Apply a key the keyboard layer routed to the visible overlay.
    pub(super) fn apply_completion_key(&mut self, key: CompletionKey, cx: &mut Context<Self>) {
        match key {
            CompletionKey::Accept => {
                self.completion_accept(cx);
                return;
            }
            CompletionKey::Dismiss => self.completion.dismiss(),
            CompletionKey::SelectFirst | CompletionKey::SelectNext | CompletionKey::SelectPrev => {
                if let Some(c) = self.completion.controller.as_mut() {
                    match key {
                        CompletionKey::SelectFirst => {
                            c.select_first_if_none();
                        }
                        CompletionKey::SelectNext => c.select_next(),
                        _ => c.select_prev(),
                    }
                }
            }
        }
        cx.notify();
    }

    /// Accept the selected suggestion: write its terminal edit bytes, then dismiss.
    fn completion_accept(&mut self, cx: &mut Context<Self>) {
        let bytes = self
            .completion
            .controller
            .as_ref()
            .and_then(|c| c.accept_bytes());
        if let Some(bytes) = bytes
            && !bytes.is_empty()
        {
            log::debug!("completion: accept → write {bytes:?}");
            self.session.update(cx, |s, _| {
                if let Err(e) = s.write(&bytes) {
                    log::warn!("completion: PTY write on accept failed: {e}");
                }
            });
        }
        self.completion.dismiss();
        cx.notify();
    }

    /// Capture the current input line into history when a command runs (Enter
    /// with no active selection).
    pub(super) fn completion_capture_current(&mut self, cx: &mut Context<Self>) {
        let CursorCommand { line, .. } = self.cursor_row(cx);
        if line.trim().is_empty() {
            return;
        }
        let Some(history_entity) = self.deps.completion_history.clone() else {
            return;
        };
        let now = now_ms();
        let Some(controller) = self.completion.controller.as_ref() else {
            return;
        };
        history_entity.update(cx, |h, _| {
            controller.capture(&line, now, h);
        });
        // The line is being submitted → clear any overlay.
        self.completion.dismiss();
    }
}

#[cfg(test)]
mod tests {
    use super::{strip_prompt, visible_window};

    #[test]
    fn strip_cmd_prompt() {
        let (cmd, found) = strip_prompt(r"C:\Users\Trung>d");
        assert!(found);
        assert_eq!(cmd, "d");
    }

    #[test]
    fn strip_unix_prompt() {
        let (cmd, found) = strip_prompt("trung@pc:~/proj$ git c");
        assert!(found);
        assert_eq!(cmd, "git c");
    }

    #[test]
    fn no_prompt_falls_back_to_row() {
        let (cmd, found) = strip_prompt("just some text");
        assert!(!found);
        assert_eq!(cmd, "just some text");
    }

    #[test]
    fn powershell_prompt() {
        let (cmd, found) = strip_prompt(r"PS C:\Users\Trung> Get-Ch");
        assert!(found);
        assert_eq!(cmd, "Get-Ch");
    }

    #[test]
    fn window_shows_all_when_short() {
        assert_eq!(visible_window(5, None, 8), (0, 5));
        assert_eq!(visible_window(5, Some(4), 8), (0, 5));
    }

    #[test]
    fn window_caps_and_starts_at_top_without_selection() {
        assert_eq!(visible_window(32, None, 8), (0, 8));
    }

    #[test]
    fn window_scrolls_to_keep_selection_visible() {
        // Selection within the first window → no scroll.
        assert_eq!(visible_window(32, Some(7), 8), (0, 8));
        // Selecting row 8 scrolls down by one so row 8 sits at the bottom edge.
        assert_eq!(visible_window(32, Some(8), 8), (1, 8));
        // Last row → window clamped to the end.
        assert_eq!(visible_window(32, Some(31), 8), (24, 8));
    }
}
