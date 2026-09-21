//! What an elevated OneTerm process is: its command line, the shell paths it
//! trusts, and the two process globals every other crate reads (`IN-0043`).
//!
//! Pure `std` on purpose. The token query, the `ShellExecuteExW` launch and the
//! pre-window message box are Win32 and live in `crates/app`; everything here is
//! data, so the security argument is unit-tested on every CI runner.
//!
//! The rule the whole module exists to keep (`DEC-0019` rule 4, mitigation M3):
//! **nothing the unelevated process writes may direct what the elevated process
//! executes.** The launching process contributes one token from a closed
//! three-value enum; the elevated process resolves that token to an absolute
//! path itself, without `PATH`, without `COMSPEC` and without user
//! configuration.

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};

use crate::error::AppError;

use super::shell::{LocalShellConfig, ShellKind};

/// The one flag OneTerm's binary accepts.
pub const ELEVATED_SHELL_FLAG: &str = "--elevated-shell";

/// The Windows shells OneTerm is willing to open in an elevated window.
///
/// A closed three-value enum rather than [`ShellKind`], which has seven variants
/// including `Custom` — the one M3 forbids. A type that cannot represent
/// `Custom` cannot be talked into launching it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElevatedShell {
    Cmd,
    PowerShell,
    Pwsh,
}

impl ElevatedShell {
    /// The command-line token for this shell.
    pub fn token(self) -> &'static str {
        match self {
            Self::Cmd => "cmd",
            Self::PowerShell => "powershell",
            Self::Pwsh => "pwsh",
        }
    }

    /// The kind this opens. Total: every variant maps.
    pub fn shell_kind(self) -> ShellKind {
        match self {
            Self::Cmd => ShellKind::Cmd,
            Self::PowerShell => ShellKind::PowerShell,
            Self::Pwsh => ShellKind::Pwsh,
        }
    }

    /// The elevatable form of `kind`, or `None` for one that may never run
    /// elevated: `Bash`, `Zsh`, `Sh` and — the M3 case — `Custom`, whose program
    /// comes from `terminal.json`.
    pub fn from_shell_kind(kind: ShellKind) -> Option<Self> {
        match kind {
            ShellKind::Cmd => Some(Self::Cmd),
            ShellKind::PowerShell => Some(Self::PowerShell),
            ShellKind::Pwsh => Some(Self::Pwsh),
            ShellKind::Bash | ShellKind::Zsh | ShellKind::Sh | ShellKind::Custom => None,
        }
    }

    fn from_token(token: &str) -> Option<Self> {
        [Self::Cmd, Self::PowerShell, Self::Pwsh]
            .into_iter()
            .find(|shell| shell.token() == token)
    }
}

/// The only way [`parse`] can fail: the command line was not understood.
///
/// Carries the arguments joined for the message and never interpreted — an
/// argument OneTerm did not recognize is not something to act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliError {
    Unrecognized(String),
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self::Unrecognized(arguments) = self;
        write!(
            f,
            "OneTerm does not understand this command line:\n\n  {arguments}\n\n\
             The only accepted form is\n\n  {ELEVATED_SHELL_FLAG} cmd\n  \
             {ELEVATED_SHELL_FLAG} powershell\n  {ELEVATED_SHELL_FLAG} pwsh"
        )
    }
}

/// Parse the process arguments (`args` includes `argv[0]`, which is skipped).
///
/// The whole grammar: no arguments is a normal start; `--elevated-shell` plus
/// exactly one of `cmd`, `powershell`, `pwsh` opens that shell; **anything else
/// is an error**. There is no tolerant branch on purpose — an unrecognized
/// argument means the caller is not the caller we think it is, and starting an
/// *elevated* window anyway is the one outcome that must not happen.
///
/// `DEC-0019` rule 3 / M6: an elevation request is served by the process that
/// received it. If OneTerm ever gains single-instance coalescing, this argument
/// must **never** be forwarded to an existing instance — forwarding would answer
/// an elevation request without elevating.
pub fn parse<I, S>(args: I) -> Result<Option<ElevatedShell>, CliError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let arguments: Vec<OsString> = args
        .into_iter()
        .skip(1)
        .map(|argument| argument.as_ref().to_os_string())
        .collect();

    let unrecognized = || {
        CliError::Unrecognized(
            arguments
                .iter()
                .map(|argument| argument.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join(" "),
        )
    };

    match arguments.as_slice() {
        [] => Ok(None),
        [flag, token] if flag == ELEVATED_SHELL_FLAG => token
            .to_str()
            .and_then(ElevatedShell::from_token)
            .map(Some)
            .ok_or_else(unrecognized),
        _ => Err(unrecognized()),
    }
}

