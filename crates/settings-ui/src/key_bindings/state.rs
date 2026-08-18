//! Configurable key bindings — global state, init/apply/persist, and keystroke
//! helpers.
//!
//! See [`super`] for the module overview and the design rationale behind the
//! snapshot-and-reapply strategy.

use std::collections::HashMap;

use gpui::{App, AppContext as _, FocusHandle, Global, KeyBinding, Keystroke};

use oneterm_settings::UiConfig;

use super::key_bindings_actions::BINDABLE_ACTIONS;

// ── Global state ─────────────────────────────────────────────────────

/// Live key-binding UI state: the effective keystroke per action + which action
/// (if any) is currently awaiting a key press ("capturing").
pub(crate) struct KeyBindingsState {
    /// action id → effective keystroke ("" = unbound).
    pub(super) effective: HashMap<String, String>,
    /// action id currently in "press-to-rebind" capture mode, or `None`.
    pub(super) capturing: Option<String>,
    /// Focus handle reused by the single capture element.
    pub(super) capture_focus: FocusHandle,
}

/// Global wrapper for `Entity<KeyBindingsState>`.
pub(crate) struct KeyBindingsStateGlobal(pub gpui::Entity<KeyBindingsState>);
impl Global for KeyBindingsStateGlobal {}

impl KeyBindingsState {
    pub(crate) fn global(cx: &App) -> gpui::Entity<Self> {
        cx.global::<KeyBindingsStateGlobal>().0.clone()
    }
}

/// Snapshot of all key bindings registered by `gpui_component::init` (input,
/// combobox, dialog, …), taken once before OneTerm registers its own. Used by
/// [`apply_key_bindings`] to restore them after `clear_key_bindings`.
pub(crate) struct KeyBindingsSnapshotGlobal(pub Vec<KeyBinding>);
impl Global for KeyBindingsSnapshotGlobal {}

// ── Init / apply / persist ───────────────────────────────────────────

/// Create the global `KeyBindingsState` from the persisted config (called from
/// `OneTermWorkspace::bind_keys`, after the snapshot is taken).
pub(crate) fn init_state(cx: &mut App) {
    let overrides = {
        let cfg = UiConfig::global(cx).read(cx);
        cfg.key_bindings.clone()
    };
    let mut effective = HashMap::new();
    for a in BINDABLE_ACTIONS {
        let ks = overrides
            .get(a.id)
            .cloned()
            .or_else(|| a.default.map(|s| s.to_string()))
            .unwrap_or_default();
        effective.insert(a.id.to_string(), ks);
    }
    let capture_focus = cx.focus_handle();
    let entity = cx.new(|_| KeyBindingsState {
        effective,
        capturing: None,
        capture_focus,
    });
    cx.set_global(KeyBindingsStateGlobal(entity));
}

/// Clear the keymap and re-register: the gpui-component snapshot (filtered)
/// + the effective OneTerm bindings. Called at startup and after every rebind.
///
/// The snapshot is filtered by action name: every binding whose action name
/// matches a rebindable action is removed, so the effective binding (which may
/// use a different keystroke or context, or be unbound entirely) replaces it
/// cleanly. Bindings for non-rebindable actions (combobox, dialog, etc.) are
/// preserved as-is.
pub(crate) fn apply_key_bindings(cx: &mut App) {
    let mut snapshot = cx.global::<KeyBindingsSnapshotGlobal>().0.clone();
    let effective = KeyBindingsState::global(cx).read(cx).effective.clone();

    // Collect the action names of all rebindable actions so we can strip their
    // default bindings from the snapshot (prevents stale duplicates).
    let rebindable_names: Vec<&'static str> =
        BINDABLE_ACTIONS.iter().map(|a| (a.name_fn)()).collect();
    snapshot.retain(|b| !rebindable_names.contains(&b.action().name()));

    cx.clear_key_bindings();
    cx.bind_keys(snapshot);
    let mut bindings = Vec::new();
    for a in BINDABLE_ACTIONS {
        if let Some(ks) = effective.get(a.id) {
            if let Some(b) = (a.make)(ks, a.context) {
                bindings.push(b);
            }
        }
    }
    cx.bind_keys(bindings);
}

/// Write the effective bindings (only overrides — entries equal to the built-in
/// default are omitted) into `ui_config.json` and save.
pub(super) fn save_key_bindings(cx: &mut App) {
    let map = {
        let state = KeyBindingsState::global(cx).read(cx);
        overrides_from_effective(&state.effective)
    };
    UiConfig::global(cx).update(cx, |cfg, _| cfg.key_bindings = map);
    UiConfig::persist(cx);
}

