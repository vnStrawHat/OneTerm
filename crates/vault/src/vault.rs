//! The vault lifecycle (set up, unlock, change, reset) and sealing
//! (`crypto-and-format.md` § 2, `high-level-design.md` § Vault API).
//!
//! The runtime phases map onto types: **Uninitialised** is
//! [`VaultStatus::Uninitialised`]; **Locked** is holding a [`VaultHeader`]
//! (from [`VaultStatus::Ready`] or [`UnlockedVault::header`]) and nothing
//! else; **Unlocked** is holding an [`UnlockedVault`]. Lock is dropping the
//! `UnlockedVault`. [`unlock`] verifies against the header the caller holds in
//! memory and never re-reads `vault.json`, so replacing the file cannot open a
//! locked window (`idle-lock-overlay.md`).

use std::path::Path;

use aes_gcm::aead::{Aead, Generate, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use oneterm_core::SecretString;
use zeroize::Zeroizing;

use crate::document::{self, DocumentChange, VaultHeader};
use crate::envelope::{self, Envelope, KID_LEN, NONCE_LEN};
use crate::kdf::{self, KEY_LEN, KdfParams, SALT_LEN};
use crate::{OpenError, SealedSecret, VaultError};

/// Purpose of the data-key wrap in the associated data. Internal: no caller
/// seals with it.
const KEY_WRAP_PURPOSE: &str = "key-wrap";

/// What a sealed value is for. Part of the associated data, so a value sealed
/// for one purpose never opens as another.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretPurpose {
    /// An SSH login password, bound to host, port and user.
    SshPassword,
}

impl SecretPurpose {
    fn label(self) -> &'static str {
        match self {
            Self::SshPassword => "ssh-password",
        }
    }

    /// The longest plaintext this purpose seals, in bytes.
    fn max_plaintext(self) -> usize {
        match self {
            Self::SshPassword => 1024,
        }
    }
}

/// The address a sealed SSH password belongs to. The session id is
/// deliberately not part of it, so a duplicated session keeps a working copy.
#[derive(Debug, Clone, Copy)]
pub struct SealContext<'a> {
    /// Host as stored in the session (UTF-8, not normalised).
    pub host: &'a str,
    /// TCP port.
    pub port: u16,
    /// User name as stored in the session.
    pub username: &'a str,
}

impl SealContext<'_> {
    fn associated_data(&self, purpose: SecretPurpose, vault_id: &[u8; 16]) -> Vec<u8> {
        envelope::associated_data(
            purpose.label(),
            vault_id,
            &[
                self.host.as_bytes(),
                &self.port.to_be_bytes(),
                self.username.as_bytes(),
            ],
        )
    }
}

/// An unlocked vault: the header plus the data key, held only inside an
/// AES-256-GCM instance whose key schedule is cleared on drop.
///
/// Dropping it is the lock. It is deliberately neither `Clone` nor `Debug`
/// nor serializable.
pub struct UnlockedVault {
    header: VaultHeader,
    cipher: Aes256Gcm,
}

/// Set up a new vault in `dir` under `password` and return it unlocked.
///
/// Creates `vault.json` exclusively: when one exists (another window or
/// process was first, or a damaged file is waiting for Reset) nothing is
/// written and the error says so. Costs one Argon2id run; call it off the UI
/// thread.
pub fn init(dir: &Path, password: &SecretString) -> Result<UnlockedVault, VaultError> {
    init_with(
        dir,
        password,
        KdfParams::DEFAULT,
        oneterm_core::elevation::is_restricted(),
    )
}

/// Unlock the vault `header` describes with `password`.
///
/// Uses only the header the caller holds; never reads `vault.json`. Costs one
/// Argon2id run whether the password is right or wrong.
pub fn unlock(header: &VaultHeader, password: &SecretString) -> Result<UnlockedVault, VaultError> {
    if password.is_empty() {
        return Err(VaultError::EmptyPassword);
    }
    let data_key = unwrap_data_key(header, password)?;
    Ok(UnlockedVault {
        header: header.clone(),
        cipher: cipher_for(&data_key)?,
    })
}

