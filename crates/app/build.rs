//! Build script — copy conpty.dll + OpenConsole.exe ra target directory.
//!
//! alacritty_terminal tự load conpty.dll (qua LoadLibraryW) nếu tìm thấy
//! trong thư mục của exe hoặc PATH. conpty.dll dùng OpenConsole.exe
//! (từ Windows Terminal project) thay cho system conhost.exe →
//! ConPTY xử lý Ctrl+C đúng cách: signal chỉ đến child process,
//! không exit shell, không exit myTerm2.
//!
//! Cấu trúc:
//!   target/debug/myterm2.exe
//!   target/debug/conpty.dll
//!   target/debug/x64/OpenConsole.exe

use std::path::PathBuf;

fn main() {
    // Chỉ chạy trên Windows.
    #[cfg(target_os = "windows")]
    {
        let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
        let assets_dir = manifest_dir.join("assets");

        // Target directory = OUT_DIR lên 3 cấp (target/debug/build/<hash>/out → target/debug)
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

        // Re-run build script nếu assets thay đổi.
        println!("cargo:rerun-if-changed=assets/conpty.dll");
        println!("cargo:rerun-if-changed=assets/x64/OpenConsole.exe");
    }
}
