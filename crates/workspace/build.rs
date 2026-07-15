//! Build script for oneterm-workspace.
//!
//! Publishes the repo-root `VERSION` as `ONETERM_VERSION` so the About action
//! (`layout/workspace/actions.rs`) can read it via `env!("ONETERM_VERSION")`.

use std::{env, fs, path::Path};

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    let manifest = Path::new(&manifest_dir);

    // crates/workspace → crates → repo root (two levels up).
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
