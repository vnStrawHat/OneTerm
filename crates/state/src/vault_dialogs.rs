//! The master-password dialogs (`IN-0050`, `high-level-design.md` § UI
//! Wireframe): Set up, Unlock, Change and Reset, plus [`ensure_unlocked`], the
//! "create or unlock first" entry point saved SSH passwords use (`US-0158`).
//!
//! Each dialog is a [`FormDialog`] whose body is a [`VaultForm`] view. Submit
//! hands the typed text to [`VaultState`], which runs Argon2id on the
//! background executor; the confirm button spins meanwhile and the dialog closes
//! itself when the operation succeeds.
//!
//! **Memory limit (documented, `IN-0050` Security Analysis 4):** gpui's
//! `InputState` keeps its text in a rope that cannot be zeroized. At submit
//! each field is copied out of the rope exactly once, straight into a
//! [`SecretString`] (zeroized on drop); the strength hint only counts
//! characters and copies nothing. A rejected password is cleared from its
//! input (`set_value` also clears the undo history), and the inputs are dropped
//! with the dialog.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    App, AppContext as _, Context, Entity, Focusable as _, IntoElement, ParentElement as _, Render,
    SharedString, Styled as _, Subscription, Window, div,
};
use gpui_component::{
    ActiveTheme as _, Disableable as _, IconName, WindowExt as _,
    button::{Button, ButtonVariants as _},
    input::{Input, InputEvent, InputState},
    notification::NotificationType,
    v_flex,
};
use oneterm_core::SecretString;

use crate::form_dialog::{FieldRequirement, FormDialog, labelled_field};
use crate::vault_state::{
    MIN_PASSWORD_CHARS, VaultActionError, VaultPhase, VaultState, retry_message,
};

/// Why an elevated window offers no set-up, change or reset (`DEC-0019` M4).
pub const READ_ONLY_NOTE: &str = "Managed from the normal OneTerm window.";

/// What the Reset dialog asks the user to type.
pub const RESET_CONFIRMATION: &str = "RESET";

/// Shown under a new master password in Set up and Change (intake Q8).
const IRREVERSIBLE_WARNING: &str = "OneTerm cannot recover this password. If you forget it, the only way back is Reset, which deletes every saved password. Your sessions are kept.";

type OnReady = Box<dyn FnOnce(&mut Window, &mut App)>;

/// Open the "Create a master password" dialog.
pub fn open_set_up_dialog(window: &mut Window, cx: &mut App) {
    open(DialogKind::SetUp, None, window, cx);
}

/// Open the "Unlock saved passwords" dialog.
pub fn open_unlock_dialog(window: &mut Window, cx: &mut App) {
    open(DialogKind::Unlock, None, window, cx);
}

/// Open the "Change master password" dialog.
pub fn open_change_dialog(window: &mut Window, cx: &mut App) {
    open(DialogKind::Change, None, window, cx);
}

/// Open the "Reset master password" dialog (typed confirmation).
pub fn open_reset_dialog(window: &mut Window, cx: &mut App) {
    open(DialogKind::Reset, None, window, cx);
}

/// Run `on_ready` once the vault is unlocked: at once when it already is,
/// after the Unlock dialog when it is locked, after the Set up dialog when no
/// master password exists. Cancelling the dialog runs nothing; so does an
/// unavailable vault, whose reason the caller shows. An elevated window with no
/// vault says where to set one up instead of offering a set-up it would refuse.
pub fn ensure_unlocked(
    window: &mut Window,
    cx: &mut App,
    on_ready: impl FnOnce(&mut Window, &mut App) + 'static,
) {
    let Some(state) = VaultState::try_global(cx) else {
        return;
    };
    let state = state.read(cx);
    match state.phase() {
        VaultPhase::Unlocked => on_ready(window, cx),
        VaultPhase::Locked => open(DialogKind::Unlock, Some(Box::new(on_ready)), window, cx),
        VaultPhase::Uninitialised if state.is_read_only() => window.push_notification(
            (
                NotificationType::Warning,
                format!("No master password is set up. {READ_ONLY_NOTE}"),
            ),
            cx,
        ),
        VaultPhase::Uninitialised => open(DialogKind::SetUp, Some(Box::new(on_ready)), window, cx),
        VaultPhase::Unavailable => {}
    }
}

