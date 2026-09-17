//! Configurable key bindings — global state, init/apply/persist, and keystroke
//! helpers.
//!
//! See [`super`] for the module overview and the design rationale behind the
//! snapshot-and-reapply strategy.

use std::collections::HashMap;

use gpui::{App, AppContext as _, FocusHandle, Global, KeyBinding, Keystroke, Subscription};

use oneterm_settings::UiConfig;

use super::key_bindings_actions::{BINDABLE_ACTIONS, BindableAction};

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
    /// Why the last captured keystroke was rejected (shown in the capture row
    /// until the next key press), e.g. a conflict with another action (CORR-55).
    pub(super) capture_rejection: Option<String>,
    /// A message pinned under one row: `(action id, text)`. Used when a click
    /// has an outcome the row alone does not explain — today, a **Reset** that
    /// `DEC-0018`'s collision rule immediately undoes. It lives in the page
    /// rather than in a notification because the Settings window cannot draw a
    /// notification above its own page content (see `panel.rs`).
    pub(super) notice: Option<(String, String)>,
    /// Keystroke interceptor alive while capturing. It runs before gpui's key
    /// binding dispatch, so the captured key can never trigger an action bound
    /// in the settings window (CORR-56).
    pub(super) capture_interceptor: Option<Subscription>,
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
        capture_rejection: None,
        capture_interceptor: None,
        notice: None,
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
    resolve_default_collisions(cx);

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

/// Settle `DEC-0018`'s collision rule before the keymap is built: a shipped
/// default that a surviving user override already holds loses.
///
/// This is the one place [`apply_key_bindings`] decides rather than registers.
/// It is here, and not in [`init_state`], because it must also catch a collision
/// that **Reset** reintroduces — resetting an action writes its default straight
/// into `effective` without passing through the capture UI's
/// [`conflicting_action`] check — and `apply_key_bindings` runs after every
/// rebind and every reset as well as at startup.
///
/// The displaced action is emptied rather than merely skipped when binding, so
/// the keymap and the Key Bindings page tell the same story: the row shows the
/// action as unbound and is one click from being rebound. Any future source of
/// bindings must route through this function or the guarantee stops holding.
fn resolve_default_collisions(cx: &mut App) {
    let displaced = {
        let effective = &KeyBindingsState::global(cx).read(cx).effective;
        collisions_with_overrides(effective)
    };
    if displaced.is_empty() {
        return;
    }
    for (losing_id, winning_id) in &displaced {
        log::warn!(
            "key binding: the default for `{losing_id}` is held by your own binding for \
             `{winning_id}`; `{losing_id}` is left unbound and can be rebound from \
             Settings > Key Bindings."
        );
    }
    KeyBindingsState::global(cx).update(cx, |state, cx| {
        for (losing_id, _) in displaced {
            state.effective.insert(losing_id.to_owned(), String::new());
        }
        cx.notify();
    });
}

/// Actions still sitting on their shipped default whose keystroke a *different*
/// action holds as a user override, in the same key context.
///
/// Returns `(displaced action id, overriding action id)`. Pure over
/// `(BINDABLE_ACTIONS, effective)`, which is what makes `DEC-0018`'s rule
/// testable without a window.
fn collisions_with_overrides(
    effective: &HashMap<String, String>,
) -> Vec<(&'static str, &'static str)> {
    let keystroke_of = |id: &str| {
        effective
            .get(id)
            .map(|binding| binding.as_str())
            .unwrap_or("")
    };
    BINDABLE_ACTIONS
        .iter()
        .filter_map(|action| {
            let binding = keystroke_of(action.id);
            // An unbound action holds no keystroke, so nobody can take it from
            // it. (`Keystroke::parse("")` succeeds, so this guard is load-
            // bearing: without it every unbound action would "collide" with
            // every other unbound one.)
            if binding.is_empty() || !is_at_default(binding, action.default) {
                return None;
            }
            let wanted = Keystroke::parse(binding).ok()?;
            let winner = BINDABLE_ACTIONS.iter().find(|other| {
                other.id != action.id && other.context == action.context && {
                    let theirs = keystroke_of(other.id);
                    !theirs.is_empty()
                        && !is_at_default(theirs, other.default)
                        && Keystroke::parse(theirs).is_ok_and(|stroke| stroke == wanted)
                }
            })?;
            Some((action.id, winner.id))
        })
        .collect()
}

/// Whether `effective` is still the built-in default for an action whose
/// shipped default is `default` (an unbound action with no default included).
///
/// This is the one definition of "unchanged": [`overrides_from_effective`] uses
/// it to decide what reaches `ui_config.json`, and the Key Bindings row uses it
/// to decide whether printing the default under the chip would say anything
/// (`US-0121`).
pub(super) fn is_at_default(effective: &str, default: Option<&str>) -> bool {
    effective == default.unwrap_or("")
}

/// Reduce the effective bindings to the persisted override map: only entries
/// that differ from the built-in default are kept, and an unbound action whose
/// default is bound is stored as an empty string.
fn overrides_from_effective(effective: &HashMap<String, String>) -> HashMap<String, String> {
    BINDABLE_ACTIONS
        .iter()
        .filter_map(|a| {
            let eff = effective.get(a.id).map(|s| s.as_str()).unwrap_or("");
            if is_at_default(eff, a.default) {
                None
            } else {
                Some((a.id.to_string(), eff.to_string()))
            }
        })
        .collect()
}

/// [`overrides_from_effective`], for the Key Bindings UI tests: the row's
/// `Default:` line is asserted against what persistence actually writes rather
/// than against the predicate the row already calls.
#[cfg(test)]
pub(super) fn overrides_for_test(effective: &HashMap<String, String>) -> HashMap<String, String> {
    overrides_from_effective(effective)
}

