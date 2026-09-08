//! Terminal input: the decision layer between GPUI events and
//! `TerminalSession`.
//!
//! `keys` classifies a keystroke, `mouse` runs the pointer state machine over
//! the `render::metrics::GridGeometry` hit-test contract, `edit` holds the four
//! clipboard/screen commands shared by keys, menu and panel actions, and `menu`
//! builds the right-click menu. Nothing here owns view state: the view passes
//! what it knows in and applies the returned outcome.

pub(crate) mod edit;
pub(crate) mod keys;
pub(crate) mod menu;
pub(crate) mod mouse;

pub(crate) use edit::{EditCommand, clear_screen, copy_selection, paste_clipboard, select_all};
pub(crate) use keys::{
    CompletionKey, KeyAction, KeyContext, classify_key, interrupt, map_key, send_key,
};
pub(crate) use menu::{MenuContext, MenuSplitContext, build_menu};
pub(crate) use mouse::{MouseInputs, MouseOutcome, MouseState, UrlOpen};

#[cfg(test)]
mod edit_tests;
#[cfg(test)]
mod keys_tests;
#[cfg(test)]
mod menu_tests;
#[cfg(test)]
mod mouse_tests;
