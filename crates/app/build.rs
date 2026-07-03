//! Build script — embeds the app icon and copies runtime assets (conpty.dll, OpenConsole.exe).
//!
//! Responsibilities:
//! 1. Generate a Windows resource script from the `assets/oneterm.rc` template by
//!    injecting the version (read from the repo-root `VERSION` file) and absolute
//!    icon paths, then compile it into a `.res` linked into the exe
//!    (oneterm-debug in dev, oneterm in release), embedding the app icon
//!    (48px + 96px) and VS_VERSION_INFO. Windows only.
//! 2. Copy `conpty.dll` + `x64/OpenConsole.exe` to the target directory so they ship with the exe.
//!
//! alacritty_terminal loads conpty.dll itself (via LoadLibraryW) if found in the exe's
//! directory or on PATH. conpty.dll uses OpenConsole.exe (from the Windows Terminal
//! project) instead of the system conhost.exe, so ConPTY handles Ctrl+C correctly:
//! the signal reaches only the child process and does not exit the shell or OneTerm.
//!
//! Layout after build:
//!   target/debug/oneterm-debug.exe   (dev bin; gated by the dev-bin feature)
//!   target/release/oneterm.exe       (release bin; gated by the release-bin feature)
//!   target/{debug,release}/conpty.dll
//!   target/{debug,release}/x64/OpenConsole.exe

use std::path::{Path, PathBuf};

fn main() {
    // All logic runs on Windows only.
    #[cfg(target_os = "windows")]
    {
        let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
        let assets_dir = manifest_dir.join("assets");

        // ── 1. Generate + compile the resource script (icon + version info) ──
        //
        // `assets/oneterm.rc` is a TEMPLATE with placeholders:
        //   {{ASSETS_DIR}}, {{VERSION_COMMA}}, {{VERSION_STR}}.
        // We read the repo-root VERSION file, parse it into Windows 4-part form,
        // substitute the placeholders, and write the generated .rc to OUT_DIR
        // (so the source tree stays clean). embed-resource then compiles it;
        // icon paths in the generated file are absolute so they resolve regardless
        // of where the generated .rc lives.
        let rc_template = assets_dir.join("oneterm.rc");
        if rc_template.exists() {
            let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
            let generated_rc = out_dir.join("oneterm-generated.rc");

            match generate_rc(&rc_template, &generated_rc, &assets_dir, manifest_dir.as_path()) {
                Ok(()) => {
                    if let Err(e) =
                        embed_resource::compile(&generated_rc, embed_resource::NONE).manifest_required()
                    {
                        println!(
                            "cargo:warning=Failed to embed app icon from {}: {e}",
                            generated_rc.display()
                        );
                    }
                }
                Err(e) => {
                    println!("cargo:warning=Failed to generate resource script: {e}");
                }
            }
        }

        // ── 2. Copy runtime assets to the target directory ────────────────
        //
        // Target directory = OUT_DIR up 3 levels
        // (target/debug/build/<hash>/out → target/debug).
        let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
        let target_dir = out_dir.ancestors().nth(3).unwrap().to_path_buf();

        // Copy conpty.dll → target/conpty.dll
        let conpty_src = assets_dir.join("conpty.dll");
        let conpty_dst = target_dir.join("conpty.dll");
        if conpty_src.exists() {
            if let Err(e) = std::fs::copy(&conpty_src, &conpty_dst) {
                println!("cargo:warning=Failed to copy conpty.dll: {e}");
            }
        }

        // Copy x64/OpenConsole.exe → target/x64/OpenConsole.exe
        let openconsole_src = assets_dir.join("x64").join("OpenConsole.exe");
        let openconsole_dst = target_dir.join("x64").join("OpenConsole.exe");
        if openconsole_src.exists() {
            let _ = std::fs::create_dir_all(target_dir.join("x64"));
            if let Err(e) = std::fs::copy(&openconsole_src, &openconsole_dst) {
                println!("cargo:warning=Failed to copy OpenConsole.exe: {e}");
            }
        }

        // Re-run the build script when assets / resources / VERSION change.
        println!("cargo:rerun-if-changed=assets/oneterm.rc");
        println!("cargo:rerun-if-changed=assets/icons/terminal-48x48.ico");
        println!("cargo:rerun-if-changed=assets/icons/terminal-96x96.ico");
        println!("cargo:rerun-if-changed=assets/conpty.dll");
        println!("cargo:rerun-if-changed=assets/x64/OpenConsole.exe");
        // Repo-root VERSION drives the embedded version info.
        println!("cargo:rerun-if-changed=../../VERSION");
    }
}

/// Read the repo-root `VERSION` file (semver-ish, 2-4 dot-separated ints), parse it into
/// Windows VS_VERSION_INFO 4-part form, and substitute placeholders in the `.rc`
/// template. Writes the generated script to `out`.
///
/// Example: VERSION="0.1.0"  →  comma="0,1,0,0", str="0.1.0.0".
fn generate_rc(template: &Path, out: &Path, assets_dir: &Path, manifest_dir: &Path) -> std::io::Result<()> {
    // VERSION lives at the repo root (two levels up from crates/app).
    let repo_root = manifest_dir.ancestors().nth(2).unwrap();
    let version_file = repo_root.join("VERSION");
    let raw = std::fs::read_to_string(&version_file)?;
    let raw = raw.trim();

    let (comma, dotted) = parse_version_for_rc(raw);

    let template_src = std::fs::read_to_string(template)?;
    // Use forward slashes — rc.exe accepts them on Windows too.
    let assets_dir_str = assets_dir.to_string_lossy().replace('\\', "/");
    let generated = template_src
        .replace("{{ASSETS_DIR}}", &assets_dir_str)
        .replace("{{VERSION_COMMA}}", &comma)
        .replace("{{VERSION_STR}}", &dotted);

    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(out, generated)?;
    Ok(())
}

/// Parse a version string like "0.1.0" or "0.1.0.4" into Windows 4-part form.
/// Returns (comma_separated, dot_separated), each padded to 4 components with 0.
#[cfg(target_os = "windows")]
fn parse_version_for_rc(version: &str) -> (String, String) {
    let mut parts: Vec<u32> = version
        .split('.')
        .filter_map(|p| p.trim().parse::<u32>().ok())
        .collect();
    while parts.len() < 4 {
        parts.push(0);
    }
    parts.truncate(4);
    let comma = parts.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(",");
    let dotted = parts.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(".");
    (comma, dotted)
}
