//! Keyboard handler for `LocalTerminalView`.
//!
//! The decision tree — which chord does what — is the pure
//! [`classify_key`] (TEST-03); the `on_key_down` closure only gathers the
//! [`KeyContext`], applies the [`KeyAction`], and threads the completion
//! overlay in between (that step needs the view's mutable state).

use gpui::{App, Entity, Focusable as _, InteractiveElement as _, KeyDownEvent, Keystroke};
use gpui_component::ActiveTheme as _;

use alacritty_terminal::term::TermMode;
use oneterm_settings::TerminalSettings;
use oneterm_terminal::{KeyMods, KeySpec, TerminalSession, encode_key};

use super::super::view::LocalTerminalView;
use super::super::view::key::map_key;
use super::edit;

/// A Shift+key scrollback navigation action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScrollKey {
    /// Scroll by this many lines (positive = towards history).
    Lines(i32),
    Top,
    Bottom,
}

/// A platform+`-`/`=`/`0` zoom action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ZoomKey {
    In,
    Out,
    Reset,
}

/// What a key-down does. The variants are ordered the way the closure checks
/// them; the completion overlay is consulted between `TriggerCompletion` and
/// `Zoom` (see [`attach_key`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum KeyAction {
    /// Platform+F: open / close the search bar.
    ToggleSearch,
    /// Enter propagated from the search input — swallow it so it never
    /// reaches the terminal.
    SwallowForSearchInput,
    /// Ctrl+Shift+Space: force the completion overlay open.
    TriggerCompletion,
    Zoom(ZoomKey),
    Scroll(ScrollKey),
    Copy,
    Paste,
    /// Printable text on the primary screen with no modifier: let the IME
    /// path (`replace_text_in_range`) deliver it — do nothing here and do not
    /// stop propagation.
    LetImeHandle,
    /// AltGr layout text (Windows reports it as Ctrl+Alt): let the platform
    /// deliver it as text.
    AltGrText,
    /// Ctrl+C: SIGINT via `send_ctrl_c`, snapping to the live screen.
    Interrupt,
    /// Encode `spec` with `mods` and write it to the PTY, snapping to the
    /// live screen and clearing the bell.
    Send(KeySpec, KeyMods),
    /// Nothing to send (no mapping for the key).
    Ignore,
}

/// The view state a key decision depends on.
#[derive(Debug, Clone, Copy)]
pub(crate) struct KeyContext {
    /// The search bar's input owns focus.
    pub search_focused: bool,
    /// The alternate screen (TUI) is active — no IME path.
    pub alt_screen: bool,
    /// Viewport height in rows (Shift+PageUp/Down amount).
    pub viewport_rows: i32,
    /// This platform reports AltGr as Ctrl+Alt (Windows).
    pub altgr_as_ctrl_alt: bool,
}

