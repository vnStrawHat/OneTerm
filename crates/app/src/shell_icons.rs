//! An icon for each kind of shell (specs/oneterm, "M4 in detail"): stroked outlines on a grid of 24,
//! as the kit's, drawn for OneTerm; known by the shell's name or its program's file name.

use quadra_design::icons;

/// Command Prompt: a window with a prompt.
pub const CMD: &str = r#"<rect width="20" height="16" x="2" y="4" rx="2"/><path d="M2 8h20"/><path d="m6 12 2 2-2 2"/><path d="M11 16h4"/>"#;
/// PowerShell, both: a slanted box with `>_`.
pub const POWERSHELL: &str = r#"<path d="M6 4h16l-4 16H2z"/><path d="m8 9 4 3-4 3"/><path d="M12.5 15h3"/>"#;
/// Git Bash: a diamond with a branch.
pub const GIT: &str = r#"<path d="m12 2 10 10-10 10L2 12z"/><circle cx="12" cy="8" r="1.5"/><circle cx="12" cy="16" r="1.5"/><circle cx="15.5" cy="11.5" r="1.5"/><path d="M12 9.5v5"/><path d="m13 9 1.4 1.4"/>"#;
/// WSL: a penguin.
pub const LINUX: &str = r#"<path d="M12 2.5c-3 0-4.5 2.5-4.5 5.5 0 2.2-1.6 3.8-2.6 6.3C3.9 17 5.4 20 8.4 20h7.2c3 0 4.5-3 3.5-5.7-1-2.5-2.6-4.1-2.6-6.3 0-3-1.5-5.5-4.5-5.5z"/><path d="M10.2 7.5h.01"/><path d="M13.8 7.5h.01"/><path d="m10.5 10 1.5 1 1.5-1"/><path d="M9.5 19c-1-1.5-1-4.5 2.5-6 3.5 1.5 3.5 4.5 2.5 6"/>"#;
/// Nushell: `nu`.
pub const NU: &str = r#"<path d="M3 18v-7a3.5 3.5 0 0 1 7 0v7"/><path d="M14 11v3.5a3.5 3.5 0 0 0 7 0V11"/>"#;
/// Another bash (MSYS2, Cygwin) or a Unix shell: a box with `$`.
pub const BASH: &str = r#"<rect width="20" height="16" x="2" y="4" rx="2"/><path d="M10 9.5A1.5 1.5 0 0 0 8.5 8h-1a1.5 1.5 0 0 0 0 3h1a1.5 1.5 0 0 1 0 3h-1A1.5 1.5 0 0 1 6 12.5"/><path d="M8 6.5v9"/><path d="M13 16h5"/>"#;

/// The icon of a shell by its name (`PowerShell 7`) or its program's file name (`pwsh.exe`);
/// the terminal square for anything else.
pub(crate) fn of(name: &str) -> &'static str {
    let name = name.trim().to_ascii_lowercase();
    match name.strip_suffix(".exe").unwrap_or(&name) {
        "command prompt" | "cmd" => CMD,
        "windows powershell" | "powershell 7" | "powershell" | "pwsh" => POWERSHELL,
        "git bash" | "git-bash" => GIT,
        "wsl" | "ubuntu" | "debian" | "kali" | "opensuse" => LINUX,
        "nushell" | "nu" => NU,
        "msys2" | "cygwin" | "bash" | "zsh" | "fish" | "sh" => BASH,
        _ => icons::SQUARE_TERMINAL,
    }
}

#[cfg(test)]
#[test]
fn shells_are_known_by_name_and_by_file() {
    for (name, icon) in [
        ("Command Prompt", CMD),
        ("cmd.exe", CMD),
        ("Windows PowerShell", POWERSHELL),
        ("PowerShell 7", POWERSHELL),
        ("PWSH.EXE", POWERSHELL),
        ("Git Bash", GIT),
        ("WSL", LINUX),
        ("Nushell", NU),
        ("nu.exe", NU),
        ("MSYS2", BASH),
        ("Cygwin", BASH),
        ("zsh", BASH),
        ("python.exe", icons::SQUARE_TERMINAL),
    ] {
        assert_eq!(of(name), icon, "{name}");
    }
}