/// Remove `dir/vault.json` and its backup. A ready or damaged file (or none)
/// is reset; one written by a newer OneTerm ([`crate::VaultStatus::Unsupported`]) is
/// refused with [`VaultError::Unsupported`] and left in place.
///
/// Every owner of sealed values must wipe them **before** this runs
/// (`crypto-and-format.md` § 8); envelopes left behind afterwards read as
/// [`OpenError::OtherVault`] under a new vault.
pub fn reset(dir: &Path) -> Result<(), VaultError> {
    reset_with(dir, oneterm_core::elevation::is_restricted())
}

pub(crate) fn init_with(
    dir: &Path,
    password: &SecretString,
    params: KdfParams,
    elevated: bool,
) -> Result<UnlockedVault, VaultError> {
    if password.is_empty() {
        return Err(VaultError::EmptyPassword);
    }
    let vault_id: [u8; 16] = random()?;
    let data_key = Zeroizing::new(random::<KEY_LEN>()?);
    let header = wrap_data_key(vault_id, data_key.as_ref(), password, params)?;
    document::write(dir, DocumentChange::Create(&header), elevated)?;
    Ok(UnlockedVault {
        header,
        cipher: cipher_for(data_key.as_ref())?,
    })
}

pub(crate) fn reset_with(dir: &Path, elevated: bool) -> Result<(), VaultError> {
    document::write(dir, DocumentChange::Remove, elevated)
}

impl UnlockedVault {
    /// The vault's id; envelopes carry its first 8 bytes.
    pub fn vault_id(&self) -> [u8; 16] {
        self.header.vault_id
    }

    /// The header this vault was unlocked from (or rewrapped to). Keep it when
    /// locking: it is what a later [`unlock`] verifies against.
    pub fn header(&self) -> &VaultHeader {
        &self.header
    }

    /// Whether the header was written with cheaper parameters than this
    /// binary's defaults; if so, rewrap it with [`Self::change_password`]
    /// using the same password.
    pub fn needs_rehash(&self) -> bool {
        self.header.kdf.is_below_default()
    }

    /// Seal `plaintext` for `purpose` at `context` under a fresh random nonce.
    pub fn seal(
        &self,
        purpose: SecretPurpose,
        context: &SealContext<'_>,
        plaintext: &SecretString,
    ) -> Result<SealedSecret, VaultError> {
        let plaintext = plaintext.expose_secret().as_bytes();
        if plaintext.len() > purpose.max_plaintext() {
            return Err(VaultError::SecretTooLong);
        }
        let aad = context.associated_data(purpose, &self.header.vault_id);
        seal_bytes(
            &self.cipher,
            &self.header.vault_id,
            &random()?,
            &aad,
            plaintext,
        )
    }

    /// Open a value sealed by [`Self::seal`] for the same purpose and context.
    pub fn open(
        &self,
        purpose: SecretPurpose,
        context: &SealContext<'_>,
        sealed: &SealedSecret,
    ) -> Result<SecretString, OpenError> {
        let envelope = Envelope::decode(sealed, purpose.max_plaintext())?;
        if envelope.kid != self.header.vault_id[..KID_LEN] {
            return Err(OpenError::OtherVault);
        }
        let aad = context.associated_data(purpose, &self.header.vault_id);
        let plaintext = open_bytes(&self.cipher, &envelope, &aad)?;
        // Validate before copying, so a failure leaves no unzeroized copy behind.
        let text = std::str::from_utf8(&plaintext).map_err(|_| OpenError::Rejected)?;
        Ok(SecretString::new(text))
    }

    /// Rewrap the same data key under `new_password` with a fresh salt and
    /// the current default parameters, and write it to `dir/vault.json`
    /// (scrubbing `vault.bak`). Items sealed before stay valid.
    ///
    /// `old_password` is checked by unwrapping the data key with it, which is
    /// also how the raw data key is obtained for the rewrap: an unlocked vault
    /// keeps the key only inside its cipher. Refused when the file on disk is
    /// not this vault any more. Costs two Argon2id runs.
    pub fn change_password(
        &mut self,
        dir: &Path,
        old_password: &SecretString,
        new_password: &SecretString,
    ) -> Result<(), VaultError> {
        self.change_password_with(
            dir,
            old_password,
            new_password,
            KdfParams::DEFAULT,
            oneterm_core::elevation::is_restricted(),
        )
    }

