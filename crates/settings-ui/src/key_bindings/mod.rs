//! Configurable key bindings — global state, init/apply/persist, and keystroke
//! helpers.
//!
//! A curated set of OneTerm actions is exposed for rebinding. The user's
//! overrides live in `ui_config.json` (the `key_bindings` map) and are applied
//! at startup by [`apply_key_bindings`].
//!
//! Rebinding is "press-to-rebind": clicking **Edit** on a row focuses a capture
//! element and installs a gpui keystroke interceptor; the next key becomes the
//! new binding (Escape cancels). Interceptors run before key-binding dispatch,
//! so a captured key never triggers an action bound in the settings window
//! (CORR-56). A key already held by another action in the same key context is
//! rejected with an inline message naming that action (CORR-55). This avoids a
//! free-text keystroke field (which would fire per-keystroke and round-trip
//! badly with `SettingField`).
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
//! The action registry lives in [`key_bindings_actions`], the global state and
//! init/apply/persist logic in [`state`], and the settings-page UI (page
//! builder, row rendering, event handlers) in [`key_bindings_ui`].

mod key_bindings_actions;
mod key_bindings_ui;
mod state;

pub(crate) use key_bindings_ui::page;
pub(crate) use state::{
    KeyBindingsSnapshotGlobal, KeyBindingsState, apply_key_bindings, init_state,
};
