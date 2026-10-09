//! OneTerm's master-password vault (`IN-0050`, `DEC-0021`).
//!
//! A master password that only the user knows derives a key-encryption key
//! (Argon2id); that key wraps a random data key (AES-256-GCM); secrets are
//! sealed under the data key into `otv1:` envelopes that live in the document
//! they belong to. `vault.json` holds the salt, the parameters and the wrapped
//! key, and nothing that reveals the password.
//!
//! gpui-free and headless. The only crate in the workspace that depends on the
//! crypto crates. Runtime phase, dialogs and the idle lock live above it, in
//! `oneterm-state` and `oneterm-workspace`.

mod document;
mod envelope;
mod error;
mod kdf;
mod vault;

pub use document::{DOCUMENT_NAME, VaultHeader, VaultStatus, load};
pub use envelope::SealedSecret;
pub use error::{OpenError, VaultError};
pub use kdf::KdfParams;
pub use vault::{SealContext, SecretPurpose, UnlockedVault, init, reset, unlock};

#[cfg(test)]
mod test_support;
