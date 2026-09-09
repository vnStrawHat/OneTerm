//! Keyboard classification.
//!
//! [`classify_key`] is pure: everything a decision needs is in the
//! [`Keystroke`] and the [`KeyContext`] the view fills in. The view applies the
//! returned [`KeyAction`]; only [`send_key`] and [`interrupt`] touch the
//! session, so the PTY-facing half of the contract (snap to bottom, encode,
//! report the write) lives here instead of being retyped per call site.

use gpui::{App, Entity, Keystroke, Modifiers};
use oneterm_terminal::{
    KeyMods, KeySpec, NamedKey, TerminalSession, encode_key, report_generated_input,
};

/// What a key-down does. The order of the variants mirrors the classification
/// table in `low-level-design/input.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum KeyAction {
    /// Platform+F: open / close the search bar.
    ToggleSearch,
    /// Enter propagated from the focused search input — swallow it so it never
    /// reaches the terminal.
    SwallowInSearch,
    /// Ctrl+Shift+Space: force the completion overlay open.
    TriggerCompletion,
    /// The visible completion overlay consumed the key.
    Completion(CompletionKey),
    ZoomIn,
    ZoomOut,
    ZoomReset,
    /// Scroll the scrollback by whole lines (positive = towards history).
    ScrollLines(i32),
    /// Scroll by whole viewports (positive = towards history).
    ScrollPages(i32),
    ScrollTop,
    ScrollBottom,
    Copy,
    Paste,
    /// Ctrl+C: SIGINT via `send_ctrl_c`, regardless of the selection.
    Interrupt,
    /// Encode and write to the PTY (see [`send_key`]).
    Send(KeySpec, KeyMods),
    /// Deliberately not sent: the IME / platform path delivers this text.
    /// The view must not stop propagation.
    Ignore,
    /// No mapping exists. The view must not stop propagation.
    Unhandled,
}

/// What a key does while the completion overlay is visible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CompletionKey {
    /// Tab with nothing selected yet.
    SelectFirst,
    SelectNext,
    SelectPrev,
    /// Write the suggestion's `accept_bytes` and dismiss.
    Accept,
    Dismiss,
}

/// The view state a key decision depends on.
///
/// `completion_*` describe the overlay `US-0049` owns; `alt_screen` is what
/// keeps plain characters out of the PTY on the primary screen so the IME path
/// (`replace_text_in_range`) is the only writer of typed text.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct KeyContext {
    /// The search bar's input owns focus.
    pub search_focused: bool,
    /// The alternate screen (TUI) is active — no IME path.
    pub alt_screen: bool,
    pub completion_visible: bool,
    /// A suggestion is highlighted; navigation and Enter only bind then.
    pub completion_selected: bool,
    /// `completion.accept_tab` setting.
    pub completion_accept_tab: bool,
}

