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
            // Use key_char for character input. When modifiers (ctrl/alt/platform)
            // are held, key_char is typically None (e.g. Ctrl+C doesn't produce
            // a printable character), so fall back to keystroke.key — which IS
            // the character name for letter keys.
            //
            // When NO modifiers are held and key_char is None, return None to
            // prevent unrecognized named keys (e.g. "f25") from being sent as
            // literal text.
            let ch = keystroke
                .key_char
                .clone()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| keystroke.key.clone());
            // If the fallback key is a single character, it's a real key press.
            // If it's multi-character (like "f25") and no modifiers are held,
            // it's an unrecognized named key — return None.
            if ch.chars().count() != 1 && !mods.control && !mods.alt && !mods.platform {
                return None;
            }
            KeySpec::Character(ch)
        };
        Some((spec, keymods))
    }
}

#[cfg(test)]
#[allow(clippy::needless_update)]
mod tests {
    use super::super::LocalTerminalView;
    use gpui::{Keystroke, Modifiers};
    use oneterm_core::terminal::KeySpec;

    fn ks(key: &str, key_char: Option<&str>, mods: Modifiers) -> Keystroke {
        Keystroke {
            key: key.into(),
            key_char: key_char.map(|s| s.into()),
            modifiers: mods,
            ..Default::default()
        }
    }

    #[test]
    fn ctrl_c_maps_to_character() {
        // Ctrl+C: key_char is None, but control modifier is held.
        // Must return Some(Character("c")) so the SIGINT handler can match it.
        let (spec, mods) = LocalTerminalView::map_key(&ks(
            "c",
            None,
            Modifiers {
                control: true,
                ..Default::default()
            },
        ))
        .expect("Ctrl+C must map to a key");
        assert!(matches!(spec, KeySpec::Character(ref ch) if ch == "c"));
        assert!(mods.ctrl);
    }

    #[test]
    fn unknown_named_key_without_modifiers_returns_none() {
        // An unrecognized named key like "f25" with no modifiers should
        // return None to prevent it being sent as literal text.
        assert!(LocalTerminalView::map_key(&ks("f25", None, Modifiers::default())).is_none());
    }

    #[test]
    fn plain_a_maps_to_character() {
        let (spec, _) =
            LocalTerminalView::map_key(&ks("a", Some("a"), Modifiers::default())).unwrap();
        assert!(matches!(spec, KeySpec::Character(ref ch) if ch == "a"));
    }
}