// ── Keystroke helpers ────────────────────────────────────────────────

/// The other action (same key context) already bound to `binding`, if any.
/// Keystrokes are compared after parsing, so `ctrl-shift-t` and `shift-ctrl-t`
/// count as the same key (CORR-55).
pub(super) fn conflicting_action(
    effective: &HashMap<String, String>,
    id: &str,
    binding: &str,
) -> Option<&'static BindableAction> {
    let wanted = Keystroke::parse(binding).ok()?;
    let context = BINDABLE_ACTIONS
        .iter()
        .find(|a| a.id == id)
        .and_then(|a| a.context);
    BINDABLE_ACTIONS.iter().find(|other| {
        other.id != id
            && other.context == context
            && effective
                .get(other.id)
                .and_then(|current| Keystroke::parse(current).ok())
                .is_some_and(|current| current == wanted)
    })
}

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
    fn conflicts_are_detected_within_the_same_context_only() {
        let (first, second) = {
            let mut same_context = BINDABLE_ACTIONS.iter().filter(|a| a.context.is_none());
            (
                same_context.next().expect("a context-free action"),
                same_context.next().expect("a second context-free action"),
            )
        };

        let mut effective: HashMap<String, String> = HashMap::new();
        effective.insert(second.id.to_owned(), "ctrl-shift-t".to_owned());

        // Same key, different modifier order, same (empty) context → conflict.
        let conflict = conflicting_action(&effective, first.id, "shift-ctrl-t");
        assert_eq!(conflict.map(|a| a.id), Some(second.id));
        // The action itself is never its own conflict.
        assert!(conflicting_action(&effective, second.id, "ctrl-shift-t").is_none());
        // A different key context does not conflict (only checkable once the
        // registry contains a contextual action).
        if let Some(contextual) = BINDABLE_ACTIONS.iter().find(|a| a.context.is_some()) {
            assert!(conflicting_action(&effective, contextual.id, "ctrl-shift-t").is_none());
        }
        // Unbound / different keys are free.
        assert!(conflicting_action(&effective, first.id, "ctrl-shift-y").is_none());
        assert!(conflicting_action(&effective, first.id, "").is_none());
    }

    /// `effective` as `init_state` builds it: `override.or(default)`.
    fn effective_with(overrides: &[(&str, &str)]) -> HashMap<String, String> {
        BINDABLE_ACTIONS
            .iter()
            .map(|action| {
                let binding = overrides
                    .iter()
                    .find(|(id, _)| *id == action.id)
                    .map(|(_, keystroke)| (*keystroke).to_owned())
                    .unwrap_or_else(|| action.default.unwrap_or("").to_owned());
                (action.id.to_owned(), binding)
            })
            .collect()
    }

    #[test]
    fn a_profile_with_no_entries_lands_on_the_new_defaults_and_collides_with_nothing() {
        // The common case: the table edit *is* the migration.
        let effective = effective_with(&[]);
        assert_eq!(effective["new_ssh_session"], "ctrl-shift-n");
        assert_eq!(effective["quit"], "ctrl-shift-q");
        assert_eq!(effective["about"], "f1");
        assert_eq!(effective["toggle_gutter"], "");
        assert!(collisions_with_overrides(&effective).is_empty());
        // ...and nothing was written back, because nothing differs.
        assert!(overrides_from_effective(&effective).is_empty());
    }

    #[test]
    fn an_override_that_differs_from_every_default_is_left_alone() {
        let effective = effective_with(&[("quit", "ctrl-alt-q")]);
        assert!(collisions_with_overrides(&effective).is_empty());
        assert_eq!(overrides_from_effective(&effective)["quit"], "ctrl-alt-q");
    }

    #[test]
    fn a_surviving_override_beats_a_new_default_and_unbinds_the_displaced_action() {
        // The user bound Quit to what is now New SSH Session's default.
        let effective = effective_with(&[("quit", "ctrl-shift-n")]);
        assert_eq!(
            collisions_with_overrides(&effective),
            vec![("new_ssh_session", "quit")]
        );

        // Applying the rule leaves the user's choice intact and the displaced
        // action unbound, which is what the Key Bindings page then shows.
        let mut resolved = effective.clone();
        for (losing_id, _) in collisions_with_overrides(&resolved) {
            resolved.insert(losing_id.to_owned(), String::new());
        }
        assert_eq!(resolved["quit"], "ctrl-shift-n");
        assert_eq!(resolved["new_ssh_session"], "");
        // Stable: re-applying finds nothing left to resolve.
        assert!(collisions_with_overrides(&resolved).is_empty());
    }

    #[test]
    fn modifier_order_does_not_hide_a_collision() {
        let effective = effective_with(&[("quit", "shift-ctrl-n")]);
        assert_eq!(
            collisions_with_overrides(&effective),
            vec![("new_ssh_session", "quit")]
        );
    }

    #[test]
    fn an_action_left_unbound_by_its_own_override_displaces_nobody() {
        let effective = effective_with(&[("new_ssh_session", "")]);
        assert!(collisions_with_overrides(&effective).is_empty());
    }

    #[test]
    fn two_actions_both_at_their_defaults_never_displace_each_other() {
        // Only a *user* override can win, so a table-internal clash would be a
        // bug in the table, not something this rule papers over. The table test
        // in `key_bindings_actions` is what catches that.
        let effective = effective_with(&[]);
        assert!(collisions_with_overrides(&effective).is_empty());
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
