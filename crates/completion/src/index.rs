//! The compile-time embedded catalog index.
//!
//! `build.rs` walks `assets/**/*.json` and generates `catalog_index.rs`, which
//! defines [`CATALOG_FILES`] as `&[(name, source, category, json)]` where each
//! `json` is an `include_str!` of the file (docs/auto-completion/07 §5).

include!(concat!(env!("OUT_DIR"), "/catalog_index.rs"));
