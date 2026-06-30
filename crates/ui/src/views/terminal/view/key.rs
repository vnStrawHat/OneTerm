//! Map GPUI `Keystroke` → terminal `KeySpec` + `KeyMods`.

use oneterm_core::terminal::{KeyMods, KeySpec, NamedKey};

impl super::LocalTerminalView {
    /// Map GPUI `Keystroke` → `KeySpec` + `KeyMods`.
    pub(crate) fn map_key(keystroke: &gpui::Keystroke) -> Option<(KeySpec, KeyMods)> {
        let mods = keystroke.modifiers;
        let keymods = KeyMods {
            shift: mods.shift,
            ctrl: mods.control,
            alt: mods.alt,
        };
        let named = match keystroke.key.as_str() {
            "enter" | "return" => Some(NamedKey::Enter),
            "backspace" => Some(NamedKey::Backspace),
            "delete" => Some(NamedKey::Delete),
            "tab" => Some(NamedKey::Tab),
            "escape" => Some(NamedKey::Escape),
            "up" => Some(NamedKey::ArrowUp),
            "down" => Some(NamedKey::ArrowDown),
            "left" => Some(NamedKey::ArrowLeft),
            "right" => Some(NamedKey::ArrowRight),
            "home" => Some(NamedKey::Home),
            "end" => Some(NamedKey::End),
            "pageup" => Some(NamedKey::PageUp),
            "pagedown" => Some(NamedKey::PageDown),
            "insert" => Some(NamedKey::Insert),
            _ => None,
        };
        let spec = if let Some(n) = named {
            KeySpec::Named(n)
        } else {
            let ch = keystroke
                .key_char
                .clone()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| keystroke.key.clone());
            KeySpec::Character(ch)
        };
        Some((spec, keymods))
    }
}
