//! Terminal auto-completion engine for OneTerm (Phase 1).
//!
//! A gpui-free, alacritty-free crate that turns the current prompt line into a
//! ranked, deduped, redaction-safe suggestion list. It owns:
//!
//! - the **data model** ([`Suggestion`], [`SuggestionKind`], [`ShellFamily`],
//!   the internal command-catalog model),
//! - **catalog loading** from the compile-time embedded index ([`build.rs`]),
//! - **line parsing** + subcommand-tree resolution,
//! - **matching + frecency ranking**,
//! - the in-session [`CompletionHistory`] store, and
//! - sensitive-value [`redact`]ion.
//!
//! It depends only on `oneterm-core` (for [`oneterm_core::ShellKind`]). See
//! `docs/auto-completion/` for the full design.

mod catalog;
mod engine;
mod family;
mod history;
mod params;
mod parse;
pub mod redact;

pub use engine::{CompletionContext, Engine, Suggestion, SuggestionKind};
pub use family::ShellFamily;
pub use history::{CompletionHistory, FrecencyRecord};
pub use params::{CompletionParams, SourceToggles};
pub use parse::ParsedLine;