/// Resolve `shell` to the absolute path OneTerm trusts, or `Err` with the path
/// it looked for so the message can name it.
///
/// All three paths sit under a directory a standard user cannot write, which is
/// the whole of the check: after resolution the path is tested for existence and
/// nothing else. `PATH`, `COMSPEC` and `terminal.json` are never consulted.
///
/// `versions` and `exists` are parameters for the same reason `resolve_unix_shell`
/// (`config/shell.rs`) takes its lookup and existence checks that way: the
/// resolution order is testable without touching the host.
///
/// > ponytail: existence check only. Ceiling — this trusts the ACLs of
/// > `%SystemRoot%\System32` and `%ProgramFiles%`. Upgrade path if that is ever
/// > not enough: `GetFileSecurityW` plus an owner check against
/// > `BUILTIN\Administrators` / `NT SERVICE\TrustedInstaller`.
pub fn trusted_program(
    shell: ElevatedShell,
    system_root: &Path,
    program_files: &Path,
    versions: impl Fn(&Path) -> Vec<OsString>,
    exists: impl Fn(&Path) -> bool,
) -> Result<PathBuf, PathBuf> {
    match shell {
        // Not `COMSPEC`: that is a plain environment variable any parent process
        // can set to anything.
        ElevatedShell::Cmd => found(system_root.join("System32").join("cmd.exe"), exists),
        // Windows PowerShell 5.1 ships in the OS at this fixed path; the `v1.0`
        // directory name is historical and has not changed since 2.0.
        ElevatedShell::PowerShell => found(
            system_root
                .join("System32")
                .join("WindowsPowerShell")
                .join("v1.0")
                .join("powershell.exe"),
            exists,
        ),
        ElevatedShell::Pwsh => {
            // The PowerShell 7 MSI installs to `%ProgramFiles%\PowerShell\<major>`.
            // The registry's `InstallLocation` is deliberately not read: it is
            // admin-written but points wherever the installer was told to
            // install, which can be a directory a standard user may write.
            let root = program_files.join("PowerShell");
            let mut majors: Vec<u32> = versions(&root)
                .iter()
                .filter_map(|entry| entry.to_str()?.parse().ok())
                .collect();
            majors.sort_unstable_by(|left, right| right.cmp(left));
            majors
                .into_iter()
                .map(|major| root.join(major.to_string()).join("pwsh.exe"))
                .find(|candidate| exists(candidate))
                .ok_or_else(|| root.join("7").join("pwsh.exe"))
        }
    }
}

fn found(path: PathBuf, exists: impl Fn(&Path) -> bool) -> Result<PathBuf, PathBuf> {
    if exists(&path) { Ok(path) } else { Err(path) }
}

/// [`trusted_program`] against this machine.
///
/// `%SystemRoot%` and `%ProgramFiles%` are read in the **elevated** process,
/// whose environment block was built by the AppInfo service from the elevated
/// token's profile — `SHELLEXECUTEINFOW` carries no environment block at all, so
/// the launching process cannot reach them.
pub fn trusted_program_for(shell: ElevatedShell) -> Result<PathBuf, PathBuf> {
    trusted_program(
        shell,
        &protected_root("SystemRoot", r"C:\Windows"),
        &protected_root("ProgramFiles", r"C:\Program Files"),
        |directory| {
            std::fs::read_dir(directory)
                .into_iter()
                .flatten()
                .filter_map(|entry| Some(entry.ok()?.file_name()))
                .collect()
        },
        |path| path.is_file(),
    )
}

