//! Local shell: pick a kind and resolve it to `(program, args, env)`.
//!
//! Windows-first. See `docs/terminal-backend.md` §6.1.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

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

impl ShellKind {
    /// The name the UI shows for this kind — the "+" menu's row, the Settings
    /// shell dropdown, and the tab label of a local-shell tab (`US-0114`). One
    /// list, so the row you pick, the dropdown you set and the tab you get can
    /// never disagree. `crates/settings-ui` kept a fourth wording until the
    /// `US-0114` rework folded it in here.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Cmd => "Command Prompt",
            Self::PowerShell => "PowerShell",
            Self::Pwsh => "PowerShell 7",
            Self::Bash => "Bash",
            Self::Zsh => "Zsh",
            Self::Sh => "Sh",
            Self::Custom => "Custom Shell",
        }
    }
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
    /// Working directory, or `None` for the spawner's default (the user's home).
    ///
    /// Carried here rather than read from the caller's [`LocalShellConfig`] so
    /// that **everything** a spawn needs comes out of [`resolve_shell`]. That is
    /// what makes the elevation guard at the top of that function complete: a
    /// spawner that reads the original config for one more field would walk
    /// around the guard, which is exactly how `cwd` crossed the trust boundary
    /// before (`IN-0043` MAJ-1). `cmd.exe` searches the working directory before
    /// `PATH`, so this field decides what runs just as surely as `program` does.
    pub cwd: Option<PathBuf>,
}

/// `cmd.exe` `PROMPT`: OSC 7 (cwd), OSC 133 `A`, the prompt text, OSC 133 `B`.
/// `$E` = ESC, `$P` = cwd, `$G` = `>`.
///
/// `A` and `B` are the whole set `cmd` can reach: `PROMPT` is its only hook, it
/// is expanded once before the prompt, and its `$` codes have nothing for the
/// error level — so there is no place to put `C` (output start) or `D` (command
/// done). See `docs/terminal-backend.md` §6.1.2.
const CMD_OSC7_PROMPT: &str = "$E]7;$P$E\\$E]133;A$E\\$P$G$E]133;B$E\\";
/// zsh PS1 with OSC 133 `D`/`A`/`B` markers. Each OSC is terminated by `ESC \`
/// (ST); the wire must carry exactly one backslash after ESC — a doubled `\\`
/// in the Rust literal would print a stray `\` and skew the `%{…%}` width.
///
/// `%?` is zsh's own prompt escape for the last command's exit status, which is
/// what makes `D;<code>` reachable from `PS1` alone. `C` (output start) is not:
/// it needs `preexec`, which is a *function*, and no environment variable
/// carries zsh code (`US-0136`).
const ZSH_OSC133_PS1: &str =
    "%{\x1b]133;D;%?\x1b\\\x1b]133;A\x1b\\%}%n@%m:%~ %# %{\x1b]133;B\x1b\\%}";