/// Classify a key-down. Pure: everything it needs is in `ks` and `ctx`.
pub(crate) fn classify_key(ks: &Keystroke, ctx: &KeyContext) -> KeyAction {
    let mods = ks.modifiers;
    let key = ks.key.as_str();
    // Platform modifier: Cmd on macOS, Ctrl on Windows/Linux.
    let plat = mods.control || mods.platform;

    // ── Search (platform+F) ──
    if plat && !mods.shift && !mods.alt && key == "f" {
        return KeyAction::ToggleSearch;
    }

    // The search input emits PressEnter to navigate matches, then propagates
    // Enter by design. Do not let that propagated key reach the terminal
    // while the search input owns focus.
    if matches!(key, "enter" | "return") && ctx.search_focused {
        return KeyAction::SwallowForSearchInput;
    }

    // Manual completion trigger: Ctrl+Shift+Space (docs/auto-completion/06 §6).
    if mods.control && mods.shift && key == "space" {
        return KeyAction::TriggerCompletion;
    }

    // ── Zoom shortcuts (platform +/−/0) ──
    if plat && !mods.alt {
        match key {
            "-" => return KeyAction::Zoom(ZoomKey::Out),
            "=" | "+" => return KeyAction::Zoom(ZoomKey::In),
            "0" => return KeyAction::Zoom(ZoomKey::Reset),
            _ => {}
        }
    }

    // ── Scroll keyboard actions ──
    if mods.shift {
        if let Some(action) = scroll_key_action(key, plat, ctx.viewport_rows) {
            return KeyAction::Scroll(action);
        }
    }

    // Copy/paste: Ctrl+Shift+C/V (Linux/Windows) or Cmd+C/V (macOS).
    let copy_paste = (mods.control && mods.shift) || (mods.platform && !mods.shift);
    if copy_paste {
        match key {
            "c" => return KeyAction::Copy,
            "v" => return KeyAction::Paste,
            _ => {}
        }
    }

    // ── Shift+Insert = paste (X11 convention) ──
    if mods.shift && key == "insert" {
        return KeyAction::Paste;
    }

    // IME active (not alt-screen): normal characters are handled by
    // replace_text_in_range, so skip on_key_down to avoid double input.
    if !ctx.alt_screen && !mods.control && !mods.alt && !mods.platform {
        if let Some(ch) = ks.key_char.as_deref() {
            if !ch.is_empty() && !ch.chars().any(|c| c.is_control()) {
                return KeyAction::LetImeHandle;
            }
        }
    }

    // AltGr on Windows arrives as Ctrl+Alt. When that chord produced
    // printable text it is a layout character (`@ { [ € ~` on DE/FR
    // layouts), not a control chord: return without stopping
    // propagation so the platform delivers it as text
    // (WM_CHAR → replace_text_in_range) instead of `encode_key`
    // turning it into a control byte.
    if is_altgr_text(ctx.altgr_as_ctrl_alt, &mods, ks.key_char.as_deref()) {
        return KeyAction::AltGrText;
    }

    let Some((spec, key_mods)) = map_key(ks) else {
        return KeyAction::Ignore;
    };

    // Ctrl+C (without Shift) = SIGINT — use send_ctrl_c().
    if key_mods.ctrl && !key_mods.shift {
        if let KeySpec::Character(ch) = &spec {
            if ch == "c" || ch == "C" {
                return KeyAction::Interrupt;
            }
        }
    }

    KeyAction::Send(spec, key_mods)
}

