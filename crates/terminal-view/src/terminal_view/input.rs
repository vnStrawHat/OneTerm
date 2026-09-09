//! The wrapper div's listeners: gather what `crate::input` needs from the
//! view, apply the decision it returns, and repaint.
//!
//! Every path that writes bytes goes through `crate::input` (which snaps the
//! viewport to the live screen); this module owns only the view-side effects:
//! clearing the bell, marking the scrollbar, the completion overlay, zoom, and
//! the URL confirmation dialog.

use gpui::{
    App, Context, KeyDownEvent, ModifiersChangedEvent, MouseButton, MouseDownEvent, MouseExitEvent,
    MouseMoveEvent, MouseUpEvent, ParentElement as _, Pixels, Point, ScrollWheelEvent, Window,
};
use gpui_component::{
    ActiveTheme as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    dialog::DialogFooter,
};
use oneterm_state::BroadcastInput;
use oneterm_terminal::TargetDecision;

use super::TerminalView;
use crate::input::{
    KeyAction, KeyContext, MouseInputs, MouseOutcome, UrlOpen, classify_key, copy_selection,
    interrupt, paste_clipboard, send_key,
};
use crate::url::DetectedUrl;

impl TerminalView {
    pub(super) fn on_key_down(
        &mut self,
        e: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let session = self.session.clone();
        let key = e.keystroke.key.as_str();
        let alt_screen = session.read(cx).is_alt_screen();
        // A TUI took over while the overlay was open: nothing to complete.
        if alt_screen && self.completion.is_visible() {
            self.completion.dismiss();
        }
        let ctx = KeyContext {
            search_focused: matches!(key, "enter" | "return")
                && self.search.input_is_focused(window, cx),
            alt_screen,
            completion_visible: self.completion.is_visible(),
            completion_selected: self.completion.has_selection(),
            completion_accept_tab: self.completion.accept_tab(),
        };

        let action = classify_key(&e.keystroke, e.prefer_character_input, ctx);
        match action {
            KeyAction::ToggleSearch => self.toggle_search(window, cx),
            KeyAction::SwallowInSearch => {}
            KeyAction::TriggerCompletion => self.trigger_completion(cx),
            KeyAction::Completion(action) => self.apply_completion_key(action, cx),
            KeyAction::ZoomIn | KeyAction::ZoomOut | KeyAction::ZoomReset => {
                // Zoom mutates the live settings only; persisting is the
                // settings UI's explicit step.
                let theme_default = f32::from(cx.theme().mono_font_size);
                self.deps.settings.update(cx, |settings, cx| {
                    match action {
                        KeyAction::ZoomIn => settings.zoom_in(theme_default),
                        KeyAction::ZoomOut => settings.zoom_out(theme_default),
                        _ => settings.reset_zoom(),
                    }
                    cx.notify();
                });
                cx.notify();
            }
            KeyAction::ScrollPages(pages) => {
                let rows = session.read(cx).query_state().rows as i32;
                session.update(cx, |s, _| s.scroll(pages * rows));
                self.scrolled(cx);
            }
            KeyAction::ScrollLines(lines) => {
                session.update(cx, |s, _| s.scroll(lines));
                self.scrolled(cx);
            }
            KeyAction::ScrollTop => {
                session.update(cx, |s, _| s.scroll_to_top());
                self.scrolled(cx);
            }
            KeyAction::ScrollBottom => {
                session.update(cx, |s, _| s.scroll_to_bottom());
                self.scrolled(cx);
            }
            KeyAction::Copy => {
                let origin = self.broadcast_origin(cx);
                copy_selection(&session, &origin, window, cx);
            }
            KeyAction::Paste => {
                let origin = self.broadcast_origin(cx);
                paste_clipboard(&session, &origin, window, cx);
            }
            // The platform / IME path delivers the text: do not stop
            // propagation, or the character never arrives.
            KeyAction::Ignore | KeyAction::Unhandled => return,
            KeyAction::Interrupt => {
                interrupt(&session, cx);
                self.deps
                    .fan_out(cx.entity_id(), BroadcastInput::Interrupt, cx);
                self.clear_bell(cx);
            }
            KeyAction::Send(spec, mods) => {
                // Run-first: Enter with nothing selected runs the command —
                // capture the typed line into history before it scrolls away.
                if matches!(key, "enter" | "return") {
                    self.completion_capture_current(cx);
                }
                // The frame's DECCKM flag is at most one paint old.
                let app_cursor = self.render_state.borrow().frame.app_cursor();
                let Some(bytes) = send_key(&session, &spec, mods, app_cursor, cx) else {
                    return;
                };
                self.deps
                    .fan_out(cx.entity_id(), BroadcastInput::Bytes(&bytes), cx);
                self.clear_bell(cx);
            }
        }
        cx.stop_propagation();
    }

