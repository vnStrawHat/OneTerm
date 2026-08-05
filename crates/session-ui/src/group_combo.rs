//! [`GroupComboDelegate`] + [`group_combobox`] — Combobox delegate and widget
//! for the Group field in the session dialog.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    App, InteractiveElement as _, IntoElement, ParentElement as _, SharedString, Styled, Task,
    Window, div,
};
use gpui_component::{
    ActiveTheme as _, Disableable as _, Icon, IconName, IndexPath, Sizable as _,
    button::{Button, ButtonVariants as _},
    combobox::{Combobox, ComboboxState},
    h_flex,
    searchable_list::{SearchableListDelegate, SearchableListItem, SearchableVec},
};

/// Shared mutable cell for the query text and group value.
/// Uses `Rc<RefCell<>>` so the delegate (inside ComboboxState) and the footer
/// button (outside) can both access it.
pub(crate) type SharedCell = Rc<RefCell<String>>;

/// Delegate for the Group Combobox — wraps [`SearchableVec`] + tracks the query.
pub(crate) struct GroupComboDelegate {
    inner: SearchableVec<SharedString>,
    /// Current search query (updated in `perform_search`).
    query: SharedCell,
    /// Last group value (updated in `on_confirm` or on footer click).
    group_value: SharedCell,
}

impl GroupComboDelegate {
    pub(crate) fn new(
        items: Vec<SharedString>,
        query: SharedCell,
        group_value: SharedCell,
    ) -> Self {
        Self {
            inner: SearchableVec::new(items),
            query,
            group_value,
        }
    }
}

impl SearchableListDelegate for GroupComboDelegate {
    type Item = SharedString;

    fn items_count(&self, section: usize) -> usize {
        self.inner.items_count(section)
    }

    fn item(&self, ix: IndexPath) -> Option<&SharedString> {
        self.inner.item(ix)
    }

    fn position<V>(&self, value: &V) -> Option<IndexPath>
    where
        SharedString: SearchableListItem<Value = V>,
        V: PartialEq,
    {
        self.inner.position(value)
    }

    fn perform_search(&mut self, query: &str, window: &mut Window, cx: &mut App) -> Task<()> {
        *self.query.borrow_mut() = query.to_string();
        self.inner.perform_search(query, window, cx)
    }

    fn on_confirm(&mut self, final_selection: &[(IndexPath, SharedString)]) {
        if let Some((_, item)) = final_selection.first() {
            *self.group_value.borrow_mut() = item.to_string();
        } else {
            *self.group_value.borrow_mut() = String::new();
        }
    }
}

/// Render the Group field as a searchable [`Combobox`] with:
/// - **Trigger**: shows `group_value` (or the placeholder if empty) +
///   chevron-down + an optional clear (×) button.
/// - **Footer**: a "Create '<query>'" button — on click → sets `group_value`
///   = query text (allows creating a new group).
pub(crate) fn group_combobox(
    state: &gpui::Entity<ComboboxState<GroupComboDelegate>>,
    group_value: &SharedCell,
    query_cell: &SharedCell,
    cx: &App,
) -> impl IntoElement {
    let group_value = group_value.clone();
    let query_cell = query_cell.clone();
    let muted_fg = cx.theme().muted_foreground;

    Combobox::new(state)
        .placeholder("Select or type group...")
        .search_placeholder("Search or type group name...")
        .w_full()
        .render_trigger({
            let group_value = group_value.clone();
            move |ctx, _, cx| {
                let val = group_value.borrow().clone();
                let placeholder = ctx.placeholder.cloned().unwrap_or_default();

                h_flex()
                    .w_full()
                    .items_center()
                    .gap_1()
                    .child(
                        div()
                            .w_full()
                            .overflow_hidden()
                            .truncate()
                            .when(val.is_empty(), |this| {
                                this.text_color(cx.theme().muted_foreground)
                                    .child(placeholder)
                            })
                            .when(!val.is_empty(), |this| this.child(SharedString::from(val))),
                    )
                    .when(!ctx.open, |this| {
                        // Clear (×) button — only shown when the dropdown is closed and a value exists.
                        this.when(!group_value.borrow().is_empty(), |this| {
                            let gv = group_value.clone();
                            this.child(
                                div()
                                    .id("clear-group")
                                    .on_mouse_down(gpui::MouseButton::Left, move |_, _, cx| {
                                        cx.stop_propagation();
                                        *gv.borrow_mut() = String::new();
                                    })
                                    .child(
                                        Icon::new(IconName::CircleX).xsmall().text_color(muted_fg),
                                    ),
                            )
                        })
                    })
                    .child(
                        Icon::new(IconName::ChevronDown)
                            .xsmall()
                            .text_color(cx.theme().muted_foreground),
                    )
                    .into_any_element()
            }
        })
        .footer({
            let group_value = group_value.clone();
            let query_cell = query_cell.clone();
            move |_, cx| {
                let query = query_cell.borrow().trim().to_string();
                let label = if query.is_empty() {
                    "Type to create new group".to_string()
                } else {
                    format!("Create \"{}\"", query)
                };
                let enabled = !query.is_empty();

                Button::new("create-group")
                    .ghost()
                    .label(label)
                    .icon(Icon::new(IconName::Plus))
                    .text_color(cx.theme().foreground)
                    .w_full()
                    .justify_start()
                    .when(!enabled, |this| this.disabled(true))
                    .when(enabled, |this| {
                        let gv = group_value.clone();
                        let q = query.clone();
                        this.on_click(move |_, _, _cx| {
                            *gv.borrow_mut() = q.clone();
                        })
                    })
                    .into_any_element()
            }
        })
}