/// Attach the keyboard handler.
pub(crate) fn attach_key(
    div: gpui::Stateful<gpui::Div>,
    session: Entity<Box<dyn TerminalSession>>,
    view: Entity<LocalTerminalView>,
) -> gpui::Stateful<gpui::Div> {
    div.on_key_down({
        let s = session.clone();
        let view = view.clone();
        move |e: &KeyDownEvent, window, cx: &mut App| {
            let ctx = {
                let query = s.read(cx).query_state();
                let search_input = view.read(cx).search.input.clone();
                KeyContext {
                    search_focused: matches!(e.keystroke.key.as_str(), "enter" | "return")
                        && search_input.is_some_and(|input| {
                            input.read(cx).focus_handle(cx).is_focused(window)
                        }),
                    alt_screen: query.mode.contains(TermMode::ALT_SCREEN),
                    viewport_rows: query.rows as i32,
                    altgr_as_ctrl_alt: ALTGR_REPORTS_AS_CTRL_ALT,
                }
            };
            let action = classify_key(&e.keystroke, &ctx);

            match action {
                KeyAction::ToggleSearch => {
                    view.update(cx, |v, cx| v.toggle_search(window, cx));
                    cx.stop_propagation();
                    return;
                }
                KeyAction::SwallowForSearchInput => {
                    cx.stop_propagation();
                    return;
                }
                KeyAction::TriggerCompletion => {
                    view.update(cx, |v, cx| v.trigger_completion(cx));
                    cx.stop_propagation();
                    return;
                }
                _ => {}
            }

            // ── Auto-completion overlay key handling (before PTY delivery) ──
            {
                let key = e.keystroke.key.as_str();
                // Navigation / accept while the overlay is visible.
                let consumed = view.update(cx, |v, cx| {
                    v.completion_handle_key(key, e.keystroke.modifiers.control, cx)
                });
                if consumed {
                    cx.stop_propagation();
                    return;
                }
                // Run-first: Enter with no selection runs the command — capture
                // the typed line into history first, then let Enter reach the PTY.
                if matches!(key, "enter" | "return") {
                    view.update(cx, |v, cx| v.completion_capture_current(cx));
                }
            }

            let notify_view = |cx: &mut App| view.update(cx, |_, cx| cx.notify());
            let clear_bell = |cx: &mut App| {
                view.update(cx, |view, cx| {
                    if view.has_bell {
                        view.has_bell = false;
                        cx.notify();
                    }
                })
            };

            match action {
                KeyAction::ToggleSearch
                | KeyAction::SwallowForSearchInput
                | KeyAction::TriggerCompletion => unreachable!("handled above"),
                KeyAction::Zoom(zoom) => {
                    let settings_e = view.read(cx).deps.settings.clone();
                    let theme_default = f32::from(cx.theme().mono_font_size);
                    settings_e.update(cx, |st, cx| {
                        apply_zoom(st, zoom, theme_default);
                        cx.notify();
                    });
                    notify_view(cx);
                }
                KeyAction::Scroll(scroll) => {
                    s.update(cx, |s, _| match scroll {
                        ScrollKey::Lines(delta) => s.scroll(delta),
                        ScrollKey::Top => s.scroll_to_top(),
                        ScrollKey::Bottom => s.scroll_to_bottom(),
                    });
                    view.update(cx, |v, cx| {
                        v.scrollbar.mark_scrolled();
                        cx.notify();
                    });
                }
                KeyAction::Copy => edit::copy_selection(&s, cx),
                KeyAction::Paste => edit::paste_clipboard(&s, cx),
                // Let the platform / IME deliver the text: no stop_propagation.
                KeyAction::LetImeHandle | KeyAction::AltGrText | KeyAction::Ignore => return,
                KeyAction::Interrupt => {
                    s.update(cx, |s, _| {
                        s.scroll_to_bottom();
                        s.send_ctrl_c();
                    });
                    clear_bell(cx);
                }
                KeyAction::Send(spec, mods) => {
                    let app_cursor = s.read(cx).query_state().mode.contains(TermMode::APP_CURSOR);
                    let Some(bytes) = encode_key(&spec, mods, app_cursor) else {
                        return;
                    };
                    s.update(cx, |s, _| {
                        // Typing snaps the viewport back to the live screen (output
                        // alone no longer does — the user may be reading scrollback).
                        s.scroll_to_bottom();
                        if let Err(error) = s.write(&bytes) {
                            log::warn!("terminal key delivery failed: {error}");
                        }
                    });
                    // Clear the bell indicator when the user presses a key.
                    clear_bell(cx);
                }
            }
            cx.stop_propagation();
        }
    })
}

/// Apply a zoom key to the live settings.
fn apply_zoom(settings: &mut TerminalSettings, zoom: ZoomKey, theme_default: f32) {
    match zoom {
        ZoomKey::In => settings.zoom_in(theme_default),
        ZoomKey::Out => settings.zoom_out(theme_default),
        ZoomKey::Reset => settings.reset_zoom(),
    }
}

/// Map a Shift+key chord to a scrollback action: PageUp/PageDown scroll a
/// viewport, Home/End jump to the ends, and Platform+Shift+Up/Down scroll one
/// line. `None` for every other key.
fn scroll_key_action(key: &str, platform: bool, viewport: i32) -> Option<ScrollKey> {
    match key {
        "pageup" => Some(ScrollKey::Lines(viewport)),
        "pagedown" => Some(ScrollKey::Lines(-viewport)),
        "home" => Some(ScrollKey::Top),
        "end" => Some(ScrollKey::Bottom),
        "up" if platform => Some(ScrollKey::Lines(1)),
        "down" if platform => Some(ScrollKey::Lines(-1)),
        _ => None,
    }
}

/// Whether this platform reports AltGr as Ctrl+Alt (Windows) rather than as a
/// dedicated level-3 modifier (X11/Wayland/macOS).
const ALTGR_REPORTS_AS_CTRL_ALT: bool = cfg!(windows);

/// Classify a Ctrl+Alt chord that produced printable `key_char` as AltGr text.
///
/// Only meaningful when `altgr_as_ctrl_alt` (Windows): there the platform
/// cannot distinguish AltGr+q (`@` on a German layout) from a real Ctrl+Alt+q,
/// but a real Ctrl+Alt chord never yields printable text from `ToUnicode`, so
/// printable text is the discriminator. Elsewhere Ctrl+Alt is genuinely
/// Ctrl+Alt and must reach the encoder.
pub(crate) fn is_altgr_text(
    altgr_as_ctrl_alt: bool,
    mods: &gpui::Modifiers,
    key_char: Option<&str>,
) -> bool {
    if !altgr_as_ctrl_alt || !mods.control || !mods.alt || mods.platform {
        return false;
    }
    key_char.is_some_and(|text| !text.is_empty() && !text.chars().any(char::is_control))
}

