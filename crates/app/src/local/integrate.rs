//! Shell integration (specs/oneterm, "M4 in detail"): what a shell OneTerm found is started with so
//! that it says where it is (OSC 9;9, OSC 7) and where its prompts, commands and outputs
//! begin and end (OSC 133), and writes UTF-8. A program the user typed is left as typed.

use std::path::PathBuf;

use crate::shells::{CUSTOM, Shell};

/// Command Prompt's `PROMPT`, its one hook: its directory, OSC 133 `A`, the prompt, `B`.
/// It has nowhere to say that a command started or how it ended.
const CMD_PROMPT: &str = "$e]9;9;$P$e\\$e]133;A$e\\$P$G$e]133;B$e\\";

/// bash's `PROMPT_COMMAND`: OSC 133 `D` with the status (not before the first prompt), OSC 7,
/// `A`, and `B` put once at the end of whatever `PS1` the rc files left. `$?` is taken first
/// and given back last, so a hook of the user's after this one sees its own command's status.
/// `B` is raw bytes between readline's marks of what prints nothing (`\001`, `\002`): nothing
/// in it for prompt expansion to join with the end of the user's `PS1`.
const BASH_PROMPT_COMMAND: &str = r#"__ot=$?; __ot_b=$'\001\033]133;B\007\002'; if [ -n "${__ot_seen-}" ]; then printf '\033]133;D;%s\033\\' "$__ot"; fi; __ot_seen=1; printf '\033]7;file://%s%s\033\\' "$HOSTNAME" "$PWD"; printf '\033]133;A\033\\'; case $PS1 in *"$__ot_b"*) ;; *) PS1="$PS1$__ot_b" ;; esac; ( exit $__ot )"#;
/// bash's `PS0`, expanded once a command is read and before it runs: OSC 133 `C`.
const BASH_PS0: &str = r"\e]133;C\e\\";

/// PowerShell's start (both): UTF-8 out, the `prompt` function wrapped (`D` with the status,
/// the directory, `A`, the user's own prompt, `B`), and Enter bound through PSReadLine to
/// say `C`, calling what Enter was bound to. The status is `$?`, `$LASTEXITCODE` only its
/// number: that one is stale after a cmdlet. `B` is added to what the prompt returns, which
/// the host writes after the function ends. Enter is bound only when what it was bound to
/// is a method that can still be called (a script block is not). No `"` in it: it is one
/// quoted argument of the command line.
const POWERSHELL_START: &str = concat!(
    r"[Console]::OutputEncoding=[Text.UTF8Encoding]::new();",
    r"$global:__OtPrompt=$function:prompt;",
    r"function global:prompt{",
    r"$ok=$?;$e=[char]27;",
    r"if($global:__OtRan){",
    r"$global:__OtRan=$false;",
    r"$c=if($ok){0}elseif($global:LASTEXITCODE -gt 0){$global:LASTEXITCODE}else{1};",
    r"[Console]::Write($e+']133;D;'+$c+$e+'\')",
    r"};",
    r"if($pwd.Provider.Name -eq 'FileSystem'){[Console]::Write($e+']9;9;'+$pwd.ProviderPath+$e+'\')};",
    r"[Console]::Write($e+']133;A'+$e+'\');",
    r"$p='';",
    r"try{$p=(& $global:__OtPrompt) -join [Environment]::NewLine}",
    r"catch{$p='PS '+$pwd.Path+'> '};",
    r"$p+$e+']133;B'+$e+'\'",
    r"};",
    r"if(Get-Command Set-PSReadLineKeyHandler -ErrorAction Ignore){",
    r"$b=Get-PSReadLineKeyHandler -Bound|Where-Object{$_.Key -eq 'Enter'}|Select-Object -First 1;",
    r"$f=if($b){$b.Function}else{'AcceptLine'};",
    r"if([Microsoft.PowerShell.PSConsoleReadLine].GetMethod($f)){",
    r"$global:__OtEnter=$f;",
    r"Set-PSReadLineKeyHandler -Key Enter -ScriptBlock{",
    r"$m=$global:__OtEnter;",
    r"[Microsoft.PowerShell.PSConsoleReadLine]::$m();",
    r"$global:__OtRan=$true;",
    r"[Console]::Write([char]27+']133;C'+[char]27+'\')",
    r"}}}",
);

