//! Launch a local file in an editor for the SFTP browser's "Edit" workflow.
//!
//! Pure, gpui-free logic: an [`EditorChoice`] (a settings-agnostic view of the
//! editor configuration) plus a path in, a spawned process out. Kept in `core`
//! so it is unit-testable and free of any UI dependency. The `settings` crate
//! owns `EditorConfig`; the UI maps it to [`EditorChoice`] to avoid a
//! `core -> settings` edge (DEC-0004).

use std::ffi::OsString;
use std::path::Path;
use std::process::Command;

use crate::{AppError, Result};

/// What editor to launch — a UI/settings-agnostic view of the editor config.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorChoice {
    /// Use the OS default application for the file's type.
    OsDefault,
    /// Spawn `program` with `args` followed by the file path.
    Custom { program: String, args: Vec<String> },
}

impl EditorChoice {
    /// Whether this choice resolves to a usable custom command. A `Custom`
    /// variant with a blank program is treated as unconfigured and falls back
    /// to the OS default.
    fn custom_program(&self) -> Option<(&str, &[String])> {
        match self {
            EditorChoice::Custom { program, args } if !program.trim().is_empty() => {
                Some((program.as_str(), args.as_slice()))
            }
            _ => None,
        }
    }
}

/// Build the argument list (config args followed by the file path) for a custom
/// editor. The program itself is passed to `Command::new` separately.
///
/// The file path is always the final, separate element, so a path containing
/// spaces or shell metacharacters cannot inject a command or be split. Extracted
/// from the spawn so it can be unit-tested without launching a process.
fn editor_args(args: &[String], path: &Path) -> Vec<OsString> {
    let mut argv: Vec<OsString> = Vec::with_capacity(args.len() + 1);
    argv.extend(args.iter().map(OsString::from));
    argv.push(path.as_os_str().to_os_string());
    argv
}

/// Launch `path` in the chosen editor.
///
/// Returns once the editor process has been spawned (fire-and-forget); the
/// editor keeps running independently of the caller. A `Custom` choice with a
/// blank program falls back to the OS default.
pub fn launch_editor(choice: &EditorChoice, path: &Path) -> Result<()> {
    match choice.custom_program() {
        Some((program, args)) => spawn_custom(program, args, path),
        None => open_with_os_default(path),
    }
}

/// Spawn a custom editor command with an explicit argv (never a shell string).
fn spawn_custom(program: &str, args: &[String], path: &Path) -> Result<()> {
    Command::new(program)
        .args(editor_args(args, path))
        .spawn()
        .map(|_child| ())
        .map_err(|e| AppError::msg(format!("failed to launch editor '{program}': {e}")))
}

/// Open `path` with the operating system's default application. Uses the `open`
/// crate, which handles the per-OS launcher quirks (e.g. the Windows `start`
/// title-argument pitfall). Non-blocking: the handler runs independently.
fn open_with_os_default(path: &Path) -> Result<()> {
    open::that_detached(path)
        .map_err(|e| AppError::msg(format!("failed to open '{}': {e}", path.display())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn blank_custom_program_falls_back_to_os_default() {
        let choice = EditorChoice::Custom {
            program: "   ".into(),
            args: vec!["-n".into()],
        };
        assert!(choice.custom_program().is_none());

        let os_default = EditorChoice::OsDefault;
        assert!(os_default.custom_program().is_none());
    }

    #[test]
    fn custom_program_is_detected() {
        let choice = EditorChoice::Custom {
            program: "code".into(),
            args: vec!["-n".into()],
        };
        let (program, args) = choice.custom_program().unwrap();
        assert_eq!(program, "code");
        assert_eq!(args, ["-n"]);
    }

    #[test]
    fn args_keep_path_as_a_single_final_element() {
        let path = PathBuf::from("/tmp/a b & c;rm.txt");
        let argv = editor_args(&["-n".into(), "--wait".into()], &path);
        assert_eq!(argv.len(), 3);
        assert_eq!(argv[0], OsString::from("-n"));
        assert_eq!(argv[1], OsString::from("--wait"));
        // The path stays exactly one argv entry, verbatim — no splitting on the
        // spaces or shell metacharacters.
        assert_eq!(argv[2], path.as_os_str());
    }

    #[test]
    fn args_without_extra_args_is_just_the_path() {
        let path = PathBuf::from("/tmp/file.txt");
        let argv = editor_args(&[], &path);
        assert_eq!(argv.len(), 1);
        assert_eq!(argv[0], path.as_os_str());
    }
}