#[cfg(test)]
mod tests {
    use gpui::{Keystroke, Modifiers};
    use oneterm_terminal::{KeySpec, NamedKey};

    use super::{
        KeyAction, KeyContext, ScrollKey, ZoomKey, classify_key, is_altgr_text, scroll_key_action,
    };

    fn ctx() -> KeyContext {
        KeyContext {
            search_focused: false,
            alt_screen: false,
            viewport_rows: 24,
            altgr_as_ctrl_alt: false,
        }
    }

    fn ks(key: &str, mods: Modifiers, key_char: Option<&str>) -> Keystroke {
        Keystroke {
            modifiers: mods,
            key: key.to_string(),
            key_char: key_char.map(str::to_string),
        }
    }

    fn ctrl() -> Modifiers {
        Modifiers {
            control: true,
            ..Default::default()
        }
    }

    fn ctrl_shift() -> Modifiers {
        Modifiers {
            control: true,
            shift: true,
            ..Default::default()
        }
    }

    fn shift() -> Modifiers {
        Modifiers {
            shift: true,
            ..Default::default()
        }
    }

    #[test]
    fn platform_f_toggles_search_but_shift_or_alt_do_not() {
        assert_eq!(
            classify_key(&ks("f", ctrl(), None), &ctx()),
            KeyAction::ToggleSearch
        );
        assert_ne!(
            classify_key(&ks("f", ctrl_shift(), None), &ctx()),
            KeyAction::ToggleSearch
        );
    }

    #[test]
    fn enter_is_swallowed_only_while_the_search_input_owns_focus() {
        let focused = KeyContext {
            search_focused: true,
            ..ctx()
        };
        assert_eq!(
            classify_key(&ks("enter", Modifiers::default(), Some("\r")), &focused),
            KeyAction::SwallowForSearchInput
        );
        assert!(matches!(
            classify_key(&ks("enter", Modifiers::default(), Some("\r")), &ctx()),
            KeyAction::Send(KeySpec::Named(NamedKey::Enter), _)
        ));
    }

    #[test]
    fn ctrl_shift_space_triggers_completion() {
        assert_eq!(
            classify_key(&ks("space", ctrl_shift(), None), &ctx()),
            KeyAction::TriggerCompletion
        );
    }

    #[test]
    fn platform_plus_minus_zero_zoom() {
        assert_eq!(
            classify_key(&ks("=", ctrl(), None), &ctx()),
            KeyAction::Zoom(ZoomKey::In)
        );
        assert_eq!(
            classify_key(&ks("-", ctrl(), None), &ctx()),
            KeyAction::Zoom(ZoomKey::Out)
        );
        assert_eq!(
            classify_key(&ks("0", ctrl(), None), &ctx()),
            KeyAction::Zoom(ZoomKey::Reset)
        );
        // Alt disables the zoom chords.
        let ctrl_alt = Modifiers {
            control: true,
            alt: true,
            ..Default::default()
        };
        assert_ne!(
            classify_key(&ks("0", ctrl_alt, None), &ctx()),
            KeyAction::Zoom(ZoomKey::Reset)
        );
    }

    #[test]
    fn shift_navigation_scrolls_the_scrollback() {
        assert_eq!(
            classify_key(&ks("pageup", shift(), None), &ctx()),
            KeyAction::Scroll(ScrollKey::Lines(24))
        );
        assert_eq!(
            classify_key(&ks("end", shift(), None), &ctx()),
            KeyAction::Scroll(ScrollKey::Bottom)
        );
        assert_eq!(
            classify_key(&ks("up", ctrl_shift(), None), &ctx()),
            KeyAction::Scroll(ScrollKey::Lines(1))
        );
    }

