//! Closing what holds editors with changes not saved (specs/oneterm, "Editing in OneTerm"): a space,
//! a tab, the other tabs or the window ask first, one editor at a time.

use std::rc::Rc;

use crate::model::Model;

/// What goes on once an editor's question is answered with Save or Don't save: the closing.
pub(crate) type Then = Rc<dyn Fn()>;

impl Model {
    /// Whether an editor in the tabs `ids` has changes not saved: the first one asks, its tab
    /// shown and its space focused, and `then` runs once it is answered.
    pub(crate) fn asks(&self, ids: &[u64], then: Then) -> bool {
        let tabs = self.tabs().get_untracked();
        for tab in tabs.iter().filter(|it| ids.contains(&it.id)) {
            let holds = |it: &crate::model::Space| it.edit.as_ref().is_some_and(|e| e.unsaved());
            let Some(at) = tab.spaces.with_untracked(|all| all.iter().position(holds)) else { continue };
            // The workspace shows, where the question is; the space is focused before the
            // dialog opens, which then keeps the focus.
            self.ui().expanded.set(false);
            self.ui().settings.set(false);
            self.active().set(tab.id);
            tab.focus.set(at);
            let edit = tab.spaces.with_untracked(|all| all[at].edit.clone());
            return edit.is_some_and(|it| it.holds(then));
        }
        false
    }

    /// Whether an editor of any tab asks before the window closes.
    pub(crate) fn asks_all(&self, then: Then) -> bool {
        let ids: Vec<u64> = self.tabs().with_untracked(|tabs| tabs.iter().map(|it| it.id).collect());
        self.asks(&ids, then)
    }

    /// A closing stopped (Cancel, the dialog given up, a save failed): every Don't save of
    /// it is taken back.
    pub(crate) fn keep_edits(&self) {
        for tab in self.tabs().get_untracked() {
            for space in tab.spaces.get_untracked() {
                if let Some(edit) = &space.edit {
                    edit.keep();
                }
            }
        }
    }
}
