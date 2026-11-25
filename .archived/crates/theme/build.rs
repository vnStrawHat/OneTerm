//! Build script for oneterm-theme.
//!
//! Checks that `assets/icons/` exists and re-runs when it changes, so cargo recompiles the
//! crate (and `icon_named!` rescans the SVG files) when an icon is added or removed. No path
//! is exported: the macro resolves `assets/icons` against the crate's `CARGO_MANIFEST_DIR` at
//! expansion time, so nothing absolute is cached in a target shared between checkouts.

use std::{env, path::Path};

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    let icons_dir = Path::new(&manifest_dir).join("assets/icons");
    if !icons_dir.is_dir() {
        panic!(
            "expected icons at {}, but the directory is missing",
            icons_dir.display(),
        );
    }
    println!("cargo:rerun-if-changed=assets/icons");
}
