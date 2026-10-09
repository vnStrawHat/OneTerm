//! `vault.json`, schema 1 (`persistence-and-migration.md` § 2-4): reading it
//! into a [`VaultStatus`] and the one function every write goes through.
//!
//! Unlike every other OneTerm document it is **never quarantined and never
//! replaced by a default**: a default would be "no vault", and the next set-up
//! would orphan every sealed value. Only Reset removes it.

use std::fmt;
use std::io;
use std::path::Path;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use oneterm_core::{AppError, remove_document, schema_version, update_secret_json_file};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::envelope::{Envelope, KID_LEN, TAG_LEN};
use crate::kdf::{ALGORITHM_NAME, ALGORITHM_VERSION, KEY_LEN, KdfParams, SALT_LEN_RANGE};
use crate::{OpenError, SealedSecret, VaultError};

/// File name of the vault header in `oneterm_core::config_dir()`.
pub const DOCUMENT_NAME: &str = "vault.json";
const SCHEMA_VERSION: u32 = 1;

/// The vault header: everything `vault.json` holds. Nothing in it reveals the
/// master password; `Debug` leaves the wrapped key out.
#[derive(Clone)]
pub struct VaultHeader {
    pub(crate) vault_id: [u8; 16],
    pub(crate) kdf: KdfParams,
    pub(crate) salt: Vec<u8>,
    pub(crate) wrapped_key: SealedSecret,
}

impl fmt::Debug for VaultHeader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VaultHeader")
            .field("vault_id", &hex(&self.vault_id))
            .field("kdf", &self.kdf)
            .finish_non_exhaustive()
    }
}

impl VaultHeader {
    /// The id of the vault generation this header describes; a set-up after
    /// a reset gets a new one. Comparing it with a fresh [`load`] tells a
    /// caller the vault was reset or replaced elsewhere.
    pub fn vault_id(&self) -> [u8; 16] {
        self.vault_id
    }

    fn to_document(&self) -> Value {
        json!({
            "schema_version": SCHEMA_VERSION,
            "vault_id": hex(&self.vault_id),
            "kdf": self.kdf_fields(),
            "wrapped_key": self.wrapped_key,
        })
    }

    fn kdf_fields(&self) -> Value {
        json!({
            "algorithm": ALGORITHM_NAME,
            "version": ALGORITHM_VERSION,
            "memory_kib": self.kdf.memory_kib,
            "iterations": self.kdf.iterations,
            "parallelism": self.kdf.parallelism,
            "salt": STANDARD.encode(&self.salt),
        })
    }
}

/// What `vault.json` says about the vault (`persistence-and-migration.md` § 3).
#[derive(Debug)]
pub enum VaultStatus {
    /// No `vault.json`: no master password has been set up.
    Uninitialised,
    /// A valid schema-1 header.
    Ready(VaultHeader),
    /// Newer schema, unknown algorithm, or parameters outside the accepted
    /// bounds: nothing is attempted with it.
    Unsupported(String),
    /// Present but not parseable. Never quarantined, never overwritten; only
    /// Reset removes it.
    Damaged(String),
}