fn protected_root(variable: &str, default: &str) -> PathBuf {
    std::env::var_os(variable)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(default))
}

/// The shell configuration an **elevated** process is allowed to spawn: the
/// trusted program for `cfg.kind`, and the kind's own arguments only.
///
/// `cfg.program` and `cfg.args` come from `terminal.json`, a file the unelevated
/// process can write, so both are dropped (M3). A kind that cannot be elevated
/// — `Custom` above all — is an error rather than a substitution: an elevated
/// window that silently ran something else would be worse than one that says it
/// cannot.
///
/// `resolve` is [`trusted_program_for`] in production and is injected by the
/// tests, which must not depend on the host having Windows.
pub fn trusted_shell_config(
    cfg: &LocalShellConfig,
    resolve: impl Fn(ElevatedShell) -> Result<PathBuf, PathBuf>,
) -> Result<LocalShellConfig, AppError> {
    let shell =
        ElevatedShell::from_shell_kind(cfg.kind).ok_or_else(|| AppError::ShellResolution {
            shell: cfg.kind.display_name().to_string(),
            reason:
                "an elevated OneTerm window runs only Command Prompt, PowerShell or PowerShell 7"
                    .to_string(),
        })?;
    let program = resolve(shell).map_err(|looked_for| AppError::ShellResolution {
        shell: cfg.kind.display_name().to_string(),
        reason: format!("not installed at {}", looked_for.display()),
    })?;
    // Built field by field, deliberately **not** with `..cfg.clone()`: struct
    // update syntax would silently carry any field added to `LocalShellConfig`
    // later across the trust boundary, which is how `env` and `cwd` crossed it
    // the first time. Listing every field means the next one added fails the
    // build until somebody decides which side of the boundary it belongs on.
    Ok(LocalShellConfig {
        kind: cfg.kind,
        program: Some(program),
        // The kind's own args only; `resolve_shell` appends `cfg.args` and finds
        // nothing to append.
        args: Vec::new(),
        // `shell.env` reaches the child's environment block **ahead** of the
        // inherited one, so a `PATH` or `PSModulePath` written into
        // `terminal.json` would decide what the elevated shell executes on the
        // first unqualified command. The elevated process therefore takes the
        // base environment and no custom entry at all.
        env: HashMap::new(),
        // `cmd.exe` searches the current directory before `PATH`, so a working
        // directory is the same escalation with one fewer field. The system
        // default (the user's home) is used instead.
        cwd: None,
        // Presentation only: the UTF-8 codepage of the console the user gets.
        utf8: cfg.utf8,
    })
}

// ── Process globals ──────────────────────────────────────────────────
//
// Both are written exactly once, at the top of `oneterm_app::run()`, before any
// UI exists; everything else only reads them. A global rather than a parameter
// because the panel registry builds panels **by name** and has nowhere to carry
// a payload, and because the elevation flag gates seams in five crates.

/// What the process token said about this process.
///
/// Three values and not a `bool`, because the query can in principle fail and
/// the two honest answers to that are not the same one. `DEC-0019` rule 2
/// forbids a window claiming an elevation the token did not confirm, so an
/// unknown process does **not** say "Administrator"; M1-M7 exist to contain an
/// administrator token, so an unknown process **is** restricted. Fail closed on
/// the restrictions, fail honest on the marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Elevation {
    /// The token says this process is not elevated. Every gate is a no-op.
    #[default]
    NotElevated,
    /// The token says this process is elevated.
    Elevated,
    /// The query failed. Theoretical — `OpenProcessToken` on one's own process
    /// with `TOKEN_QUERY` cannot be denied — but fail-open on a security switch
    /// is the wrong default even for a branch nobody expects to reach.
    Unknown,
}

