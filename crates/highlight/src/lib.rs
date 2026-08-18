//! Semantic highlighting for plain-text terminal output.
//!
//! A pure-Rust crate (no GPUI dependency) that classifies tokens in terminal
//! output by *meaning* — error/warn/success words, paths, IPs, numbers, prompt
//! signs, command words — so the UI layer can colorize plain-text logs, `cat`
//! output, router `show`, etc. on top of the existing ANSI/SGR color layer.
//!
//! Design: [`docs/terminal-semantic-highlighting.md`](../../docs/terminal-semantic-highlighting.md).
//!
//! ## Layering
//!
//! This crate is intentionally GPUI-free. The color types ([`Hsla`], [`Rgba`])
//! are plain mirror structs with the same field layout as `gpui::Hsla` (hue
//! in degrees, 0..=360, where gpui uses 0..=1) — the `ui` crate's
//! `highlight::bridge` converts them by copying the fields and scaling the
//! hue. This keeps the "pure core, GPUI only in ui" layering intact.
//!
//! ## Core types
//!
//! - [`Class`] — closed `#[repr(u8)]` enum (~19 variants + reserved range).
//! - [`RuleSet`] — compiled keyword sets (Aho-Corasick) + structural regexes.
//! - [`ShellProfile`] — per-shell prompt regex + path/option syntax.
//! - [`scan_line`] — single-pass line scanner → `Vec<u8>` of `Class`.
//! - [`ClassStyles`] — flat `[Style; Class::COUNT]` array for O(1) theme lookup.

mod class;
mod color;
mod profile;
mod role;
mod rules;
mod scanner;
mod theme;

pub use class::Class;
pub use color::{Hsla, Rgba, parse_hex};
pub use profile::ShellProfile;
pub use role::{RowRole, RowRoles};
pub use rules::RuleSet;
pub use scanner::scan_line;
pub use theme::{ClassStyle, ClassStyles, Decoration, FontStyle};
