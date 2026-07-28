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
    let backup = replace_directory_contents(&staged.package_dir, install_dir)?;
    if let Err(error) = Command::new(current_exe).spawn() {
        if let Err(restore_error) = restore_directory_contents(install_dir, &backup) {
            log::error!("failed to restore previous install after launch error: {restore_error}");
        }
        return Err(error.into());
    }
    cleanup_staging_dir(staged);
    Ok(InstallOutcome::Restarted)
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

#[cfg(all(unix, not(target_os = "macos")))]
fn replace_directory_contents(source_dir: &Path, install_dir: &Path) -> Result<PathBuf> {
    let backup = backup_path(install_dir);
    std::fs::create_dir_all(&backup)?;
    for entry in std::fs::read_dir(install_dir)? {
        let entry = entry?;
        let target = backup.join(entry.file_name());
        std::fs::rename(entry.path(), target)?;
    }
    if let Err(error) = copy_dir_contents(source_dir, install_dir) {
        if let Err(restore_error) = restore_directory_contents(install_dir, &backup) {
            return Err(AppError::msg(format!(
                "failed to replace {}: {error}; rollback failed: {restore_error}",
                install_dir.display()
            )));
        }
        return Err(error);
    }
    Ok(backup)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn restore_directory_contents(install_dir: &Path, backup: &Path) -> Result<()> {
    clear_directory_contents(install_dir)?;
    for entry in std::fs::read_dir(backup)? {
        let entry = entry?;
        let target = install_dir.join(entry.file_name());
        std::fs::rename(entry.path(), target)?;
    }
    Ok(())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn clear_directory_contents(path: &Path) -> Result<()> {
    if !path.exists() {
        std::fs::create_dir_all(path)?;
        return Ok(());
    }

    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let entry_path = entry.path();
        if entry.file_type()?.is_dir() {
            std::fs::remove_dir_all(entry_path)?;
        } else {
            std::fs::remove_file(entry_path)?;
        }
    }
    Ok(())
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

#[cfg(any(target_os = "macos", all(unix, not(target_os = "macos"))))]
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

#[cfg(any(target_os = "macos", all(unix, not(target_os = "macos"))))]
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

#[cfg(any(target_os = "macos", all(unix, not(target_os = "macos"))))]
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

    #[cfg(target_os = "macos")]
    fn test_dir(name: &str) -> PathBuf {
        let suffix = chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default();
        std::env::temp_dir().join(format!(
            "oneterm-update-{name}-{}-{suffix}",
            std::process::id()
        ))
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

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn replace_directory_contents_restores_old_install_when_copy_fails() {
        let root = test_dir("replace-dir-rollback");
        let install = root.join("install");
        std::fs::create_dir_all(&install).unwrap();
        std::fs::write(install.join("oneterm"), b"old").unwrap();
        let missing_source = root.join("missing-package");

        let result = replace_directory_contents(&missing_source, &install);

        assert!(result.is_err());
        assert_eq!(std::fs::read(install.join("oneterm")).unwrap(), b"old");
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
