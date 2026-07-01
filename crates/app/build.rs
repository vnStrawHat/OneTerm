//! Build script — embeds the app icon and copies runtime assets (conpty.dll, OpenConsole.exe).
//!
//! Responsibilities:
//! 1. Compile `assets/oneterm.rc` into a `.res` file linked into the exe
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

use std::path::PathBuf;

fn main() {
    // All logic runs on Windows only.
    #[cfg(target_os = "windows")]
    {
        let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
        let assets_dir = manifest_dir.join("assets");

        // ── 1. Embed app icon + version info via the resource script ───────
        //
        // embed-resource finds rc.exe (MSVC) or windres (GNU) automatically.
        // Paths in the .rc file are relative to the .rc file's location (assets/).
        let rc = assets_dir.join("oneterm.rc");
        if rc.exists() {
            if let Err(e) = embed_resource::compile(&rc, embed_resource::NONE).manifest_required() {
                println!(
                    "cargo:warning=Failed to embed app icon from {}: {e}",
                    rc.display()
                );
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

        // Re-run the build script when assets / resources change.
        println!("cargo:rerun-if-changed=assets/oneterm.rc");
        println!("cargo:rerun-if-changed=assets/icons/terminal-48x48.ico");
        println!("cargo:rerun-if-changed=assets/icons/terminal-96x96.ico");
        println!("cargo:rerun-if-changed=assets/conpty.dll");
        println!("cargo:rerun-if-changed=assets/x64/OpenConsole.exe");
    }
}
