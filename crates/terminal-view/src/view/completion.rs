//! `LocalTerminalView` ↔ completion wiring (docs/auto-completion/05, 06, 07).
//!
//! Reads the live input line from the grid each render, feeds the gpui-free
//! [`CompletionController`](crate::completion::CompletionController), renders the
//! cursor-anchored overlay, and handles the navigation/accept keys before they
//! reach the PTY. History lives in the process-global
//! `oneterm_state::GlobalCompletionHistory`.

use std::time::{SystemTime, UNIX_EPOCH};

use gpui::{Anchor, App, Context, IntoElement, ParentElement as _, anchored, deferred, point, px};

use alacritty_terminal::term::TermMode;
use oneterm_core::config::ShellKind;
use oneterm_settings::TerminalSettings;
use oneterm_state::GlobalCompletionHistory;

use super::LocalTerminalView;
use crate::completion::{CompletionController, overlay::CompletionOverlay};

mod grid;
use grid::{extract_cursor_command, visible_window};

/// Milliseconds since the Unix epoch — the caller-supplied clock for frecency.
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

impl LocalTerminalView {
    /// Lazily create the completion controller (needs `cx` for settings + kind).
    fn ensure_completion(&mut self, cx: &App) {
        if self.completion.is_some() {
            return;
        }
        let settings_entity = TerminalSettings::global(cx);
        let settings = settings_entity.read(cx);
        let kind = if self.session.read(cx).is_local() {
            settings.shell.kind
        } else {
            // SSH targets are virtually always Unix (docs 03 §6).
            ShellKind::Bash
        };
        let controller = CompletionController::new(kind, &settings.completion);
        log::info!(
            "completion: controller initialized (kind={kind:?}, family={:?}, enabled={})",
            controller.family(),
            controller.enabled()
        );
        self.completion = Some(controller);
    }

    /// Per-render update: sync settings, feed gating signals + the live input
    /// line, and recompute suggestions. Called at the top of `render`.
    pub(crate) fn update_completion(&mut self, cx: &mut Context<Self>) {
        self.ensure_completion(cx);

        // Read settings + shell kind (immutable borrows).
        let settings_entity = TerminalSettings::global(cx);
        let (kind, settings_snapshot) = {
            let s = settings_entity.read(cx);
            let kind = if self.session.read(cx).is_local() {
                s.shell.kind
            } else {
                ShellKind::Bash
            };
            (kind, s.completion.clone())
        };

        // Sync settings + master-enable gate.
        {
            let Some(c) = self.completion.as_mut() else {
                return;
            };
            c.sync_settings(kind, &settings_snapshot);
            if !c.enabled() {
                c.dismiss();
                self.completion_anchor = None;
                return;
            }
        }

        // Cheap query for the alt-screen gate — skip the full grid clone on the
        // alternate screen (TUIs), which is the perf-sensitive path.
        let query = self.session.read(cx).query_state();
        let on_alt = query.mode.contains(TermMode::ALT_SCREEN);
        {
            let c = self.completion.as_mut().unwrap();
            c.set_alt_screen(on_alt);
            // Cheap pre-grid gate: enabled + alt-screen only. The prompt-region
            // gate is applied after we read the line (it depends on the line).
            if !c.pre_gate_ok() {
                c.dismiss();
                self.completion_anchor = None;
                return;
            }
        }

        // Skip the expensive grid snapshot when the cursor has not moved and no
        // settings/gating change requested a recompute — this avoids cloning the
        // grid on idle frames (cursor blink) and during fast primary-screen output.
        let cursor_pos = (query.cursor_line, query.cursor_col);
        let cursor_moved = self.completion_last_cursor != Some(cursor_pos);
        let wants = self
            .completion
            .as_ref()
            .map(|c| c.wants_recompute(cursor_moved))
            .unwrap_or(false);
        if !wants {
            return;
        }
        self.completion_last_cursor = Some(cursor_pos);

        // At a prompt on the primary screen: read the input line from the grid.
        let content = self.session.read(cx).snapshot_query();
        let (line, prompt_found, anchor) = extract_cursor_command(&content);

        let history_entity = match GlobalCompletionHistory::try_global(cx) {
            Some(h) => h,
            None => {
                log::warn!(
                    "completion: GlobalCompletionHistory not initialized — completion disabled"
                );
                self.completion_anchor = None;
                return;
            }
        };
        let now = now_ms();
        {
            let history = history_entity.read(cx);
            let c = self.completion.as_mut().unwrap();
            c.set_in_prompt_region(prompt_found);
            let allowed = c.gating_allows();
            if !allowed {
                log::debug!("completion: line={line:?} gating=false (hidden)");
                c.dismiss();
                self.completion_anchor = None;
                return;
            }
            c.recompute(&line, line.len(), now, history, false);
            log::debug!(
                "completion: line={line:?} prompt_found={prompt_found} visible={} n={}",
                c.is_visible(),
                c.suggestions().len()
            );
            if c.is_visible() {
                // Anchor under the start of the token the user is editing.
                let (aline, acol) = anchor;
                let token_start = acol.saturating_sub(c.typed_len());
                self.completion_anchor = Some((aline, token_start));
            } else {
                self.completion_anchor = None;
            }
        }
    }

