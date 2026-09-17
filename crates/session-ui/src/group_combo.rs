//! [`GroupComboDelegate`] + [`group_combobox`] — Combobox delegate and widget
//! for the Group field in the session dialog.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    App, InteractiveElement as _, IntoElement, ParentElement as _, SharedString, Styled, Task,
    Window, div,
};
use gpui_base::actions::{Cancel, Confirm};
use gpui_component::{
    ActiveTheme as _, Disableable as _, Icon, IconName, IndexPath, Sizable as _,
    button::{Button, ButtonVariants as _},
    combobox::{Combobox, ComboboxState},
    h_flex,
    searchable_list::{SearchableListDelegate, SearchableListItem, SearchableVec},
};
use oneterm_state::form_dialog::control_label;

/// Shared mutable cell for the query text and group value.
/// Uses `Rc<RefCell<>>` so the delegate (inside ComboboxState) and the footer
/// button (outside) can both access it.
pub(crate) type SharedCell = Rc<RefCell<String>>;

/// How many rows the current query left in the dropdown. Shared so the Enter
/// handler outside the delegate can tell "nothing to select" from "the list has
/// a match and Enter belongs to it".
pub(crate) type MatchCount = Rc<Cell<usize>>;

/// What the dropdown's no-match area says.
///
/// Derived from the same live values the rest of the control reads, so the three
/// surfaces — search box, empty area, footer — cannot tell three different
/// stories (`US-0118` rework).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EmptyMessage {
    /// The store holds no groups at all, or nothing is typed yet.
    NoGroupsYet,
    /// Groups exist, but the typed query matches none of them.
    NoMatch(String),
}

impl EmptyMessage {
    pub(crate) fn text(&self) -> String {
        match self {
            Self::NoGroupsYet => "No groups yet. Type a name to create one.".to_string(),
            Self::NoMatch(query) => {
                format!("No group matches \"{query}\". Press Enter to create it.")
            }
        }
    }
}

/// Decide the no-match text from the live query and whether the store holds any
/// group at all.
///
/// "No groups yet" is true only when there are none. It used to be shown
/// whenever the filtered list came back empty, which claimed there were no
/// groups while `infra` sat one keystroke away.
pub(crate) fn empty_message(query: &str, has_any_group: bool) -> EmptyMessage {
    let query = query.trim();
    if query.is_empty() || !has_any_group {
        return EmptyMessage::NoGroupsYet;
    }
    EmptyMessage::NoMatch(query.to_string())
}

/// What pressing Enter in the group combobox should do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum GroupCommit {
    /// Create the typed group and select it.
    Create(String),
    /// Leave it to the list: it has a row to select, or there is nothing typed.
    Ignore,
}

/// Decide what Enter does, from the typed query and the number of rows the
/// search left on screen.
///
/// Enter creates only when the user typed something and the list offered
/// nothing: with a row on screen Enter belongs to the list, which selects it —
/// and creating "Lab" while "Laboratory" is highlighted would both ignore the
/// selection and add a group the user did not ask for.
pub(crate) fn group_commit(query: &str, match_count: usize) -> GroupCommit {
    let query = query.trim();
    if query.is_empty() || match_count > 0 {
        return GroupCommit::Ignore;
    }
    GroupCommit::Create(query.to_string())
}

/// Delegate for the Group Combobox — wraps [`SearchableVec`] + tracks the query.
pub(crate) struct GroupComboDelegate {
    inner: SearchableVec<SharedString>,
    /// Current search query (updated in `perform_search`).
    query: SharedCell,
    /// Rows left after the last search (updated in `perform_search`).
    match_count: MatchCount,
    /// Last group value (updated in `on_confirm` or on footer click).
    group_value: SharedCell,
}

