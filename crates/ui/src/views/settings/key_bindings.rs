//! Configurable key bindings.
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

use std::collections::HashMap;

use gpui::{
    Action, AnyElement, App, AppContext as _, FocusHandle, Global, InteractiveElement as _,
    IntoElement, KeyBinding, KeyDownEvent, Keystroke, ParentElement as _, Styled, Window, div,
};
use gpui_component::{
    ActiveTheme as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    kbd::Kbd,
    setting::{SettingGroup, SettingItem},
};

use crate::actions::{
    About, AddPanel, NewSession, OpenSettings, Quit, ToggleAutoHideRightDock, ToggleGutter,
};
use crate::state::UiConfig;

// ── Bindable action registry ─────────────────────────────────────────

/// One rebindable action. `make` builds a `KeyBinding` for the given keystroke
/// string (returning `None` if the keystroke is empty or fails to parse, since
/// `KeyBinding::new` panics on parse errors).
pub(super) struct BindableAction {
    pub id: &'static str,
    pub label: &'static str,
    pub default: Option<&'static str>,
    pub make: fn(&str) -> Option<KeyBinding>,
}

/// Build a `KeyBinding` for `action` at `ks`, validating first (empty → unbound,
/// unparseable → ignored) so `KeyBinding::new` never panics.
fn make_binding<A: Action>(ks: &str, action: A) -> Option<KeyBinding> {
    if ks.is_empty() || Keystroke::parse(ks).is_err() {
        return None;
    }
    Some(KeyBinding::new(ks, action, None))
}

/// The curated set of rebindable actions (order = display order).
pub(super) const BINDABLE_ACTIONS: &[BindableAction] = &[
    BindableAction {
        id: "toggle_zoom",
        label: "Zoom Active Panel",
        default: Some("shift-escape"),
        make: |ks| make_binding(ks, gpui_component::dock::ToggleZoom),
    },
    BindableAction {
        id: "close_panel",
        label: "Close Panel",
        default: Some("ctrl-w"),
        make: |ks| make_binding(ks, gpui_component::dock::ClosePanel),
    },
    BindableAction {
        id: "new_terminal_tab",
        label: "New Terminal Tab",
        default: None,
        make: |ks| make_binding(ks, AddPanel(gpui_component::dock::DockPlacement::Center)),
    },
    BindableAction {
        id: "new_ssh_session",
        label: "New SSH Session",
        default: None,
        make: |ks| make_binding(ks, NewSession),
    },
    BindableAction {
        id: "toggle_gutter",
        label: "Toggle Gutter",
        default: None,
        make: |ks| make_binding(ks, ToggleGutter),
    },
    BindableAction {
        id: "auto_hide_right_dock",
        label: "Auto-hide Right Dock",
        default: None,
        make: |ks| make_binding(ks, ToggleAutoHideRightDock),
    },
    BindableAction {
        id: "about",
        label: "About OneTerm",
        default: None,
        make: |ks| make_binding(ks, About),
    },
    BindableAction {
        id: "quit",
        label: "Quit",
        default: None,
        make: |ks| make_binding(ks, Quit),
    },
    BindableAction {
        id: "open_settings",
        label: "Open Settings",
        default: Some("ctrl-,"),
        make: |ks| make_binding(ks, OpenSettings),
    },
];

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

