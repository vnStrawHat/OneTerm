//! Private-key file parsing: one round trip per key algorithm OneTerm supports.
//!
//! `load_private_key` is the only place OneTerm reads a key from disk, and the
//! whole decode path below it (`ssh-key` -> `ecdsa` / `ed25519-dalek` / `rsa` /
//! `pkcs1` / `pkcs8` / `sec1` / `spki` / `der`) moved in `IN-0036`. Before that
//! bump nothing here was covered directly: the loopback SSH suites all generate
//! their keys in memory and never touch a file.

use super::*;

/// A throwaway 2048-bit RSA key in OpenSSH format, written by `ssh-keygen`.
///
/// RSA is a fixture rather than a generated key because RSA key generation is
/// far too slow for a unit test in a debug build (the same reason
/// `handler_tests.rs` keeps a fixed RSA public key). Using real `ssh-keygen`
/// output also makes this the one case that proves interoperability with
/// OpenSSH's writer rather than a russh round trip.
///
/// It guards nothing: it exists only inside this test.
const RSA_2048_OPENSSH: &str = "\
-----BEGIN OPENSSH PRIVATE KEY-----
b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAABFwAAAAdzc2gtcn
NhAAAAAwEAAQAAAQEA0KjRE4L615cP1aCVYQ0XQ0JnZoGSFHEt1K1qRnwZJ2Y9HRwpLpJd
2FDpe4gLODbDMC6EFuUvMRqz/n0Bb6JMCPTGnDPQrlefAh3smPtdbOICSMIdSyVu4qfoRr
a+6J/FpzPHlQlVRFd+WmczrPXMrK1nJorAxxCf2CAVkEmsSeHhR+L6cwoG4beDKcrCEdgJ
6uZAdJUkoX6kLmqLbEd5041ZcgaDjpQJF2K+39kRscgLYKgT3UdQ3yAK7ByYfW88NFzzIL
9WQ4nQ4Pp8RsPiI3oWaMPoqhs2insurX5G+vzuWhqr8pnPjRlhz3LFbyOT6qDDVyU8mzQp
28iiBOBjqwAAA8hrmam6a5mpugAAAAdzc2gtcnNhAAABAQDQqNETgvrXlw/VoJVhDRdDQm
dmgZIUcS3UrWpGfBknZj0dHCkukl3YUOl7iAs4NsMwLoQW5S8xGrP+fQFvokwI9MacM9Cu
V58CHeyY+11s4gJIwh1LJW7ip+hGtr7on8WnM8eVCVVEV35aZzOs9cysrWcmisDHEJ/YIB
WQSaxJ4eFH4vpzCgbht4MpysIR2Anq5kB0lSShfqQuaotsR3nTjVlyBoOOlAkXYr7f2RGx
yAtgqBPdR1DfIArsHJh9bzw0XPMgv1ZDidDg+nxGw+IjehZow+iqGzaKey6tfkb6/O5aGq
vymc+NGWHPcsVvI5PqoMNXJTybNCnbyKIE4GOrAAAAAwEAAQAAAQAGQzrbOhArTlZkVAiH
vCvZkfGmivcGdAsrGfVZnjnnC9ODvyehRTVZ27vWQFQN4N7k4FCIm2JaN/H1Dm1vm1Bq6G
XZpFh8ExcrqhhC0zCPpwzogCL+8WWtmdqH3M5IDxuQlCZGW9xaS8H4FqbfZxU4jY/OAVYd
42rYwsXC6eMo6HfJQi7+JPhwEKpaGJ4K6i94yfKm7H2SczUDugXtJy7FNdeljGxsJF8eq8
l5B7nkUP/hK9BEWwJKtQKgm2FK+oC+MsownsAAbnb5b9QKbRMKzSPhszECRkRg3rPinuwY
vSNPOQ/SRI0jAUjezKVxYVssL6QyHGRJPpy85QLbY7yRAAAAgFNxXdtbKeA8KRJ4bQHAoR
2xTJclMDJt4Bppf44th6+nqObaT2o0yAD6SayX/mCMOreO8r3FfsxR6cU4Z+lasscfusR7
4IXaDh9rr+srNSrbfEm9MVsCzMtx+8mO79b+IlANWUBSiDsbkhX8WjV4U9OyBBe40785hy
czctvcXnd6AAAAgQDrRifWtkF5FBGwR+P8aPKPyAWh+yCxHEZJzatFBO/D9kXKFz7M+DbV
ze6ZbrVeFmvIHmIpXW8qYJtIRM/cc5bn7y4UlYKKBF+C+8/TZb3BJYp0MClbX11VVh39Kn
DeoqX1RLU+iTU148OVMv9MJeAfEokt3jglxv/x4I5i8kdMOQAAAIEA4wp0b9S8Pon1KM+N
/lWz1zHrfnmRBOcep029oNVNkaHaQTccUBZ0uRKsezufe2CtlFGKUQiX50eOm6i6+4D03i
Kf6rlymRM+r0IeGnbwVVPPRKp5JFUeGpTsQd5PjyxRwsPgJpGPXbijQNJlIEMK5oW7pmx8
VG+c/VlbVg58dwMAAAAMb25ldGVybS10ZXN0AQIDBAUGBw==
-----END OPENSSH PRIVATE KEY-----
";

