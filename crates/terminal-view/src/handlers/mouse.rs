//! Mouse handlers for `LocalTerminalView`.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    App, Entity, InteractiveElement as _, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, ParentElement as _, Window,
};
use gpui_component::WindowExt as _;
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::dialog::DialogFooter;

use oneterm_terminal::TerminalSession;
use oneterm_terminal::url_policy::{TargetDecision, validate_target_with_display};

use super::super::element::RenderCache;
use super::super::url::DetectedUrl;
use super::super::view::LocalTerminalView;
use super::super::view::grid::{pixel_to_grid, sel_type};
use super::url::detect_url_at_cell;
use super::{edit, map_button, mouse_mods};

/// What a mouse-down on the grid does — the pure decision behind
/// [`handle_mouse_down`] (TEST-03).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MouseDownAction {
    /// Look for a URL under the pointer and open it (Ctrl/Cmd+click on a
    /// button that allows it); falls back to `Terminal` when there is none.
    OpenUrl,
    /// Forward to the terminal: mouse-mode encoding or start a selection.
    Terminal,
}

/// Classify a mouse-down: URL opening needs the platform modifier (Ctrl, or
/// Cmd on macOS) and a button that allows it (left; never right — that is
/// the context menu / mouse-mode button).
pub(crate) fn classify_mouse_down(allow_url_open: bool, mods: &Modifiers) -> MouseDownAction {
    if allow_url_open && (mods.control || mods.platform) {
        MouseDownAction::OpenUrl
    } else {
        MouseDownAction::Terminal
    }
}