    pub(super) fn on_modifiers_changed(
        &mut self,
        e: &ModifiersChangedEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let session = self.session.clone();
        let inputs = self.mouse_inputs(None, cx);
        let outcome = self
            .mouse
            .modifiers_changed(e, &session, &inputs, &mut self.url_hover, cx);
        if outcome != MouseOutcome::Ignored {
            cx.notify();
        }
    }

    pub(super) fn on_mouse_down(
        &mut self,
        e: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let session = self.session.clone();
        let inputs = self.mouse_inputs(Some(e.position), cx);
        let outcome = self.mouse.down(e, &session, &inputs, cx);
        // A forwarded right press is the program's (mouse mode); nothing
        // above the terminal should also react to it.
        if e.button == MouseButton::Right && outcome == MouseOutcome::Handled {
            cx.stop_propagation();
        }
        self.apply_mouse_outcome(outcome, window, cx);
    }

    pub(super) fn on_mouse_up(
        &mut self,
        e: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Any release ends a thumb drag, whichever button started it.
        self.scrollbar.end_drag();
        let session = self.session.clone();
        let inputs = self.mouse_inputs(Some(e.position), cx);
        let outcome = self.mouse.up(e, &session, &inputs, cx);
        if e.button == MouseButton::Right && outcome == MouseOutcome::Handled {
            cx.stop_propagation();
        }
        self.apply_mouse_outcome(outcome, window, cx);
    }

    pub(super) fn on_mouse_move(
        &mut self,
        e: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if e.pressed_button != Some(MouseButton::Left) {
            // A release outside the window never reaches `on_mouse_up`.
            self.scrollbar.end_drag();
        }
        let session = self.session.clone();
        let inputs = self.mouse_inputs(Some(e.position), cx);
        let was_hovering = self.url_hover.is_hovering();
        let outcome = self
            .mouse
            .moved(e, &session, &inputs, &mut self.url_hover, cx);
        match outcome {
            // A plain move only reports motion to the program; repaint just
            // when the hover highlight appeared or went away, and keep the
            // scrollbar hidden — a drag is what reveals it.
            MouseOutcome::Handled if e.pressed_button != Some(MouseButton::Left) => {
                if e.pressed_button == Some(MouseButton::Right) {
                    cx.stop_propagation();
                }
                if was_hovering != self.url_hover.is_hovering() {
                    cx.notify();
                }
            }
            other => self.apply_mouse_outcome(other, window, cx),
        }
    }

    /// The pointer left the terminal: a URL highlight must not stick.
    pub(super) fn on_mouse_exit(
        &mut self,
        e: &MouseExitEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.mouse.exit(e.position, &mut self.url_hover) != MouseOutcome::Ignored {
            cx.notify();
        }
    }

    pub(super) fn on_scroll_wheel(
        &mut self,
        e: &ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let session = self.session.clone();
        let inputs = self.mouse_inputs(Some(e.position), cx);
        let outcome = self.mouse.wheel(e, &session, &inputs, cx);
        self.apply_mouse_outcome(outcome, window, cx);
    }

    /// What the mouse machine cannot read for itself. `position` is the
    /// event position for the scrollbar hit test; `None` for events without one.
    fn mouse_inputs(&self, position: Option<Point<Pixels>>, cx: &App) -> MouseInputs {
        let geometry = self.render_state.borrow().geometry;
        let settings = self.deps.settings.read(cx);
        MouseInputs {
            geometry,
            over_scrollbar: match (position, geometry) {
                (Some(position), Some(geometry)) => {
                    self.scrollbar.hit_test(position, geometry.bounds)
                }
                _ => false,
            },
            show_context_menu: settings.show_context_menu,
            copy_on_select: settings.copy_on_select,
            scroll_multiplier: settings.scroll_multiplier,
        }
    }

