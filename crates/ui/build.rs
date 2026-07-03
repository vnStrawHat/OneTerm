//! Build script for oneterm-ui.
//!
//! 1. Publish absolute path of `assets/icons/` so the `icon_named!` proc-macro
//!    (called from `src/icon.rs`) can scan SVG files at compile time.
//!    The env var `ONETERM_UI_ICONS_DIR` is consumed by:
//!      icon_named!(AppIcon, "$ONETERM_UI_ICONS_DIR");
//!
//! 2. Read the repo-root `VERSION` file and publish it as `ONETERM_VERSION` so the
//!    About dialog (and any other UI) can show the project version at compile time
//!    via `env!("ONETERM_VERSION")`. This keeps `VERSION` as the single source of
//!    truth shared with the Windows resource script (crates/app/build.rs) and the
//!    release workflow.

use std::{env, fs, path::Path};

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    let manifest = Path::new(&manifest_dir);

    // ── 1. Icons dir for the icon_named! proc-macro ──────────────────────────
    let icons_dir = manifest.join("assets/icons");
    if !icons_dir.is_dir() {
        panic!(
            "expected icons at {}, but the directory is missing",
            icons_dir.display(),
        );
    }
    println!(
        "cargo:rustc-env=ONETERM_UI_ICONS_DIR={}",
        icons_dir.display()
    );
    println!("cargo:rerun-if-changed=assets/icons");

    // ── 2. Project version from the repo-root VERSION file ───────────────────
    // crates/ui → crates → repo root (two levels up).
    let repo_root = manifest.ancestors().nth(2).expect("could not resolve repo root");
    let version_file = repo_root.join("VERSION");
    let version = fs::read_to_string(&version_file)
        .unwrap_or_else(|e| panic!("failed to read {}: {}", version_file.display(), e))
        .trim()
        .to_owned();
    println!("cargo:rustc-env=ONETERM_VERSION={version}");
    println!("cargo:rerun-if-changed=../../VERSION");
}
