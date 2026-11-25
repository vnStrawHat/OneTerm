//! Saved passwords (specs/oneterm, "Saved passwords"): a data key sealed under the master
//! password in a vault's document, each secret sealed under the data key in an envelope, both
//! as OneTerm 0.6 writes them (`format.rs`). The data key is in memory only, once the master
//! password was typed; the master password itself is kept nowhere: it is the one that opens
//! the vault. Nothing here knows the app: where the document and the envelopes are kept, and
//! what asks for the master password, are the app's.

mod format;

pub use format::Header;

use std::rc::Rc;

use quadra_shell::Zeroizing;

/// A key of 32 bytes, wiped when dropped.
type Key = Zeroizing<[u8; 32]>;

/// A vault opened: which one, and its data key.
#[derive(Clone, PartialEq, Eq)]
pub struct Opened {
    id: [u8; 16],
    key: Key,
}

/// The saved passwords' data key in memory, shared; wiped when the last holder drops it.
pub type DataKey = Rc<Opened>;

/// A secret in memory, wiped when dropped, never shown by `Debug`.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(Zeroizing<String>);

impl Secret {
    pub fn new(text: String) -> Secret {
        Secret(Zeroizing::new(text))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(..)")
    }
}

/// The key a master password derives for a vault: Argon2id (version 19) at the vault's cost,
/// its memory wiped after.
fn derive(password: &str, salt: &[u8], cost: [u32; 3]) -> Option<Key> {
    let params = argon2::Params::new(cost[0], cost[1], cost[2], Some(32)).ok()?;
    let mut blocks = Zeroizing::new(Vec::new());
    blocks.try_reserve_exact(params.block_count()).ok()?;
    blocks.resize(params.block_count(), argon2::Block::default());
    let mut key = Zeroizing::new([0u8; 32]);
    argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params)
        .hash_password_into_with_memory(password.as_bytes(), salt, key.as_mut(), blocks.as_mut_slice())
        .ok()?;
    Some(key)
}

/// A new vault of `password`: its document, and the vault opened.
pub fn make(password: &str) -> Option<(String, Opened)> {
    let opened = Opened { id: random_bytes()?, key: random_key()? };
    Some((reseal(&opened, password)?, opened))
}

/// The document of a vault under another master password: the same vault and data key, a new
/// salt, the cost of today.
pub fn reseal(opened: &Opened, password: &str) -> Option<String> {
    let (salt, cost) = (random_bytes::<16>()?, format::COST);
    let kek = derive(password, &salt, cost)?;
    let sealed = seal(&kek, &*opened.key, &format::bound("key-wrap", &opened.id, &[]))?;
    let wrapped = format::envelope(&opened.id, &sealed);
    Some(Header { id: opened.id, cost, salt: salt.to_vec(), wrapped }.write())
}

/// What `password` opens of a vault's document: nothing when it is not the vault's. On the
/// pool: it derives.
pub fn unseal(password: &str, vault: &str) -> Option<Opened> {
    let header = Header::parse(vault)?;
    let kek = derive(password, &header.salt, header.cost)?;
    let sealed = format::sealed(&header.wrapped, &header.id)?;
    let plain = open(&kek, &sealed, &format::bound("key-wrap", &header.id, &[]))?;
    let mut key = Zeroizing::new([0u8; 32]);
    (plain.len() == 32).then(|| key.copy_from_slice(&plain))?;
    Some(Opened { id: header.id, key })
}

/// What a secret is bound to: the vault and the target it is for, so it opens for no other.
fn bound_to(vault: &Opened, host: &str, port: u16, user: &str) -> Vec<u8> {
    let fields = [host.as_bytes(), &port.to_be_bytes(), user.as_bytes()];
    format::bound("ssh-password", &vault.id, &fields)
}

/// A secret sealed under the data key for a user at a host and port, as an envelope.
pub fn seal_secret(vault: &Opened, plain: &str, host: &str, port: u16, user: &str) -> Option<String> {
    let sealed = seal(&vault.key, plain.as_bytes(), &bound_to(vault, host, port, user))?;
    Some(format::envelope(&vault.id, &sealed))
}

/// A secret opened, if it was sealed by this vault for that user at that host and port.
pub fn open_secret(vault: &Opened, sealed: &str, host: &str, port: u16, user: &str) -> Option<Secret> {
    let bytes = open(&vault.key, &format::sealed(sealed, &vault.id)?, &bound_to(vault, host, port, user))?;
    std::str::from_utf8(&bytes).ok().map(|it| Secret(Zeroizing::new(it.to_string())))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len()).step_by(2).map(|at| u8::from_str_radix(text.get(at..at + 2)?, 16).ok()).collect()
}

#[cfg(windows)]
use quadra_shell::{open, random_bytes, random_key, seal};

#[cfg(not(windows))]
fn random_bytes<const N: usize>() -> Option<[u8; N]> {
    None
}
#[cfg(not(windows))]
fn random_key() -> Option<Key> {
    None
}
#[cfg(not(windows))]
fn seal(_: &[u8; 32], _: &[u8], _: &[u8]) -> Option<Vec<u8>> {
    None
}
#[cfg(not(windows))]
fn open(_: &[u8; 32], _: &[u8], _: &[u8]) -> Option<Zeroizing<Vec<u8>>> {
    None
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    /// OneTerm 0.6's own fixture of a vault and a password sealed under it, made by an
    /// implementation that is neither OneTerm 0.6's nor this one: what it pins is the format.
    pub(crate) const ONETERM_S: &str = include_str!("../tests/fixtures/vault.json");
    const SEALED: &str = "otv1:P5wKTosdTCdcXFxcXFxcXFxcXFxbpgEB6ylBEMijZRhddr55R0kOe/uq";

    #[test]
    fn what_oneterm_sealed_opens_with_its_master_password() {
        assert!(unseal("wrong", ONETERM_S).is_none());
        let vault = unseal("correct horse battery staple", ONETERM_S).expect("the vault opened");
        let opened = open_secret(&vault, SEALED, "10.0.0.1", 22, "deploy");
        assert_eq!(opened.as_ref().map(Secret::as_str), Some("s3cret"));
        assert_eq!(open_secret(&vault, SEALED, "10.0.0.2", 22, "deploy"), None, "another target");
        assert_eq!(open_secret(&vault, SEALED, "10.0.0.1", 2222, "deploy"), None);
    }

    #[test]
    fn a_vault_opens_with_its_password_and_seals_secrets() {
        let (document, vault) = make("master").unwrap();
        assert!(unseal("wrong", &document).is_none() && !document.contains("master"));
        assert!(unseal("master", &document) == Some(vault.clone()), "the same key again");
        let sealed = seal_secret(&vault, "hunter2", "box", 22, "me").unwrap();
        assert!(sealed.starts_with("otv1:") && !sealed.contains("hunter2"));
        assert_eq!(
            open_secret(&vault, &sealed, "box", 22, "me").as_ref().map(Secret::as_str),
            Some("hunter2")
        );
        assert_eq!(open_secret(&vault, &sealed, "evil", 2222, "me"), None, "another target");
        let other = make("master").unwrap().1;
        assert_eq!(open_secret(&other, &sealed, "box", 22, "me"), None, "another vault");
        // A new master password seals the same vault and key.
        let moved = reseal(&vault, "other").unwrap();
        assert!(unseal("other", &moved) == Some(vault) && unseal("master", &moved).is_none());
    }
}