/// The length-based strength hint under a new master password (intake Q8: no
/// composition rules), from the password's length in characters. `None` while
/// the field is empty.
pub(crate) fn strength_hint(chars: usize) -> Option<String> {
    let hint = match chars {
        0 => return None,
        n if n < MIN_PASSWORD_CHARS => {
            format!("Too short: use at least {MIN_PASSWORD_CHARS} characters.")
        }
        n if n < 12 => "Strength: fair. A longer password is stronger.".to_owned(),
        n if n < 16 => "Strength: good.".to_owned(),
        _ => "Strength: strong.".to_owned(),
    };
    Some(hint)
}

/// A new master password and its confirmation, compared exactly as typed (no
/// trimming, no normalisation).
pub(crate) fn validate_new_password(password: &str, confirmation: &str) -> Result<(), String> {
    if password.chars().count() < MIN_PASSWORD_CHARS {
        return Err(format!("Use at least {MIN_PASSWORD_CHARS} characters."));
    }
    if password != confirmation {
        return Err("The two passwords differ.".to_owned());
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DialogKind {
    SetUp,
    Unlock,
    Change,
    Reset,
}

struct Field {
    label: &'static str,
    masked: bool,
}

impl DialogKind {
    fn title(self) -> &'static str {
        match self {
            Self::SetUp => "Create a master password",
            Self::Unlock => "Unlock saved passwords",
            Self::Change => "Change master password",
            Self::Reset => "Reset master password",
        }
    }

    fn confirm_label(self) -> &'static str {
        match self {
            Self::SetUp => "Create",
            Self::Unlock => "Unlock",
            Self::Change => "Change",
            Self::Reset => "Reset",
        }
    }

    fn intro(self) -> &'static str {
        match self {
            Self::SetUp => {
                "OneTerm encrypts the passwords you save with it. Choose a long one: anyone with a copy of your OneTerm folder can try to guess it."
            }
            Self::Unlock => "Enter your master password to use your saved passwords.",
            Self::Change => "Saved passwords stay valid; only the master password changes.",
            Self::Reset => {
                "Reset deletes the master password and every saved password. Your sessions are kept. This cannot be undone."
            }
        }
    }

    fn fields(self) -> &'static [Field] {
        const MASTER: Field = Field {
            label: "Master password",
            masked: true,
        };
        match self {
            Self::SetUp => &[
                MASTER,
                Field {
                    label: "Confirm master password",
                    masked: true,
                },
            ],
            Self::Unlock => &[MASTER],
            Self::Change => &[
                Field {
                    label: "Current master password",
                    masked: true,
                },
                Field {
                    label: "New master password",
                    masked: true,
                },
                Field {
                    label: "Confirm new master password",
                    masked: true,
                },
            ],
            Self::Reset => &[Field {
                label: "Type RESET to confirm",
                masked: false,
            }],
        }
    }

    /// The field that takes a new master password (strength hint under it).
    fn new_password_field(self) -> Option<usize> {
        match self {
            Self::SetUp => Some(0),
            Self::Change => Some(1),
            Self::Unlock | Self::Reset => None,
        }
    }

    /// Whether the dialog spends a password attempt (and so backs off).
    fn counts_attempts(self) -> bool {
        matches!(self, Self::Unlock | Self::Change)
    }
}

/// The body of a master-password dialog and its in-flight state.
struct VaultForm {
    kind: DialogKind,
    inputs: Vec<Entity<InputState>>,
    error: Option<SharedString>,
    busy: bool,
    /// A countdown task is repainting the back-off line.
    ticking: bool,
    on_ready: Option<OnReady>,
    _subscriptions: Vec<Subscription>,
}