/// Text of the confirmation dialog for a link the policy wants confirmed:
/// the visible label vs. the real target, so the user can spot a mismatch.
pub(crate) fn url_confirmation_text(url: &DetectedUrl) -> String {
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

/// Open a detected URL after the target policy has decided: allowed links
/// open directly, `Confirm` asks first, `Deny` is logged.
fn open_detected_url(url: DetectedUrl, window: &mut Window, cx: &mut App) {
    match validate_target_with_display(&url.url, url.display_text.as_deref()) {
        TargetDecision::Allow => cx.open_url(&url.url),
        TargetDecision::Confirm(reason) => {
            log::info!(
                "terminal: URL requires confirmation: {reason:?} — {}",
                url.url
            );
            confirm_open_url(url, window, cx);
        }
        TargetDecision::Deny(reason) => {
            log::warn!("terminal: URL denied: {:?} — {}", reason, url.url);
        }
    }
}

fn handle_mouse_down(
    session: &Entity<Box<dyn TerminalSession>>,
    render_cache: &Rc<RefCell<RenderCache>>,
    view: &Entity<LocalTerminalView>,
    e: &MouseDownEvent,
    window: &mut Window,
    cx: &mut App,
    button: MouseButton,
) {
    let metrics = render_cache.borrow().metrics;
    let (row, col) = match pixel_to_grid(&metrics, e.position) {
        Some(rc) => rc,
        None => return,
    };

    // Only the left button opens URLs; the right button is the context menu
    // / mouse-mode button.
    let allow_url_open = matches!(button, MouseButton::Left);
    if classify_mouse_down(allow_url_open, &e.modifiers) == MouseDownAction::OpenUrl {
        if let Some(url) = detect_url_at_cell(session, row as usize, col as usize, cx) {
            open_detected_url(url, window, cx);
            return;
        }
    }

    let mods = mouse_mods(&e.modifiers);
    session.update(cx, |s, _| {
        s.mouse_down(
            row,
            col,
            map_button(button),
            sel_type(e.click_count, e.modifiers.alt),
            mods,
        )
    });
    if matches!(button, MouseButton::Right) {
        cx.stop_propagation();
    }
    // Trigger a re-render to draw the selection highlight.
    view.update(cx, |v, cx| {
        v.scrollbar.mark_scrolled();
        cx.notify();
    });
}

fn handle_mouse_up(
    session: &Entity<Box<dyn TerminalSession>>,
    render_cache: &Rc<RefCell<RenderCache>>,
    view: &Entity<LocalTerminalView>,
    e: &MouseUpEvent,
    window: &mut Window,
    cx: &mut App,
    button: MouseButton,
) {
    let metrics = render_cache.borrow().metrics;
    let (row, col) = match pixel_to_grid(&metrics, e.position) {
        Some(rc) => rc,
        None => return,
    };
    let mods = mouse_mods(&e.modifiers);
    session.update(cx, |s, _| s.mouse_up(row, col, map_button(button), mods));
    if matches!(button, MouseButton::Right) {
        cx.stop_propagation();
    }
    // Copy-on-select is a setting (SEC-10): releasing the left button after a
    // selection only overwrites the clipboard when the user opted in.
    let copy_on_select =
        matches!(button, MouseButton::Left) && view.read(cx).deps.settings.read(cx).copy_on_select;
    if copy_on_select {
        edit::copy_selection(session, window, cx);
    }
    view.update(cx, |v, cx| {
        v.scrollbar.mark_scrolled();
        cx.notify();
    });
}

/// End a scrollbar thumb drag if one is in progress. Returns `true` when the
/// event was consumed by the drag.
fn end_scrollbar_drag(view: &Entity<LocalTerminalView>, cx: &mut App) -> bool {
    if !view.read(cx).scrollbar.is_dragging() {
        return false;
    }
    view.update(cx, |v, cx| {
        v.scrollbar.end_drag();
        cx.notify();
    });
    true
}

/// Attach mouse handlers: down / move / up / modifiers.
pub(crate) fn attach_mouse(
    div: gpui::Stateful<gpui::Div>,
    session: Entity<Box<dyn TerminalSession>>,
    render_cache: Rc<RefCell<RenderCache>>,
    view: Entity<LocalTerminalView>,
    pass_right_click: bool,
) -> gpui::Stateful<gpui::Div> {
    let div = div.on_mouse_down(MouseButton::Left, {
        let s = session.clone();
        let cache = render_cache.clone();
        let view = view.clone();
        move |e: &MouseDownEvent, window, cx: &mut App| {
            handle_mouse_down(&s, &cache, &view, e, window, cx, MouseButton::Left)
        }
    });

    let div = if pass_right_click {
        div.on_mouse_down(MouseButton::Right, {
            let s = session.clone();
            let cache = render_cache.clone();
            let view = view.clone();
            move |e: &MouseDownEvent, window, cx: &mut App| {
                handle_mouse_down(&s, &cache, &view, e, window, cx, MouseButton::Right)
            }
        })
        .on_mouse_up(MouseButton::Right, {
            let s = session.clone();
            let cache = render_cache.clone();
            let view = view.clone();
            move |e: &MouseUpEvent, window, cx: &mut App| {
                if end_scrollbar_drag(&view, cx) {
                    return;
                }
                handle_mouse_up(&s, &cache, &view, e, window, cx, MouseButton::Right)
            }
        })
    } else {
        div
    };

    div.on_mouse_move({
        let s = session.clone();
        let cache = render_cache.clone();
        let view = view.clone();
        move |e: &MouseMoveEvent, _w, cx: &mut App| {
            // Scrollbar drag: check this BEFORE selection.
            if view.read(cx).scrollbar.is_dragging() {
                // Mouse left the terminal and button released → clear drag.
                if e.pressed_button != Some(MouseButton::Left) {
                    end_scrollbar_drag(&view, cx);
                    return;
                }
                // e.position is window coordinates → subtract the terminal origin.
                let track_y = match cache.borrow().metrics.bounds {
                    Some(b) => f32::from(e.position.y - b.origin.y),
                    None => return,
                };
                view.update(cx, |v, cx| {
                    if v.scrollbar.drag_to(track_y) {
                        cx.notify();
                    }
                });
                return;
            }
            // Normal mouse move: selection drag / hover.
            let metrics = cache.borrow().metrics;
            let (row, col) = match pixel_to_grid(&metrics, e.position) {
                Some(rc) => rc,
                None => {
                    // Mouse outside grid — clear hover + save pos.
                    view.update(cx, |v, cx| {
                        if v.url_hover.leave(e.position) {
                            cx.notify();
                        }
                    });
                    return;
                }
            };
            let mods = mouse_mods(&e.modifiers);
            if e.pressed_button == Some(MouseButton::Left) {
                s.update(cx, |s, _| s.mouse_drag(row, col, mods));
                view.update(cx, |v, cx| {
                    v.scrollbar.mark_scrolled();
                    cx.notify();
                });
            } else {
                s.update(cx, |s, _| s.mouse_move(row, col, mods));
                if e.pressed_button == Some(MouseButton::Right) {
                    cx.stop_propagation();
                }
            }
            // URL detection on hover — highlight + cursor pointer (Ctrl+click to open).
            super::url::update_hovered_url(&s, &cache, &view, e.position, e.modifiers.control, cx);
        }
    })
    .on_mouse_up(MouseButton::Left, {
        let s = session.clone();
        let cache = render_cache.clone();
        let view = view.clone();
        move |e: &MouseUpEvent, window, cx: &mut App| {
            // Scrollbar drag: clear FIRST.
            if end_scrollbar_drag(&view, cx) {
                return;
            }
            handle_mouse_up(&s, &cache, &view, e, window, cx, MouseButton::Left);
        }
    })
}

#[cfg(test)]
mod tests {
    use gpui::Modifiers;

    use super::{MouseDownAction, classify_mouse_down, url_confirmation_text};
    use crate::url::DetectedUrl;

    fn ctrl() -> Modifiers {
        Modifiers {
            control: true,
            ..Default::default()
        }
    }

    #[test]
    fn url_open_needs_the_platform_modifier_and_an_allowed_button() {
        assert_eq!(classify_mouse_down(true, &ctrl()), MouseDownAction::OpenUrl);
        let cmd = Modifiers {
            platform: true,
            ..Default::default()
        };
        assert_eq!(classify_mouse_down(true, &cmd), MouseDownAction::OpenUrl);
        assert_eq!(
            classify_mouse_down(true, &Modifiers::default()),
            MouseDownAction::Terminal
        );
        // Right button (allow_url_open = false) never opens URLs.
        assert_eq!(
            classify_mouse_down(false, &ctrl()),
            MouseDownAction::Terminal
        );
    }

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
