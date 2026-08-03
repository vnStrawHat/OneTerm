//! Terminal auto-completion engine for OneTerm (Phase 1).
//!
//! A gpui-free, alacritty-free crate that turns the current prompt line into a
//! ranked, deduped, redaction-safe suggestion list. It owns:
//!
//! - the **data model** ([`Suggestion`], [`SuggestionKind`], [`ShellFamily`],
//!   [`catalog::CommandNode`]),
//! - **catalog loading** from the compile-time embedded index ([`build.rs`]),
//! - **line parsing** + subcommand-tree resolution,
//! - **matching + frecency ranking**,
//! - the in-session [`CompletionHistory`] store, and
//! - sensitive-value [`redact`]ion.
//!
//! It depends only on `oneterm-core` (for [`oneterm_core::ShellKind`]). See
//! `docs/auto-completion/` for the full design.

pub mod catalog;
pub mod engine;
pub mod family;
pub mod history;
mod index;
pub mod params;
pub mod parse;
pub mod redact;

pub use catalog::{Catalog, CommandNode, Flag};
pub use engine::{CompletionContext, Engine, Resolved, Suggestion, SuggestionKind};
pub use family::{CatalogCategory, ShellFamily};
pub use history::{CommandRing, CompletionHistory, FrecencyRecord, HistoryHit};
pub use params::{CompletionParams, SourceToggles};
pub use parse::{ParsedLine, Token};
