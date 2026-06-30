//! Publish absolute path of `assets/icons/` so the `icon_named!` proc-macro
//! (called from `src/icon.rs`) can scan SVG files at compile time.
//!
//! The env var `ONETERM_UI_ICONS_DIR` is consumed by:
//!   icon_named!(AppIcon, "$ONETERM_UI_ICONS_DIR");

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

    println!("cargo:rustc-env=ONETERM_UI_ICONS_DIR={}", icons_dir.display());
    println!("cargo:rerun-if-changed=assets/icons");
}