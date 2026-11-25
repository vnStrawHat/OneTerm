//! What is chosen in a list of files (specs/oneterm, "M9 in detail"): a click chooses a row alone,
//! with Ctrl it adds the row or takes it out, with Shift it chooses from the row clicked last
//! to this one.

use quadra::{Mods, Signal, signal};

#[derive(Clone, Copy)]
pub(crate) struct Picked {
    names: Signal<Vec<String>>,
    /// The row a Shift click reaches from: the one clicked last without Shift.
    from: Signal<Option<String>>,
}

impl Picked {
    pub fn new() -> Picked {
        Picked { names: signal(Vec::new()), from: signal(None) }
    }

    /// Whether a row is chosen; what reads it follows the choice.
    pub fn has(&self, name: &str) -> bool {
        self.names.with(|all| all.iter().any(|it| it == name))
    }

    /// The names chosen, in the order they were.
    pub fn names(&self) -> Vec<String> {
        self.names.get_untracked()
    }

    pub fn set(&self, names: Vec<String>) {
        self.from.set(names.last().cloned());
        self.names.set(names);
    }

    /// A click on a row of the list that shows `order`.
    pub fn click(&self, name: &str, mods: Mods, order: &[String]) {
        let at = |name: &str| order.iter().position(|it| it == name);
        let from = self.from.get_untracked().filter(|_| mods.contains(Mods::SHIFT));
        if let Some((a, b)) = from.and_then(|from| at(&from)).zip(at(name)) {
            return self.names.set(order[a.min(b)..=a.max(b)].to_vec());
        }
        if mods.contains(Mods::CTRL) {
            self.names.update(|all| match all.iter().position(|it| it == name) {
                Some(at) => _ = all.remove(at),
                None => all.push(name.to_string()),
            });
        } else {
            self.names.set(vec![name.to_string()]);
        }
        self.from.set(Some(name.to_string()));
    }
}
