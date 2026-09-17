//! UI for the Key Bindings settings page — the `SettingPage` builder, per-row
//! render functions, and press-to-rebind event handlers.
//!
//! See [`super`] for the global state, init/apply/persist logic, and keystroke
//! helpers.

use gpui::{
    AnyElement, App, InteractiveElement as _, IntoElement, Keystroke, ParentElement as _, Role,
    StatefulInteractiveElement as _, Styled, Window, div,
};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    kbd::Kbd,
    setting::{SettingGroup, SettingItem, SettingPage},
    v_flex,
};

use super::key_bindings_actions::{BINDABLE_ACTIONS, BindableAction};
use super::state::{
    KeyBindingsState, apply_key_bindings, conflicting_action, is_at_default, is_modifier_only,
    keystroke_to_string, save_key_bindings,
};

const KEY_CAPTURE_ROLE: Role = Role::TextInput;

// ── Page builder ─────────────────────────────────────────────────────

/// The key-binding groups, packed into pages that fit the Settings window.
///
/// Thirty-seven rows in six groups do not fit one page, and a page that does not
/// fit cannot be navigated by its own sidebar: the kit scrolls a sub-item into
/// view from the heights it has measured, and it measures only what has been laid
/// out (`docs/gui-layout.md` §"Sidebar navigation", `US-0122`). Splitting by
/// origin keeps every page short enough that every sub-item lands.
///
/// `key_binding_pages_cover_every_group_exactly_once` asserts this table against
/// `BINDABLE_ACTIONS` itself, so a group cannot be added to the registry and
/// silently left off every page.
const KEY_BINDING_PAGES: &[(&str, &[&str])] = &[
    ("Key Bindings", &["App Menu", "Edit Menu"]),
    (
        "Key Bindings: Terminal",
        &["Terminal Context Menu", "Input Channel"],
    ),
    (
        "Key Bindings: Sessions",
        &["Session Tabs Context Menu", "SFTP Context Menu"],
    ),
];

/// Build the Key Bindings settings pages — actions grouped by their origin
/// (App Menu, Edit Menu, Terminal Context Menu, Input Channel, Session Tabs
/// Context Menu, SFTP Context Menu), spread over [`KEY_BINDING_PAGES`].
pub(crate) fn pages() -> Vec<SettingPage> {
    KEY_BINDING_PAGES
        .iter()
        .map(|(title, group_titles)| {
            group_titles.iter().enumerate().fold(
                SettingPage::new(*title)
                    .resettable(true)
                    .icon(Icon::new(IconName::Menu)),
                |page, (group_ix, group_title)| {
                    // The kit shows a page's `Reset All` when any item on it is
                    // resettable and fires it by resetting every item
                    // (`setting/page.rs:111-119`), so one handler per page is
                    // enough and one is all we want: it is scoped to the groups
                    // *that* page shows. Putting it on the first item of the
                    // page's first group means each of the three pages has its
                    // own button, and none of them reaches the other two.
                    let reset_scope = (group_ix == 0).then_some(*group_titles);
                    page.group(binding_group(group_title, reset_scope))
                },
            )
        })
        .collect()
}

/// One `SettingGroup` holding every action whose registry `group` is `title`,
/// in registry order.
///
/// `reset_scope` is `Some` for the first group of a page and carries that
/// page's group titles; the group's first row then hosts the page's
/// `Reset All`, scoped to those groups.
fn binding_group(
    title: &'static str,
    reset_scope: Option<&'static [&'static str]>,
) -> SettingGroup {
    let actions: Vec<&'static BindableAction> = BINDABLE_ACTIONS
        .iter()
        .filter(|action| action.group == title)
        .collect();
    let first_row = actions.first().map(|action| action.id);

    actions
        .into_iter()
        .fold(SettingGroup::new().title(title), |group, action| {
            // The row is a custom element, so its "Default: ..." line is
            // rendered by the row itself; `SettingItem::description` only
            // applies to value items and `SettingGroup::description` would label
            // the whole group with the last action's default (CORR-35).
            let item =
                SettingItem::render(move |_, window, cx| render_binding_row(action, window, cx))
                    .keywords([action.label]);
            group.item(match reset_scope {
                Some(scope) if first_row == Some(action.id) => item.on_reset(
                    move |cx| page_bindings_are_dirty(scope, cx),
                    move |_, cx| reset_page_key_bindings(scope, cx),
                ),
                _ => item,
            })
        })
}