/// Reduce the effective bindings to the persisted override map: only entries
/// that differ from the built-in default are kept, and an unbound action whose
/// default is bound is stored as an empty string.
fn overrides_from_effective(effective: &HashMap<String, String>) -> HashMap<String, String> {
    BINDABLE_ACTIONS
        .iter()
        .filter_map(|a| {
            let eff = effective.get(a.id).map(|s| s.as_str()).unwrap_or("");
            let def = a.default.unwrap_or("");
            if eff == def {
                None
            } else {
                Some((a.id.to_string(), eff.to_string()))
            }
        })
        .collect()
}

// ── Keystroke helpers ────────────────────────────────────────────────

/// Convert a captured `Keystroke` into the binding-string format gpui parses
/// (`ctrl-`, `alt-`, `shift-`, `cmd-`/`win-`/`fn-` prefixes + key). Built manually
/// (rather than `Keystroke`'s `Display`, which uses unicode glyphs like `⊞` that
/// `Keystroke::parse` does not accept).
pub(super) fn keystroke_to_string(ks: &Keystroke) -> String {
    let mut s = String::new();
    if ks.modifiers.control {
        s.push_str("ctrl-");
    }
    if ks.modifiers.alt {
        s.push_str("alt-");
    }
    if ks.modifiers.shift {
        s.push_str("shift-");
    }
    if ks.modifiers.platform {
        s.push_str("cmd-");
    }
    if ks.modifiers.function {
        s.push_str("fn-");
    }
    s.push_str(&ks.key);
    s
}

/// A bare modifier press (no real key) — ignore so we wait for the actual key.
pub(super) fn is_modifier_only(ks: &Keystroke) -> bool {
    matches!(
        ks.key.to_ascii_lowercase().as_str(),
        "control" | "alt" | "shift" | "platform" | "function" | "cmd" | "super" | "win"
    )
}

#[cfg(test)]
mod tests {
    use gpui::Modifiers;

    use super::*;

    fn keystroke(key: &str, modifiers: Modifiers) -> Keystroke {
        Keystroke {
            modifiers,
            key: key.to_owned(),
            key_char: None,
        }
    }

    #[test]
    fn keystroke_string_lists_modifiers_in_gpui_order() {
        let stroke = keystroke(
            "t",
            Modifiers {
                control: true,
                alt: true,
                shift: true,
                platform: true,
                function: true,
            },
        );
        assert_eq!(keystroke_to_string(&stroke), "ctrl-alt-shift-cmd-fn-t");
        assert_eq!(
            keystroke_to_string(&keystroke("f5", Modifiers::default())),
            "f5"
        );
        // The result must round-trip through gpui's parser.
        assert!(Keystroke::parse(&keystroke_to_string(&stroke)).is_ok());
    }

    #[test]
    fn bare_modifier_presses_are_recognised() {
        for key in [
            "control", "Shift", "alt", "platform", "function", "cmd", "super", "win",
        ] {
            assert!(
                is_modifier_only(&keystroke(key, Modifiers::default())),
                "{key}"
            );
        }
        assert!(!is_modifier_only(&keystroke("t", Modifiers::control())));
        assert!(!is_modifier_only(&keystroke(
            "escape",
            Modifiers::default()
        )));
    }

    #[test]
    fn overrides_keep_only_entries_that_differ_from_the_default() {
        let bound = BINDABLE_ACTIONS
            .iter()
            .find(|a| a.default.is_some())
            .expect("at least one action has a default");
        let default = bound.default.unwrap();

        // Everything at its default: nothing to persist.
        let effective: HashMap<String, String> = BINDABLE_ACTIONS
            .iter()
            .map(|a| (a.id.to_owned(), a.default.unwrap_or("").to_owned()))
            .collect();
        assert!(overrides_from_effective(&effective).is_empty());

        // Rebound: persisted with the new keystroke.
        let mut rebound = effective.clone();
        rebound.insert(bound.id.to_owned(), format!("ctrl-alt-{default}"));
        let overrides = overrides_from_effective(&rebound);
        assert_eq!(overrides.len(), 1);
        assert_eq!(overrides[bound.id], format!("ctrl-alt-{default}"));

        // Unbound: persisted as an empty string so the default is suppressed.
        let mut unbound = effective;
        unbound.insert(bound.id.to_owned(), String::new());
        assert_eq!(overrides_from_effective(&unbound)[bound.id], "");
    }
}