impl VaultForm {
    fn new(
        kind: DialogKind,
        on_ready: Option<OnReady>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let inputs: Vec<_> = kind
            .fields()
            .iter()
            .map(|field| cx.new(|cx| InputState::new(window, cx).masked(field.masked)))
            .collect();
        let mut subscriptions: Vec<Subscription> = inputs
            .iter()
            .map(|input| {
                // `Change`, not `observe`: an input notifies on every cursor
                // blink, which would wipe the error at once.
                cx.subscribe(input, |form: &mut Self, _, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Change) {
                        form.error = None;
                        cx.notify();
                    }
                })
            })
            .collect();
        if let Some(state) = VaultState::try_global(cx) {
            subscriptions.push(cx.observe(&state, |_, _, cx| cx.notify()));
        }
        Self {
            kind,
            inputs,
            error: None,
            busy: false,
            ticking: false,
            on_ready,
            _subscriptions: subscriptions,
        }
    }

    fn retry_after(&self, cx: &App) -> Option<Duration> {
        if !self.kind.counts_attempts() {
            return None;
        }
        VaultState::try_global(cx).and_then(|state| state.read(cx).retry_after())
    }

    fn can_submit(&self, cx: &App) -> bool {
        !self.busy && self.retry_after(cx).is_none()
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_submit(cx) {
            return;
        }
        let Some(state) = VaultState::try_global(cx) else {
            return;
        };
        // One copy per field, straight from the rope into a zeroizing string.
        let mut values: Vec<SecretString> = self
            .inputs
            .iter()
            .map(|input| SecretString::new(input.read(cx).text().to_string()))
            .collect();
        let task = match self.kind {
            DialogKind::SetUp => {
                if let Err(message) =
                    validate_new_password(values[0].expose_secret(), values[1].expose_secret())
                {
                    return self.show_error(message, cx);
                }
                let password = values.swap_remove(0);
                state.update(cx, |state, cx| state.set_up(password, cx))
            }
            DialogKind::Unlock => {
                let password = values.swap_remove(0);
                state.update(cx, |state, cx| state.unlock(password, cx))
            }
            DialogKind::Change => {
                if let Err(message) =
                    validate_new_password(values[1].expose_secret(), values[2].expose_secret())
                {
                    return self.show_error(message, cx);
                }
                let new = values.swap_remove(1);
                let old = values.swap_remove(0);
                state.update(cx, |state, cx| state.change_password(old, new, cx))
            }
            DialogKind::Reset => {
                if values[0].expose_secret().trim() != RESET_CONFIRMATION {
                    return self.show_error(format!("Type {RESET_CONFIRMATION} to confirm."), cx);
                }
                state.update(cx, |state, cx| state.reset(cx))
            }
        };
        self.busy = true;
        self.error = None;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            // A cancelled dialog is gone; `VaultState` applied the result anyway.
            _ = this.update_in(cx, |form, window, cx| form.finish(result, window, cx));
        })
        .detach();
    }

    fn finish(
        &mut self,
        result: Result<(), VaultActionError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.busy = false;
        match result {
            Ok(()) => {
                window.close_dialog(cx);
                if let Some(on_ready) = self.on_ready.take() {
                    on_ready(window, cx);
                }
            }
            Err(error) => {
                if error.is_wrong_password() {
                    // The rejected text leaves the input (and its undo history).
                    self.inputs[0].update(cx, |input, cx| input.set_value("", window, cx));
                }
                if matches!(error, VaultActionError::LockedMeanwhile) {
                    // The password was right, but a lock won: it must be typed
                    // again, not resubmitted with Enter (`US-0157` verify R2).
                    for input in &self.inputs {
                        input.update(cx, |input, cx| input.set_value("", window, cx));
                    }
                }
                // The back-off line already says when to try again.
                self.error = match error {
                    VaultActionError::RetryIn(_) => None,
                    other => Some(other.message().into()),
                };
                cx.notify();
            }
        }
    }

    fn show_error(&mut self, message: String, cx: &mut Context<Self>) {
        self.error = Some(message.into());
        cx.notify();
    }

    /// Repaint the back-off countdown until it runs out.
    fn start_countdown(&mut self, cx: &mut Context<Self>) {
        if self.ticking {
            return;
        }
        self.ticking = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(500))
                    .await;
                let waiting = this.update(cx, |form, cx| {
                    cx.notify();
                    let waiting = form.retry_after(cx).is_some();
                    form.ticking = waiting;
                    waiting
                });
                if !matches!(waiting, Ok(true)) {
                    break;
                }
            }
        })
        .detach();
    }

    fn confirm_button(&self, form: Entity<Self>, cx: &App) -> Button {
        let button = Button::new("vault-dialog-confirm")
            .label(self.kind.confirm_label())
            // The kit draws its spinner in the icon slot only, so a busy
            // button gets one (Argon2id runs for a noticeable moment).
            .when(self.busy, |button| button.icon(IconName::LoaderCircle))
            .loading(self.busy)
            .disabled(!self.can_submit(cx))
            .on_click(move |_, window, cx| {
                form.update(cx, |form, cx| form.submit(window, cx));
            });
        if self.kind == DialogKind::Reset {
            button.danger()
        } else {
            button.primary()
        }
    }
}

