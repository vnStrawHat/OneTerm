//! Typed vault errors. None of them carries secret material: no password, no
//! key, no plaintext and no envelope text.

use oneterm_core::AppError;
use thiserror::Error;

/// Why a vault operation (set-up, unlock, change, reset, seal) failed.
#[derive(Debug, Error)]
pub enum VaultError {
    /// The master password does not unwrap the data key.
    #[error("wrong master password")]
    WrongPassword,
    /// An empty master password was given; the vault never accepts one.
    #[error("the master password is empty")]
    EmptyPassword,
    /// Set-up found a `vault.json` already there (another window or process
    /// created it first).
    #[error("a master password is already set up")]
    AlreadyInitialised,
    /// The `vault.json` on disk is not the vault this process unlocked: it was
    /// reset or replaced elsewhere.
    #[error("the master password was reset or replaced in another OneTerm window")]
    VaultReplaced,
    /// `vault.json` is still this vault, but its password was changed (or the
    /// header rehashed) by another process since this one unlocked it.
    #[error("the master password was changed in another OneTerm window")]
    ChangedElsewhere,
    /// This is an elevated window, which writes no configuration (`DEC-0019`
    /// M4).
    #[error("an elevated OneTerm window cannot change the master password")]
    ReadOnly,
    /// The secret is longer than its purpose allows.
    #[error("the secret is too long to seal")]
    SecretTooLong,
    /// The operating system's random number generator failed.
    #[error("the operating system's random number generator failed")]
    Randomness,
    /// The header or the key derivation is outside what this binary supports.
    #[error("unsupported vault: {0}")]
    Unsupported(String),
    /// Reading or writing `vault.json` failed.
    #[error("vault.json: {0}")]
    Io(AppError),
}

impl From<std::io::Error> for VaultError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(AppError::Io(error))
    }
}

/// Why a sealed value cannot be opened.
///
/// Each variant is something the UI words differently
/// (`crypto-and-format.md` § 5); none of them is ever used as a password.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum OpenError {
    /// Sealed under another vault: the master password was reset since, or a
    /// file was restored from another vault.
    #[error("saved under a master password that was reset")]
    OtherVault,
    /// Written by a newer OneTerm (`otvN:` with N > 1). Keep it untouched.
    #[error("saved by a newer OneTerm")]
    NewerFormat,
    /// Not an envelope this binary can parse, including plain text.
    #[error("not a sealed value")]
    Malformed,
    /// Authentication failed: tampered, or sealed for another purpose or
    /// address.
    #[error("cannot be used for this address")]
    Rejected,
}