impl GroupComboDelegate {
    pub(crate) fn new(
        items: Vec<SharedString>,
        query: SharedCell,
        match_count: MatchCount,
        group_value: SharedCell,
    ) -> Self {
        match_count.set(items.len());
        Self {
            inner: SearchableVec::new(items),
            query,
            match_count,
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
        let task = self.inner.perform_search(query, window, cx);
        self.match_count.set(self.inner.items_count(0));
        task
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
    match_count: &MatchCount,
    has_any_group: bool,
    cx: &App,
) -> impl IntoElement {
    let group_value = group_value.clone();
    let query_cell = query_cell.clone();
    let match_count = match_count.clone();
    let muted_fg = cx.theme().muted_foreground;

    // Whether the dropdown is showing, recorded by the trigger renderer: the
    // kit's `is_open` lives on the trigger context and not on the state, and
    // Enter means "open the list" while it is closed.
    let is_open = Rc::new(Cell::new(false));

    // Enter commits the typed name when the list has nothing to select (`F22`).
    // It must be the **capture** phase: the kit's `Confirm` is a no-op with no
    // row highlighted, but gpui stops an action after the first bubble-phase
    // listener, so the list would swallow it before an ancestor saw it. Capture
    // runs root-first and does not stop propagation, so the list still gets its
    // Enter whenever it does have a row to select. `Cancel` then closes the
    // dropdown: `ComboboxState::set_open` is private, and the cancel path is the
    // one that closes without touching the selection.
    div()
        .capture_action({
            let group_value = group_value.clone();
            let query_cell = query_cell.clone();
            let match_count = match_count.clone();
            let is_open = is_open.clone();
            let enter_state = state.clone();
            move |_: &Confirm, window, cx| {
                if !is_open.get() {
                    return;
                }
                let query = query_cell.borrow().clone();
                if let GroupCommit::Create(group) = group_commit(&query, match_count.get()) {
                    *group_value.borrow_mut() = group;
                    // Clear the *kit's* search input, not a private copy of the
                    // query. `set_query` writes the input and re-runs the search,
                    // so the box, `query_cell`, `match_count` and the footer stay
                    // one value. Clearing only the copy left the box showing the
                    // spent query, the empty area claiming there were no groups,
                    // the footer offering to create nothing — and the next
                    // keystroke appending to the stale text, so "Lab" then "inf"
                    // created "Labinf" (`US-0118` rework).
                    enter_state.update(cx, |combobox, cx| combobox.set_query("", window, cx));
                    window.defer(cx, |window, cx| {
                        window.dispatch_action(Box::new(Cancel), cx);
                    });
                }
            }
        })
        .child(
            Combobox::new(state)
                .placeholder("Select or type group...")
                .search_placeholder("Search or type group name...")
                .w_full()
                .render_trigger({
                    let group_value = group_value.clone();
                    let is_open = is_open.clone();
                    move |ctx, _, cx| {
                        is_open.set(ctx.is_open());
                        let val = group_value.borrow().clone();
                        let placeholder = ctx.placeholder().cloned().unwrap_or_default();

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
                                    .when(!val.is_empty(), |this| {
                                        this.child(SharedString::from(val))
                                    }),
                            )
                            .when(!ctx.is_open(), |this| {
                                // Clear (×) button — only shown when the dropdown is closed and a value exists.
                                this.when(!group_value.borrow().is_empty(), |this| {
                                    let gv = group_value.clone();
                                    this.child(
                                        div()
                                            .id("clear-group")
                                            .on_mouse_down(
                                                gpui::MouseButton::Left,
                                                move |_, _, cx| {
                                                    cx.stop_propagation();
                                                    *gv.borrow_mut() = String::new();
                                                },
                                            )
                                            .child(
                                                Icon::new(IconName::CircleX)
                                                    .xsmall()
                                                    .text_color(muted_fg),
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
                    let footer_state = state.clone();
                    let query_cell = query_cell.clone();
                    move |_, cx| {
                        // `query_cell`, not `state.read(cx)`: this closure runs
                        // inside the combobox's own render, and reading the
                        // entity there panics ("already being updated"). The
                        // cell is written by `perform_search`, and every path
                        // that changes the query — typing, and the `set_query`
                        // below — goes through it, so it is the live value.
                        let query = query_cell.borrow().trim().to_string();
                        let label = if query.is_empty() {
                            "Type to create new group".to_string()
                        } else {
                            format!("Create \"{}\"", query)
                        };
                        let enabled = !query.is_empty();

                        Button::new("create-group")
                            .ghost()
                            // `control_label`, not `.label(...)`: the kit clips a
                            // button label's descenders, and this one carries
                            // arbitrary typed text (`BUG-0069` rework).
                            .accessibility_label(label.clone())
                            .child(control_label(label))
                            .icon(Icon::new(IconName::Plus))
                            .text_color(cx.theme().foreground)
                            .w_full()
                            .justify_start()
                            .when(!enabled, |this| this.disabled(true))
                            .when(enabled, |this| {
                                let gv = group_value.clone();
                                let q = query.clone();
                                let click_state = footer_state.clone();
                                this.on_click(move |_, window, cx| {
                                    *gv.borrow_mut() = q.clone();
                                    // Same single source as the Enter path.
                                    click_state.update(cx, |combobox, cx| {
                                        combobox.set_query("", window, cx)
                                    });
                                    // Close the dropdown: it used to stay open still
                                    // offering to create the group it had just created.
                                    window.dispatch_action(Box::new(Cancel), cx);
                                })
                            })
                            .into_any_element()
                    }
                })
                // The kit's default empty state is a bare inbox icon (`F22`).
                .empty({
                    let query_cell = query_cell.clone();
                    move |_, cx| {
                        // Same reason as the footer: no entity read in render.
                        let query = query_cell.borrow().clone();
                        div()
                            .w_full()
                            .py_4()
                            .px_3()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(empty_message(&query, has_any_group).text())
                    }
                }),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `F22`: Enter creates the typed group when the list has nothing to
    /// select, and stays out of the way when it has.
    #[test]
    fn enter_creates_only_a_typed_name_the_list_cannot_offer() {
        assert_eq!(group_commit("Lab", 0), GroupCommit::Create("Lab".into()));
        assert_eq!(
            group_commit("  Lab  ", 0),
            GroupCommit::Create("Lab".into())
        );
        // A row is on screen: Enter belongs to the list, which selects it.
        assert_eq!(group_commit("Lab", 1), GroupCommit::Ignore);
        // Nothing typed: Enter has nothing to create.
        assert_eq!(group_commit("", 0), GroupCommit::Ignore);
        assert_eq!(group_commit("   ", 0), GroupCommit::Ignore);
    }

    /// `US-0118` rework: the empty area stops claiming there are no groups when
    /// the list is empty only because of the typed filter.
    #[test]
    fn the_empty_area_tells_no_groups_from_no_match() {
        assert_eq!(
            empty_message("Lab", true),
            EmptyMessage::NoMatch("Lab".into())
        );
        assert_eq!(
            empty_message("  Lab  ", true).text(),
            empty_message("Lab", true).text()
        );
        // Nothing typed: the list is empty because the store is.
        assert_eq!(empty_message("", true), EmptyMessage::NoGroupsYet);
        // No groups at all: "no match" would be pedantic and unhelpful.
        assert_eq!(empty_message("Lab", false), EmptyMessage::NoGroupsYet);
        assert!(
            empty_message("Lab", true)
                .text()
                .contains("Press Enter to create it")
        );
        assert!(empty_message("", false).text().contains("No groups yet"));
    }

    /// The point of the rework: once the query is cleared at its source, the
    /// next Enter cannot build a second group out of leftover text.
    #[test]
    fn a_spent_query_cannot_create_again() {
        // What the control looks like right after `set_query("")`: the input is
        // empty and the search has been re-run over every group.
        assert_eq!(group_commit("", 1), GroupCommit::Ignore);
        assert_eq!(group_commit("", 0), GroupCommit::Ignore);
        // ...so the next thing the user types stands alone.
        assert_eq!(group_commit("inf", 0), GroupCommit::Create("inf".into()));
    }
}