impl Elevation {
    /// Whether the elevated instance's restrictions (M1-M4, M7) apply.
    ///
    /// True for [`Elevation::Unknown`]: a process that cannot prove it is
    /// ordinary is treated as though it were not.
    pub fn is_restricted(self) -> bool {
        !matches!(self, Self::NotElevated)
    }
}

/// `0`/`1`/`2` = the [`Elevation`] discriminant.
static ELEVATION: AtomicU8 = AtomicU8::new(0);
/// `0` = no shell requested; otherwise the [`ElevatedShell`] discriminant + 1.
static INITIAL_SHELL: AtomicU8 = AtomicU8::new(0);

/// The window title, for the OS title bar and the in-app one.
///
/// One function for both so the two markers cannot disagree (`DEC-0019` M5).
/// The caller passes [`elevation`] rather than the function reading the global,
/// so the rule is testable without mutating process state under other tests.
pub fn window_title(elevation: Elevation) -> &'static str {
    match elevation {
        Elevation::NotElevated => "OneTerm",
        Elevation::Elevated => "OneTerm (Administrator)",
        // Restricted, and it says what it actually knows. Claiming
        // "(Administrator)" here would be a marker asserting something the token
        // never confirmed, which `DEC-0019` rule 2 forbids.
        Elevation::Unknown => "OneTerm (elevation unknown)",
    }
}

/// The same title, split where the **app** title bar colours it: the plain
/// application name, and the elevation suffix drawn beside it in the theme's
/// `warning` (`DEC-0019` M5, amended by the owner 2026-09-21 to highlight the
/// suffix).
///
/// The OS title bar takes [`window_title`] whole and plain — a window title is
/// not a place with spans — so the two must not drift. They cannot:
/// `the_two_spans_are_exactly_the_window_title` concatenates these and asserts
/// the result *is* `window_title(elevation)`, for every variant.
pub fn window_title_parts(elevation: Elevation) -> (&'static str, Option<&'static str>) {
    match elevation {
        Elevation::NotElevated => ("OneTerm", None),
        Elevation::Elevated => ("OneTerm", Some(" (Administrator)")),
        Elevation::Unknown => ("OneTerm", Some(" (elevation unknown)")),
    }
}

/// Record what the process token said. Called once, from `run()`.
pub fn set_elevation(value: Elevation) {
    ELEVATION.store(value as u8, Ordering::Relaxed);
}

/// What the process token said. [`Elevation::NotElevated`] until set, and always
/// that off Windows.
///
/// **The token decides everything; the argument only selects the shell.**
pub fn elevation() -> Elevation {
    match ELEVATION.load(Ordering::Relaxed) {
        1 => Elevation::Elevated,
        2 => Elevation::Unknown,
        _ => Elevation::NotElevated,
    }
}

/// Whether this process runs under the elevated instance's restrictions.
///
/// The one predicate every gate reads. There is deliberately no `is_elevated()`
/// beside it: two predicates that differ only in the `Unknown` case is an
/// invitation to guard a security seam with the wrong one.
pub fn is_restricted() -> bool {
    elevation().is_restricted()
}

/// Record the shell the command line asked for. Called once, from `run()`.
pub fn set_initial_shell(shell: ElevatedShell) {
    INITIAL_SHELL.store(shell as u8 + 1, Ordering::Relaxed);
}

