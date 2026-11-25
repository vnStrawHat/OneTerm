//! The local shells (specs/oneterm, "M4 in detail"): those found on this machine, and what a new
//! space is started with.

use std::path::PathBuf;

use quadra_term::vt::{CursorShape, CursorStyle};

use crate::model::Model;

/// The default shell's name when it is a program the settings type.
pub const CUSTOM: &str = "Custom program";

/// The name of a local space's shell, kept with its session: what its header says until
/// the program gives a title.
pub(crate) struct ShellName(pub String);

/// A shell's name: the found one's, a typed program's file name, or the OS's shell's.
pub(crate) fn shell_name(shell: Option<&Shell>) -> String {
    match shell {
        Some(shell) if shell.name == CUSTOM => {
            shell.program.file_name().map_or(CUSTOM.into(), |it| it.to_string_lossy().into_owned())
        }
        Some(shell) => shell.name.into(),
        None => "Command Prompt".into(),
    }
}

/// A program a space can run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shell {
    pub name: &'static str,
    pub program: PathBuf,
    pub args: Vec<String>,
}

/// What a new space starts: a shell (the default when none), in a directory (the app's when
/// none), and how its terminal is set.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Launch {
    pub shell: Option<Shell>,
    pub cwd: Option<PathBuf>,
    /// The cursor until a program sets one, and the rows of history kept.
    pub cursor: quadra_term::vt::CursorStyle,
    pub scrollback: u32,
    /// A host to connect to instead of a local shell (P2).
    pub ssh: Option<Remote>,
    /// The SSH keepalive: how often (`None`: never) and how many may go unanswered.
    pub keepalive: (Option<std::time::Duration>, usize),
    /// The saved password or passphrase of the host, opened: the first time the engine asks
    /// for it, the answer.
    pub secret: Option<crate::vault::Secret>,
    /// What is written to the shell once it is open (a saved host's startup command).
    pub startup: Option<String>,
    /// A local shell gets `FORCE_HYPERLINK=1` (the setting Force links).
    pub force_links: bool,
    /// A shell OneTerm found is started with shell integration (the setting of that name).
    pub integration: bool,
}

/// A host and how to sign in to it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Remote {
    pub target: oneterm_ssh::Target,
    pub auth: oneterm_ssh::Auth,
    /// The jump hosts on the way, the first connected to first.
    pub jumps: Vec<oneterm_ssh::Hop>,
    pub forwards: Vec<oneterm_ssh::Forward>,
}

impl Remote {
    pub fn new(target: oneterm_ssh::Target, auth: oneterm_ssh::Auth) -> Remote {
        Remote { target, auth, jumps: Vec::new(), forwards: Vec::new() }
    }

    /// The way through this host to another: its jump hosts, then itself.
    pub fn route(&self) -> Vec<oneterm_ssh::Hop> {
        let mut hops = self.jumps.clone();
        hops.push(oneterm_ssh::Hop { target: self.target.clone(), auth: self.auth.clone() });
        hops
    }
}

/// The shells whose programs are where they are installed, in the menu's order.
pub fn found() -> Vec<Shell> {
    let var = |name: &str| std::env::var_os(name).map(PathBuf::from);
    let (system, programs, drive) = (var("SystemRoot"), var("ProgramFiles"), var("SystemDrive"));
    // With the OS's separator alone: Command Prompt reads a `/cmd.exe` in its own path as
    // `/C md .exe`, makes that folder and ends.
    let under = |root: &Option<PathBuf>, path: &str| {
        root.as_ref().map(|root| root.join(path).components().collect::<PathBuf>())
    };
    // `C:` names the drive's current folder; its root is `C:\`.
    let drive = drive.map(|it| PathBuf::from(format!("{}\\", it.display())));
    // Nushell's installer puts it under Program Files, or the user's programs.
    let nu = under(&programs, "nu/bin/nu.exe")
        .filter(|it| it.is_file())
        .or_else(|| under(&var("LOCALAPPDATA"), "Programs/nu/bin/nu.exe"));
    let known: [(&'static str, Option<PathBuf>, &[&str]); 8] = [
        ("Command Prompt", under(&system, "System32/cmd.exe"), &[]),
        (
            "Windows PowerShell",
            under(&system, "System32/WindowsPowerShell/v1.0/powershell.exe"),
            &["-NoLogo"],
        ),
        ("PowerShell 7", under(&programs, "PowerShell/7/pwsh.exe"), &["-NoLogo"]),
        ("Git Bash", under(&programs, "Git/bin/bash.exe"), &["--login", "-i"]),
        ("WSL", under(&system, "System32/wsl.exe"), &[]),
        ("Nushell", nu, &[]),
        ("MSYS2", under(&drive, "msys64/usr/bin/bash.exe"), &["--login", "-i"]),
        ("Cygwin", under(&drive, "cygwin64/bin/bash.exe"), &["--login", "-i"]),
    ];
    known
        .into_iter()
        .filter_map(|(name, program, args)| {
            let program = program.filter(|path| path.is_file())?;
            Some(Shell { name, program, args: args.iter().map(|arg| arg.to_string()).collect() })
        })
        .collect()
}

impl Model {
    /// What a new space starts as the settings say: a shell (the default one when none is
    /// given), in the active space's directory or home, with the cursor and the history set.
    pub fn launch(&self, shell: Option<Shell>) -> Launch {
        let p = self.prefs();
        let named = p.shell.get_untracked();
        let program = p.shell_program.get_untracked();
        let typed = (named == CUSTOM && !program.trim().is_empty()).then(|| Shell {
            name: CUSTOM,
            program: program.trim().into(),
            args: p.shell_args.get_untracked().split_whitespace().map(Into::into).collect(),
        });
        let shell = shell.or(typed).or_else(|| found().into_iter().find(|it| it.name == named));
        let home = || std::env::var_os("USERPROFILE").map(Into::into);
        let cwd = match p.start.get_untracked() {
            0 => self.focused_cwd(),
            1 => home(),
            _ => Some(p.last_dir.get_untracked().into())
                .filter(|it: &std::path::PathBuf| it.is_dir())
                .or_else(home),
        };
        let shape =
            [CursorShape::Block, CursorShape::Beam, CursorShape::Underline][p.cursor.get_untracked().min(2)];
        let cursor = CursorStyle { shape, blinking: p.blink.get_untracked() };
        Launch {
            shell,
            cwd,
            cursor,
            scrollback: p.scrollback.get_untracked().clamp(0.0, 1.0e6) as u32,
            ssh: None,
            keepalive: p.keepalive(),
            secret: None,
            startup: None,
            force_links: p.force_links.get_untracked(),
            integration: p.integration.get_untracked(),
        }
    }

    /// The directory of the focused space of the tab that shows.
    pub(crate) fn focused_cwd(&self) -> Option<std::path::PathBuf> {
        let space = quadra::untrack(|| self.active_tab().and_then(|tab| tab.focused()))?;
        Some(space.session.cwd().get_untracked()).filter(|cwd| !cwd.is_empty()).map(Into::into)
    }
}

#[cfg(test)]
#[test]
fn a_shell_found_is_named_with_the_os_s_separator_alone() {
    let shells = found();
    assert!(shells.iter().any(|it| it.name == "Command Prompt"), "{shells:?}");
    assert!(shells.iter().all(|it| !it.program.to_string_lossy().contains('/')), "{shells:?}");
}
