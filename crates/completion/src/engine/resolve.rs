//! Command-tree resolution (docs 10 §3): walk the tokens left of the cursor to
//! the active [`CommandNode`], accumulating the breadcrumb path and the options
//! inherited from ancestor nodes. Kept separate from the `suggest` orchestrator
//! and the per-context gatherers so the tree-walk rules live in one place.

use super::{Engine, Resolved};
use crate::catalog::{CommandNode, Flag};
use crate::family::ShellFamily;
use crate::parse::ParsedLine;

impl Engine {
    /// Resolve the active command node + path from the tokens left of the cursor
    /// (docs 10 §3). Returns `None` for an unknown top-level command.
    pub fn resolve(
        &self,
        p: &ParsedLine,
        family: ShellFamily,
        allow_coreutils: bool,
    ) -> Option<Resolved> {
        let categories = family.categories(allow_coreutils);
        let head = p.head.as_deref()?;
        let root = self.catalog.lookup(head, &categories, family)?;
        let mut active: CommandNode = (*root).clone();
        let mut path_names = vec![active.name.clone()];
        let mut ancestor_options: Vec<Flag> = Vec::new();
        for tok in p.prior_tokens.iter().skip(1) {
            if family.is_option_token(tok) {
                continue;
            }
            if let Some(child) = active.child(tok, family) {
                let child = child.clone();
                ancestor_options.extend(active.options.clone());
                active = child;
                path_names.push(active.name.clone());
            } else {
                break;
            }
        }
        Some(Resolved {
            active,
            path_names,
            ancestor_options,
        })
    }
}