/// The shell the command line asked for, as the first tab's kind.
///
/// `None` for an ordinary start, which is every existing user. It *replaces* the
/// configured default shell rather than adding a tab beside it, so `DEC-0016`'s
/// "exactly one initial shell" still holds.
pub fn initial_shell() -> Option<ShellKind> {
    match INITIAL_SHELL.load(Ordering::Relaxed) {
        1 => Some(ShellKind::Cmd),
        2 => Some(ShellKind::PowerShell),
        3 => Some(ShellKind::Pwsh),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_args(arguments: &[&str]) -> Result<Option<ElevatedShell>, CliError> {
        let mut argv = vec!["oneterm.exe"];
        argv.extend_from_slice(arguments);
        parse(argv)
    }

    fn rejects(arguments: &[&str]) {
        assert!(
            matches!(parse_args(arguments), Err(CliError::Unrecognized(_))),
            "expected {arguments:?} to be rejected"
        );
    }

    #[test]
    fn no_arguments_is_a_normal_start() {
        assert_eq!(parse_args(&[]), Ok(None));
    }

    #[test]
    fn each_token_yields_its_shell() {
        for shell in [
            ElevatedShell::Cmd,
            ElevatedShell::PowerShell,
            ElevatedShell::Pwsh,
        ] {
            assert_eq!(
                parse_args(&[ELEVATED_SHELL_FLAG, shell.token()]),
                Ok(Some(shell))
            );
        }
    }

    #[test]
    fn an_unknown_flag_is_rejected() {
        rejects(&["--wat"]);
    }

    #[test]
    fn the_flag_without_a_value_is_rejected() {
        rejects(&[ELEVATED_SHELL_FLAG]);
    }

    #[test]
    fn a_unix_shell_token_is_rejected() {
        rejects(&[ELEVATED_SHELL_FLAG, "bash"]);
        rejects(&[ELEVATED_SHELL_FLAG, "zsh"]);
        rejects(&[ELEVATED_SHELL_FLAG, "sh"]);
    }

    #[test]
    fn a_custom_shell_token_is_rejected() {
        rejects(&[ELEVATED_SHELL_FLAG, "custom"]);
    }

    #[test]
    fn the_flag_given_twice_is_rejected() {
        rejects(&[ELEVATED_SHELL_FLAG, "cmd", ELEVATED_SHELL_FLAG, "pwsh"]);
    }

    #[test]
    fn a_bare_positional_is_rejected() {
        rejects(&["cmd"]);
        rejects(&[r"C:\Windows\System32\cmd.exe"]);
    }

    #[test]
    fn a_trailing_extra_argument_is_rejected() {
        rejects(&[ELEVATED_SHELL_FLAG, "cmd", "--also"]);
    }

    #[test]
    fn the_joined_form_is_rejected() {
        // `--elevated-shell=cmd` is not the grammar; there is no tolerant branch.
        rejects(&["--elevated-shell=cmd"]);
    }

    #[test]
    fn the_rejection_message_names_the_three_accepted_values() {
        let Err(error) = parse_args(&["--wat"]) else {
            panic!("expected a rejection");
        };
        let message = error.to_string();
        assert!(message.contains("--wat"));
        for token in ["cmd", "powershell", "pwsh"] {
            assert!(message.contains(token), "message must name {token}");
        }
    }

    /// The rule M5 exists for: *the argument selects the shell, the token decides
    /// everything else.* A command line can never make a window claim an
    /// elevation the process does not have.
    #[test]
    fn the_argument_never_makes_the_process_elevated() {
        assert_eq!(
            parse_args(&[ELEVATED_SHELL_FLAG, "cmd"]),
            Ok(Some(ElevatedShell::Cmd))
        );
        assert_eq!(elevation(), Elevation::NotElevated);
        assert!(!is_restricted());
        assert_eq!(window_title(elevation()), "OneTerm");
    }

    /// The app title bar draws the suffix in its own colour, so it draws two
    /// spans — and the OS title bar draws one plain string. This is what keeps
    /// them the same words: the split is a view of `window_title`, not a second
    /// copy of it that can drift.
    #[test]
    fn the_two_spans_are_exactly_the_window_title() {
        for elevation in [
            Elevation::NotElevated,
            Elevation::Elevated,
            Elevation::Unknown,
        ] {
            let (name, suffix) = window_title_parts(elevation);
            assert_eq!(
                format!("{name}{}", suffix.unwrap_or_default()),
                window_title(elevation),
                "the spans must concatenate to exactly the OS title for {elevation:?}"
            );
        }
    }

    /// An ordinary window has no suffix to highlight, so there is no second span
    /// at all — not an empty one, which would still occupy the layout.
    #[test]
    fn an_unrestricted_window_has_no_suffix_span() {
        let (name, suffix) = window_title_parts(Elevation::NotElevated);
        assert_eq!(name, "OneTerm");
        assert_eq!(suffix, None);
        // ...and both restricted states do have one, so the test above is not
        // vacuously true.
        assert!(window_title_parts(Elevation::Elevated).1.is_some());
        assert!(window_title_parts(Elevation::Unknown).1.is_some());
    }

    #[test]
    fn only_an_elevated_window_is_marked() {
        assert_eq!(window_title(Elevation::Elevated), "OneTerm (Administrator)");
        assert_eq!(window_title(Elevation::NotElevated), "OneTerm");
    }

    /// `MIN-3`: a token query that fails must fail **closed** on the
    /// restrictions and **honest** on the marker. `DEC-0019` rule 2 forbids a
    /// window claiming an elevation the token did not confirm, and M1-M7 exist
    /// to contain an administrator token, so the two answers differ.
    #[test]
    fn an_unknown_token_is_restricted_but_claims_no_elevation() {
        assert!(
            Elevation::Unknown.is_restricted(),
            "a process that cannot prove it is ordinary must be treated as though it were not"
        );
        assert_eq!(
            window_title(Elevation::Unknown),
            "OneTerm (elevation unknown)"
        );
        assert!(Elevation::Elevated.is_restricted());
        assert!(!Elevation::NotElevated.is_restricted());
    }

    #[test]
    fn tokens_and_kinds_round_trip() {
        for shell in [
            ElevatedShell::Cmd,
            ElevatedShell::PowerShell,
            ElevatedShell::Pwsh,
        ] {
            assert_eq!(ElevatedShell::from_token(shell.token()), Some(shell));
            assert_eq!(
                ElevatedShell::from_shell_kind(shell.shell_kind()),
                Some(shell)
            );
        }
    }

    #[test]
    fn no_unix_or_custom_kind_can_be_elevated() {
        for kind in [
            ShellKind::Bash,
            ShellKind::Zsh,
            ShellKind::Sh,
            ShellKind::Custom,
        ] {
            assert_eq!(ElevatedShell::from_shell_kind(kind), None);
        }
    }

    fn windows_roots() -> (PathBuf, PathBuf) {
        (
            PathBuf::from(r"X:\Windows"),
            PathBuf::from(r"X:\Program Files"),
        )
    }

    /// Every resolution runs against injected roots, so a test that passed by
    /// reading the host `PATH` would fail here instead.
    fn resolve(
        shell: ElevatedShell,
        listing: &[&str],
        present: &[&str],
    ) -> Result<PathBuf, PathBuf> {
        let (system_root, program_files) = windows_roots();
        let listing: Vec<OsString> = listing.iter().map(OsString::from).collect();
        let present: Vec<PathBuf> = present.iter().map(PathBuf::from).collect();
        trusted_program(
            shell,
            &system_root,
            &program_files,
            |_| listing.clone(),
            |path| present.iter().any(|known| known == path),
        )
    }

    #[test]
    fn cmd_resolves_under_system32_and_never_through_comspec() {
        let expected = PathBuf::from(r"X:\Windows\System32\cmd.exe");
        assert_eq!(
            resolve(ElevatedShell::Cmd, &[], &[expected.to_str().unwrap()]),
            Ok(expected)
        );
    }

    #[test]
    fn windows_powershell_resolves_under_the_v1_0_directory() {
        let expected = PathBuf::from(r"X:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe");
        assert_eq!(
            resolve(
                ElevatedShell::PowerShell,
                &[],
                &[expected.to_str().unwrap()]
            ),
            Ok(expected)
        );
    }

    #[test]
    fn a_missing_system_shell_reports_the_path_it_looked_for() {
        assert_eq!(
            resolve(ElevatedShell::Cmd, &[], &[]),
            Err(PathBuf::from(r"X:\Windows\System32\cmd.exe"))
        );
    }

    #[test]
    fn pwsh_takes_the_highest_numeric_directory() {
        assert_eq!(
            resolve(
                ElevatedShell::Pwsh,
                &["7", "8", "preview"],
                &[
                    r"X:\Program Files\PowerShell\7\pwsh.exe",
                    r"X:\Program Files\PowerShell\8\pwsh.exe",
                ]
            ),
            Ok(PathBuf::from(r"X:\Program Files\PowerShell\8\pwsh.exe"))
        );
    }

    #[test]
    fn pwsh_skips_a_numeric_directory_that_holds_no_executable() {
        assert_eq!(
            resolve(
                ElevatedShell::Pwsh,
                &["7", "9"],
                &[r"X:\Program Files\PowerShell\7\pwsh.exe"]
            ),
            Ok(PathBuf::from(r"X:\Program Files\PowerShell\7\pwsh.exe"))
        );
    }

    #[test]
    fn pwsh_with_no_install_reports_the_version_7_path() {
        assert_eq!(
            resolve(ElevatedShell::Pwsh, &["preview"], &[]),
            Err(PathBuf::from(r"X:\Program Files\PowerShell\7\pwsh.exe"))
        );
    }

    /// `MAJ-1`. Not "program and args are replaced" but **no field of
    /// `terminal.json`'s shell block survives**. `env` reaches the child's
    /// environment block ahead of the inherited one, so a `PATH` there decides
    /// what the elevated shell runs on the first unqualified command; `cmd.exe`
    /// searches the working directory before `PATH`, so `cwd` is the same
    /// escalation with one fewer field.
    #[test]
    fn an_elevated_config_keeps_no_field_of_the_configured_one() {
        let cfg = LocalShellConfig {
            kind: ShellKind::Cmd,
            program: Some(PathBuf::from(r"C:\Users\someone\evil.exe")),
            args: vec!["/c".into(), "whoami".into()],
            env: HashMap::from([
                ("PATH".to_string(), r"C:\Users\someone\bin".to_string()),
                (
                    "PSModulePath".to_string(),
                    r"C:\Users\someone\modules".to_string(),
                ),
            ]),
            cwd: Some(PathBuf::from(r"C:\Users\someone\stage")),
            utf8: true,
        };
        let trusted =
            trusted_shell_config(&cfg, |_| Ok(PathBuf::from(r"X:\Windows\System32\cmd.exe")))
                .expect("cmd is elevatable");

        assert_eq!(
            trusted.program,
            Some(PathBuf::from(r"X:\Windows\System32\cmd.exe")),
            "the program must be the trusted path, never the configured one"
        );
        assert!(
            trusted.args.is_empty(),
            "no configured argument may survive"
        );
        assert!(
            trusted.env.is_empty(),
            "no configured environment entry may survive: PATH and PSModulePath decide what runs"
        );
        assert_eq!(
            trusted.cwd, None,
            "no configured working directory may survive: cmd.exe searches it before PATH"
        );
        // The one field that is presentation and may cross: the console codepage.
        assert_eq!(trusted.utf8, cfg.utf8);
        assert_eq!(trusted.kind, cfg.kind);
    }

    #[test]
    fn a_custom_shell_cannot_be_elevated() {
        let cfg = LocalShellConfig {
            kind: ShellKind::Custom,
            program: Some(PathBuf::from(r"C:\Users\someone\evil.exe")),
            ..LocalShellConfig::default()
        };
        let error = trusted_shell_config(&cfg, |_| unreachable!("Custom must not resolve"))
            .expect_err("Custom must never be elevatable");
        assert!(error.to_string().contains("Command Prompt"));
    }

    #[test]
    fn an_elevated_config_reports_a_shell_that_is_not_installed() {
        let cfg = LocalShellConfig {
            kind: ShellKind::Pwsh,
            ..LocalShellConfig::default()
        };
        let error = trusted_shell_config(&cfg, |_| {
            Err(PathBuf::from(r"X:\Program Files\PowerShell\7\pwsh.exe"))
        })
        .expect_err("a missing pwsh must not resolve");
        assert!(error.to_string().contains("pwsh.exe"));
    }
}