impl Render for VaultForm {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let wait = self.retry_after(cx);
        if wait.is_some() {
            self.start_countdown(cx);
        }
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;
        let warning = cx.theme().warning;
        let new_field = self.kind.new_password_field();
        let fields = self
            .kind
            .fields()
            .iter()
            .zip(&self.inputs)
            .enumerate()
            .map(|(index, (field, input))| {
                let hint = (new_field == Some(index))
                    .then(|| strength_hint(input.read(cx).text().len_chars()))
                    .flatten();
                let mut control = Input::new(input).disabled(self.busy);
                if field.masked {
                    control = control.mask_toggle();
                }
                v_flex()
                    .gap_1()
                    .child(labelled_field(
                        field.label,
                        FieldRequirement::Required,
                        control,
                        cx,
                    ))
                    .when_some(hint, |this, hint| {
                        this.child(div().text_xs().text_color(muted).child(hint))
                    })
            })
            .collect::<Vec<_>>();
        v_flex()
            .gap_3()
            .w_full()
            .child(div().text_sm().text_color(muted).child(self.kind.intro()))
            .children(fields)
            .when(self.kind.new_password_field().is_some(), |this| {
                this.child(
                    div()
                        .text_sm()
                        .text_color(warning)
                        .child(IRREVERSIBLE_WARNING),
                )
            })
            .when_some(self.error.clone(), |this, error| {
                this.child(div().text_sm().text_color(danger).child(error))
            })
            .when_some(wait, |this, wait| {
                this.child(
                    div()
                        .text_sm()
                        .text_color(danger)
                        .child(retry_message(wait)),
                )
            })
    }
}

fn open(kind: DialogKind, on_ready: Option<OnReady>, window: &mut Window, cx: &mut App) {
    let form = cx.new(|cx| VaultForm::new(kind, on_ready, window, cx));
    let first_input = form.read(cx).inputs[0].clone();
    let focus_pending = Rc::new(Cell::new(true));
    FormDialog::new(
        kind.title(),
        {
            let form = form.clone();
            move |content, _, _| content.child(form.clone())
        },
        {
            let form = form.clone();
            move |window, cx| {
                form.update(cx, |form, cx| form.submit(window, cx));
                // The form closes the dialog itself once the vault answers.
                false
            }
        },
    )
    .confirm_element(move |_, cx| {
        form.read(cx)
            .confirm_button(form.clone(), cx)
            .into_any_element()
    })
    .on_render(move |window, cx| {
        if focus_pending.replace(false) {
            let focus = first_input.read(cx).focus_handle(cx);
            window.defer(cx, move |window, cx| focus.focus(window, cx));
        }
    })
    .open(window, cx);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_strength_hint_follows_length_only() {
        assert_eq!(strength_hint(0), None);
        assert!(strength_hint(5).unwrap().contains("at least 8"));
        assert!(strength_hint(8).unwrap().contains("fair"));
        // No composition rules: only the length counts.
        assert!(strength_hint(12).unwrap().contains("good"));
        assert!(strength_hint(21).unwrap().contains("strong"));
    }

    #[test]
    fn a_new_password_needs_eight_characters_and_a_matching_confirmation() {
        assert!(validate_new_password("1234567", "1234567").is_err());
        assert!(validate_new_password("12345678", "12345678").is_ok());
        assert_eq!(
            validate_new_password("12345678", "12345679").unwrap_err(),
            "The two passwords differ."
        );
        // Never trimmed: a trailing space is part of the password.
        assert!(validate_new_password("12345678 ", "12345678").is_err());
    }

    #[test]
    fn only_the_dialogs_that_spend_an_attempt_back_off() {
        assert!(DialogKind::Unlock.counts_attempts());
        assert!(DialogKind::Change.counts_attempts());
        assert!(!DialogKind::SetUp.counts_attempts());
        assert!(!DialogKind::Reset.counts_attempts());
    }
}
