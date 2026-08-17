use std::path::{Path, PathBuf};
use std::process::Command;

use oneterm_core::{AppError, Result};

use crate::StagedUpdate;

/// Result of scheduling or applying an update installation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InstallOutcome {
    /// A helper process was launched and the app should quit so replacement can run.
    RestartScheduled,
    /// Files were replaced and a new app process was started.
    Restarted,
    /// The install location is not writable, so the user must install manually.
    ManualInstall { package_dir: PathBuf },
}

/// Install a verified staged update for the current platform.
pub fn install_staged_update(staged: &StagedUpdate) -> Result<InstallOutcome> {
    let current_exe = std::env::current_exe()?;
    #[cfg(target_os = "windows")]
    {
        schedule_windows_update(staged, &current_exe)
    }
    #[cfg(target_os = "macos")]
    {
        install_macos_update(staged, &current_exe)
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        install_unix_update(staged, &current_exe)
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", unix)))]
    {
        let _ = current_exe;
        Ok(InstallOutcome::ManualInstall {
            package_dir: staged.package_dir.clone(),
        })
    }
}

#[cfg(target_os = "windows")]
fn schedule_windows_update(staged: &StagedUpdate, current_exe: &Path) -> Result<InstallOutcome> {
    let install_dir = current_exe
        .parent()
        .ok_or_else(|| AppError::msg("current executable has no parent directory"))?;
    if !is_writable_dir(install_dir) {
        return Ok(InstallOutcome::ManualInstall {
            package_dir: staged.package_dir.clone(),
        });
    }

    let pid = std::process::id();
    let timestamp = chrono::Utc::now().timestamp_millis();
    let script = std::env::temp_dir().join(format!("oneterm-install-update-{pid}-{timestamp}.cmd"));
    let exe_name = current_exe
        .file_name()
        .map(|value| value.to_os_string())
        .unwrap_or_else(|| std::ffi::OsString::from("oneterm.exe"));
    let backup_dir = install_dir
        .parent()
        .unwrap_or(install_dir)
        .join(format!(".oneterm-backup-{pid}-{timestamp}"));

    std::fs::write(&script, windows_update_script_body())?;
    let helper_env = vec![
        (
            "ONETERM_UPDATER_PID",
            std::ffi::OsString::from(pid.to_string()),
        ),
        (
            "ONETERM_INSTALL_DIR",
            install_dir.as_os_str().to_os_string(),
        ),
        (
            "ONETERM_PACKAGE_DIR",
            staged.package_dir.as_os_str().to_os_string(),
        ),
        ("ONETERM_BACKUP_DIR", backup_dir.as_os_str().to_os_string()),
        ("ONETERM_EXE_NAME", exe_name),
        (
            "ONETERM_STAGING_DIR",
            staged.staging_dir.as_os_str().to_os_string(),
        ),
        ("ONETERM_SCRIPT_PATH", script.as_os_str().to_os_string()),
    ];
    if let Err(error) = spawn_cmd_helper(&script, &helper_env) {
        let _ = std::fs::remove_file(&script);
        return Err(error);
    }
    Ok(InstallOutcome::RestartScheduled)
}

#[cfg(target_os = "macos")]
fn install_macos_update(staged: &StagedUpdate, current_exe: &Path) -> Result<InstallOutcome> {
    let Some(app_bundle) = find_app_bundle(current_exe) else {
        return Ok(InstallOutcome::ManualInstall {
            package_dir: staged.package_dir.clone(),
        });
    };
    if !is_writable_dir(app_bundle.parent().unwrap_or_else(|| Path::new("."))) {
        return Ok(InstallOutcome::ManualInstall {
            package_dir: staged.package_dir.clone(),
        });
    }
    let source_bundle = staged.package_dir.join("OneTerm.app");
    let backup = replace_path(&source_bundle, &app_bundle)?;
    if let Err(error) = Command::new("open").arg(&app_bundle).spawn() {
        if let Err(restore_error) =
            restore_path_from_backup(&app_bundle, &backup, source_bundle.is_dir())
        {
            log::error!(
                "failed to restore previous app bundle after launch error: {restore_error}"
            );
        }
        return Err(error.into());
    }
    cleanup_staging_dir(staged);
    Ok(InstallOutcome::Restarted)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn install_unix_update(staged: &StagedUpdate, current_exe: &Path) -> Result<InstallOutcome> {
    let install_dir = current_exe
        .parent()
        .ok_or_else(|| AppError::msg("current executable has no parent directory"))?;
    if !is_writable_dir(install_dir) {
        return Ok(InstallOutcome::ManualInstall {
            package_dir: staged.package_dir.clone(),
        });
    }
    replace_directory_contents_and_launch(&staged.package_dir, install_dir, || {
        Command::new(current_exe).spawn().map(|_| ())
    })?;
    cleanup_staging_dir(staged);
    Ok(InstallOutcome::Restarted)
}

/// Replace the package files inside `install_dir`, then run `launch`.
///
/// The previous files are kept in a backup directory until `launch` succeeds;
/// a launch failure restores them. On success the backup is removed so shared
/// directories such as `~/.local/bin` are not littered with stale copies.
#[cfg(any(all(unix, not(target_os = "macos")), test))]
fn replace_directory_contents_and_launch(
    source_dir: &Path,
    install_dir: &Path,
    launch: impl FnOnce() -> std::io::Result<()>,
) -> Result<()> {
    let backup = replace_directory_contents(source_dir, install_dir)?;
    if let Err(error) = launch() {
        if let Err(restore_error) = restore_directory_contents(source_dir, install_dir, &backup) {
            log::error!("failed to restore previous install after launch error: {restore_error}");
        }
        return Err(error.into());
    }
    remove_backup_dir(&backup);
    Ok(())
}

#[cfg(target_os = "macos")]
fn find_app_bundle(current_exe: &Path) -> Option<PathBuf> {
    current_exe
        .ancestors()
        .find(|path| path.extension().and_then(|ext| ext.to_str()) == Some("app"))
        .map(Path::to_path_buf)
}

#[cfg(target_os = "macos")]
fn replace_path(source: &Path, destination: &Path) -> Result<PathBuf> {
    let backup = backup_path(destination);
    std::fs::rename(destination, &backup)?;
    let copy_result = if source.is_dir() {
        copy_dir_recursive(source, destination)
    } else {
        {
            std::fs::copy(source, destination)?;
            Ok(())
        }
    };
    if let Err(error) = copy_result {
        if let Err(restore_error) = restore_path_from_backup(destination, &backup, source.is_dir())
        {
            return Err(AppError::msg(format!(
                "failed to replace {}: {error}; rollback failed: {restore_error}",
                destination.display()
            )));
        }
        return Err(error);
    }

    Ok(backup)
}

#[cfg(target_os = "macos")]
fn restore_path_from_backup(
    destination: &Path,
    backup: &Path,
    destination_is_dir: bool,
) -> Result<()> {
    if destination_is_dir {
        let _ = std::fs::remove_dir_all(destination);
    } else {
        let _ = std::fs::remove_file(destination);
    }
    std::fs::rename(backup, destination)?;
    Ok(())
}

#[cfg(any(all(unix, not(target_os = "macos")), test))]
fn replace_directory_contents(source_dir: &Path, install_dir: &Path) -> Result<PathBuf> {
    let backup = backup_path(install_dir);
    std::fs::create_dir_all(&backup)?;
    // Nothing has been copied yet, so entries still in `install_dir` are the
    // originals and only the already-moved ones need to come back.
    if let Err(error) = move_package_entries_to_backup(source_dir, install_dir, &backup) {
        return Err(rollback_error(
            install_dir,
            error,
            restore_backup_entries(install_dir, &backup),
        ));
    }
    if let Err(error) = copy_dir_contents(source_dir, install_dir) {
        return Err(rollback_error(
            install_dir,
            error,
            restore_directory_contents(source_dir, install_dir, &backup),
        ));
    }
    Ok(backup)
}

#[cfg(any(all(unix, not(target_os = "macos")), test))]
fn rollback_error(install_dir: &Path, error: AppError, restore: Result<()>) -> AppError {
    match restore {
        Ok(()) => error,
        Err(restore_error) => AppError::msg(format!(
            "failed to replace {}: {error}; rollback failed: {restore_error}",
            install_dir.display()
        )),
    }
}

/// Move only the entries shipped in the package out of `install_dir`.
///
/// The install directory may be a shared location such as `~/.local/bin`, so
/// unrelated sibling files must never be touched.
#[cfg(any(all(unix, not(target_os = "macos")), test))]
fn move_package_entries_to_backup(
    source_dir: &Path,
    install_dir: &Path,
    backup: &Path,
) -> Result<()> {
    for entry in std::fs::read_dir(source_dir)? {
        let entry = entry?;
        let existing = install_dir.join(entry.file_name());
        // `symlink_metadata` also reports dangling symlinks, which `exists()` hides.
        if std::fs::symlink_metadata(&existing).is_ok() {
            std::fs::rename(&existing, backup.join(entry.file_name()))?;
        }
    }
    Ok(())
}

/// Undo a (possibly partial) package copy and put the backed-up entries back.
#[cfg(any(all(unix, not(target_os = "macos")), test))]
fn restore_directory_contents(source_dir: &Path, install_dir: &Path, backup: &Path) -> Result<()> {
    // Entries copied from the package can only exist if the package was readable,
    // so an unreadable package means there is nothing new to remove.
    if let Ok(entries) = std::fs::read_dir(source_dir) {
        for entry in entries {
            remove_path(&install_dir.join(entry?.file_name()))?;
        }
    }
    restore_backup_entries(install_dir, backup)
}

/// Move every backed-up entry back into `install_dir` and drop the backup.
#[cfg(any(all(unix, not(target_os = "macos")), test))]
fn restore_backup_entries(install_dir: &Path, backup: &Path) -> Result<()> {
    for entry in std::fs::read_dir(backup)? {
        let entry = entry?;
        let target = install_dir.join(entry.file_name());
        std::fs::rename(entry.path(), target)?;
    }
    remove_backup_dir(backup);
    Ok(())
}

/// Remove a file, symlink, or directory tree; a missing path is not an error.
#[cfg(any(all(unix, not(target_os = "macos")), test))]
fn remove_path(path: &Path) -> Result<()> {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return Ok(());
    };
    if metadata.is_dir() {
        std::fs::remove_dir_all(path)?;
    } else {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

#[cfg(any(all(unix, not(target_os = "macos")), test))]
fn remove_backup_dir(backup: &Path) {
    if let Err(error) = std::fs::remove_dir_all(backup) {
        log::warn!(
            "failed to remove update backup directory {}: {error}",
            backup.display()
        );
    }
}

#[cfg(any(target_os = "windows", target_os = "macos", unix))]
fn is_writable_dir(path: &Path) -> bool {
    let probe = path.join(format!(".oneterm-write-test-{}", std::process::id()));
    match std::fs::write(&probe, b"test") {
        Ok(()) => {
            let _ = std::fs::remove_file(probe);
            true
        }
        Err(_) => false,
    }
}

#[cfg(any(target_os = "macos", all(unix, not(target_os = "macos"))))]
fn cleanup_staging_dir(staged: &StagedUpdate) {
    if let Err(error) = std::fs::remove_dir_all(&staged.staging_dir) {
        log::warn!(
            "failed to clean update staging directory {}: {error}",
            staged.staging_dir.display()
        );
    }
}

#[cfg(any(target_os = "macos", all(unix, not(target_os = "macos")), test))]
fn backup_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("install");
    path.with_file_name(format!(
        ".{file_name}.backup-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_millis()
    ))
}

#[cfg(any(target_os = "macos", all(unix, not(target_os = "macos")), test))]
fn copy_dir_contents(source: &Path, destination: &Path) -> Result<()> {
    std::fs::create_dir_all(destination)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let to = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&entry.path(), &to)?;
        } else {
            std::fs::copy(entry.path(), to)?;
        }
    }
    Ok(())
}

