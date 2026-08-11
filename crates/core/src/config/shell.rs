//! Local shell: pick a kind and resolve it to `(program, args, env)`.
//!
//! Windows-first. See `docs/terminal-backend.md` §6.1.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::AppError;

use super::env::base_env;

/// Kinds of local shell configurable from settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShellKind {
    /// `cmd.exe` (Windows). Default on Windows.
    Cmd,
    /// Windows PowerShell 5.x (`powershell.exe`).
    PowerShell,
    /// PowerShell 7+ (`pwsh.exe`) — cross-platform.
    Pwsh,
    /// Bash (Unix; or Git-Bash on Windows if `program` points to it).
    Bash,
    /// Zsh (Unix).
    Zsh,
    /// Sh (Unix).
    Sh,
    /// Custom command — requires setting `LocalShellConfig::program`.
    Custom,
}

impl Default for ShellKind {
    #[cfg(windows)]
    fn default() -> Self {
        Self::Cmd
    }

    #[cfg(not(windows))]
    fn default() -> Self {
        // $SHELL is usually bash/zsh; fall back to bash.
        Self::Bash
    }
}

/// Configuration for spawning a local shell.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalShellConfig {
    pub kind: ShellKind,
    /// Executable path (None → auto-detect from kind + platform).
    pub program: Option<PathBuf>,
    /// Extra command-line arguments (appended after the kind's default args).
    pub args: Vec<String>,
    /// Env overrides (TERM, COLORTERM, LANG…). TERM=xterm-256color is set by default.
    pub env: HashMap<String, String>,
    /// Working directory (None → the user's home directory).
    pub cwd: Option<PathBuf>,
    /// Force the UTF-8 codepage (Windows cmd). Default true.
    #[serde(default = "default_utf8")]
    pub utf8: bool,
}

fn default_utf8() -> bool {
    true
}

/// Return the user's home directory.
///
/// On Windows we prefer `USERPROFILE`; on Unix we use `HOME`. Returns `None`
/// if neither env var is set (the caller falls back to the process cwd).
pub fn home_dir() -> Option<PathBuf> {
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .ok()
        .map(PathBuf::from)
}

/// Root directory holding all JSON settings files.
///
/// - **Debug**   → `target/`        (handy for dev — files live inside the repo,
///   easy to git-ignore, easy to wipe/rebuild).
/// - **Release** → `~/.OneTerm/`    (standard app location — independent of cwd).
///   The directory is auto-created (`create_dir_all`) if it does not yet exist,
///   so the first run after install does not fail to write.
///
/// All config files (`terminal.json`, `ssh_session.json`, `ui_config.json`,
/// `docks.json`) use `config_dir().join("<file>.json")` instead of hardcoding
/// the path, centralizing path logic in one place.
pub fn config_dir() -> PathBuf {
    #[cfg(debug_assertions)]
    {
        PathBuf::from("target")
    }
    #[cfg(not(debug_assertions))]
    {
        let home = home_dir().unwrap_or_else(|| PathBuf::from("."));
        let dir = home.join(".OneTerm");
        // Create the directory if missing (first release run).
        if let Err(e) = std::fs::create_dir_all(&dir) {
            log::error!("Failed to create config dir {:?}: {e}", dir);
        }
        dir
    }
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

/// Result of resolution: program + args + env to spawn.
pub struct ResolvedShell {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
}

const CMD_OSC7_PROMPT: &str = "$E]7;$P$E\\$E]133;A$E\\$P$G$E]133;B$E\\";
const POWERSHELL_OSC7_PROMPT_INIT: &str = r#"$global:__OneTermOriginalPrompt=$function:prompt;function global:prompt{$e=[char]27;[Console]::Write($e+']7;'+$pwd.Path+$e+'\');& $global:__OneTermOriginalPrompt}"#;

fn powershell_init(utf8: bool) -> String {
    if utf8 {
        format!(
            "[Console]::OutputEncoding=[Text.UTF8Encoding]::new();{POWERSHELL_OSC7_PROMPT_INIT}"
        )
    } else {
        POWERSHELL_OSC7_PROMPT_INIT.to_string()
    }
}

#[cfg(windows)]
fn comspec() -> PathBuf {
    std::env::var_os("COMSPEC")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows\System32\cmd.exe"))
}

/// Find an executable in PATH (Windows: uses `where`, falls back to a PATHEXT scan).
/// Returns None if not found — the caller decides the fallback.
fn find_in_path(name: &str) -> Option<PathBuf> {
    // Prefer the `which` crate if available; this is a manual impl that adds no dep.
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
        #[cfg(windows)]
        {
            // Try adding the .exe suffix if missing.
            let with_exe = dir.join(format!("{name}.exe"));
            if with_exe.is_file() {
                return Some(with_exe);
            }
        }
    }
    None
}