    /// Force-open the overlay at the cursor (the `TriggerCompletion` action),
    /// bypassing `min_prefix_len`.
    pub(crate) fn trigger_completion(&mut self, cx: &mut Context<Self>) {
        self.ensure_completion(cx);
        let query = self.session.read(cx).query_state();
        if query.mode.contains(TermMode::ALT_SCREEN) {
            return;
        }
        let content = self.session.read(cx).snapshot_query();
        let (line, prompt_found, anchor) = extract_cursor_command(&content);
        let Some(history_entity) = GlobalCompletionHistory::try_global(cx) else {
            return;
        };
        let now = now_ms();
        let history = history_entity.read(cx);
        let Some(c) = self.completion.as_mut() else {
            return;
        };
        c.set_in_prompt_region(prompt_found);
        c.recompute(&line, line.len(), now, history, true);
        if c.is_visible() {
            let (aline, acol) = anchor;
            let token_start = acol.saturating_sub(c.typed_len());
            self.completion_anchor = Some((aline, token_start));
        }
        cx.notify();
    }

    /// Build the positioned completion overlay element, if visible.
    pub(crate) fn completion_overlay_element(&self, cx: &App) -> Option<impl IntoElement> {
        let c = self.completion.as_ref()?;
        if !c.is_visible() {
            return None;
        }
        let (line, col) = self.completion_anchor?;
        let m = self.metrics.borrow();

        // Only render a window of `max_visible` rows, scrolled to keep the
        // selected row in view; the engine keeps more candidates than we show.
        let all = c.suggestions();
        let (offset, count) = visible_window(all.len(), c.selected(), c.max_visible());
        let slice = &all[offset..offset + count];
        let local_selected = c
            .selected()
            .filter(|&i| i >= offset && i < offset + count)
            .map(|i| i - offset);
        let hidden_above = offset;
        let hidden_below = all.len() - (offset + count);

        // Decide whether the list fits *below* the input row. If not, flip it
        // *above* the row so it never covers what the user is typing. The number
        // of visible rows (list + hint rows) times the line height estimates the
        // overlay height closely enough for the flip decision.
        let row_count = count + usize::from(hidden_above > 0) + usize::from(hidden_below > 0);
        let est_height = m.line_height * (row_count as f32) + px(10.0);
        let row_top = m.grid_origin.y + m.line_height * (line as f32);
        let row_bottom = row_top + m.line_height;
        let viewport_bottom = m
            .bounds
            .map(|b| b.origin.y + b.size.height)
            .unwrap_or_else(|| m.grid_origin.y + m.line_height * (m.rows as f32));
        let x = m.grid_origin.x + m.cell_width * (col as f32);
        // `snap_to_window_with_margin` still clamps horizontally on-screen; the
        // explicit corner flip handles vertical placement so the list never
        // overlaps the prompt when the cursor sits near the bottom edge.
        let (anchor, pos_y) = if est_height <= (viewport_bottom - row_bottom) {
            (Anchor::TopLeft, row_bottom)
        } else {
            (Anchor::BottomLeft, row_top)
        };

        let overlay =
            CompletionOverlay::new(slice, local_selected, None, hidden_above, hidden_below);
        let _ = cx;
        Some(
            deferred(
                anchored()
                    .snap_to_window_with_margin(px(8.0))
                    .anchor(anchor)
                    .position(point(x, pos_y))
                    .child(overlay),
            )
            .with_priority(1),
        )
    }

