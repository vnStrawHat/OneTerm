//! Splitting and closing spaces (specs/oneterm, "Spaces"): a tab's spaces are the leaves of a
//! pane tree, its groups their ids; the list of spaces is kept in the tree's reading order.
//! A tab arranges its spaces by itself until a split to a side or a seam dragged makes the
//! tree the user's.

use quadra_design::work::{Axis, Pane};

use crate::model::{Model, Side, Space, Tab};
use crate::shells::Launch;

/// The tree a tab arranges its spaces in: a row of equal columns; beside the rail, from three
/// on, the first on the left and the others stacked at its right.
pub(crate) fn arranged(ids: &[String], rail: bool) -> Pane {
    fn run(ids: &[String], axis: Axis) -> Pane {
        match ids {
            [] => Pane::leaf(""),
            [one] => Pane::leaf(one.as_str()),
            [first, rest @ ..] => Pane::Split {
                axis,
                ratio: 1.0 / ids.len() as f32,
                first: Box::new(Pane::leaf(first.as_str())),
                second: Box::new(run(rest, axis)),
            },
        }
    }
    match ids {
        [first, rest @ ..] if rail && rest.len() >= 2 => {
            Pane::row(Pane::leaf(first.as_str()), run(rest, Axis::Column), 0.5)
        }
        _ => run(ids, Axis::Row),
    }
}

impl Tab {
    fn ids(&self) -> Vec<String> {
        self.spaces.with_untracked(|spaces| spaces.iter().map(|it| it.id.to_string()).collect())
    }

    /// The tree by the rule, for the spaces as they are and where the tabs are.
    fn by_rule(&self) -> Pane {
        arranged(&self.ids(), self.prefs.vertical.get_untracked())
    }

    /// Builds the tree again by the rule, while the tab arranges itself.
    pub(crate) fn arrange(&self) {
        if self.auto.get_untracked() {
            self.layout.set(self.by_rule());
        }
    }

    /// After the tree changed: one that is not the rule's (a seam dragged) is the user's now.
    pub(crate) fn follow_layout(&self) {
        if self.auto.get_untracked() && self.layout.with_untracked(|tree| *tree != self.by_rule()) {
            self.auto.set(false);
        }
    }
}

impl Side {
    fn axis(self) -> Axis {
        match self {
            Side::Right | Side::Left => Axis::Row,
            Side::Down | Side::Up => Axis::Column,
        }
    }

    fn before(self) -> bool {
        matches!(self, Side::Left | Side::Up)
    }
}

impl Model {
    /// A new space after the focused one of the active tab, with the focus (Split space).
    pub fn split(&self) {
        self.split_with(Launch { cwd: self.focused_cwd(), ..self.launch(None) });
    }

    /// A new local shell in the focused space's cwd on a side of it, with the focus.
    pub fn split_to(&self, side: Side) {
        self.split_to_with(side, Launch { cwd: self.focused_cwd(), ..self.launch(None) });
    }

    /// A new space running a launch after the focused one, with the focus: the tab arranged
    /// again, or in a layout of the user's to the right of it.
    pub fn split_with(&self, launch: Launch) {
        let Some(tab) = self.active_tab() else { return };
        self.add(tab, (!tab.auto.get_untracked()).then_some(Side::Right), self.space(launch));
    }

    /// A new space running a launch on a side of the focused one of the active tab, in half
    /// of its room (a run of splits along one axis shared evenly), with the focus.
    pub fn split_to_with(&self, side: Side, launch: Launch) {
        let Some(tab) = self.active_tab() else { return };
        tab.auto.set(false);
        self.add(tab, Some(side), self.space(launch));
    }

    /// A space made elsewhere (an editor's) after the focused one of a tab, as Split space
    /// puts one, with the focus.
    pub(crate) fn open_space(&self, tab: Tab, space: Space) {
        self.add(tab, (!tab.auto.get_untracked()).then_some(Side::Right), space);
    }

    /// A new space in a tab beside its focused one: on a side of it, or by the rule.
    fn add(&self, tab: Tab, side: Option<Side>, space: Space) {
        let Some(focused) = quadra::untrack(|| tab.focused()) else { return };
        let after = side.is_none_or(|side| !side.before());
        let (new, at) = (space.id.to_string(), tab.focus.get_untracked() + usize::from(after));
        tab.spaces.update(|spaces| spaces.insert(at.min(spaces.len()), space));
        match side {
            Some(side) => tab.layout.update(|tree| {
                tree.split(&focused.id.to_string(), side.axis(), &new, side.before());
                tree.even(&new);
            }),
            None => tab.arrange(),
        }
        tab.focus.set(at);
    }

    /// Closes the focused space of the active tab, and the tab with its last space. False
    /// when no tab is left.
    pub fn close_focused(&self) -> bool {
        match self.active_tab().and_then(|tab| tab.focused()) {
            Some(space) => self.close_space(space.id),
            None => false,
        }
    }

    /// Closes a space, and its tab when it was the last. False when no tab is left.
    pub fn close_space(&self, id: u64) -> bool {
        let tabs = self.tabs().get_untracked();
        let found = tabs.iter().find_map(|tab| {
            tab.spaces.with_untracked(|spaces| spaces.iter().position(|it| it.id == id)).map(|at| (*tab, at))
        });
        let Some((tab, at)) = found else { return !tabs.is_empty() };
        // An editor with changes asks first.
        let me = self.clone();
        let then: crate::editing::Then = std::rc::Rc::new(move || _ = me.close_space(id));
        if tab.spaces.with_untracked(|all| all[at].edit.as_ref().is_some_and(|it| it.holds(then))) {
            return true;
        }
        let mut gone = None;
        tab.spaces.update(|spaces| gone = Some(spaces.remove(at)));
        // Its view goes before its scope does.
        let left = tab.spaces.with_untracked(Vec::len);
        let kept = if left == 0 {
            self.close_tab(tab.id)
        } else {
            let focus = tab.focus.get_untracked();
            let focus = if focus > at { focus - 1 } else { focus.min(left - 1) };
            let next = tab.spaces.with_untracked(|spaces| spaces[focus].id.to_string());
            // Back at one space, the tab arranges itself again.
            if left == 1 {
                tab.auto.set(true);
            }
            if tab.auto.get_untracked() {
                tab.arrange();
            } else {
                tab.layout.update(|tree| {
                    tree.remove(&id.to_string());
                    tree.even(&next);
                });
            }
            tab.focus.set(focus);
            true
        };
        if let Some(space) = gone {
            end(space);
        }
        kept
    }

    /// Moves the focus to the next space of the active tab, or the one before.
    pub fn focus_next(&self, forward: bool) {
        let Some(tab) = self.active_tab() else { return };
        let count = tab.spaces.with_untracked(Vec::len);
        let at = tab.focus.get_untracked();
        tab.focus.set(if forward { (at + 1).min(count - 1) } else { at.saturating_sub(1) });
    }
}

/// Ends a space: its program, then its scope (and its channel: no tab holds it).
pub(crate) fn end(space: Space) {
    space.session.close();
    space.scope.dispose();
}