/// The action ids a page's `Reset All` restores: every action in the groups that
/// page shows, in registry order, and no other.
///
/// This is the decision `F-R2` found wrong. Before the page split one page
/// carried all thirty-seven rows and one `Reset All` meant what it looked like;
/// after it, a single registry-wide handler put a destructive button on a page
/// whose visible rows were all clean and silently reverted twenty-four rows the
/// user could not see from there.
fn actions_reset_by(page_groups: &[&str]) -> Vec<&'static str> {
    BINDABLE_ACTIONS
        .iter()
        .filter(|action| page_groups.contains(&action.group))
        .map(|action| action.id)
        .collect()
}

/// The "Default: \u2026" line under a row's label, or `None` when the row is at
/// its default and the line would only repeat the chip beside it (`US-0121`).
///
/// A row that differs keeps the line, because that is what makes **Reset**
/// mean something: it names the keystroke Reset would restore.
fn show_default(effective: &str, default: Option<&str>) -> Option<String> {
    if is_at_default(effective, default) {
        return None;
    }
    Some(match default {
        Some(default) => format!("Default: {default}"),
        None => "Default: (unbound)".to_owned(),
    })
}

// ── Row rendering ─────────────────────────────────────────────────────

/// Render one binding row: label + binding chip + Edit/Reset, or the capture
/// element when this action is in "press-to-rebind" mode.
fn render_binding_row(
    a: &'static BindableAction,
    _window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let (capturing, eff, handle, rejection, notice) = {
        let state = KeyBindingsState::global(cx).read(cx);
        (
            state.capturing.as_deref() == Some(a.id),
            state.effective.get(a.id).cloned().unwrap_or_default(),
            state.capture_focus.clone(),
            state.capture_rejection.clone(),
            state
                .notice
                .as_ref()
                .filter(|(id, _)| id == a.id)
                .map(|(_, message)| message.clone()),
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
            .role(KEY_CAPTURE_ROLE)
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
                v_flex()
                    .gap_0p5()
                    .child(a.label)
                    .children(show_default(&eff, a.default).map(|line| {
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(line)
                    }))
                    // Said here, not in a toast: the Settings window cannot draw
                    // a notification above its own page content (`panel.rs`),
                    // and beside the button that was pressed is where it belongs
                    // anyway.
                    .children(notice.map(|message| {
                        div()
                            .text_xs()
                            .text_color(cx.theme().warning)
                            .child(message)
                    })),
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
        s.notice = None;
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
///
/// When the restored default is a keystroke a user override already holds,
/// `apply_key_bindings` unbinds this action again by `DEC-0018`'s collision rule
/// and the row snaps straight back to "—". Without a word from the application
/// that reads as a button that does nothing, so the outcome is said out loud
/// (`US-0123` rework).
fn on_reset(id: &'static str, cx: &mut App) {
    let default = BINDABLE_ACTIONS
        .iter()
        .find(|a| a.id == id)
        .and_then(|a| a.default)
        .map(|s| s.to_string())
        .unwrap_or_default();
    KeyBindingsState::global(cx).update(cx, |s, cx| {
        s.effective.insert(id.to_string(), default);
        s.notice = None;
        end_capture(s);
        cx.notify();
    });
    save_key_bindings(cx);
    apply_key_bindings(cx);

    if let Some(winner) = displacing_action(id, cx) {
        let message = format!(
            "Still unbound: {} holds this key. Rebind either one to free it.",
            winner.label
        );
        KeyBindingsState::global(cx).update(cx, |s, cx| {
            s.notice = Some((id.to_owned(), message));
            cx.notify();
        });
    }
}

/// The action holding `id`'s keystroke, when `id` came out of the last apply
/// unbound because `DEC-0018`'s collision rule gave the key to a user override.
fn displacing_action(id: &str, cx: &App) -> Option<&'static BindableAction> {
    let state = KeyBindingsState::global(cx).read(cx);
    if !state.effective.get(id).is_none_or(String::is_empty) {
        return None;
    }
    let default = BINDABLE_ACTIONS.iter().find(|a| a.id == id)?.default?;
    conflicting_action(&state.effective, id, default)
}

/// Whether any row this page shows differs from its shipped default — which is
/// exactly when the page's `Reset All` appears.
fn page_bindings_are_dirty(page_groups: &[&str], cx: &App) -> bool {
    let state = KeyBindingsState::global(cx).read(cx);
    actions_reset_by(page_groups).into_iter().any(|id| {
        let effective = state
            .effective
            .get(id)
            .map(String::as_str)
            .unwrap_or_default();
        let default = BINDABLE_ACTIONS
            .iter()
            .find(|action| action.id == id)
            .and_then(|action| action.default);
        !is_at_default(effective, default)
    })
}

/// Restore the shipped default for every row this page shows, and nothing else.
fn reset_page_key_bindings(page_groups: &[&str], cx: &mut App) {
    let ids = actions_reset_by(page_groups);
    KeyBindingsState::global(cx).update(cx, |state, cx| {
        state.notice = None;
        for action in BINDABLE_ACTIONS {
            if !ids.contains(&action.id) {
                continue;
            }
            state.effective.insert(
                action.id.to_string(),
                action.default.unwrap_or_default().to_string(),
            );
        }
        end_capture(state);
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
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn key_binding_pages_cover_every_group_exactly_once() {
        // Real data, not a worked example: the page table is checked against
        // `BINDABLE_ACTIONS` itself. Adding a group to the registry without
        // placing it on a page fails here rather than silently hiding every
        // action in it.
        let mut placed: Vec<&str> = KEY_BINDING_PAGES
            .iter()
            .flat_map(|(_, groups)| groups.iter().copied())
            .collect();
        let before = placed.len();
        placed.sort_unstable();
        placed.dedup();
        assert_eq!(before, placed.len(), "a group is placed on two pages");

        let mut registered: Vec<&str> =
            BINDABLE_ACTIONS.iter().map(|action| action.group).collect();
        registered.sort_unstable();
        registered.dedup();

        assert_eq!(placed, registered);
    }

    #[test]
    fn each_page_resets_exactly_the_actions_it_shows() {
        let mut reset_by_some_page: Vec<&str> = Vec::new();

        for (title, groups) in KEY_BINDING_PAGES {
            let reset = actions_reset_by(groups);
            let shown: Vec<&str> = BINDABLE_ACTIONS
                .iter()
                .filter(|action| groups.contains(&action.group))
                .map(|action| action.id)
                .collect();
            assert_eq!(reset, shown, "{title} resets something it does not show");
            assert!(
                !reset.is_empty(),
                "{title} has a Reset All that resets nothing"
            );
            reset_by_some_page.extend(reset);
        }

        // The three pages partition the registry: every action is reset by
        // exactly one page, so no row is unreachable and none is reset twice.
        let mut unique = reset_by_some_page.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(
            reset_by_some_page.len(),
            unique.len(),
            "an action is reset by two pages"
        );
        assert_eq!(unique.len(), BINDABLE_ACTIONS.len());
    }

    #[test]
    fn a_pages_reset_all_does_not_reach_another_pages_bindings() {
        // The `F-R2` regression, named: Reset All on "Key Bindings" used to
        // revert Split Right, which is on "Key Bindings: Terminal", and the
        // SFTP rows, which are on "Key Bindings: Sessions".
        let first_page = actions_reset_by(KEY_BINDING_PAGES[0].1);
        assert!(first_page.contains(&"new_ssh_session"));
        assert!(first_page.contains(&"terminal_copy"));
        assert!(!first_page.contains(&"split_right"));
        assert!(!first_page.contains(&"join_input_channel_a"));
        assert!(!first_page.contains(&"sftp_open"));

        // ...and the page that does show Split Right is the one that resets it.
        let terminal_page = actions_reset_by(KEY_BINDING_PAGES[1].1);
        assert!(terminal_page.contains(&"split_right"));
        assert!(!terminal_page.contains(&"new_ssh_session"));
    }

    #[test]
    fn every_key_binding_page_holds_more_than_one_group() {
        // The kit only lists sub-items for a page with several groups
        // (`settings.rs:209`), so a one-group page would lose its sidebar
        // entries. Splitting the page must not cost the navigation it exists
        // to fix.
        for (title, groups) in KEY_BINDING_PAGES {
            assert!(groups.len() > 1, "{title} would have no sidebar sub-items");
        }
    }

    #[test]
    fn capture_target_exposes_text_input_accessibility_role() {
        assert!(matches!(KEY_CAPTURE_ROLE, Role::TextInput));
    }

    #[test]
    fn a_row_at_its_default_prints_its_binding_once() {
        // Chip and line would say the same thing, so the line goes.
        assert_eq!(show_default("ctrl-shift-t", Some("ctrl-shift-t")), None);
        // An action that ships unbound and is still unbound: nothing to say.
        assert_eq!(show_default("", None), None);
    }

    #[test]
    fn a_rebound_row_still_names_the_default_reset_would_restore() {
        assert_eq!(
            show_default("ctrl-alt-t", Some("ctrl-shift-t")),
            Some("Default: ctrl-shift-t".to_owned())
        );
        // Unbound by the user, but the action ships with a default.
        assert_eq!(
            show_default("", Some("ctrl-shift-t")),
            Some("Default: ctrl-shift-t".to_owned())
        );
        // Bound by the user, but the action ships unbound.
        assert_eq!(
            show_default("ctrl-alt-t", None),
            Some("Default: (unbound)".to_owned())
        );
    }

    #[test]
    fn the_default_line_agrees_with_what_is_persisted_as_an_override() {
        // Both the row and `overrides_from_effective` route through
        // `is_at_default`, so comparing one against the other would move
        // together under any mutation of it and could never fail. The expected
        // column is therefore written out here: this is the spec of "changed",
        // and both the row and the persistence layer are held to it.
        let action = BINDABLE_ACTIONS
            .iter()
            .find(|action| action.default == Some("ctrl-t"))
            .expect("New Terminal Tab still defaults to ctrl-t");

        // keystroke, is this a change the user made?
        let cases = [
            ("ctrl-t", false), // still the shipped default
            ("ctrl-w", true),  // rebound
            ("f5", true),      // rebound
            ("", true),        // deliberately unbound: a change, and persisted
        ];

        for (keystroke, changed) in cases {
            let effective: HashMap<String, String> = BINDABLE_ACTIONS
                .iter()
                .map(|other| {
                    let binding = if other.id == action.id {
                        keystroke
                    } else {
                        other.default.unwrap_or("")
                    };
                    (other.id.to_owned(), binding.to_owned())
                })
                .collect();
            let persisted = super::super::state::overrides_for_test(&effective);

            assert_eq!(
                show_default(keystroke, action.default).is_some(),
                changed,
                "the row's Default: line for {keystroke:?}"
            );
            assert_eq!(
                persisted.contains_key(action.id),
                changed,
                "what ui_config.json would carry for {keystroke:?}"
            );
        }
    }
}
