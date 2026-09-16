//! The wrapper div's listeners: gather what `crate::input` needs from the
//! view, apply the decision it returns, and repaint.
//!
//! Every path that writes bytes goes through `crate::input` (which snaps the
//! viewport to the live screen); this module owns only the view-side effects:
//! clearing the bell, marking the scrollbar, the completion overlay, zoom, and
//! the URL confirmation dialog.

use gpui::{
    App, Context, KeyDownEvent, KeyUpEvent, ModifiersChangedEvent, MouseButton, MouseDownEvent,
    MouseExitEvent, MouseMoveEvent, MouseUpEvent, ParentElement as _, Pixels, Point,
    ScrollWheelEvent, Window,
};
use gpui_component::{
    ActiveTheme as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    dialog::DialogFooter,
};
use oneterm_state::BroadcastInput;
use oneterm_terminal::{KeyEvent, KeyEventKind, KeyMods, KeySpec, KeyboardFlags, TargetDecision};

use super::TerminalView;
use crate::input::{
    KeyAction, KeyContext, MouseInputs, MouseOutcome, UrlOpen, canonical_key, classify_key,
    copy_selection, interrupt, map_key, paste_clipboard, send_key,
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
        // The frame's modes are at most one paint old; read once, so the
        // classification and the encoding cannot disagree about them.
        let modes = self.render_state.borrow().frame.modes();
        let ctx = KeyContext {
            search_focused: matches!(key, "enter" | "return")
                && self.search.input_is_focused(window, cx),
            alt_screen,
            completion_visible: self.completion.is_visible(),
            completion_selected: self.completion.has_selection(),
            completion_accept_tab: self.completion.accept_tab(),
            ctrl_c_is_a_key: modes.keyboard_flags.intersects(
                KeyboardFlags::DISAMBIGUATE_ESC_CODES | KeyboardFlags::REPORT_ALL_KEYS_AS_ESC,
            ),
        };

        // GPUI's own held flag — on Windows the `WM_KEYDOWN` previous-key-state
        // bit, so the cadence is the user's own repeat rate. Nothing here times,
        // counts or synthesises a repeat.
        let kind = if e.is_held {
            KeyEventKind::Repeat
        } else {
            KeyEventKind::Press
        };
        let action = classify_key(&e.keystroke, e.prefer_character_input, kind, ctx);
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
            KeyAction::Interrupt(encoded) => {
                match encoded {
                    // The program negotiated the `CSI u` rung for ctrl chords,
                    // so this pane's own terminal gets the key it asked for --
                    // and owes it a release like any other written press.
                    Some(event) => {
                        if send_key(&session, &event, &modes, cx).is_some() {
                            self.hold_key(&event.key);
                        }
                    }
                    None => interrupt(&session, cx),
                }
                // The peers are not this pane: each negotiated its own flags,
                // or none, and an interrupt is the one form all of them
                // understand. Same rule as a release, which is never fanned out
                // either; re-encoding per target is the named follow-up.
                self.deps
                    .fan_out(cx.entity_id(), BroadcastInput::Interrupt, cx);
                self.clear_bell(cx);
            }
            KeyAction::Send(event) => {
                // Run-first: Enter with nothing selected runs the command —
                // capture the typed line into history before it scrolls away.
                if matches!(key, "enter" | "return") {
                    self.completion_capture_current(cx);
                }
                let Some(bytes) = send_key(&session, &event, &modes, cx) else {
                    return;
                };
                // Only a press that actually reached the PTY is owed a release.
                self.hold_key(&event.key);
                self.deps
                    .fan_out(cx.entity_id(), BroadcastInput::Bytes(&bytes), cx);
                self.clear_bell(cx);
            }
        }
        cx.stop_propagation();
    }

    /// The key-up half. Deliberately **not** `classify_key`: the table is full
    /// of view-side shortcuts, and running it here would risk zooming or
    /// toggling the search bar on a release. A release is also never fanned out
    /// to broadcast peers — a peer whose program negotiated nothing must not
    /// receive a `:3` sequence it has never seen a press for.
    pub(super) fn on_key_up(
        &mut self,
        e: &KeyUpEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(event) = map_key(&e.keystroke, KeyEventKind::Release) else {
            return;
        };
        // The press never reached the PTY (a swallowed chord, a key the IME
        // owns, a chord with no encoding), so neither does the release.
        let canonical = canonical_key(&event.key);
        let Some(at) = self.held_keys.iter().position(|held| *held == canonical) else {
            return;
        };
        self.held_keys.remove(at);
        let modes = self.render_state.borrow().frame.modes();
        // This pane's own session and no fan-out: a peer that negotiated
        // nothing must not receive a `:3` it has never seen a press for.
        send_key(&self.session, &event, &modes, cx);
        // No `stop_propagation`: the up path consumes nothing another handler
        // might want.
    }

    /// Record that a press reached the PTY, so its release will too.
    ///
    /// Idempotent: a fresh press of a key already held (a release the platform
    /// never delivered, or one lost to a name this canonical form does not
    /// collapse) leaves one entry rather than two, so a missed release cannot
    /// compound into a stuck key.
    fn hold_key(&mut self, spec: &KeySpec) {
        let canonical = canonical_key(spec);
        if !self.held_keys.contains(&canonical) {
            self.held_keys.push(canonical);
        }
    }

    /// Focus left with keys still down: send the release the program is owed,
    /// or a `vim` in kitty mode believes the key is held forever.
    ///
    /// **Untested in this repository, in both places.** The GPUI test window is
    /// never active, so its focus events carry no previous focus path and the
    /// `on_blur` subscription cannot fire — `US-0108`'s
    /// `the_blur_drain_cannot_be_reached_from_a_test_window` demonstrates that
    /// rather than asserting it. The wiring is proved only by the manual
    /// Windows walk in `IN-0040`'s detail design.
    pub(super) fn release_held_keys(&mut self, cx: &mut App) {
        if self.held_keys.is_empty() {
            return;
        }
        let session = self.session.clone();
        let modes = self.render_state.borrow().frame.modes();
        // The modifiers that were held are not part of a blur, so each release
        // is reported unmodified — the key itself is what the program tracks.
        for spec in std::mem::take(&mut self.held_keys) {
            let mut event = KeyEvent::new(spec, KeyMods::default());
            event.kind = KeyEventKind::Release;
            send_key(&session, &event, &modes, cx);
        }
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
            middle_click_paste: settings.middle_click_paste,
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
            MouseOutcome::Paste => {
                let origin = self.broadcast_origin(cx);
                paste_clipboard(&self.session, &origin, window, cx);
                self.clear_bell(cx);
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