/// A temp key file that is removed on drop, including when the test panics
/// halfway (ERR-15).
struct TempKeyFile(std::path::PathBuf);

impl Drop for TempKeyFile {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_file(&self.0) {
            if error.kind() != std::io::ErrorKind::NotFound {
                eprintln!("failed to remove {}: {error}", self.0.display());
            }
        }
    }
}

fn temp_key_file(contents: &str) -> TempKeyFile {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);

    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let path =
        std::env::temp_dir().join(format!("oneterm-keyfile-{}-{sequence}", std::process::id()));
    std::fs::write(&path, contents).expect("write temp key");
    TempKeyFile(path)
}

/// Write `key` in OpenSSH format and read it back through `load_private_key`.
fn round_trip(key: &PrivateKey, passphrase: Option<&str>) -> PrivateKey {
    let encoded = key
        .to_openssh(russh::keys::ssh_key::LineEnding::LF)
        .expect("encode OpenSSH private key");
    let file = temp_key_file(&encoded);
    load_private_key(&file.0, passphrase).expect("load the key just written")
}

/// Every algorithm OneTerm can be handed a key for survives a write/read trip
/// with its public half intact.
#[test]
fn every_supported_key_algorithm_round_trips_through_a_file() {
    use russh::keys::{Algorithm, EcdsaCurve};

    let algorithms = [
        Algorithm::Ed25519,
        Algorithm::Ecdsa {
            curve: EcdsaCurve::NistP256,
        },
        Algorithm::Ecdsa {
            curve: EcdsaCurve::NistP384,
        },
        Algorithm::Ecdsa {
            curve: EcdsaCurve::NistP521,
        },
    ];

    for algorithm in algorithms {
        let key = PrivateKey::random(&mut rand::rng(), algorithm.clone())
            .unwrap_or_else(|error| panic!("generate {algorithm} key: {error}"));
        let loaded = round_trip(&key, None);
        assert_eq!(
            loaded.public_key(),
            key.public_key(),
            "{algorithm} key did not survive the file round trip"
        );
    }
}

/// RSA is the algorithm whose decode path still runs through release-candidate
/// crates (`rsa`, `pkcs1`), so it gets its own check against real `ssh-keygen`
/// output.
#[test]
fn an_openssh_written_rsa_key_loads() {
    let file = temp_key_file(RSA_2048_OPENSSH);
    let loaded = load_private_key(&file.0, None).expect("load the ssh-keygen RSA key");

    assert!(
        loaded.public_key().algorithm().is_rsa(),
        "expected an RSA key, got {}",
        loaded.public_key().algorithm()
    );
    // Re-encoding and reloading must land on the same public key, which proves
    // the decode was complete and not merely non-erroring.
    let again = round_trip(&loaded, None);
    assert_eq!(again.public_key(), loaded.public_key());
}

/// A passphrase-protected key loads with the right passphrase.
#[test]
fn an_encrypted_key_loads_with_its_passphrase() {
    let key = PrivateKey::random(&mut rand::rng(), russh::keys::Algorithm::Ed25519)
        .expect("generate ed25519 key");
    let encrypted = key
        .encrypt(&mut rand::rng(), "correct horse")
        .expect("encrypt the key");
    assert!(encrypted.is_encrypted(), "the fixture must be encrypted");

    let loaded = round_trip(&encrypted, Some("correct horse"));
    assert_eq!(loaded.public_key(), key.public_key());
}

/// The wrong passphrase is an error, never a panic and never a silent success.
#[test]
fn an_encrypted_key_rejects_the_wrong_passphrase() {
    let key = PrivateKey::random(&mut rand::rng(), russh::keys::Algorithm::Ed25519)
        .expect("generate ed25519 key");
    let encrypted = key
        .encrypt(&mut rand::rng(), "correct horse")
        .expect("encrypt the key");
    let encoded = encrypted
        .to_openssh(russh::keys::ssh_key::LineEnding::LF)
        .expect("encode the encrypted key");
    let file = temp_key_file(&encoded);

    let error = load_private_key(&file.0, Some("wrong"))
        .expect_err("a wrong passphrase must not load the key");
    // The message must name the file so the connect dialog can tell the user
    // which key failed.
    assert!(
        error.to_string().contains("failed to load key"),
        "unexpected error text: {error}"
    );
}

/// A passphrase-protected key without a passphrase is an error, not a load of
/// the still-encrypted key material.
#[test]
fn an_encrypted_key_without_a_passphrase_is_refused() {
    let key = PrivateKey::random(&mut rand::rng(), russh::keys::Algorithm::Ed25519)
        .expect("generate ed25519 key");
    let encrypted = key
        .encrypt(&mut rand::rng(), "correct horse")
        .expect("encrypt the key");
    let encoded = encrypted
        .to_openssh(russh::keys::ssh_key::LineEnding::LF)
        .expect("encode the encrypted key");
    let file = temp_key_file(&encoded);

    assert!(
        load_private_key(&file.0, None).is_err(),
        "an encrypted key must not load without its passphrase"
    );
}