/// Resolve `LocalShellConfig` → `(program, args, env)` ready to spawn.
///
/// Returns an error if `Custom` has no `program`, or the default shell cannot be found.
pub fn resolve_shell(cfg: &LocalShellConfig) -> Result<ResolvedShell, AppError> {
    let mut env = base_env();
    // User env overrides (override the base).
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
                    // /K chcp 65001 >nul — keep the prompt open, set the UTF-8 codepage.
                    a.push("/K".into());
                    a.push("chcp".into());
                    a.push("65001".into());
                    a.push(">nul".into());
                }
                (prog, a)
            }
            #[cfg(not(windows))]
            {
                // cmd does not exist outside Windows → fall back to sh.
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
                .ok_or_else(|| AppError::msg("powershell.exe not found in PATH"))?;
            // -NoExit keeps the session interactive after the integration command runs.
            // The prompt wrapper emits OSC 7 before delegating to the original prompt.
            let a = vec![
                "-NoLogo".into(),
                "-NoExit".into(),
                "-Command".into(),
                powershell_init(cfg.utf8),
            ];
            (prog, a)
        }
        ShellKind::Pwsh => {
            let prog = cfg
                .program
                .clone()
                .or_else(|| find_in_path("pwsh"))
                .or_else(|| find_in_path("pwsh.exe"))
                .ok_or_else(|| AppError::msg("pwsh not found in PATH"))?;
            let a = vec![
                "-NoLogo".into(),
                "-NoExit".into(),
                "-Command".into(),
                powershell_init(cfg.utf8),
            ];
            (prog, a)
        }
        ShellKind::Bash | ShellKind::Zsh | ShellKind::Sh => {
            let name = match cfg.kind {
                ShellKind::Bash => "bash",
                ShellKind::Zsh => "zsh",
                // The outer arm restricts `cfg.kind` to Bash/Zsh/Sh, so the only
                // remaining kind here is Sh.
                _ => "sh",
            };
            let prog = cfg
                .program
                .clone()
                .or_else(|| std::env::var_os("SHELL").map(PathBuf::from))
                .unwrap_or_else(|| PathBuf::from(format!("/bin/{name}")));
            // Login shell for bash/zsh so the profile is loaded.
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
                .ok_or_else(|| AppError::msg("ShellKind::Custom requires `program`"))?;
            (prog, Vec::new())
        }
    };

    // Shell integration via env vars — fully silent, no temp files,
    // no script written to the PTY. The shell reads the env var at startup.
    match cfg.kind {
        ShellKind::Cmd => {
            // cmd.exe reads the PROMPT env var at startup.
            // $E = ESC, $P = current path, $G = '>', $\ = literal backslash.
            // Do not override if the user already set PROMPT in cfg.env.
            if !env.contains_key("PROMPT") {
                env.insert("PROMPT".into(), CMD_OSC7_PROMPT.into());
            }
        }
        ShellKind::Bash => {
            // PROMPT_COMMAND runs before each prompt — emit OSC 7 (cwd) + OSC 133 A.
            if !env.contains_key("PROMPT_COMMAND") {
                env.insert(
                    "PROMPT_COMMAND".into(),
                    "printf '\\x1b]7;file://%s%s\\x1b\\\\' \"$HOSTNAME\" \"$PWD\"; printf '\\x1b]133;A\\x1b\\\\'"
                        .into(),
                );
            }
        }
        ShellKind::Zsh => {
            // zsh does not support PROMPT_COMMAND — set PS1 with OSC 133 markers.
            // The %{...%} wrapper stops zsh from counting escape chars for cursor position.
            if !env.contains_key("PS1") {
                env.insert(
                    "PS1".into(),
                    "%{\x1b]133;A\x1b\\\\%}%n@%m:%~ %# %{\x1b]133;B\x1b\\\\%}".into(),
                );
            }
        }
        _ => {}
    }

    // User's extra args (after the default args).
    args.extend(cfg.args.iter().cloned());

    Ok(ResolvedShell { program, args, env })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_windows_prompts_emit_osc_7() {
        assert!(CMD_OSC7_PROMPT.contains("$E]7;$P$E\\"));
        assert!(POWERSHELL_OSC7_PROMPT_INIT.contains("']7;'+$pwd.Path"));
        assert!(POWERSHELL_OSC7_PROMPT_INIT.contains("& $global:__OneTermOriginalPrompt"));
        assert!(!POWERSHELL_OSC7_PROMPT_INIT.contains('"'));
    }

    #[test]
    fn default_kind_windows() {
        // On the build machine (Windows) → Cmd.
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
        // Program is cmd.exe (COMSPEC).
        assert!(
            r.program
                .to_string_lossy()
                .to_ascii_lowercase()
                .ends_with("cmd.exe"),
            "cmd resolve → cmd.exe, got {:?}",
            r.program
        );
        // Args must include chcp 65001 (force UTF-8).
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
