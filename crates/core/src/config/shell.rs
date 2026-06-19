//! Shell cục bộ: chọn kind, resolve ra `(program, args, env)`.
//!
//! Windows-first. Tham chiếu `docs/terminal-backend.md` §6.1.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::AppError;

/// Loại shell cục bộ có thể config được từ settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShellKind {
    /// `cmd.exe` (Windows). Mặc định trên Windows.
    Cmd,
    /// Windows PowerShell 5.x (`powershell.exe`).
    PowerShell,
    /// PowerShell 7+ (`pwsh.exe`) — cross-platform.
    Pwsh,
    /// Bash (Unix; hoặc Git-Bash trên Windows nếu `program` trỏ tới).
    Bash,
    /// Zsh (Unix).
    Zsh,
    /// Sh (Unix).
    Sh,
    /// Lệnh tùy chỉnh — bắt buộc set `LocalShellConfig::program`.
    Custom,
}

impl Default for ShellKind {
    #[cfg(windows)]
    fn default() -> Self {
        Self::Cmd
    }

    #[cfg(not(windows))]
    fn default() -> Self {
        // $SHELL thường là bash/zsh; fallback bash.
        Self::Bash
    }
}

/// Cấu hình spawn shell cục bộ.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalShellConfig {
    pub kind: ShellKind,
    /// Đường dẫn executable (None → tự detect theo kind + nền tảng).
    pub program: Option<PathBuf>,
    /// Tham số dòng lệnh thêm (sau args mặc định của kind).
    pub args: Vec<String>,
    /// Env override (TERM, COLORTERM, LANG…). Mặc định đã set TERM=xterm-256color.
    pub env: HashMap<String, String>,
    /// Thư mục làm việc (None → cwd hiện tại của app).
    pub cwd: Option<PathBuf>,
    /// Ép UTF-8 codepage (Windows cmd). Mặc định true.
    #[serde(default = "default_utf8")]
    pub utf8: bool,
}

fn default_utf8() -> bool {
    true
}

impl Default for LocalShellConfig {
    fn default() -> Self {
        Self {
            kind: ShellKind::default(),
            program: None,
            args: Vec::new(),
            env: HashMap::new(),
            cwd: None,
            utf8: true,
        }
    }
}

/// Kết quả resolve: chương trình + args + env để spawn.
pub struct ResolvedShell {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
}

/// Mặc định env cho mọi shell.
fn base_env() -> HashMap<String, String> {
    let mut env = HashMap::new();
    env.insert("TERM".into(), "xterm-256color".into());
    env.insert("COLORTERM".into(), "truecolor".into());
    // LANG: ưu tiên env hiện tại, fallback en_US.UTF-8.
    let lang = std::env::var("LANG").unwrap_or_else(|_| "en_US.UTF-8".into());
    env.insert("LANG".into(), lang);
    env
}

#[cfg(windows)]
fn comspec() -> PathBuf {
    std::env::var_os("COMSPEC")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows\System32\cmd.exe"))
}

/// Tìm executable trong PATH (Windows: dùng `where`, fallback PATHEXT scan).
/// Trả None nếu không thấy — caller quyết định fallback.
fn find_in_path(name: &str) -> Option<PathBuf> {
    // Ưu tiên dùng crate `which` nếu có; đây là impl thủ công không thêm dep.
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
        #[cfg(windows)]
        {
            // Thử thêm đuôi .exe nếu thiếu.
            let with_exe = dir.join(format!("{name}.exe"));
            if with_exe.is_file() {
                return Some(with_exe);
            }
        }
    }
    None
}