    pub(crate) fn change_password_with(
        &mut self,
        dir: &Path,
        old_password: &SecretString,
        new_password: &SecretString,
        params: KdfParams,
        elevated: bool,
    ) -> Result<(), VaultError> {
        if new_password.is_empty() {
            return Err(VaultError::EmptyPassword);
        }
        let data_key = unwrap_data_key(&self.header, old_password)?;
        let header = wrap_data_key(self.header.vault_id, &data_key[..], new_password, params)?;
        document::write(
            dir,
            DocumentChange::Rewrap {
                from: &self.header,
                to: &header,
            },
            elevated,
        )?;
        self.header = header;
        Ok(())
    }
}

/// Derive a key-encryption key under a fresh salt and wrap `data_key` with it.
fn wrap_data_key(
    vault_id: [u8; 16],
    data_key: &[u8],
    password: &SecretString,
    params: KdfParams,
) -> Result<VaultHeader, VaultError> {
    let salt: [u8; SALT_LEN] = random()?;
    let kek = kdf::derive_kek(password, &salt, params)?;
    let aad = envelope::associated_data(KEY_WRAP_PURPOSE, &vault_id, &[]);
    let wrapped_key = seal_bytes(
        &cipher_for(kek.as_ref())?,
        &vault_id,
        &random()?,
        &aad,
        data_key,
    )?;
    Ok(VaultHeader {
        vault_id,
        kdf: params,
        salt: salt.to_vec(),
        wrapped_key,
    })
}

/// The data key `header` wraps, if `password` derives the key that wrapped it.
/// The GCM tag of the wrap is the password check.
fn unwrap_data_key(
    header: &VaultHeader,
    password: &SecretString,
) -> Result<Zeroizing<Vec<u8>>, VaultError> {
    let kek = kdf::derive_kek(password, &header.salt, header.kdf)?;
    let envelope = Envelope::decode(&header.wrapped_key, KEY_LEN)
        .map_err(|_| VaultError::Unsupported("wrapped_key is not a v1 envelope".to_owned()))?;
    let aad = envelope::associated_data(KEY_WRAP_PURPOSE, &header.vault_id, &[]);
    let data_key = open_bytes(&cipher_for(kek.as_ref())?, &envelope, &aad)
        .map_err(|_| VaultError::WrongPassword)?;
    if data_key.len() != KEY_LEN {
        return Err(VaultError::WrongPassword);
    }
    Ok(data_key)
}

fn cipher_for(key: &[u8]) -> Result<Aes256Gcm, VaultError> {
    Aes256Gcm::new_from_slice(key)
        .map_err(|_| VaultError::Unsupported("AES-256 key length".to_owned()))
}

fn random<const N: usize>() -> Result<[u8; N], VaultError> {
    <[u8; N]>::try_generate().map_err(|_| VaultError::Randomness)
}

/// AES-256-GCM-seal `plaintext` into a v1 envelope under an explicit nonce.
fn seal_bytes(
    cipher: &Aes256Gcm,
    vault_id: &[u8; 16],
    nonce: &[u8; NONCE_LEN],
    aad: &[u8],
    plaintext: &[u8],
) -> Result<SealedSecret, VaultError> {
    let sealed = cipher
        .encrypt(
            &Nonce::from(*nonce),
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| VaultError::SecretTooLong)?;
    let mut kid = [0u8; KID_LEN];
    kid.copy_from_slice(&vault_id[..KID_LEN]);
    Ok(Envelope {
        kid,
        nonce: *nonce,
        sealed,
    }
    .encode())
}

/// AES-256-GCM-open a v1 envelope; any authentication failure is
/// [`OpenError::Rejected`].
fn open_bytes(
    cipher: &Aes256Gcm,
    envelope: &Envelope,
    aad: &[u8],
) -> Result<Zeroizing<Vec<u8>>, OpenError> {
    cipher
        .decrypt(
            &Nonce::from(envelope.nonce),
            Payload {
                msg: &envelope.sealed,
                aad,
            },
        )
        .map(Zeroizing::new)
        .map_err(|_| OpenError::Rejected)
}

#[cfg(test)]
#[path = "vault_tests.rs"]
mod vault_tests;
