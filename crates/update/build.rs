//! Expose the compilation target triple to the crate as `env!("TARGET")`.
//!
//! Cargo sets `TARGET` for build scripts but not for the crate itself, and the
//! updater needs the exact triple to pick its release asset.

fn main() {
    println!(
        "cargo::rustc-env=TARGET={}",
        std::env::var("TARGET").expect("TARGET is set for build scripts")
    );
    println!("cargo::rerun-if-changed=build.rs");
}
