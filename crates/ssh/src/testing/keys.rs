//! The keys of the test servers and users: made from a seed, written as OpenSSH writes them.

use russh::keys::PrivateKey;
use russh::keys::ssh_key::private::Ed25519Keypair;

/// Writes the server of `seed` on `port` to a known hosts file.
pub fn trust(port: u16, seed: u8, known_hosts: &std::path::Path) {
    russh::keys::known_hosts::learn_known_hosts_path("127.0.0.1", port, key(seed).public_key(), known_hosts)
        .unwrap();
}

/// Writes the user key the server takes (seed 9) to a file, as OpenSSH writes it.
pub fn write_user_key(path: &std::path::Path) {
    let text = key(9).to_openssh(russh::keys::ssh_key::LineEnding::LF).unwrap();
    std::fs::write(path, text.as_bytes()).unwrap();
}

/// As [`write_user_key`], the key sealed by `passphrase` (salt and nonce fixed).
pub fn write_locked_user_key(path: &std::path::Path, passphrase: &str) {
    let locked = key(9).encrypt(&mut Fixed, passphrase).unwrap();
    std::fs::write(path, locked.to_openssh(russh::keys::ssh_key::LineEnding::LF).unwrap().as_bytes())
        .unwrap();
}

/// Salt and nonce for a key a test seals: no randomness wanted.
struct Fixed;

impl russh::keys::ssh_key::rand_core::TryRng for Fixed {
    type Error = std::convert::Infallible;

    fn try_next_u32(&mut self) -> Result<u32, Self::Error> {
        Ok(7)
    }

    fn try_next_u64(&mut self) -> Result<u64, Self::Error> {
        Ok(7)
    }

    fn try_fill_bytes(&mut self, dst: &mut [u8]) -> Result<(), Self::Error> {
        dst.fill(7);
        Ok(())
    }
}

impl russh::keys::ssh_key::rand_core::TryCryptoRng for Fixed {}

pub fn key(seed: u8) -> PrivateKey {
    PrivateKey::from(Ed25519Keypair::from_seed(&[seed; 32]))
}
