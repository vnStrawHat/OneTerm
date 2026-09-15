//! The embedder's guide: how to build a terminal on top of this crate.
//!
//! Thirteen chapters, in reading order. The API reference beside them says what
//! an item is; these say why you would reach for it and what you owe it back.
//!
//! Each chapter is one Markdown file under `docs/guide/` in this crate, included
//! here so `cargo doc` renders it and `cargo test --doc` compiles every Rust
//! block in it. A chapter that describes an API it no longer has fails the
//! build.
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

#[doc = include_str!("../docs/guide/07-search.md")]
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

#[doc = include_str!("../docs/guide/13-pty.md")]
pub mod ch13_pty {}