/// bash `PROMPT_COMMAND`: OSC 133 `D;<code>`, OSC 7 (cwd), OSC 133 `A`, and the
/// one-time append of `B` to whatever `PS1` holds.
///
/// Three things are deliberate:
///
/// * `$?` is captured first and restored by the trailing subshell, so a
///   user-supplied `PROMPT_COMMAND` appended after this one still sees the
///   status of *their* command.
/// * `B` is appended to `PS1` here rather than injected as a `PS1` environment
///   variable, because an rc file sets `PS1` after the environment is read and
///   would drop the marker. `PROMPT_COMMAND` runs after every rc file; the
///   `case` keeps the append to one.
/// * `D` is skipped on the first prompt (`__ot_seen`): a `D` before any `C`
///   would report a completed command that never ran.
const BASH_OSC133_PROMPT_COMMAND: &str = r#"__ot=$?; __ot_b=$'\033]133;B\033\\'; if [ -n "${__ot_seen-}" ]; then printf '\033]133;D;%s\033\\' "$__ot"; fi; __ot_seen=1; printf '\033]7;file://%s%s\033\\' "$HOSTNAME" "$PWD"; printf '\033]133;A\033\\'; case $PS1 in *"$__ot_b"*) ;; *) PS1="$PS1\[$__ot_b\]" ;; esac; ( exit $__ot )"#;
/// bash `PS0` — expanded after a complete command is read and before it runs
/// (bash ≥ 4.4), which is exactly OSC 133 `C`. `\e` and `\\` are prompt
/// escapes, so the wire carries `ESC ] 1 3 3 ; C ESC \`.
const BASH_OSC133_PS0: &str = r"\e]133;C\e\\";
/// The PowerShell / pwsh `-Command` startup string: wrap the global `prompt`
/// function so it emits `D;<code>`, OSC 7 and `A` before the original prompt
/// and `B` after it, and bind `Enter` through PSReadLine so `C` is emitted when
/// a command is submitted.
///
/// * The exit code is `$?` first and `$LASTEXITCODE` only as the *number* for a
///   failure: `$LASTEXITCODE` is stale after a cmdlet (measured — `cmd /c exit 3`
///   then `Get-Item .` leaves it at 3 with `$?` true), and `$?` carries no number.
/// * `B` is appended to the original prompt's return value rather than written
///   to the console, because the host writes that value *after* this function
///   returns; writing `B` here would put it before the prompt text.
/// * The `Enter` handler is installed only when `Set-PSReadLineKeyHandler`
///   resolves. A host without PSReadLine keeps its own `Enter` and simply
///   reports no `C` — the alternative, replacing `PSConsoleHostReadLine`, costs
///   the user their line editor when it is wrong.
/// * No `"` anywhere: this is one `-Command` argument and goes through Windows
///   command-line quoting.
const POWERSHELL_OSC133_PROMPT_INIT: &str = concat!(
    r"$global:__OneTermOriginalPrompt=$function:prompt;",
    r"function global:prompt{",
    r"$ok=$?;$e=[char]27;",
    r"if($global:__OneTermRan){",
    r"$global:__OneTermRan=$false;",
    r"$c=if($ok){0}elseif($global:LASTEXITCODE -gt 0){$global:LASTEXITCODE}else{1};",
    r"[Console]::Write($e+']133;D;'+$c+$e+'\')",
    r"};",
    r"[Console]::Write($e+']7;'+$pwd.Path+$e+'\'+$e+']133;A'+$e+'\');",
    r"((& $global:__OneTermOriginalPrompt) -join '')+$e+']133;B'+$e+'\'",
    r"};",
    r"if(Get-Command Set-PSReadLineKeyHandler -ErrorAction Ignore){",
    r"Set-PSReadLineKeyHandler -Key Enter -ScriptBlock{",
    r"[Microsoft.PowerShell.PSConsoleReadLine]::AcceptLine();",
    r"$global:__OneTermRan=$true;",
    r"[Console]::Write([char]27+']133;C'+[char]27+'\')",
    r"}}",
);