#[cfg(any(target_os = "macos", all(unix, not(target_os = "macos")), test))]
fn copy_dir_recursive(source: &Path, destination: &Path) -> Result<()> {
    std::fs::create_dir_all(destination)?;
    copy_dir_contents(source, destination)
}

#[cfg(target_os = "windows")]
fn windows_update_script_body() -> &'static str {
    r#"@echo off
setlocal DisableDelayedExpansion
set "pid=%ONETERM_UPDATER_PID%"

rem Wait until the current OneTerm process exits. Avoid PowerShell because it is
rem blocked by policy in some locked-down environments.

rem All paths are passed through environment variables by the Rust parent process.
rem Do not inline paths into this file: cmd.exe parses batch files before command
rem execution, so percent/caret metacharacters can corrupt or inject commands.

:wait_for_exit
tasklist /FI "PID eq %pid%" 2>NUL | find "%pid%" >NUL
if not errorlevel 1 (
    timeout /T 1 /NOBREAK >NUL
    goto wait_for_exit
)

mkdir "%ONETERM_BACKUP_DIR%" >NUL 2>NUL
xcopy "%ONETERM_INSTALL_DIR%\*" "%ONETERM_BACKUP_DIR%\" /E /I /H /Y >NUL
if errorlevel 2 exit /B 1
xcopy "%ONETERM_PACKAGE_DIR%\*" "%ONETERM_INSTALL_DIR%\" /E /I /H /Y >NUL
if errorlevel 2 goto restore_backup
start "" "%ONETERM_INSTALL_DIR%\%ONETERM_EXE_NAME%"
cd /d "%TEMP%" >NUL 2>NUL
rmdir /S /Q "%ONETERM_STAGING_DIR%" >NUL 2>NUL
del "%ONETERM_SCRIPT_PATH%" >NUL 2>NUL
exit /B 0

