//! UI for the Key Bindings settings page — the `SettingPage` builder, per-row
//! render functions, and press-to-rebind event handlers.
//!
//! See [`super`] for the global state, init/apply/persist logic, and keystroke
//! helpers.

use gpui::{
    AnyElement, App, InteractiveElement as _, IntoElement, Keystroke, ParentElement as _, Styled,
    Window, div,
};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    kbd::Kbd,
    setting::{SettingGroup, SettingItem, SettingPage},
    v_flex,
};

use crate::separator;

use super::key_bindings_actions::{BINDABLE_ACTIONS, BindableAction};
use super::state::{
    KeyBindingsState, apply_key_bindings, conflicting_action, is_modifier_only,
    keystroke_to_string, save_key_bindings,
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
        // The row is a custom element, so its "Default: …" line is rendered by
        // the row itself; `SettingItem::description` only applies to value
        // items and `SettingGroup::description` would label the whole group
        // with the last action's default (CORR-35).
        current_group = current_group.item(
            SettingItem::render(move |_, window, cx| render_binding_row(a, window, cx))
                .keywords([label]),
        );
    }
    // Flush the last group.
    if current_title.is_some() {
        page = page.group(current_group);
    }
    page
}

/// Description line showing the built-in default keystroke (or "(unbound)").
fn show_default(default: Option<&str>) -> String {
    match default {
        Some(default) => format!("Default: {default}"),
        None => "Default: (unbound)".to_owned(),
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
    let (capturing, eff, handle, rejection) = {
        let state = KeyBindingsState::global(cx).read(cx);
        (
            state.capturing.as_deref() == Some(a.id),
            state.effective.get(a.id).cloned().unwrap_or_default(),
            state.capture_focus.clone(),
            state.capture_rejection.clone(),
        )
    };

    if capturing {
        // Keys are consumed by the interceptor installed in `on_edit`, which runs
        // before key-binding dispatch; the element only carries the focus.
        let prompt = match rejection {
            Some(reason) => format!("{reason} — press another key (Esc to cancel)"),
            None => "Press keys…  (Esc to cancel)".to_owned(),
        };
        div()
            .id("kbd-capture")
            .track_focus(&handle)
            .w_full()
            .h_7()
            .flex()
            .items_center()
            .px_2()
            .rounded_sm()
            .border_1()
            .border_color(cx.theme().ring)
            .text_sm()
            .child(prompt)
            .into_any_element()
    } else {
        let chip = binding_chip(&eff, cx);
        h_flex()
            .w_full()
            .justify_between()
            .items_center()
            .child(
                v_flex().gap_0p5().child(a.label).child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(show_default(a.default)),
                ),
            )
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
///
/// A keystroke interceptor consumes every key pressed while the capture element
/// is focused. Interceptors run before gpui matches key bindings, so the pressed
/// key can never fire an action registered in the settings window (CORR-56).
fn on_edit(id: &'static str, window: &mut Window, cx: &mut App) {
    let handle = KeyBindingsState::global(cx).read(cx).capture_focus.clone();
    let interceptor = cx.intercept_keystrokes(move |event, window, cx| {
        let armed = {
            let state = KeyBindingsState::global(cx).read(cx);
            state.capturing.as_deref() == Some(id) && state.capture_focus.is_focused(window)
        };
        if !armed {
            return;
        }
        on_capture_key(id, &event.keystroke, cx);
        cx.stop_propagation();
    });
    KeyBindingsState::global(cx).update(cx, |s, cx| {
        s.capturing = Some(id.to_string());
        s.capture_rejection = None;
        s.capture_interceptor = Some(interceptor);
        cx.notify();
    });
    handle.focus(window, cx);
}

/// Leave capture mode (drops the interceptor).
fn end_capture(s: &mut KeyBindingsState) {
    s.capturing = None;
    s.capture_rejection = None;
    s.capture_interceptor = None;
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
        end_capture(s);
        cx.notify();
    });
    save_key_bindings(cx);
    apply_key_bindings(cx);
}

/// A key was pressed while capturing → set it as the new binding (Escape cancels;
/// bare modifiers are ignored; unparseable combinations are ignored). A key
/// already bound to another action in the same context is rejected and the row
/// says which action holds it, so a rebind never silently shadows a binding
/// (CORR-55).
fn on_capture_key(id: &'static str, ks: &Keystroke, cx: &mut App) {
    if ks.key == "escape" {
        KeyBindingsState::global(cx).update(cx, |s, cx| {
            end_capture(s);
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
    let state = KeyBindingsState::global(cx);
    let conflict = conflicting_action(&state.read(cx).effective, id, &binding);
    if let Some(other) = conflict {
        state.update(cx, |s, cx| {
            s.capture_rejection = Some(format!("{binding} is already used by {}", other.label));
            cx.notify();
        });
        return;
    }
    state.update(cx, |s, cx| {
        s.effective.insert(id.to_string(), binding);
        end_capture(s);
        cx.notify();
    });
    save_key_bindings(cx);
    apply_key_bindings(cx);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_line_names_the_keystroke_or_unbound() {
        assert_eq!(show_default(Some("ctrl-shift-t")), "Default: ctrl-shift-t");
        assert_eq!(show_default(None), "Default: (unbound)");
    }
}