/// The program and the arguments a shell starts with, and what `env` gains: by the name of
/// the program's file. The OS's shell (`None`) is Command Prompt.
pub fn start(shell: Option<Shell>, env: &mut Vec<(String, String)>) -> Option<(String, Vec<String>)> {
    let (program, mut args, typed) = match shell {
        Some(shell) => (shell.program, shell.args, shell.name == CUSTOM),
        // By its whole path: a `cmd.exe` in the directory it starts in is not it.
        None => (
            PathBuf::from(std::env::var_os("SystemRoot")?).join("System32").join("cmd.exe"),
            Vec::new(),
            false,
        ),
    };
    let kind = program.file_stem().map(|it| it.to_string_lossy().to_lowercase()).unwrap_or_default();
    match kind.as_str() {
        _ if typed => {}
        // Verbatim on the command line: the code page first, without its line.
        "cmd" => {
            args.extend(["/K".to_string(), "chcp 65001 >nul".to_string()]);
            env.push(("PROMPT".into(), CMD_PROMPT.into()));
        }
        "powershell" | "pwsh" => {
            args.extend(["-NoExit".to_string(), "-Command".to_string(), format!("\"{POWERSHELL_START}\"")]);
        }
        "bash" => {
            let mine = |env: &[(String, String)], name: &str| {
                env.iter()
                    .rev()
                    .find(|it| it.0 == name)
                    .map(|it| it.1.trim().to_string())
                    .filter(|it| !it.is_empty())
            };
            // A hook of the user's that says OSC 133 itself is left alone; another runs after.
            let command = match mine(env, "PROMPT_COMMAND") {
                Some(theirs) if theirs.contains("133;") => None,
                Some(theirs) => Some(format!("{BASH_PROMPT_COMMAND}; {theirs}")),
                None => Some(BASH_PROMPT_COMMAND.to_string()),
            };
            if let Some(command) = command {
                env.push(("PROMPT_COMMAND".into(), command));
                if mine(env, "PS0").is_none() {
                    env.push(("PS0".into(), BASH_PS0.into()));
                }
            }
        }
        _ => {}
    }
    Some((program.display().to_string(), args))
}

/// The folder a bash of Windows (MSYS2, Git's, Cygwin) or WSL names its own way, `/c/work`,
/// `/cygdrive/c/work`, `/mnt/c/work`: `C:\work`, when that is a folder.
/// ponytail: a path under the shell's own root (`/usr/bin`) has no folder here; its install
/// folder would give it one.
pub fn windows_folder(path: &str) -> Option<String> {
    let rest = path.strip_prefix('/')?;
    let rest = rest.strip_prefix("cygdrive/").or_else(|| rest.strip_prefix("mnt/")).unwrap_or(rest);
    let (drive, under) = rest.split_once('/').unwrap_or((rest, ""));
    let letter = drive.chars().next().filter(|it| drive.len() == 1 && it.is_ascii_alphabetic())?;
    let folder = format!("{}:\\{}", letter.to_ascii_uppercase(), under.replace('/', "\\"));
    std::path::Path::new(&folder).is_dir().then_some(folder)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shell(name: &'static str, program: &str) -> Option<Shell> {
        Some(Shell { name, program: program.into(), args: vec!["-x".into()] })
    }

    #[test]
    fn each_shell_found_is_started_its_own_way_and_a_typed_program_as_typed() {
        let mut env = Vec::new();
        let (_, args) = start(shell("Command Prompt", "C:/Windows/System32/cmd.exe"), &mut env).unwrap();
        assert_eq!(args, ["-x", "/K", "chcp 65001 >nul"]);
        assert!(env[0].0 == "PROMPT" && env[0].1.contains("]9;9;$P") && env[0].1.contains("]133;A"));

        let mut env = Vec::new();
        let (_, args) =
            start(shell("PowerShell 7", "C:/Program Files/PowerShell/7/pwsh.exe"), &mut env).unwrap();
        assert_eq!(args[..3], ["-x", "-NoExit", "-Command"]);
        let script = &args[3];
        assert!(script.starts_with('"') && script.ends_with('"') && env.is_empty());
        assert!(
            !script[1..script.len() - 1].contains('"') && !script.ends_with("\\\""),
            "one quoted argument"
        );

        // bash: the user's hook runs after OneTerm's; one that says OSC 133 itself stands alone.
        let mut env = vec![("PROMPT_COMMAND".to_string(), "history -a".to_string())];
        start(shell("Git Bash", "C:/Program Files/Git/bin/bash.exe"), &mut env).unwrap();
        assert!(env[1].0 == "PROMPT_COMMAND" && env[1].1.ends_with("; history -a") && env[2].0 == "PS0");
        let mut env = vec![("PROMPT_COMMAND".to_string(), r"printf '\e]133;A\a'".to_string())];
        start(shell("Git Bash", "C:/Program Files/Git/bin/bash.exe"), &mut env).unwrap();
        assert_eq!(env.len(), 1);

        let mut env = Vec::new();
        let (_, args) = start(shell(CUSTOM, "C:/tools/pwsh.exe"), &mut env).unwrap();
        assert!(args == ["-x"] && env.is_empty());
    }

    #[test]
    fn a_bash_s_path_of_a_drive_is_the_folder() {
        let here = std::env::current_dir().unwrap().display().to_string();
        let (drive, under) = here.split_once(":\\").expect("a drive");
        let unix = format!("{}/{}", drive.to_lowercase(), under.replace('\\', "/"));
        for said in [format!("/{unix}"), format!("/cygdrive/{unix}"), format!("/mnt/{unix}")] {
            assert_eq!(windows_folder(&said).as_deref(), Some(here.as_str()), "{said}");
        }
        assert_eq!(windows_folder("/usr/bin"), None);
        assert_eq!(windows_folder(&here), None);
    }
}
