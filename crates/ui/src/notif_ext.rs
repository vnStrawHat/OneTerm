//! Alert-style variant coloring for [`Notification`], without patching the library.
//!
//! The stock `Notification` only changes its **icon** based on `NotificationType`.
//! Background, border, and text color stay the same regardless of type. In contrast,
//! [`Alert`](gpui_component::alert::Alert) tints all three via `AlertVariant`.
//!
//! This module closes that gap: it builds a [`Notification`] whose `NotificationType`
//! drives not only the icon but also the background / border / text color — replicating
//! `AlertVariant`'s color scheme.
//!
//! ## How it works (no patch needed)
//!
//! `Notification` implements [`gpui::Styled`], so `.bg()`, `.border_color()`, and
//! `.text_color()` are available as builder methods. In `Notification::render` the call
//! order is:
//!
//! ```text
//! .bg(cx.theme().tokens.popover)   // hardcoded default
//! .border_color(cx.theme().border) // hardcoded default
//! …
//! .refine_style(&self.style)       // ← OUR overrides win (applied last)
//! ```
//!
//! Because `refine_style` runs **after** the hardcoded values, anything we set via
//! `Styled` overrides the defaults. Title and message `div`s don't set their own
//! `text_color`, so they inherit the root's `.text_color()`.

use gpui::{App, Hsla, SharedString, Styled, transparent_white};
use gpui_component::{
    ActiveTheme as _, Colorize as _,
    notification::{Notification, NotificationType},
};

/// Build a [`Notification`] whose background, border, and text color are tinted by
/// `type_`, matching [`Alert`](gpui_component::alert::Alert)'s `AlertVariant` palette.
///
/// This is a drop-in replacement for the tuple form
/// `(NotificationType::Warning, "msg")` — which only sets the icon — giving you a
/// fully color-coded toast like an `Alert`.
///
/// # Example
///
/// ```ignore
/// window.push_notification(notify(NotificationType::Warning, "Network unstable.", cx), cx);
/// ```
pub fn notify(type_: NotificationType, message: impl Into<SharedString>, cx: &App) -> Notification {
    let (fg, bg, border) = variant_colors(type_, cx);
    Notification::new()
        .message(message)
        .with_type(type_) // keeps the correct icon + icon color
        .text_color(fg)
        .bg(bg)
        .border_color(border)
}

/// Same as [`notify`] but also sets a title.
pub fn notify_with_title(
    type_: NotificationType,
    message: impl Into<SharedString>,
    title: impl Into<SharedString>,
    cx: &App,
) -> Notification {
    notify(type_, message, cx).title(title)
}

/// Compute the (foreground, background, border) triple for a given [`NotificationType`],
/// using the **exact same formulas** as [`gpui_component::alert::AlertVariant`].
fn variant_colors(type_: NotificationType, cx: &App) -> (Hsla, Hsla, Hsla) {
    let color = variant_base_color(type_, cx);

    // Background: variant color mixed 4 % with transparent_white — same as AlertVariant::bg.
    let bg = color.mix_oklab(transparent_white(), 0.04);
    // Border: variant color mixed 30 % with transparent_white — same as AlertVariant::border_color.
    let border = color.mix_oklab(transparent_white(), 0.3);
    // Foreground (title + message text): the variant color itself — same as AlertVariant::fg.
    let fg = color;

    (fg, bg, border)
}

/// Resolve the theme color for a [`NotificationType`].
fn variant_base_color(type_: NotificationType, cx: &App) -> Hsla {
    match type_ {
        NotificationType::Info => cx.theme().info,
        NotificationType::Success => cx.theme().success,
        NotificationType::Warning => cx.theme().warning,
        NotificationType::Error => cx.theme().danger,
    }
}