/// Classify a key-down. `prefer_char` is `KeyDownEvent::prefer_character_input`
/// — the platform's own "this is layout text, not a chord" flag.
pub(crate) fn classify_key(ks: &Keystroke, prefer_char: bool, ctx: KeyContext) -> KeyAction {
    // Row 0: the visible overlay sees every key first, and falls through for
    // the ones it does not bind.
    if ctx.completion_visible
        && let Some(action) = intercept_completion(ks, ctx)
    {
        return KeyAction::Completion(action);
    }

    let mods = ks.modifiers;
    let key = ks.key.as_str();
    // Cmd on macOS, Ctrl everywhere else.
    let secondary = mods.secondary();

    if secondary && !mods.shift && !mods.alt && key == "f" {
        return KeyAction::ToggleSearch;
    }

    // The search input emits its own navigation on Enter and then propagates
    // the key by design; it must not also reach the terminal.
    if matches!(key, "enter" | "return") && ctx.search_focused {
        return KeyAction::SwallowInSearch;
    }

    if mods.control && mods.shift && key == "space" {
        return KeyAction::TriggerCompletion;
    }

    if secondary && !mods.alt {
        match key {
            "-" => return KeyAction::ZoomOut,
            "=" | "+" => return KeyAction::ZoomIn,
            "0" => return KeyAction::ZoomReset,
            _ => {}
        }
    }

    if mods.shift {
        match key {
            "pageup" => return KeyAction::ScrollPages(1),
            "pagedown" => return KeyAction::ScrollPages(-1),
            "home" => return KeyAction::ScrollTop,
            "end" => return KeyAction::ScrollBottom,
            "up" if secondary => return KeyAction::ScrollLines(1),
            "down" if secondary => return KeyAction::ScrollLines(-1),
            _ => {}
        }
    }

    if is_clipboard_chord(&mods) {
        match key {
            "c" | "C" => return KeyAction::Copy,
            "v" | "V" => return KeyAction::Paste,
            _ => {}
        }
    }

    // X11 convention.
    if mods.shift && key == "insert" {
        return KeyAction::Paste;
    }

    // Typed text on the primary screen belongs to the IME path; sending it
    // here as well would double every character.
    if !ctx.alt_screen
        && !mods.control
        && !mods.alt
        && !mods.platform
        && is_layout_text(ks.key_char.as_deref())
    {
        return KeyAction::Ignore;
    }

    // AltGr on Windows arrives as Ctrl+Alt. A real Ctrl+Alt chord never
    // produces printable text, so printable text is the discriminator: let the
    // platform deliver it (WM_CHAR) instead of encoding a control byte.
    if (cfg!(windows) || prefer_char)
        && mods.control
        && mods.alt
        && !mods.platform
        && is_layout_text(ks.key_char.as_deref())
    {
        return KeyAction::Ignore;
    }

    if mods.control && !mods.shift && matches!(key, "c" | "C") {
        return KeyAction::Interrupt;
    }

    match map_key(ks) {
        Some((spec, key_mods)) => KeyAction::Send(spec, key_mods),
        None => KeyAction::Unhandled,
    }
}

/// Row 0 of the table: `None` means "not bound, keep classifying".
fn intercept_completion(ks: &Keystroke, ctx: KeyContext) -> Option<CompletionKey> {
    let ctrl = ks.modifiers.control;
    let selected = ctx.completion_selected;
    match ks.key.as_str() {
        // Navigation only binds once the user picked an entry, so the first
        // Down/Up after the overlay appears still reaches the shell.
        "down" => selected.then_some(CompletionKey::SelectNext),
        "n" if ctrl => selected.then_some(CompletionKey::SelectNext),
        "up" => selected.then_some(CompletionKey::SelectPrev),
        "p" if ctrl => selected.then_some(CompletionKey::SelectPrev),
        "escape" => Some(CompletionKey::Dismiss),
        // Enter with nothing selected runs the typed command instead.
        "enter" | "return" => selected.then_some(CompletionKey::Accept),
        "tab" if ctx.completion_accept_tab => Some(if selected {
            CompletionKey::Accept
        } else {
            CompletionKey::SelectFirst
        }),
        _ => None,
    }
}

/// Copy/paste chord: Cmd+key on macOS, Ctrl+Shift+key elsewhere.
fn is_clipboard_chord(m: &Modifiers) -> bool {
    if cfg!(target_os = "macos") {
        m.platform && !m.shift && !m.control && !m.alt
    } else {
        m.control && m.shift && !m.platform && !m.alt
    }
}

/// Whether `key_char` is layout text rather than a control byte.
fn is_layout_text(key_char: Option<&str>) -> bool {
    key_char.is_some_and(|text| !text.is_empty() && !text.chars().any(char::is_control))
}

