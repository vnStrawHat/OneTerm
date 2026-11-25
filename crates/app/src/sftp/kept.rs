//! Each space's dock (specs/oneterm, "M9 in detail"): the folder, its entries, the entry chosen and
//! the transfers stay with the space they were made in, and come back when it shows again.

use std::collections::HashMap;
use std::rc::Rc;

use oneterm_ssh::Entry;

use super::{Dock, Transfer};
use crate::model::Model;

/// What a space's dock showed when another space took its place.
#[derive(Default)]
struct Left {
    dir: String,
    entries: Vec<Entry>,
    said: String,
    selected: Option<String>,
    transfers: Vec<Rc<Transfer>>,
}

/// The space the dock shows, and what the others left.
#[derive(Default)]
pub(crate) struct Kept {
    pub(super) at: Option<u64>,
    left: HashMap<u64, Left>,
}

impl Dock {
    /// Shows the dock of a space: as it was left, or its home folder the first time. The same
    /// space again lists its folder again.
    pub fn show_space(&self, model: &Model, space: Option<u64>) {
        let was = self.kept.with_untracked(|it| it.at);
        if was == space {
            return self.refresh(model);
        }
        let mine = Left {
            dir: self.dir.get_untracked(),
            entries: self.entries.get_untracked(),
            said: self.said.get_untracked(),
            selected: self.selected.get_untracked(),
            transfers: self.transfers.get_untracked(),
        };
        let live: Vec<u64> = model
            .tabs()
            .get_untracked()
            .iter()
            .flat_map(|tab| tab.spaces.with_untracked(|all| all.iter().map(|it| it.id).collect::<Vec<_>>()))
            .collect();
        let mut found = None;
        // Nothing reads the map but this, so it is written without telling readers.
        self.kept.update_if(|kept| {
            if let Some(was) = was {
                kept.left.insert(was, mine);
            }
            kept.left.retain(|id, _| live.contains(id));
            found = space.and_then(|id| kept.left.remove(&id));
            kept.at = space;
            false
        });
        match found {
            Some(left) => self.put(left),
            None => {
                self.put(Left::default());
                self.load(model, None);
            }
        }
    }

    fn put(&self, left: Left) {
        self.dir.set(left.dir);
        self.entries.set(left.entries);
        self.said.set(left.said);
        self.selected.set(left.selected);
        self.transfers.set_always(left.transfers);
    }

    /// The space the dock shows, so an answer for another one is dropped.
    pub(super) fn at(&self) -> Option<u64> {
        self.kept.with_untracked(|it| it.at)
    }
}