    /// Finish a mouse event: the two outcomes that need the view, then the
    /// repaint every handled event gets.
    fn apply_mouse_outcome(
        &mut self,
        outcome: MouseOutcome,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match outcome {
            MouseOutcome::Ignored => return,
            MouseOutcome::Handled => {}
            MouseOutcome::ScrollbarDrag { track_y } => {
                self.scrollbar.drag_to(track_y);
                // The press is the scrollbar's, not a Space activation.
                cx.stop_propagation();
            }
            MouseOutcome::OpenUrl(open) => open_detected_url(open, window, cx),
            MouseOutcome::CopySelection => {
                let origin = self.broadcast_origin(cx);
                copy_selection(&self.session, &origin, window, cx);
            }
        }
        self.scrolled(cx);
    }

    /// The viewport moved (or may have): show the scrollbar and repaint.
    fn scrolled(&mut self, cx: &mut Context<Self>) {
        self.scrollbar.mark_scrolled();
        cx.notify();
    }

    /// The user typed: the bell indicator has been seen.
    fn clear_bell(&mut self, cx: &mut Context<Self>) {
        if self.has_bell {
            self.has_bell = false;
            cx.notify();
        }
    }
}

/// Text of the confirmation dialog for a link the policy wants confirmed:
/// the visible label vs. the real target, so the user can spot a mismatch.
fn url_confirmation_text(url: &DetectedUrl) -> String {
    match url.display_text.as_deref() {
        Some(label) if !label.trim().is_empty() && !label.eq_ignore_ascii_case(&url.url) => {
            format!(
                "This link is labelled \"{label}\" but opens {}. Open it?",
                url.url
            )
        }
        _ => format!("Open {} in your browser?", url.url),
    }
}

/// Ask the user before opening a link the target policy flagged (SEC-03).
fn confirm_open_url(url: DetectedUrl, window: &mut Window, cx: &mut App) {
    let description = url_confirmation_text(&url);
    let target = url.url;
    window.open_alert_dialog(cx, move |alert, _window, _cx| {
        let target = target.clone();
        alert
            .confirm()
            .title("Open link?")
            .description(description.clone())
            .footer(
                DialogFooter::new()
                    .child(
                        Button::new("url-cancel")
                            .label("Cancel")
                            .outline()
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child(Button::new("url-open").label("Open").primary().on_click(
                        move |_, window, cx| {
                            cx.open_url(&target);
                            window.close_dialog(cx);
                        },
                    )),
            )
    });
}

/// Open a Ctrl-clicked URL after the target policy decided: allowed links
/// open directly, `Confirm` asks first, `Deny` is logged.
fn open_detected_url(open: UrlOpen, window: &mut Window, cx: &mut App) {
    let UrlOpen { url, decision } = open;
    match decision {
        TargetDecision::Allow => cx.open_url(&url.url),
        TargetDecision::Confirm(reason) => {
            log::info!(
                "terminal: URL requires confirmation: {reason:?} — {}",
                url.url
            );
            confirm_open_url(url, window, cx);
        }
        TargetDecision::Deny(reason) => {
            log::warn!("terminal: URL denied: {reason:?} — {}", url.url);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::url_confirmation_text;
    use crate::url::DetectedUrl;

    #[test]
    fn confirmation_text_shows_the_label_when_it_differs_from_the_target() {
        let url = DetectedUrl {
            url: "https://evil.example".to_string(),
            display_text: Some("https://good.example".to_string()),
            row: 0,
            start_col: 0,
            end_col: 5,
        };
        let text = url_confirmation_text(&url);
        assert!(text.contains("https://good.example"));
        assert!(text.contains("https://evil.example"));
        // Same label as target (or a plain-text URL) → simple prompt.
        let plain = DetectedUrl {
            display_text: None,
            ..url.clone()
        };
        assert_eq!(
            url_confirmation_text(&plain),
            "Open https://evil.example in your browser?"
        );
    }
}
