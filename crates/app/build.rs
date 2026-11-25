//! Links `assets/logo.ico` as the executable's icon (Quadra's specs/16, "An executable's icon"), and
//! tells the app which terminal engine it is built with: the version the lock file names.

fn main() {
    quadra_build::icon("assets/logo.ico");
    // The lock file of the workspace this builds in: the nearest above.
    let here = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default());
    let lock = here.ancestors().map(|it| it.join("Cargo.lock")).find(|it| it.is_file());
    let text = lock.as_ref().and_then(|it| std::fs::read_to_string(it).ok()).unwrap_or_default();
    let version = text
        .split("name = \"oneterm-vt\"")
        .nth(1)
        .and_then(|after| after.split('"').nth(1))
        .expect("Cargo.lock names the version of oneterm-vt");
    println!("cargo:rustc-env=ONETERM_VT_VERSION={version}");
    if let Some(lock) = lock {
        println!("cargo:rerun-if-changed={}", lock.display());
    }
}