/// Clear the keymap and re-register: the gpui-component snapshot + the effective
/// OneTerm bindings. Called at startup and after every rebind.
pub(crate) fn apply_key_bindings(cx: &mut App) {
    let snapshot = cx.global::<KeyBindingsSnapshotGlobal>().0.clone();
    let effective = KeyBindingsState::global(cx).read(cx).effective.clone();
    cx.clear_key_bindings();
    cx.bind_keys(snapshot);
    let mut bindings = Vec::new();
    for a in BINDABLE_ACTIONS {
        if let Some(ks) = effective.get(a.id) {
            if let Some(b) = (a.make)(ks) {
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

// ── UI ───────────────────────────────────────────────────────────────

/// Build the editable "Key Bindings" group for the General page.
pub(crate) fn key_bindings_group() -> SettingGroup {
    let mut group = SettingGroup::new().title("Key Bindings");
    for a in BINDABLE_ACTIONS {
        let a: &'static BindableAction = a;
        let label = a.label;
        group = group
            .item(
                SettingItem::render(move |_, window, cx| render_binding_row(a, window, cx))
                    .keywords([label]),
            )
            .description(show_default(a.default));
    }
    group
}

/// Description line showing the built-in default keystroke (or "Unbound").
fn show_default(default: Option<&'static str>) -> &'static str {
    // Leak a one-time formatted string per default — small fixed set, lives for the
    // program lifetime, so a 'static leak is acceptable and keeps the API simple
    // (SettingItem::description takes &'static str).
    match default {
        Some(d) => Box::leak(format!("Default: {d}").into_boxed_str()),
        None => "Default: (unbound)",
    }
}

/// Render one binding row: label + binding chip + Edit/Reset, or the capture
/// element when this action is in "press-to-rebind" mode.
fn render_binding_row(
    a: &'static BindableAction,
    _window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let (capturing, eff, handle) = {
        let state = KeyBindingsState::global(cx).read(cx);
        (
            state.capturing.as_deref() == Some(a.id),
            state.effective.get(a.id).cloned().unwrap_or_default(),
            state.capture_focus.clone(),
        )
    };

    if capturing {
        let id = a.id;
        div()
            .id("kbd-capture")
            .track_focus(&handle)
            .on_key_down(move |event: &KeyDownEvent, _window, cx| on_capture_key(id, event, cx))
            .w_full()
            .h_7()
            .flex()
            .items_center()
            .px_2()
            .rounded_sm()
            .border_1()
            .border_color(cx.theme().ring)
            .text_sm()
            .child("Press keys…  (Esc to cancel)")
            .into_any_element()
    } else {
        let chip = binding_chip(&eff, cx);
        h_flex()
            .w_full()
            .justify_between()
            .items_center()
            .child(a.label)
            .child(
                h_flex()
                    .gap_1()
                    .items_center()
                    .child(chip)
                    .child(
                        Button::new(format!("edit-{}", a.id))
                            .ghost()
                            .small()
                            .label("Edit")
                            .on_click(move |_, window, cx| on_edit(a.id, window, cx)),
                    )
                    .child(
                        Button::new(format!("reset-{}", a.id))
                            .ghost()
                            .small()
                            .label("Reset")
                            .on_click(move |_, _, cx| on_reset(a.id, cx)),
                    ),
            )
            .into_any_element()
    }
}

/// A Kbd chip for a valid keystroke, or a muted dash when unbound/invalid.
fn binding_chip(ks: &str, cx: &App) -> AnyElement {
    if ks.is_empty() {
        return dash_label(cx);
    }
    match Keystroke::parse(ks) {
        Ok(stroke) => Kbd::new(stroke).into_any_element(),
        Err(_) => dash_label(cx),
    }
}

/// Muted dash used as the "no binding" placeholder.
fn dash_label(cx: &App) -> AnyElement {
    div()
        .text_color(cx.theme().muted_foreground)
        .child("—")
        .into_any_element()
}

// ── Event handlers ───────────────────────────────────────────────────

/// "Edit" clicked → enter capture mode + focus the capture element.
fn on_edit(id: &'static str, window: &mut Window, cx: &mut App) {
    let handle = KeyBindingsState::global(cx).read(cx).capture_focus.clone();
    KeyBindingsState::global(cx).update(cx, |s, cx| {
        s.capturing = Some(id.to_string());
        cx.notify();
    });
    handle.focus(window, cx);
}

/// "Reset" clicked → restore the built-in default (or unbind), persist, re-apply.
fn on_reset(id: &'static str, cx: &mut App) {
    let default = BINDABLE_ACTIONS
        .iter()
        .find(|a| a.id == id)
        .and_then(|a| a.default)
        .map(|s| s.to_string())
        .unwrap_or_default();
    KeyBindingsState::global(cx).update(cx, |s, cx| {
        s.effective.insert(id.to_string(), default);
        s.capturing = None;
        cx.notify();
    });
    save_key_bindings(cx);
    apply_key_bindings(cx);
}

/// A key was pressed while capturing → set it as the new binding (Escape cancels;
/// bare modifiers are ignored; unparseable combinations are ignored).
fn on_capture_key(id: &'static str, event: &KeyDownEvent, cx: &mut App) {
    let ks = &event.keystroke;
    if ks.key == "escape" {
        KeyBindingsState::global(cx).update(cx, |s, cx| {
            s.capturing = None;
            cx.notify();
        });
        return;
    }
    if is_modifier_only(ks) {
        return;
    }
    let binding = keystroke_to_string(ks);
    if Keystroke::parse(&binding).is_err() {
        return;
    }
    KeyBindingsState::global(cx).update(cx, |s, cx| {
        s.effective.insert(id.to_string(), binding);
        s.capturing = None;
        cx.notify();
    });
    save_key_bindings(cx);
    apply_key_bindings(cx);
}
