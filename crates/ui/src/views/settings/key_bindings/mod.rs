//! Configurable key bindings — global state, init/apply/persist, and keystroke
//! helpers.
//!
//! A curated set of OneTerm actions is exposed for rebinding. The user's
//! overrides live in `ui_config.json` (the `key_bindings` map) and are applied
//! at startup by [`apply_key_bindings`].
//!
//! Rebinding is "press-to-rebind": clicking **Edit** on a row focuses a capture
//! element whose next `key_down` becomes the new binding (Escape cancels). This
//! avoids a free-text keystroke field (which would fire per-keystroke and
//! round-trip badly with `SettingField`).
//!
//! Replacing a binding cleanly (freeing the old keystroke) requires clearing the
//! keymap — gpui has no per-binding remove API. [`apply_key_bindings`] snapshots
//! the gpui-component bindings (registered during `gpui_component::init`) once at
//! startup, then on every apply does `clear_key_bindings` + re-add the snapshot
//! + the effective OneTerm set. This preserves input/combobox/dialog bindings
//! while replacing ours.
//!
//! Some rebindable actions (e.g. `gpui_component::input::Copy`) already have
//! default bindings in the snapshot, scoped to a context like `"Input"`. To
//! avoid stale duplicates when the user rebinds or unbinds such an action,
//! [`apply_key_bindings`] also filters the snapshot by action name — every
//! binding whose action name matches a rebindable action is removed before the
//! snapshot is re-registered, then the effective binding (which carries the
//! correct context) is added back from the OneTerm set.
//!
//! The action registry lives in [`key_bindings_actions`] and the settings-page UI
//! (page builder, row rendering, event handlers) lives in [`key_bindings_ui`].

mod key_bindings_actions;
mod key_bindings_ui;

use key_bindings_actions::BINDABLE_ACTIONS;
pub(crate) use key_bindings_ui::page;

use std::collections::HashMap;

use gpui::{App, AppContext as _, FocusHandle, Global, KeyBinding, Keystroke};

use crate::state::UiConfig;

// ── Global state ─────────────────────────────────────────────────────

/// Live key-binding UI state: the effective keystroke per action + which action
/// (if any) is currently awaiting a key press ("capturing").
pub(super) struct KeyBindingsState {
    /// action id → effective keystroke ("" = unbound).
    pub effective: HashMap<String, String>,
    /// action id currently in "press-to-rebind" capture mode, or `None`.
    pub capturing: Option<String>,
    /// Focus handle reused by the single capture element.
    pub capture_focus: FocusHandle,
}

/// Global wrapper for `Entity<KeyBindingsState>`.
pub struct KeyBindingsStateGlobal(pub gpui::Entity<KeyBindingsState>);
impl Global for KeyBindingsStateGlobal {}

impl KeyBindingsState {
    pub fn global(cx: &App) -> gpui::Entity<Self> {
        cx.global::<KeyBindingsStateGlobal>().0.clone()
    }
}

/// Snapshot of all key bindings registered by `gpui_component::init` (input,
/// combobox, dialog, …), taken once before OneTerm registers its own. Used by
/// [`apply_key_bindings`] to restore them after `clear_key_bindings`.
pub struct KeyBindingsSnapshotGlobal(pub Vec<KeyBinding>);
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
fn save_key_bindings(cx: &mut App) {
    let map: HashMap<String, String> = {
        let state = KeyBindingsState::global(cx).read(cx);
        BINDABLE_ACTIONS
            .iter()
            .filter_map(|a| {
                let eff = state.effective.get(a.id).map(|s| s.as_str()).unwrap_or("");
                let def = a.default.unwrap_or("");
                if eff == def {
                    None
                } else {
                    Some((a.id.to_string(), eff.to_string()))
                }
            })
            .collect()
    };
    UiConfig::global(cx).update(cx, |cfg, _| cfg.key_bindings = map);
    UiConfig::persist(cx);
}

// ── Keystroke helpers ────────────────────────────────────────────────

/// Convert a captured `Keystroke` into the binding-string format gpui parses
/// (`ctrl-`, `alt-`, `shift-`, `cmd-`/`win-`/`fn-` prefixes + key). Built manually
/// (rather than `Keystroke`'s `Display`, which uses unicode glyphs like `⊞` that
/// `Keystroke::parse` does not accept).
fn keystroke_to_string(ks: &Keystroke) -> String {
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
fn is_modifier_only(ks: &Keystroke) -> bool {
    matches!(
        ks.key.to_ascii_lowercase().as_str(),
        "control" | "alt" | "shift" | "platform" | "function" | "cmd" | "super" | "win"
    )
}
