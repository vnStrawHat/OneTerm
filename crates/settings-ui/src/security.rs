//! "Security" settings page — the master password (`IN-0050`, `US-0157`).
//!
//! One group: the status of the master password and only the actions that
//! apply to it now (`high-level-design.md` § Settings > Security). The phase
//! lives in [`VaultState`] and the dialogs in [`vault_dialogs`], both in
//! `oneterm-state`, because saved SSH passwords and the lock screen use them
//! too. The page re-renders on every `VaultState` change ([`super::panel`]).
//!
//! The elevated window shows the page read-only: the status, a note, and the
//! two actions that write nothing, Unlock and Lock Now (`DEC-0019` M4, intake
//! Q5). `VaultState` refuses the writes there as well.

use gpui::{App, ParentElement as _, prelude::FluentBuilder as _};
use gpui_component::{
    Disableable as _, Icon,
    button::{Button, ButtonVariants as _},
    setting::{RenderOptions, SettingField, SettingGroup, SettingItem, SettingPage},
};
use oneterm_state::{VaultPhase, VaultState, vault_dialogs};
use oneterm_theme::icon::AppIcon;

/// Shown on the status row of the elevated window (M4).
pub(crate) const ELEVATED_NOTE: &str = "Managed from the normal OneTerm window.";

/// Build the "Security" settings page.
pub(crate) fn page(cx: &App) -> SettingPage {
    SettingPage::new("Security")
        .icon(Icon::new(AppIcon::Lock))
        // Nothing here has a default to reset to.
        .resettable(false)
        .group(master_password_group(cx))
}

/// What the page shows for one state of the vault.
#[derive(Debug, PartialEq, Eq)]
struct Rows {
    status: String,
    primary: Option<Primary>,
    change: bool,
    lock: bool,
    reset: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Primary {
    SetUp,
    Unlock,
}

fn rows(state: &VaultState) -> Rows {
    let phase = state.phase();
    let read_only = state.is_read_only();
    let mut status = match phase {
        VaultPhase::Uninitialised => "Not set up.".to_owned(),
        VaultPhase::Locked => {
            "Set, locked. Saved passwords need it before they can be used.".to_owned()
        }
        VaultPhase::Unlocked => "Set, unlocked.".to_owned(),
        VaultPhase::Unavailable => state
            .unavailable_reason()
            .unwrap_or("Unavailable.")
            .to_owned(),
    };
    if read_only {
        status.push(' ');
        status.push_str(ELEVATED_NOTE);
    }
    let primary = match phase {
        VaultPhase::Uninitialised if !read_only => Some(Primary::SetUp),
        VaultPhase::Locked => Some(Primary::Unlock),
        _ => None,
    };
    Rows {
        status,
        primary,
        change: state.can_change(),
        lock: phase == VaultPhase::Unlocked,
        reset: state.can_reset(),
    }
}

fn master_password_group(cx: &App) -> SettingGroup {
    let Some(state) = VaultState::try_global(cx) else {
        return SettingGroup::new().title("Master Password");
    };
    let state = state.read(cx);
    let busy = state.is_busy();
    let rows = rows(state);
    let primary = rows.primary;
    let mut items = vec![
        SettingItem::new(
            "Status",
            SettingField::render(move |_: &RenderOptions, _, _| {
                gpui::div().when_some(primary, |this, primary| {
                    this.child(match primary {
                        Primary::SetUp => Button::new("security-set-up")
                            .primary()
                            .label("Set Up...")
                            .disabled(busy)
                            .on_click(|_, window, cx| {
                                vault_dialogs::open_set_up_dialog(window, cx)
                            }),
                        Primary::Unlock => Button::new("security-unlock")
                            .primary()
                            .label("Unlock...")
                            .disabled(busy)
                            .on_click(|_, window, cx| {
                                vault_dialogs::open_unlock_dialog(window, cx)
                            }),
                    })
                })
            }),
        )
        .description(rows.status),
    ];
    if rows.change {
        items.push(
            SettingItem::new(
                "Change",
                SettingField::render(move |_: &RenderOptions, _, _| {
                    Button::new("security-change")
                        .outline()
                        .label("Change...")
                        .disabled(busy)
                        .on_click(|_, window, cx| vault_dialogs::open_change_dialog(window, cx))
                }),
            )
            .description("Saved passwords stay valid; only the master password changes."),
        );
    }
    if rows.lock {
        items.push(
            SettingItem::new(
                "Lock Now",
                SettingField::render(|_: &RenderOptions, _, _| {
                    Button::new("security-lock")
                        .outline()
                        .label("Lock Now")
                        .on_click(|_, _, cx| {
                            if let Some(state) = VaultState::try_global(cx) {
                                state.update(cx, |state, cx| state.lock(cx));
                            }
                        })
                }),
            )
            .description("Forget the master password until it is typed again."),
        );
    }
    if rows.reset {
        items.push(
            SettingItem::new(
                "Reset",
                SettingField::render(move |_: &RenderOptions, _, _| {
                    Button::new("security-reset")
                        .danger()
                        .label("Reset...")
                        .disabled(busy)
                        .on_click(|_, window, cx| vault_dialogs::open_reset_dialog(window, cx))
                }),
            )
            .description("Delete the master password and every saved password. Sessions are kept."),
        );
    }
    SettingGroup::new().title("Master Password").items(items)
}

#[cfg(test)]
#[path = "security_tests.rs"]
mod security_tests;
