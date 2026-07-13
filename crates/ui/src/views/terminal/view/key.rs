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
            "f1" => Some(NamedKey::F1),
            "f2" => Some(NamedKey::F2),
            "f3" => Some(NamedKey::F3),
            "f4" => Some(NamedKey::F4),
            "f5" => Some(NamedKey::F5),
            "f6" => Some(NamedKey::F6),
            "f7" => Some(NamedKey::F7),
            "f8" => Some(NamedKey::F8),
            "f9" => Some(NamedKey::F9),
            "f10" => Some(NamedKey::F10),
            "f11" => Some(NamedKey::F11),
            "f12" => Some(NamedKey::F12),
            "f13" => Some(NamedKey::F13),
            "f14" => Some(NamedKey::F14),
            "f15" => Some(NamedKey::F15),
            "f16" => Some(NamedKey::F16),
            "f17" => Some(NamedKey::F17),
            "f18" => Some(NamedKey::F18),
            "f19" => Some(NamedKey::F19),
            "f20" => Some(NamedKey::F20),
            "f21" => Some(NamedKey::F21),
            "f22" => Some(NamedKey::F22),
            "f23" => Some(NamedKey::F23),
            "f24" => Some(NamedKey::F24),
            _ => None,
        };
        let spec = if let Some(n) = named {
            KeySpec::Named(n)
        } else {
            // Only use key_char for character input. If there's no key_char
            // (e.g. a named key we don't recognize like "f1"), return None
            // to prevent sending the key name as literal text.
            let ch = keystroke.key_char.clone().filter(|s| !s.is_empty())?;
            KeySpec::Character(ch)
        };
        Some((spec, keymods))
    }
}