    /// Handle a key while the overlay is visible. Returns `true` if the key was
    /// consumed (the caller must then `stop_propagation` and not send to the PTY).
    ///
    /// Called from the keyboard handler **before** PTY delivery. `key` is the
    /// gpui key name; `ctrl` indicates the control modifier.
    pub(crate) fn completion_handle_key(
        &mut self,
        key: &str,
        ctrl: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(c) = self.completion.as_mut() else {
            return false;
        };
        if !c.is_visible() {
            return false;
        }
        let accept_tab = c.accept_tab();
        match key {
            "down" => {
                c.select_next();
                cx.notify();
                true
            }
            "up" => {
                c.select_prev();
                cx.notify();
                true
            }
            "n" if ctrl => {
                c.select_next();
                cx.notify();
                true
            }
            "p" if ctrl => {
                c.select_prev();
                cx.notify();
                true
            }
            "escape" => {
                c.dismiss();
                self.completion_anchor = None;
                cx.notify();
                true
            }
            "enter" | "return" => {
                // Run-first: only accept when the user has navigated to a row.
                if c.selected().is_some() {
                    self.completion_accept(cx);
                    true
                } else {
                    // No selection → let Enter run the command (not consumed).
                    false
                }
            }
            "tab" => {
                if accept_tab {
                    // Auto-select the first row if none, then accept.
                    if c.selected().is_none() {
                        c.select_next();
                    }
                    self.completion_accept(cx);
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    /// Accept the selected suggestion: append its remainder to the PTY, dismiss.
    fn completion_accept(&mut self, cx: &mut Context<Self>) {
        let bytes = self.completion.as_ref().and_then(|c| c.accept_bytes());
        if let Some(remainder) = bytes {
            if !remainder.is_empty() {
                log::debug!("completion: accept → append {remainder:?}");
                self.session.update(cx, |s, _| {
                    if let Err(e) = s.write(remainder.as_bytes()) {
                        log::warn!("completion: PTY write on accept failed: {e}");
                    }
                });
            }
        }
        if let Some(c) = self.completion.as_mut() {
            c.dismiss();
        }
        self.completion_anchor = None;
        cx.notify();
    }

    /// Capture the current input line into history when a command runs (Enter
    /// with no active selection). Called from the keyboard handler.
    pub(crate) fn completion_capture_current(&mut self, cx: &mut Context<Self>) {
        if self.completion.is_none() {
            return;
        }
        let content = self.session.read(cx).snapshot_query();
        let (line, _found, _anchor) = extract_cursor_command(&content);
        if line.trim().is_empty() {
            return;
        }
        let Some(history_entity) = GlobalCompletionHistory::try_global(cx) else {
            return;
        };
        let now = now_ms();
        let controller = self.completion.as_ref().unwrap();
        history_entity.update(cx, |h, _| {
            controller.capture(&line, now, h);
        });
        // The line is being submitted → clear any overlay.
        if let Some(c) = self.completion.as_mut() {
            c.dismiss();
        }
        self.completion_anchor = None;
    }
}
