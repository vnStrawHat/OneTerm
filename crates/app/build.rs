//! Build script — nhúng app icon + copy runtime assets (conpty.dll, OpenConsole.exe).
//!
//! Trách nhiệm:
//! 1. Biên dịch `assets/oneterm.rc` → file `.res` liên kết vào exe (oneterm-debug ở dev, oneterm ở release),
//!    nhúng app icon (48px + 96px) + VS_VERSION_INFO. Chỉ Windows.
//! 2. Copy `conpty.dll` + `x64/OpenConsole.exe` ra thư mục target để chạy kèm exe.
//!
//! alacritty_terminal tự load conpty.dll (qua LoadLibraryW) nếu tìm thấy
//! trong thư mục của exe hoặc PATH. conpty.dll dùng OpenConsole.exe
//! (từ Windows Terminal project) thay cho system conhost.exe →
//! ConPTY xử lý Ctrl+C đúng cách: signal chỉ đến child process,
//! không exit shell, không exit OneTerm.
//!
//! Cấu trúc sau build:
//!   target/debug/oneterm-debug.exe   (dev bin; gated bởi feature dev-bin)
//!   target/release/oneterm.exe       (release bin; gated bởi feature release-bin)
//!   target/{debug,release}/conpty.dll
//!   target/{debug,release}/x64/OpenConsole.exe

use std::path::PathBuf;

fn main() {
    // Toàn bộ logic chỉ chạy trên Windows.
    #[cfg(target_os = "windows")]
    {
        let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
        let assets_dir = manifest_dir.join("assets");

        // ── 1. Nhúng app icon + version info qua resource script ───────────
        //
        // embed-resource tự tìm rc.exe (MSVC) hoặc windres (GNU).
        // Path trong .rc là tương đối so với vị trí file .rc (assets/).
        let rc = assets_dir.join("oneterm.rc");
        if rc.exists() {
            if let Err(e) = embed_resource::compile(&rc, embed_resource::NONE).manifest_required() {
                println!(
                    "cargo:warning=Failed to embed app icon from {}: {e}",
                    rc.display()
                );
            }
        }

        // ── 2. Copy runtime assets ra thư mục target ───────────────────────
        //
        // Target directory = OUT_DIR lên 3 cấp
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

        // Re-run build script khi assets / resource thay đổi.
        println!("cargo:rerun-if-changed=assets/oneterm.rc");
        println!("cargo:rerun-if-changed=assets/icons/terminal-48x48.ico");
        println!("cargo:rerun-if-changed=assets/icons/terminal-96x96.ico");
        println!("cargo:rerun-if-changed=assets/conpty.dll");
        println!("cargo:rerun-if-changed=assets/x64/OpenConsole.exe");
    }
}