    #[test]
    fn copy_paste_chords() {
        assert_eq!(
            classify_key(&ks("c", ctrl_shift(), None), &ctx()),
            KeyAction::Copy
        );
        assert_eq!(
            classify_key(&ks("v", ctrl_shift(), None), &ctx()),
            KeyAction::Paste
        );
        let cmd = Modifiers {
            platform: true,
            ..Default::default()
        };
        assert_eq!(classify_key(&ks("v", cmd, None), &ctx()), KeyAction::Paste);
        assert_eq!(
            classify_key(&ks("insert", shift(), None), &ctx()),
            KeyAction::Paste
        );
    }

    #[test]
    fn plain_text_is_left_to_the_ime_on_the_primary_screen_only() {
        assert_eq!(
            classify_key(&ks("a", Modifiers::default(), Some("a")), &ctx()),
            KeyAction::LetImeHandle
        );
        let alt = KeyContext {
            alt_screen: true,
            ..ctx()
        };
        assert!(matches!(
            classify_key(&ks("a", Modifiers::default(), Some("a")), &alt),
            KeyAction::Send(KeySpec::Character(_), _)
        ));
    }

    #[test]
    fn ctrl_c_interrupts_and_ctrl_alt_text_is_altgr_on_windows() {
        assert_eq!(
            classify_key(&ks("c", ctrl(), None), &ctx()),
            KeyAction::Interrupt
        );
        let ctrl_alt = Modifiers {
            control: true,
            alt: true,
            ..Default::default()
        };
        let windows = KeyContext {
            altgr_as_ctrl_alt: true,
            ..ctx()
        };
        assert_eq!(
            classify_key(&ks("q", ctrl_alt, Some("@")), &windows),
            KeyAction::AltGrText
        );
        assert!(matches!(
            classify_key(&ks("q", ctrl_alt, Some("@")), &ctx()),
            KeyAction::Send(_, _)
        ));
    }

    #[test]
    fn shift_page_and_home_end_navigate_scrollback() {
        assert_eq!(
            scroll_key_action("pageup", false, 24),
            Some(ScrollKey::Lines(24))
        );
        assert_eq!(
            scroll_key_action("pagedown", false, 24),
            Some(ScrollKey::Lines(-24))
        );
        assert_eq!(scroll_key_action("home", false, 24), Some(ScrollKey::Top));
        assert_eq!(scroll_key_action("end", false, 24), Some(ScrollKey::Bottom));
    }

    #[test]
    fn line_scroll_needs_the_platform_modifier() {
        assert_eq!(scroll_key_action("up", false, 24), None);
        assert_eq!(scroll_key_action("up", true, 24), Some(ScrollKey::Lines(1)));
        assert_eq!(
            scroll_key_action("down", true, 24),
            Some(ScrollKey::Lines(-1))
        );
        assert_eq!(scroll_key_action("a", true, 24), None);
    }

    fn ctrl_alt() -> Modifiers {
        Modifiers {
            control: true,
            alt: true,
            ..Default::default()
        }
    }

    #[test]
    fn ctrl_alt_with_printable_text_is_altgr() {
        for text in ["@", "{", "[", "€", "~", "\\"] {
            assert!(is_altgr_text(true, &ctrl_alt(), Some(text)), "{text}");
        }
    }

    #[test]
    fn ctrl_alt_without_text_is_a_control_chord() {
        assert!(!is_altgr_text(true, &ctrl_alt(), None));
        assert!(!is_altgr_text(true, &ctrl_alt(), Some("")));
        // Control characters are never layout text.
        assert!(!is_altgr_text(true, &ctrl_alt(), Some("\u{1b}")));
    }

    #[test]
    fn plain_ctrl_or_alt_is_never_altgr() {
        let ctrl = Modifiers {
            control: true,
            ..Default::default()
        };
        let alt = Modifiers {
            alt: true,
            ..Default::default()
        };
        assert!(!is_altgr_text(true, &ctrl, Some("2")));
        assert!(!is_altgr_text(true, &alt, Some("a")));
        assert!(!is_altgr_text(true, &Modifiers::default(), Some("a")));
    }

    #[test]
    fn platforms_with_a_real_altgr_modifier_keep_ctrl_alt() {
        // X11/Wayland/macOS report AltGr separately, so Ctrl+Alt+2 there is a
        // real chord and must still be encoded (ESC NUL).
        assert!(!is_altgr_text(false, &ctrl_alt(), Some("2")));
    }
}
