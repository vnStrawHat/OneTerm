//! UI for the Key Bindings settings page — the `SettingPage` builder, per-row
//! render functions, and press-to-rebind event handlers.
//!
//! See [`super`] for the global state, init/apply/persist logic, and keystroke
//! helpers.

use gpui::{
    AnyElement, App, InteractiveElement as _, IntoElement, KeyDownEvent, ParentElement as _,
    Styled, Window, div,
};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    kbd::Kbd,
    setting::{SettingGroup, SettingItem, SettingPage},
};

use crate::separator;

use super::key_bindings_actions::{BINDABLE_ACTIONS, BindableAction};
use super::state::{
    KeyBindingsState, apply_key_bindings, is_modifier_only, keystroke_to_string, save_key_bindings,
};

// ── Page builder ─────────────────────────────────────────────────────

/// Build the "Key Bindings" settings page — a dedicated page (peer to General,
/// Terminal, Appearance, About) with actions grouped by their origin (App Menu,
/// Edit Menu, Terminal Context Menu, Session Tabs Context Menu, SFTP Context
/// Menu).
pub(crate) fn page() -> SettingPage {
    let mut page = SettingPage::new("Key Bindings")
        .resettable(true)
        .icon(Icon::new(IconName::Menu));
    // Iterate actions in registry order, grouping consecutive actions that share
    // the same `group` field into one `SettingGroup` per unique group title.
    let mut current_title: Option<&str> = None;
    let mut current_group = SettingGroup::new();
    let mut group_item_count = 0;
    for a in BINDABLE_ACTIONS {
        let a: &'static BindableAction = a;
        let label = a.label;
        if current_title != Some(a.group) {
            // Flush the previous group (if any) onto the page.
            if current_title.is_some() {
                page = page.group(current_group);
                current_group = SettingGroup::new();
            }
            current_title = Some(a.group);
            current_group = current_group.title(a.group);
            group_item_count = 0;
        }
        // Insert a separator between consecutive items in the same group.
        if group_item_count > 0 {
            current_group = current_group.item(separator());
        }
        group_item_count += 1;
        current_group = current_group
            .item(
                SettingItem::render(move |_, window, cx| render_binding_row(a, window, cx))
                    .keywords([label]),
            )
            .description(show_default(a.default));
    }
    // Flush the last group.
    if current_title.is_some() {
        page = page.group(current_group);
    }
    page
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

// ── Row rendering ─────────────────────────────────────────────────────

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
    match gpui::Keystroke::parse(ks) {
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
    if gpui::Keystroke::parse(&binding).is_err() {
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