/// Read `dir/vault.json`.
///
/// A missing file is [`VaultStatus::Uninitialised`]; a read failure other than
/// "not found" is [`AppError::ConfigLoad`] and leaves the file untouched.
/// Nothing is ever written by a load.
pub fn load(dir: &Path) -> Result<VaultStatus, AppError> {
    match std::fs::read_to_string(dir.join(DOCUMENT_NAME)) {
        Ok(raw) => Ok(parse(&raw)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(VaultStatus::Uninitialised),
        Err(error) => Err(AppError::config_load(DOCUMENT_NAME, error)),
    }
}

/// The fields this binary reads. Unknown fields are ignored (and preserved by
/// the writers, which touch only their own fields).
#[derive(Deserialize)]
struct HeaderFields {
    vault_id: String,
    kdf: KdfFields,
    wrapped_key: SealedSecret,
}

#[derive(Deserialize)]
struct KdfFields {
    algorithm: String,
    version: u32,
    memory_kib: u32,
    iterations: u32,
    parallelism: u32,
    salt: String,
}

fn parse(raw: &str) -> VaultStatus {
    match serde_json::from_str::<Value>(raw) {
        Ok(value) => parse_value(value),
        Err(_) => VaultStatus::Damaged("not JSON".to_owned()),
    }
}

/// The load rules on an already parsed document; also what a rewrap checks the
/// file against, inside the lock, so it never writes over anything `load`
/// would not call `Ready`.
fn parse_value(value: Value) -> VaultStatus {
    let damaged = |reason: &str| VaultStatus::Damaged(reason.to_owned());

    match schema_version(&value) {
        Ok(SCHEMA_VERSION) => {}
        Ok(0) => return damaged("no schema_version"),
        Ok(version) => {
            return VaultStatus::Unsupported(format!(
                "schema version {version} is newer than supported version {SCHEMA_VERSION}"
            ));
        }
        Err(_) => return damaged("schema_version is not an integer"),
    }
    let Ok(fields) = serde_json::from_value::<HeaderFields>(value) else {
        return damaged("missing or mistyped fields");
    };

    let kdf = &fields.kdf;
    if kdf.algorithm != ALGORITHM_NAME || kdf.version != ALGORITHM_VERSION {
        return VaultStatus::Unsupported(format!(
            "key derivation {} version {}",
            kdf.algorithm, kdf.version
        ));
    }
    let params = KdfParams {
        memory_kib: kdf.memory_kib,
        iterations: kdf.iterations,
        parallelism: kdf.parallelism,
    };
    if !params.is_within_bounds() {
        return VaultStatus::Unsupported(format!(
            "Argon2id parameters outside the accepted bounds: {params:?}"
        ));
    }
    let Some(vault_id) = parse_hex_id(&fields.vault_id) else {
        return damaged("vault_id is not 32 lowercase hex digits");
    };
    let Ok(salt) = STANDARD.decode(&kdf.salt) else {
        return damaged("salt is not base64");
    };
    if !SALT_LEN_RANGE.contains(&salt.len()) {
        return VaultStatus::Unsupported(format!("salt of {} bytes", salt.len()));
    }
    match Envelope::decode(&fields.wrapped_key, KEY_LEN) {
        Ok(envelope) if envelope.kid != vault_id[..KID_LEN] => {
            return damaged("wrapped_key belongs to another vault");
        }
        Ok(envelope) if envelope.sealed.len() != KEY_LEN + TAG_LEN => {
            return damaged("wrapped_key has the wrong length");
        }
        Ok(_) => {}
        Err(OpenError::NewerFormat) => {
            return VaultStatus::Unsupported("wrapped_key uses a newer envelope".to_owned());
        }
        Err(_) => return damaged("wrapped_key is not an envelope"),
    }
    VaultStatus::Ready(VaultHeader {
        vault_id,
        kdf: params,
        salt,
        wrapped_key: fields.wrapped_key,
    })
}

/// The changes a process can make to `vault.json`.
pub(crate) enum DocumentChange<'a> {
    /// Set-up: create the document; refused when any file is there already,
    /// whatever it holds.
    Create(&'a VaultHeader),
    /// Change password or rehash: replace the KDF fields and the wrapped key.
    /// Refused unless the file loads as `Ready` and is still exactly the
    /// header this process unlocked from (`from`): same id, same salt, same
    /// wrapped key.
    Rewrap {
        from: &'a VaultHeader,
        to: &'a VaultHeader,
    },
    /// Reset: remove `vault.json` and `vault.bak`. Refused when the file loads
    /// as `Unsupported` (written by a newer OneTerm); `Ready` and `Damaged`
    /// files may be reset.
    Remove,
}

/// The one function every `vault.json` write goes through, so the elevated
/// refusal covers every writer, present and future (`persistence.md`,
/// elevation rule 3).
///
/// `elevated` is a parameter rather than a read of the process global so the
/// refusal is testable without flipping process state under the other tests.
/// Writes use [`update_secret_json_file`]: `vault.bak` always equals the new
/// document, so no old wrapped key survives a password change, and the files
/// are owner-only on Unix.
pub(crate) fn write(
    dir: &Path,
    change: DocumentChange<'_>,
    elevated: bool,
) -> Result<(), VaultError> {
    if elevated {
        return Err(VaultError::ReadOnly);
    }
    let path = dir.join(DOCUMENT_NAME);
    let mut refusal = None;
    let result = match change {
        DocumentChange::Remove => {
            // The status is read outside the lock `remove_document` takes: a file
            // turning Unsupported in between would be a newer OneTerm's set-up
            // racing this reset, and only a remove that takes a check inside the
            // lock would close that window.
            match load(dir) {
                Ok(VaultStatus::Unsupported(reason)) => {
                    return Err(VaultError::Unsupported(reason));
                }
                Err(error) => return Err(VaultError::Io(error)),
                Ok(_) => remove_document(&path),
            }
        }
        DocumentChange::Create(header) => {
            // A file that is there already is refused before anything parses it
            // (an unreadable one would otherwise surface as a JSON error), and
            // again inside the lock, where the transaction sees a `{}` file and a
            // missing one alike. A directory in its place is an I/O failure.
            if path.is_file() {
                return Err(VaultError::AlreadyInitialised);
            }
            update_secret_json_file(&path, |document| {
                if path.is_file() {
                    return refuse(&mut refusal, VaultError::AlreadyInitialised);
                }
                *document = header.to_document();
                Ok(())
            })
        }
        DocumentChange::Rewrap { from, to: header } => update_secret_json_file(&path, |document| {
            let on_disk = match parse_value(document.clone()) {
                VaultStatus::Ready(on_disk) if on_disk.vault_id == from.vault_id => on_disk,
                // Missing (reads as `{}`), another vault, or anything this binary
                // cannot read: not the vault this process unlocked, never touched.
                _ => return refuse(&mut refusal, VaultError::VaultReplaced),
            };
            if on_disk.salt != from.salt || on_disk.wrapped_key != from.wrapped_key {
                return refuse(&mut refusal, VaultError::ChangedElsewhere);
            }
            let fields = header.kdf_fields();
            match document.get_mut("kdf").and_then(Value::as_object_mut) {
                Some(kdf) => {
                    if let Value::Object(new_fields) = fields {
                        kdf.extend(new_fields);
                    }
                }
                None => document["kdf"] = fields,
            }
            document["wrapped_key"] = json!(header.wrapped_key);
            Ok(())
        }),
    };
    match (result, refusal) {
        (_, Some(refusal)) => Err(refusal),
        (Ok(()), None) => Ok(()),
        (Err(error), None) => Err(error.into()),
    }
}

/// Record why a transaction refused and abort it without writing.
fn refuse(slot: &mut Option<VaultError>, reason: VaultError) -> io::Result<()> {
    *slot = Some(reason);
    Err(io::Error::other("vault.json transaction refused"))
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn parse_hex_id(text: &str) -> Option<[u8; 16]> {
    if text.len() != 32
        || !text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return None;
    }
    let mut id = [0u8; 16];
    for (index, byte) in id.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[index * 2..index * 2 + 2], 16).ok()?;
    }
    Some(id)
}

#[cfg(test)]
#[path = "document_tests.rs"]
mod document_tests;