/// Resolve `LocalShellConfig` → `(program, args, env)` sẵn sàng spawn.
///
/// Trả lỗi nếu `Custom` mà không có `program`, hoặc không tìm thấy shell mặc định.
pub fn resolve_shell(cfg: &LocalShellConfig) -> Result<ResolvedShell, AppError> {
    let mut env = base_env();
    // Env override của user (ghi đè base).
    for (k, v) in &cfg.env {
        env.insert(k.clone(), v.clone());
    }

    let (program, mut args) = match cfg.kind {
        ShellKind::Cmd => {
            #[cfg(windows)]
            {
                let prog = cfg.program.clone().unwrap_or_else(comspec);
                let mut a = Vec::new();
                if cfg.utf8 {
                    // /K chcp 65001 >nul — giữ prompt mở, set codepage UTF-8.
                    a.push("/K".into());
                    a.push("chcp".into());
                    a.push("65001".into());
                    a.push(">nul".into());
                }
                (prog, a)
            }
            #[cfg(not(windows))]
            {
                // cmd không tồn tại ngoài Windows → fallback sh.
                let prog = cfg
                    .program
                    .clone()
                    .or_else(|| std::env::var_os("SHELL").map(PathBuf::from))
                    .unwrap_or_else(|| PathBuf::from("/bin/sh"));
                (prog, Vec::new())
            }
        }
        ShellKind::PowerShell => {
            let prog = cfg
                .program
                .clone()
                .or_else(|| find_in_path("powershell"))
                .or_else(|| find_in_path("powershell.exe"))
                .ok_or_else(|| AppError::msg("powershell.exe không tìm thấy trong PATH"))?;
            let mut a = vec!["-NoLogo".into()];
            if cfg.utf8 {
                // Ép OutputEncoding UTF-8 ngay khi khởi động.
                a.push("-Command".into());
                a.push("[Console]::OutputEncoding=[Text.UTF8Encoding]::new()".into());
            }
            (prog, a)
        }
        ShellKind::Pwsh => {
            let prog = cfg
                .program
                .clone()
                .or_else(|| find_in_path("pwsh"))
                .or_else(|| find_in_path("pwsh.exe"))
                .ok_or_else(|| AppError::msg("pwsh không tìm thấy trong PATH"))?;
            let mut a = vec!["-NoLogo".into()];
            if cfg.utf8 {
                a.push("-Command".into());
                a.push("[Console]::OutputEncoding=[Text.UTF8Encoding]::new()".into());
            }
            (prog, a)
        }
        ShellKind::Bash | ShellKind::Zsh | ShellKind::Sh => {
            let name = match cfg.kind {
                ShellKind::Bash => "bash",
                ShellKind::Zsh => "zsh",
                ShellKind::Sh => "sh",
                _ => unreachable!(),
            };
            let prog = cfg
                .program
                .clone()
                .or_else(|| std::env::var_os("SHELL").map(PathBuf::from))
                .unwrap_or_else(|| PathBuf::from(format!("/bin/{name}")));
            // Login shell cho bash/zsh để load profile.
            let a = if matches!(cfg.kind, ShellKind::Bash | ShellKind::Zsh) {
                vec!["-l".into()]
            } else {
                Vec::new()
            };
            (prog, a)
        }
        ShellKind::Custom => {
            let prog = cfg
                .program
                .clone()
                .ok_or_else(|| AppError::msg("ShellKind::Custom bắt buộc có `program`"))?;
            (prog, Vec::new())
        }
    };

    // Args thêm của user (sau args mặc định).
    args.extend(cfg.args.iter().cloned());

    Ok(ResolvedShell {
        program,
        args,
        env,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_kind_windows() {
        // Trên máy build (Windows) → Cmd.
        let cfg = LocalShellConfig::default();
        if cfg!(windows) {
            assert_eq!(cfg.kind, ShellKind::Cmd);
        }
    }

    #[test]
    fn custom_requires_program() {
        let cfg = LocalShellConfig {
            kind: ShellKind::Custom,
            program: None,
            ..Default::default()
        };
        assert!(resolve_shell(&cfg).is_err());
    }

    #[test]
    fn custom_with_program_ok() {
        let cfg = LocalShellConfig {
            kind: ShellKind::Custom,
            program: Some(PathBuf::from("/bin/myshell")),
            args: vec!["--debug".into()],
            ..Default::default()
        };
        let r = resolve_shell(&cfg).unwrap();
        assert_eq!(r.program, PathBuf::from("/bin/myshell"));
        assert_eq!(r.args, vec!["--debug"]);
        assert_eq!(r.env.get("TERM").unwrap(), "xterm-256color");
    }

    #[test]
    fn env_override_wins() {
        let mut env = HashMap::new();
        env.insert("TERM".into(), "vt100".into());
        let cfg = LocalShellConfig {
            kind: ShellKind::Custom,
            program: Some(PathBuf::from("/bin/x")),
            env,
            ..Default::default()
        };
        let r = resolve_shell(&cfg).unwrap();
        assert_eq!(r.env.get("TERM").unwrap(), "vt100");
    }

    #[cfg(windows)]
    #[test]
    fn windows_cmd_resolve_includes_chcp() {
        let cfg = LocalShellConfig {
            kind: ShellKind::Cmd,
            utf8: true,
            ..Default::default()
        };
        let r = resolve_shell(&cfg).unwrap();
        // Program là cmd.exe (COMSPEC).
        assert!(
            r.program.to_string_lossy().to_ascii_lowercase().ends_with("cmd.exe"),
            "cmd resolve → cmd.exe, got {:?}",
            r.program
        );
        // Args phải có chcp 65001 (ép UTF-8).
        assert!(r.args.iter().any(|a| a == "chcp"));
        assert!(r.args.iter().any(|a| a == "65001"));
    }

    #[cfg(windows)]
    #[test]
    fn windows_cmd_utf8_false_omits_chcp() {
        let cfg = LocalShellConfig {
            kind: ShellKind::Cmd,
            utf8: false,
            ..Default::default()
        };
        let r = resolve_shell(&cfg).unwrap();
        assert!(!r.args.iter().any(|a| a == "chcp"));
    }
}