:restore_backup
rem The package copy may have partially overwritten files. Clear the install
rem directory and copy the backup back before reporting helper failure.
del /F /Q "%ONETERM_INSTALL_DIR%\*" >NUL 2>NUL
for /D %%D in ("%ONETERM_INSTALL_DIR%\*") do rmdir /S /Q "%%D" >NUL 2>NUL
xcopy "%ONETERM_BACKUP_DIR%\*" "%ONETERM_INSTALL_DIR%\" /E /I /H /Y >NUL
exit /B 1
"#
}

#[cfg(target_os = "windows")]
fn spawn_cmd_helper(script: &Path, env: &[(&str, std::ffi::OsString)]) -> Result<()> {
    let script_dir = script
        .parent()
        .ok_or_else(|| AppError::msg("updater script has no parent directory"))?;
    let script_file = script
        .file_name()
        .ok_or_else(|| AppError::msg("updater script has no file name"))?;

    let mut command = Command::new("cmd");
    command
        .arg("/D")
        .arg("/S")
        .arg("/C")
        .arg(script_file)
        .current_dir(script_dir);
    for (name, value) in env {
        command.env(name, value);
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command.spawn()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir(name: &str) -> PathBuf {
        // A process-wide sequence keeps directories distinct even when parallel
        // tests read the same coarse timestamp (as on macOS).
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);

        let suffix = chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default();
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "oneterm-update-{name}-{}-{suffix}-{sequence}",
            std::process::id()
        ))
    }

    /// Install directory with the app binary plus an unrelated sibling tool,
    /// and a staged package carrying a new binary and a new asset directory.
    struct InstallFixture {
        root: PathBuf,
        install: PathBuf,
        package: PathBuf,
    }

    impl InstallFixture {
        fn new(name: &str) -> Self {
            let root = test_dir(name);
            let install = root.join("install");
            let package = root.join("package");
            std::fs::create_dir_all(&install).unwrap();
            std::fs::create_dir_all(package.join("assets")).unwrap();
            std::fs::write(install.join("oneterm"), b"old").unwrap();
            std::fs::write(install.join("other-tool"), b"keep me").unwrap();
            std::fs::write(package.join("oneterm"), b"new").unwrap();
            std::fs::write(package.join("assets").join("icon"), b"icon").unwrap();
            Self {
                root,
                install,
                package,
            }
        }

        fn install_entries(&self) -> Vec<String> {
            let mut names: Vec<String> = std::fs::read_dir(&self.install)
                .unwrap()
                .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            names.sort();
            names
        }

        fn backup_dirs(&self) -> Vec<PathBuf> {
            std::fs::read_dir(&self.root)
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .filter(|path| {
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.starts_with(".install.backup-"))
                })
                .collect()
        }
    }

    impl Drop for InstallFixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    /// Keeps a package file unreadable for the duration of the guard so a copy
    /// from it fails part-way through the package.
    struct UnreadableFile {
        #[cfg(unix)]
        path: PathBuf,
        #[cfg(windows)]
        _handle: std::fs::File,
    }

    impl UnreadableFile {
        fn new(path: &Path) -> Self {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o000)).unwrap();
                Self {
                    path: path.to_path_buf(),
                }
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::OpenOptionsExt;
                // Exclusive share mode makes any concurrent open of the file fail.
                let handle = std::fs::OpenOptions::new()
                    .read(true)
                    .share_mode(0)
                    .open(path)
                    .unwrap();
                Self { _handle: handle }
            }
        }
    }

    #[cfg(unix)]
    impl Drop for UnreadableFile {
        fn drop(&mut self) {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&self.path, std::fs::Permissions::from_mode(0o644));
        }
    }

    #[test]
    fn replace_directory_contents_leaves_unrelated_siblings_untouched() {
        let fixture = InstallFixture::new("replace-dir-siblings");

        let backup = replace_directory_contents(&fixture.package, &fixture.install).unwrap();

        assert_eq!(
            std::fs::read(fixture.install.join("other-tool")).unwrap(),
            b"keep me"
        );
        assert!(!backup.join("other-tool").exists());
    }

    #[test]
    fn replace_directory_contents_replaces_package_files_and_backs_up_old_ones() {
        let fixture = InstallFixture::new("replace-dir-package");

        let backup = replace_directory_contents(&fixture.package, &fixture.install).unwrap();

        assert_eq!(
            fixture.install_entries(),
            vec!["assets", "oneterm", "other-tool"]
        );
        assert_eq!(
            std::fs::read(fixture.install.join("oneterm")).unwrap(),
            b"new"
        );
        assert_eq!(
            std::fs::read(fixture.install.join("assets").join("icon")).unwrap(),
            b"icon"
        );
        assert_eq!(std::fs::read(backup.join("oneterm")).unwrap(), b"old");
    }

    #[test]
    fn replace_directory_contents_restores_old_install_when_package_is_missing() {
        let fixture = InstallFixture::new("replace-dir-missing");
        let missing_source = fixture.root.join("missing-package");

        let result = replace_directory_contents(&missing_source, &fixture.install);

        assert!(result.is_err());
        assert_eq!(fixture.install_entries(), vec!["oneterm", "other-tool"]);
        assert_eq!(
            std::fs::read(fixture.install.join("oneterm")).unwrap(),
            b"old"
        );
        assert!(fixture.backup_dirs().is_empty());
    }

    #[test]
    fn replace_directory_contents_restores_old_install_when_copy_fails() {
        let fixture = InstallFixture::new("replace-dir-rollback");
        let unreadable = UnreadableFile::new(&fixture.package.join("assets").join("icon"));

        let result = replace_directory_contents(&fixture.package, &fixture.install);
        drop(unreadable);

        assert!(result.is_err());
        assert_eq!(fixture.install_entries(), vec!["oneterm", "other-tool"]);
        assert_eq!(
            std::fs::read(fixture.install.join("oneterm")).unwrap(),
            b"old"
        );
        assert_eq!(
            std::fs::read(fixture.install.join("other-tool")).unwrap(),
            b"keep me"
        );
        assert!(fixture.backup_dirs().is_empty());
    }

    #[test]
    fn launch_success_removes_backup_directory() {
        let fixture = InstallFixture::new("launch-success");

        replace_directory_contents_and_launch(&fixture.package, &fixture.install, || Ok(()))
            .unwrap();

        assert_eq!(
            std::fs::read(fixture.install.join("oneterm")).unwrap(),
            b"new"
        );
        assert!(fixture.backup_dirs().is_empty());
    }

    #[test]
    fn launch_failure_restores_old_install_and_removes_backup_directory() {
        let fixture = InstallFixture::new("launch-failure");

        let result =
            replace_directory_contents_and_launch(&fixture.package, &fixture.install, || {
                Err(std::io::Error::other("launch failed"))
            });

        assert!(result.is_err());
        assert_eq!(fixture.install_entries(), vec!["oneterm", "other-tool"]);
        assert_eq!(
            std::fs::read(fixture.install.join("oneterm")).unwrap(),
            b"old"
        );
        assert!(fixture.backup_dirs().is_empty());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn replace_path_restores_old_file_when_copy_fails() {
        let root = test_dir("replace-path-rollback");
        std::fs::create_dir_all(&root).unwrap();
        let destination = root.join("oneterm");
        std::fs::write(&destination, b"old").unwrap();
        let missing_source = root.join("missing-oneterm");

        let result = replace_path(&missing_source, &destination);

        assert!(result.is_err());
        assert_eq!(std::fs::read(&destination).unwrap(), b"old");
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_update_script_reads_paths_from_environment() {
        let script = windows_update_script_body();

        assert!(script.contains("setlocal DisableDelayedExpansion"));
        assert!(script.contains("%ONETERM_INSTALL_DIR%"));
        assert!(script.contains("%ONETERM_PACKAGE_DIR%"));
        assert!(script.contains(":restore_backup"));
        assert!(script.contains("goto restore_backup"));
        assert!(!script.contains("installDir={install_dir}"));
        assert!(!script.contains("packageDir={package_dir}"));
    }
}