fn powershell_init(utf8: bool) -> String {
    if utf8 {
        format!(
            "[Console]::OutputEncoding=[Text.UTF8Encoding]::new();{POWERSHELL_OSC133_PROMPT_INIT}"
        )
    } else {
        POWERSHELL_OSC133_PROMPT_INIT.to_string()
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

/// Resolve a requested Unix shell (`bash` / `zsh` / `sh`) to an executable.
///
/// Order: PATH, then `/bin/{name}`, then `$SHELL` — but only when `$SHELL`
/// is that very shell. `$SHELL` is the user's login shell, not the requested
/// one: falling back to it blindly made `ShellKind::Zsh` spawn bash while
/// still injecting the zsh PS1. The final `/bin/{name}` default lets the
/// spawn error report the missing shell by name.
///
/// `lookup` and `exists` are parameters so the order can be tested without
/// touching the host PATH or filesystem; production passes
/// [`find_in_path`] and `Path::is_file`.
fn resolve_unix_shell(
    name: &str,
    shell_env: Option<&Path>,
    lookup: impl Fn(&str) -> Option<PathBuf>,
    exists: impl Fn(&Path) -> bool,
) -> PathBuf {
    if let Some(found) = lookup(name) {
        return found;
    }
    let default = PathBuf::from(format!("/bin/{name}"));
    if exists(&default) {
        return default;
    }
    if let Some(shell) = shell_env {
        if shell.file_name().is_some_and(|file| file == name) {
            return shell.to_path_buf();
        }
    }
    default
}

/// `AppError::ShellResolution` for a shell program that is not on `PATH`.
fn shell_not_found(shell: &str) -> AppError {
    AppError::ShellResolution {
        shell: shell.to_string(),
        reason: "not found in PATH".to_string(),
    }
}

/// Resolve `LocalShellConfig` → `(program, args, env)` ready to spawn.
///
/// Returns an error if `Custom` has no `program`, or the default shell cannot be found.
pub fn resolve_shell(cfg: &LocalShellConfig) -> Result<ResolvedShell, AppError> {
    // M3 (`DEC-0019` rule 4, `IN-0043`): an elevated process never lets
    // `terminal.json` name the program it executes or add arguments to it. One
    // guard here covers every local spawn in that process, because every one of
    // them resolves through this function.
    let trusted;
    let cfg = if super::elevation::is_restricted() {
        trusted =
            super::elevation::trusted_shell_config(cfg, super::elevation::trusted_program_for)?;
        &trusted
    } else {
        cfg
    };

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
                .ok_or_else(|| shell_not_found("powershell.exe"))?;
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
                .ok_or_else(|| shell_not_found("pwsh"))?;
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
            let prog = cfg.program.clone().unwrap_or_else(|| {
                resolve_unix_shell(
                    name,
                    std::env::var_os("SHELL").map(PathBuf::from).as_deref(),
                    find_in_path,
                    Path::is_file,
                )
            });
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
                .ok_or_else(|| AppError::ShellResolution {
                    shell: "custom".to_string(),
                    reason: "ShellKind::Custom requires `program`".to_string(),
                })?;
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
            // PROMPT_COMMAND runs before each prompt (D + OSC 7 + A, and the
            // PS1 append for B); PS0 runs between reading a command and running
            // it (C). A user-supplied PROMPT_COMMAND is *appended to*, never
            // replaced — OneTerm's part restores `$?` before it runs.
            let value = match env.get("PROMPT_COMMAND") {
                Some(user) if !user.trim().is_empty() => {
                    format!("{BASH_OSC133_PROMPT_COMMAND}; {user}")
                }
                _ => BASH_OSC133_PROMPT_COMMAND.to_string(),
            };
            env.insert("PROMPT_COMMAND".into(), value);
            env.entry("PS0".into())
                .or_insert_with(|| BASH_OSC133_PS0.to_string());
        }
        ShellKind::Zsh => {
            // zsh does not support PROMPT_COMMAND — set PS1 with OSC 133 markers.
            // The %{...%} wrapper stops zsh from counting escape chars for cursor position.
            if !env.contains_key("PS1") {
                env.insert("PS1".into(), ZSH_OSC133_PS1.into());
            }
        }
        _ => {}
    }

    // User's extra args (after the default args).
    args.extend(cfg.args.iter().cloned());

    Ok(ResolvedShell {
        program,
        args,
        env,
        cwd: cfg.cwd.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_windows_prompts_emit_osc_7() {
        assert!(CMD_OSC7_PROMPT.contains("$E]7;$P$E\\"));
        assert!(POWERSHELL_OSC133_PROMPT_INIT.contains("']7;'+$pwd.Path"));
        assert!(POWERSHELL_OSC133_PROMPT_INIT.contains("& $global:__OneTermOriginalPrompt"));
        assert!(!POWERSHELL_OSC133_PROMPT_INIT.contains('"'));
    }

    /// `US-0136`: `cmd.exe` marks the prompt region and nothing else — its only
    /// hook is `PROMPT`, which runs once before the prompt and has no `$` code
    /// for the error level, so `C` and `D` are unreachable.
    #[test]
    fn cmd_prompt_emits_the_prompt_region_only() {
        assert!(CMD_OSC7_PROMPT.contains("$E]133;A$E\\"));
        assert!(CMD_OSC7_PROMPT.contains("$E]133;B$E\\"));
        assert!(!CMD_OSC7_PROMPT.contains("133;C"));
        assert!(!CMD_OSC7_PROMPT.contains("133;D"));
    }

    #[test]
    fn zsh_ps1_carries_the_exit_code_and_one_backslash_after_esc() {
        // OSC 133 D/A/B must be terminated by ST = ESC + one backslash; a second
        // backslash would print and skew the %{…%} zero-width accounting.
        // `%?` is zsh's own last-exit-status escape — `US-0136`.
        assert_eq!(
            ZSH_OSC133_PS1.as_bytes(),
            b"%{\x1b]133;D;%?\x1b\\\x1b]133;A\x1b\\%}%n@%m:%~ %# %{\x1b]133;B\x1b\\%}"
        );
        assert!(!ZSH_OSC133_PS1.contains("\\\\"));
        // `C` needs `preexec`, which no environment variable can carry.
        assert!(!ZSH_OSC133_PS1.contains("133;C"));
    }

    #[test]
    fn zsh_kind_injects_ps1_verbatim() {
        let cfg = LocalShellConfig {
            kind: ShellKind::Zsh,
            program: Some(PathBuf::from("/usr/local/bin/zsh")),
            ..Default::default()
        };
        let r = resolve_shell(&cfg).unwrap();
        assert_eq!(r.env.get("PS1").map(String::as_str), Some(ZSH_OSC133_PS1));
        assert_eq!(r.args, vec!["-l"]);
    }

    fn resolved_bash(env: &[(&str, &str)]) -> ResolvedShell {
        let cfg = LocalShellConfig {
            kind: ShellKind::Bash,
            program: Some(PathBuf::from("/bin/bash")),
            env: env
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
            ..Default::default()
        };
        resolve_shell(&cfg).unwrap()
    }

    /// `US-0136`: bash reaches all four marks — `D` and `A` from
    /// `PROMPT_COMMAND`, `C` from `PS0`, `B` appended to `PS1` at prompt time.
    #[test]
    fn bash_emits_the_full_mark_set() {
        let r = resolved_bash(&[]);
        let pc = r.env.get("PROMPT_COMMAND").expect("PROMPT_COMMAND");
        // `D` carries the exit code in the form the engine parses
        // (`crates/vt/src/terminal/dispatch.rs`, `133;D;<code>`).
        assert!(
            pc.contains(r#"printf '\033]133;D;%s\033\\' "$__ot""#),
            "{pc}"
        );
        assert!(pc.contains(r"printf '\033]133;A\033\\'"), "{pc}");
        assert!(pc.contains(r#"PS1="$PS1\[$__ot_b\]""#));
        assert_eq!(r.env.get("PS0").map(String::as_str), Some(BASH_OSC133_PS0));
        assert_eq!(BASH_OSC133_PS0, r"\e]133;C\e\\");
    }

    /// The command's status must survive OneTerm's hook: captured first,
    /// skipped on the very first prompt, restored by the trailing subshell.
    #[test]
    fn bash_preserves_the_exit_status_and_skips_the_first_prompt() {
        let pc = BASH_OSC133_PROMPT_COMMAND;
        assert!(pc.starts_with("__ot=$?;"), "{pc}");
        assert!(pc.trim_end().ends_with("( exit $__ot )"), "{pc}");
        assert!(pc.contains(r#"if [ -n "${__ot_seen-}" ]"#), "{pc}");
    }

    /// A user's own `PROMPT_COMMAND` is appended to, never replaced — and it
    /// runs after the subshell that restores `$?`.
    #[test]
    fn bash_appends_to_a_user_prompt_command() {
        let r = resolved_bash(&[("PROMPT_COMMAND", "my_hook")]);
        let pc = r.env.get("PROMPT_COMMAND").expect("PROMPT_COMMAND");
        assert_eq!(pc, &format!("{BASH_OSC133_PROMPT_COMMAND}; my_hook"));
        assert!(pc.starts_with(BASH_OSC133_PROMPT_COMMAND));
        assert!(pc.ends_with("; my_hook"));
    }

    /// A user's own `PS0` wins outright; there is nothing to append to a single
    /// expansion point.
    #[test]
    fn bash_keeps_a_user_ps0() {
        let r = resolved_bash(&[("PS0", "mine")]);
        assert_eq!(r.env.get("PS0").map(String::as_str), Some("mine"));
    }

    /// `US-0136`: the PowerShell prompt wrapper emits `D` + OSC 7 + `A` before
    /// the original prompt, appends `B` to what it returns, and binds `Enter`
    /// for `C` only when PSReadLine is there.
    #[test]
    fn powershell_init_emits_the_full_mark_set() {
        let init = powershell_init(true);
        assert!(init.starts_with("[Console]::OutputEncoding="));
        for marker in [
            "']133;D;'+$c+$e+'\\'",
            "']133;A'+$e+'\\'",
            "']133;B'+$e+'\\'",
            "']133;C'+[char]27+'\\'",
        ] {
            assert!(init.contains(marker), "missing {marker} in {init}");
        }
        // `$?` first, `$LASTEXITCODE` only as the number for a failure: it is
        // stale after a cmdlet.
        assert!(init.contains("$ok=$?;"));
        assert!(init.contains("$c=if($ok){0}elseif($global:LASTEXITCODE -gt 0)"));
        // `C` is optional, never at the cost of the line editor.
        assert!(init.contains("if(Get-Command Set-PSReadLineKeyHandler -ErrorAction Ignore){"));
        assert!(init.contains("[Microsoft.PowerShell.PSConsoleReadLine]::AcceptLine();"));
        // No `D` before the first command.
        assert!(init.contains("if($global:__OneTermRan){"));
        // One `-Command` argument, so no double quote may appear in it.
        assert!(!init.contains('"'));
        assert_eq!(powershell_init(false), POWERSHELL_OSC133_PROMPT_INIT);
    }

    /// `resolve_unix_shell` against a fake PATH and filesystem.
    fn resolve(
        name: &str,
        path_hits: &[(&str, &str)],
        files: &[&str],
        shell_env: Option<&str>,
    ) -> PathBuf {
        resolve_unix_shell(
            name,
            shell_env.map(Path::new),
            |wanted: &str| {
                path_hits
                    .iter()
                    .find(|(n, _)| *n == wanted)
                    .map(|(_, p)| PathBuf::from(p))
            },
            |path: &Path| files.iter().any(|f| Path::new(f) == path),
        )
    }

    #[test]
    fn unix_shell_prefers_path_lookup() {
        assert_eq!(
            resolve(
                "zsh",
                &[("zsh", "/opt/homebrew/bin/zsh")],
                &["/bin/zsh"],
                Some("/bin/bash")
            ),
            PathBuf::from("/opt/homebrew/bin/zsh")
        );
    }

    #[test]
    fn unix_shell_falls_back_to_bin_before_shell_env() {
        assert_eq!(
            resolve("zsh", &[], &["/bin/zsh"], Some("/usr/local/bin/zsh")),
            PathBuf::from("/bin/zsh")
        );
    }

    #[test]
    fn unix_shell_uses_shell_env_only_when_it_matches() {
        // $SHELL is zsh but zsh is neither on PATH nor in /bin → use $SHELL.
        assert_eq!(
            resolve("zsh", &[], &[], Some("/usr/local/bin/zsh")),
            PathBuf::from("/usr/local/bin/zsh")
        );
        // $SHELL is bash: requesting zsh must NOT resolve to bash.
        assert_eq!(
            resolve("zsh", &[], &[], Some("/bin/bash")),
            PathBuf::from("/bin/zsh")
        );
        // Requesting bash while $SHELL is zsh resolves to the bash default.
        assert_eq!(
            resolve("bash", &[], &[], Some("/bin/zsh")),
            PathBuf::from("/bin/bash")
        );
    }

    #[test]
    fn unix_shell_without_shell_env_defaults_to_bin() {
        assert_eq!(resolve("sh", &[], &[], None), PathBuf::from("/bin/sh"));
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
