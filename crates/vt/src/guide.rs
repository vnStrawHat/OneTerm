//! The embedder's guide: how to build a terminal on top of this crate.
//!
//! Thirteen chapters, in reading order. The API reference beside them says what
//! an item is; these say why you would reach for it and what you owe it back.
//!
//! Each chapter is one Markdown file, included below so `cargo doc` renders it
//! and `cargo test --doc` compiles every Rust block in it. A chapter that
//! describes an API the crate no longer has fails the build.
//!
//! The module names carry a numeric prefix because rustdoc sorts a module list
//! alphabetically and that is the only way to impose a reading order.

#[doc = include_str!("../docs/guide/01-overview.md")]
pub mod ch01_overview {}

#[doc = include_str!("../docs/guide/02-embedding.md")]
pub mod ch02_embedding {}

#[doc = include_str!("../docs/guide/03-threading.md")]
pub mod ch03_threading {}

#[doc = include_str!("../docs/guide/04-events.md")]
pub mod ch04_events {}

#[doc = include_str!("../docs/guide/05-osc.md")]
pub mod ch05_osc {}

#[doc = include_str!("../docs/guide/06-input.md")]
pub mod ch06_input {}

// The search chapter's regex section is the only other part of the guide whose
// subject can be compiled away, so it is the other part whose block would have
// to be `ignore`d. Unlike the transport, `regex` is **off** by default, so
// gating the whole chapter would stub it in the common build; only the tail
// section moves, and the other arm says in its place what turning the feature
// on buys.
#[doc = include_str!("../docs/guide/07-search.md")]
#[cfg_attr(feature = "regex", doc = include_str!("../docs/guide/07-search-regex.md"))]
#[cfg_attr(
    not(feature = "regex"),
    doc = "
## Regular expressions, behind a feature

The `regex` feature -- **off** in this build -- adds one more
`search::SearchPattern` variant, taking a reference to a compiled
`regex::Regex`, and with it the `regex` crate and its three dependencies. Turn
it on and this section is the rest of the chapter: what a regular expression
matches against (one row at a time, exactly as wide as the grid), why
`search::SearchOptions` is ignored for one, and why compiling the pattern stays
yours.

`search::SearchPattern` is `#[non_exhaustive]`, so a `match` on it always needs
a wildcard arm. That is what lets the same code compile whether or not the
feature is on."
)]
pub mod ch07_search {}

#[doc = include_str!("../docs/guide/08-graphics.md")]
pub mod ch08_graphics {}

#[doc = include_str!("../docs/guide/09-resize.md")]
pub mod ch09_resize {}

#[doc = include_str!("../docs/guide/10-limits.md")]
pub mod ch10_limits {}

#[doc = include_str!("../docs/guide/11-conformance.md")]
pub mod ch11_conformance {}

#[doc = include_str!("../docs/guide/12-versioning.md")]
pub mod ch12_versioning {}

// The transport chapter is the one chapter whose subject can be compiled away,
// and its poll loop is the block an embedder copies, so it must be compiled
// rather than merely `ignore`d. Gating the module is what lets that block name
// `pty` types without breaking the doctest run of a build that has no `pty`
// module; the other arm keeps the chapter in the sidebar in that build, rather
// than leaving a hole between 12 and nothing.
#[cfg(feature = "pty")]
#[doc = include_str!("../docs/guide/13-pty.md")]
pub mod ch13_pty {}

/// # 13. The pseudo-console
///
/// This chapter documents the `pty` feature, which is **off** in this build:
/// there is no transport module to write about, and its code examples name
/// types that do not exist here.
///
/// Turn the feature on -- it is on by default -- and this chapter is the
/// transport: three ways to have one, the evented poll loop, thread lifetimes,
/// and the console host a Windows embedder has to ship for itself.
///
/// Chapters 1 to 12 are true at every feature setting.
#[cfg(not(feature = "pty"))]
pub mod ch13_pty {}
