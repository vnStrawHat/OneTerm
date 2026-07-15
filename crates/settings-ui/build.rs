//! Build script for oneterm-settings-ui.
//!
//! Publishes the repo-root `VERSION` as `ONETERM_VERSION` so the About page
//! (`about.rs`) can show the project version via `env!("ONETERM_VERSION")`.
//! `VERSION` is the single source of truth shared with the Windows resource
//! script (crates/app/build.rs) and the release workflow.

use std::{env, fs, path::Path};

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    let manifest = Path::new(&manifest_dir);

    // crates/settings-ui → crates → repo root (two levels up).
    let repo_root = manifest
        .ancestors()
        .nth(2)
        .expect("could not resolve repo root");
    let version_file = repo_root.join("VERSION");
    let version = fs::read_to_string(&version_file)
        .unwrap_or_else(|e| panic!("failed to read {}: {}", version_file.display(), e))
        .trim()
        .to_owned();
    println!("cargo:rustc-env=ONETERM_VERSION={version}");
    println!("cargo:rerun-if-changed=../../VERSION");
}
