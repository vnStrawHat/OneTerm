//! Shell integration against the shells of this machine (specs/oneterm, "M4 in detail"), each behind
//! a real pseudo console: Command Prompt and Windows PowerShell, which every Windows has, and
//! Git's bash where it is installed. Each says its directory and marks its prompt; bash marks a
//! command's start and its end with the status. Skipped where no console can be made.
//!
//! Nothing is typed into PowerShell: PSReadLine writes each line it accepts to the user's own
//! history file, whose place no variable of the environment moves, and a test writes nothing
//! outside the project. That its Enter says `C`, that the next prompt says `D` with the status
//! and that it writes UTF-8 were seen with this test while it still typed, when the integration
//! was written (2026-11-24).
//! bash keeps its history where `HISTFILE` says, under `.tmp`.

#![cfg(windows)]

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use oneterm::Shell;
use quadra_term::{Pty, PtyOptions};

/// Starts a shell as OneTerm does with shell integration, runs `command` (if any) once its first
/// prompt is marked, and gives everything it wrote once all of `wanted` is in it.
fn run(
    name: &'static str,
    program: PathBuf,
    args: &[&str],
    command: Option<&str>,
    wanted: &[&str],
) -> Option<String> {
    let shell = Shell { name, program, args: args.iter().map(|it| it.to_string()).collect() };
    // What a shell keeps of a session (PSReadLine's and bash's history files) stays in the
    // project: its folders are the test's.
    let home = std::path::absolute(".tmp/shell-marks").unwrap().join(name.replace(' ', "-"));
    std::fs::create_dir_all(&home).unwrap();
    let here = |under: &str| home.join(under).display().to_string();
    let mut env: Vec<(String, String)> = [("APPDATA", "appdata"), ("HOME", "home"), ("HISTFILE", "history")]
        .map(|(name, under)| (name.to_string(), here(under)))
        .into();
    std::fs::create_dir_all(home.join("home")).unwrap();
    let program = oneterm::integrate::start(Some(shell), &mut env);
    let out = Arc::new(Mutex::new(Vec::new()));
    let seen = out.clone();
    let options = PtyOptions { program, env, rows: 30, cols: 200, ..PtyOptions::default() };
    let spawned = Pty::spawn(
        options,
        move |bytes| {
            seen.lock().unwrap().extend_from_slice(bytes);
            Vec::new()
        },
        |_| {},
    );
    let Ok(pty) = spawned else {
        eprintln!("no pseudo console here: skipped");
        return None;
    };
    let text = || String::from_utf8_lossy(&out.lock().unwrap()).into_owned();
    let wait = |what: &[&str]| {
        let end = Instant::now() + Duration::from_secs(60);
        while !what.iter().all(|it| text().contains(it)) {
            assert!(Instant::now() < end, "{name} never said {what:?}: {:?}", text());
            std::thread::sleep(Duration::from_millis(20));
        }
    };
    wait(&["\x1b]133;A", "\x1b]133;B"]);
    if let Some(command) = command {
        pty.write(format!("{command}\r").as_bytes());
    }
    wait(wanted);
    pty.close();
    Some(text())
}

fn under(var: &str, path: &str) -> PathBuf {
    // With the OS's separator alone, as the shells OneTerm finds are named.
    PathBuf::from(std::env::var_os(var).unwrap_or_default()).join(path).components().collect()
}

#[test]
fn command_prompt_says_its_directory_marks_its_prompt_and_writes_utf8() {
    let cmd = under("SystemRoot", "System32/cmd.exe");
    let Some(said) = run("Command Prompt", cmd, &[], Some("chcp"), &["65001"]) else { return };
    // The directory with the prompt, and no line of the code page set at the start.
    let here = std::env::current_dir().unwrap().display().to_string();
    assert!(said.contains(&format!("\x1b]9;9;{here}")), "{said:?}");
    assert_eq!(said.matches("65001").count(), 1, "only what `chcp` answered: {said:?}");
}

#[test]
fn powershell_says_its_directory_and_marks_its_prompt() {
    let shell = under("SystemRoot", "System32/WindowsPowerShell/v1.0/powershell.exe");
    let Some(said) = run("Windows PowerShell", shell, &["-NoLogo"], None, &["\x1b]9;9;"]) else { return };
    let here = std::env::current_dir().unwrap().display().to_string();
    assert!(said.contains(&format!("\x1b]9;9;{here}")), "{said:?}");
}

#[test]
fn git_s_bash_marks_its_commands_and_says_its_directory() {
    let bash = under("ProgramFiles", "Git/bin/bash.exe");
    if !bash.is_file() {
        eprintln!("no Git bash here: skipped");
        return;
    }
    let wanted = ["marks-ok", "\x1b]133;C", "\x1b]133;D;0", "\x1b]7;file://"];
    run("Git Bash", bash, &["--login", "-i"], Some("echo marks-ok"), &wanted);
}
