//! Shared scaffolding for the small "form in a dialog" pattern used by every
//! feature crate: a titled dialog with a content builder, a footer with
//! **Cancel** + one confirm button, and a submit callback that runs both when
//! the confirm button is clicked and when the keyboard `Enter` (`on_ok`) fires.
//!
//! The footer buttons use direct `on_click` handlers instead of dialog actions
//! so the click never depends on action dispatch through the focus chain.
//!
//! [`labelled_field`] renders the matching "label (+ required marker) above
//! input" row so forms across crates look the same.

use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, IntoElement, ParentElement as _, Pixels, SharedString, Styled, Window, div, px,
};
use gpui_component::{
    ActiveTheme as _, WindowExt as _,
    button::Button,
    dialog::{DialogButtonProps, DialogContent, DialogFooter},
    h_flex, v_flex,
};

/// Submit callback shared by the confirm button and keyboard `Enter`.
/// Return `true` to close the dialog, `false` to keep it open (validation
/// failed, or the dialog closes itself once a background operation completes).
pub type SubmitFn = Rc<dyn Fn(&mut Window, &mut App) -> bool>;

/// Hook run when the dialog is dismissed with **Cancel** or `Escape`.
pub type CancelFn = Rc<dyn Fn(&mut Window, &mut App)>;

/// Builds the dialog body each time the dialog renders.
pub type ContentFn = Rc<dyn Fn(DialogContent, &mut Window, &mut App) -> DialogContent>;

/// Renders the confirm element in the footer (a plain [`Button`] by default).
type ConfirmFn = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

/// Whether a form field must be filled in. Controls the required marker (`*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldRequirement {
    Required,
    Optional,
}

/// Render one form field: label (with `*` when required) above the input.
pub fn labelled_field(
    label: impl Into<SharedString>,
    requirement: FieldRequirement,
    input: impl IntoElement,
    cx: &App,
) -> impl IntoElement {
    let danger = cx.theme().danger;
    v_flex()
        .gap_1()
        .w_full()
        .child(
            h_flex()
                .gap_1()
                .text_sm()
                .child(label.into())
                .when(requirement == FieldRequirement::Required, |t| {
                    t.child(div().text_color(danger).child("*"))
                }),
        )
        .child(input)
}

/// A titled dialog with a form body and a **Cancel** + confirm footer.
///
/// ```ignore
/// FormDialog::new("Rename", content, submit)
///     .confirm_label("Rename")
///     .open(window, cx);
/// ```
pub struct FormDialog {
    title: SharedString,
    width: Pixels,
    content: ContentFn,
    submit: SubmitFn,
    confirm_label: SharedString,
    confirm: Option<ConfirmFn>,
    on_cancel: Option<CancelFn>,
    on_render: Option<Rc<dyn Fn(&mut Window, &mut App)>>,
}

impl FormDialog {
    /// Dialog width used by every OneTerm form dialog unless overridden.
    pub const DEFAULT_WIDTH: Pixels = px(440.);

    /// A dialog titled `title` whose body is built by `content` and whose
    /// confirm button / `Enter` key run `submit`.
    pub fn new(
        title: impl Into<SharedString>,
        content: impl Fn(DialogContent, &mut Window, &mut App) -> DialogContent + 'static,
        submit: impl Fn(&mut Window, &mut App) -> bool + 'static,
    ) -> Self {
        Self {
            title: title.into(),
            width: Self::DEFAULT_WIDTH,
            content: Rc::new(content),
            submit: Rc::new(submit),
            confirm_label: SharedString::from("Save"),
            confirm: None,
            on_cancel: None,
            on_render: None,
        }
    }

    /// Text of the confirm button (default `Save`).
    pub fn confirm_label(mut self, label: impl Into<SharedString>) -> Self {
        self.confirm_label = label.into();
        self
    }

    /// Replace the default confirm button with a custom element (for example a
    /// stateful "Connecting…" button). The element must call the submit
    /// callback itself; keyboard `Enter` still runs it.
    pub fn confirm_element(
        mut self,
        confirm: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.confirm = Some(Rc::new(confirm));
        self
    }

    /// Run `on_cancel` when the dialog is dismissed with **Cancel** or `Escape`
    /// (for example to cancel an in-flight connection attempt).
    pub fn on_cancel(mut self, on_cancel: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_cancel = Some(Rc::new(on_cancel));
        self
    }

    /// Run `on_render` every time the dialog is (re)built — used to defer
    /// initial focus into a field once the dialog exists.
    pub fn on_render(mut self, on_render: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_render = Some(Rc::new(on_render));
        self
    }

    /// Dialog width (default [`Self::DEFAULT_WIDTH`]).
    pub fn width(mut self, width: Pixels) -> Self {
        self.width = width;
        self
    }

    /// Open the dialog on `window`.
    pub fn open(self, window: &mut Window, cx: &mut App) {
        let dialog_spec = Rc::new(self);
        window.open_dialog(cx, move |dialog, window, cx| {
            let spec = dialog_spec.clone();
            if let Some(on_render) = &spec.on_render {
                on_render(window, cx);
            }
            let submit_for_keyboard = spec.submit.clone();
            let cancel_for_keyboard = spec.on_cancel.clone();
            let content = spec.content.clone();
            dialog
                .title(spec.title.clone())
                .w(spec.width)
                .content(move |body, window, cx| content(body, window, cx))
                .footer(spec.footer(window, cx))
                .button_props(
                    DialogButtonProps::default()
                        .on_cancel(move |_, window, cx| {
                            if let Some(on_cancel) = &cancel_for_keyboard {
                                on_cancel(window, cx);
                            }
                            true
                        })
                        .on_ok(move |_, window, cx| submit_for_keyboard(window, cx)),
                )
        });
    }

    fn footer(&self, window: &mut Window, cx: &mut App) -> DialogFooter {
        let on_cancel = self.on_cancel.clone();
        let cancel_button =
            Button::new("cancel")
                .label("Cancel")
                .outline()
                .on_click(move |_, window, cx| {
                    if let Some(on_cancel) = &on_cancel {
                        on_cancel(window, cx);
                    }
                    window.close_dialog(cx);
                });
        let confirm = match &self.confirm {
            Some(confirm) => confirm(window, cx),
            None => {
                let submit = self.submit.clone();
                Button::new("confirm")
                    .label(self.confirm_label.clone())
                    .on_click(move |_, window, cx| {
                        if submit(window, cx) {
                            window.close_dialog(cx);
                        }
                    })
                    .into_any_element()
            }
        };
        DialogFooter::new().child(cancel_button).child(confirm)
    }
}
