//! Command-tree resolution (docs 10 §3): walk the tokens left of the cursor to
//! the active [`CommandNode`], accumulating the options inherited from ancestor
//! nodes. Kept separate from the `suggest` orchestrator and the per-context
//! gatherers so the tree-walk rules live in one place.

use std::rc::Rc;

use super::{Engine, Resolved};
use crate::catalog::{CommandNode, Flag};
use crate::family::ShellFamily;
use crate::parse::ParsedLine;

impl Engine {
    /// Resolve the active command node from the tokens left of the cursor
    /// (docs 10 §3). Returns `None` for an unknown top-level command.
    pub(crate) fn resolve(
        &self,
        p: &ParsedLine,
        family: ShellFamily,
        allow_coreutils: bool,
    ) -> Option<Resolved> {
        let categories = family.categories(allow_coreutils);
        let head = p.head.as_deref()?;
        let mut active: Rc<CommandNode> = self.catalog.lookup(head, &categories, family)?;
        let mut ancestor_options: Vec<Flag> = Vec::new();
        for tok in p.prior_tokens.iter().skip(1) {
            if family.is_option_token(tok) {
                continue;
            }
            let Some(child) = active.child(tok, family) else {
                break;
            };
            // Subcommands are stored inline (not `Rc`), so descending clones only
            // the subtree we walk into — never the whole root, on every keystroke.
            let child = Rc::new(child.clone());
            ancestor_options.extend(active.options.iter().cloned());
            active = child;
        }
        Some(Resolved {
            active,
            ancestor_options,
        })
    }
}