/// Map a GPUI [`Keystroke`] to the encoder's [`KeySpec`] + [`KeyMods`].
///
/// `key_char` is the literal text; `key` is the chord name and is the only
/// thing available once Ctrl/Alt suppress the character. Multi-character names
/// have no terminal encoding, so they return `None` rather than being sent as
/// literal text — except `"space"`, translated to `" "` so Ctrl+Space encodes
/// NUL.
pub(crate) fn map_key(ks: &Keystroke) -> Option<(KeySpec, KeyMods)> {
    let mods = ks.modifiers;
    let key_mods = KeyMods {
        shift: mods.shift,
        ctrl: mods.control,
        alt: mods.alt,
    };
    if let Some(named) = named_key(ks.key.as_str()) {
        return Some((KeySpec::Named(named), key_mods));
    }
    let text = ks
        .key_char
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            if ks.key == "space" {
                " ".to_string()
            } else {
                ks.key.clone()
            }
        });
    if text.chars().count() != 1 {
        // An unrecognised multi-character name ("print", "f25"): no encoding
        // exists, with or without modifiers.
        return None;
    }
    Some((KeySpec::Character(text), key_mods))
}

fn named_key(key: &str) -> Option<NamedKey> {
    Some(match key {
        "enter" | "return" => NamedKey::Enter,
        "backspace" => NamedKey::Backspace,
        "delete" => NamedKey::Delete,
        "tab" => NamedKey::Tab,
        "escape" => NamedKey::Escape,
        "up" => NamedKey::ArrowUp,
        "down" => NamedKey::ArrowDown,
        "left" => NamedKey::ArrowLeft,
        "right" => NamedKey::ArrowRight,
        "home" => NamedKey::Home,
        "end" => NamedKey::End,
        "pageup" => NamedKey::PageUp,
        "pagedown" => NamedKey::PageDown,
        "insert" => NamedKey::Insert,
        "f1" => NamedKey::F1,
        "f2" => NamedKey::F2,
        "f3" => NamedKey::F3,
        "f4" => NamedKey::F4,
        "f5" => NamedKey::F5,
        "f6" => NamedKey::F6,
        "f7" => NamedKey::F7,
        "f8" => NamedKey::F8,
        "f9" => NamedKey::F9,
        "f10" => NamedKey::F10,
        "f11" => NamedKey::F11,
        "f12" => NamedKey::F12,
        "f13" => NamedKey::F13,
        "f14" => NamedKey::F14,
        "f15" => NamedKey::F15,
        "f16" => NamedKey::F16,
        "f17" => NamedKey::F17,
        "f18" => NamedKey::F18,
        "f19" => NamedKey::F19,
        "f20" => NamedKey::F20,
        "f21" => NamedKey::F21,
        "f22" => NamedKey::F22,
        "f23" => NamedKey::F23,
        "f24" => NamedKey::F24,
        _ => return None,
    })
}

/// Apply [`KeyAction::Send`]: snap the viewport back to the live screen (the
/// user is typing, so they want to see the echo) and write the encoding.
///
/// Returns the bytes that were sent so the caller can repeat them on its
/// broadcast channel, or `None` when the chord has no encoding (Ctrl +
/// non-ASCII), in which case nothing was written and nothing failed.
pub(crate) fn send_key(
    session: &Entity<Box<dyn TerminalSession>>,
    spec: &KeySpec,
    mods: KeyMods,
    app_cursor: bool,
    cx: &mut App,
) -> Option<Vec<u8>> {
    let bytes = encode_key(spec, mods, app_cursor)?;
    session.update(cx, |s, _| {
        s.scroll_to_bottom();
        report_generated_input("key", s.write(&bytes));
    });
    Some(bytes)
}

/// Apply [`KeyAction::Interrupt`]: SIGINT, snapping to the live screen. Runs
/// regardless of the selection — Ctrl+C is never "copy" on this side.
pub(crate) fn interrupt(session: &Entity<Box<dyn TerminalSession>>, cx: &mut App) {
    session.update(cx, |s, _| {
        s.scroll_to_bottom();
        s.send_ctrl_c();
    });
}
