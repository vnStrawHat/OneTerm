//! Visual separators between setting items.
//!
//! The gpui-component `SettingGroup` renders its items inside a `GroupBox`
//! with only a `.gap_4()` between them — no visible divider. To make each item
//! visually distinguishable we insert thin horizontal separator lines between
//! consecutive items, built as `SettingItem::Element` (custom render) items.
//!
//! When the user searches, separators are hidden automatically: a
//! `SettingItem::Element` with no keywords only matches when the query is
//! empty (see `SettingItem::is_match`).

use gpui::{App, Styled, Window, div, px};
use gpui_component::{
    ActiveTheme as _,
    setting::{RenderOptions, SettingItem},
};

/// A thin horizontal divider line rendered between setting items.
///
/// Slots into the standard `SettingGroup::item` pipeline as a custom-element
/// `SettingItem`. Hidden during search (no keywords → only matches empty query).
pub(crate) fn separator() -> SettingItem {
    SettingItem::render(
        |_options: &RenderOptions, _window: &mut Window, cx: &mut App| {
            div().w_full().h(px(1.)).bg(cx.theme().border)
        },
    )
}

/// Interleave the given items with `separator()` between each consecutive pair.
///
/// A single item (or empty slice) is returned unchanged — no leading/trailing
/// separator is added.
pub(crate) fn items_with_separators(items: Vec<SettingItem>) -> Vec<SettingItem> {
    let mut out = Vec::with_capacity(items.len() * 2);
    for (i, item) in items.into_iter().enumerate() {
        if i > 0 {
            out.push(separator());
        }
        out.push(item);
    }
    out
}
