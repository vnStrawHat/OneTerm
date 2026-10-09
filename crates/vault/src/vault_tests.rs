//! Tests for `vault.rs`: lifecycle, sealing, tamper resistance, elevation and
//! the cost contract.

use std::fs;
use std::time::{Duration, Instant};

use super::*;
use crate::test_support::{FAST_PARAMS, test_dir};
use crate::{DOCUMENT_NAME, VaultStatus, load};

fn secret(text: &str) -> SecretString {
    SecretString::new(text)
}

const CONTEXT: SealContext<'static> = SealContext {
    host: "10.0.0.1",
    port: 22,
    username: "deploy",
};

fn fast_vault(dir: &Path, password: &str) -> UnlockedVault {
    init_with(dir, &secret(password), FAST_PARAMS, false).unwrap()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn unhex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&text[index..index + 2], 16).unwrap())
        .collect()
}

/// An unlocked vault with a fixed data key and id, no file: the test-only
/// constructor behind the known-answer tests.
fn fixed_vault(data_key: &[u8], vault_id: [u8; 16], kdf: KdfParams) -> UnlockedVault {
    let envelope = Envelope {
        kid: [0; KID_LEN],
        nonce: [0; NONCE_LEN],
        sealed: Vec::new(),
    };
    UnlockedVault {
        header: VaultHeader {
            vault_id,
            kdf,
            salt: vec![0; SALT_LEN],
            wrapped_key: envelope.encode(),
        },
        cipher: cipher_for(data_key).unwrap(),
    }
}

/// NIST CAVP `gcmEncryptExtIV256.rsp` (Keylen 256, IVlen 96, PTlen 128,
/// AADlen 128, Taglen 128) through OneTerm's own seal / open internals.
#[test]
fn nist_aes_256_gcm_vector_through_seal_and_open() {
    let cipher = cipher_for(&unhex(
        "92e11dcdaa866f5ce790fd24501f92509aacf4cb8b1339d50c9c1240935dd08b",
    ))
    .unwrap();
    let nonce: [u8; NONCE_LEN] = unhex("ac93a1a6145299bde902f21a").try_into().unwrap();
    let aad = unhex("1e0889016f67601c8ebea4943bc23ad6");
    let plaintext = unhex("2d71bcfa914e4ac045b2aa60955fad24");

    let sealed = seal_bytes(&cipher, &[0x5a; 16], &nonce, &aad, &plaintext).unwrap();
    let envelope = Envelope::decode(&sealed, 1024).unwrap();
    assert_eq!(envelope.kid, [0x5a; KID_LEN]);
    assert_eq!(envelope.nonce, nonce);
    assert_eq!(
        hex(&envelope.sealed),
        "8995ae2e6df3dbf96fac7b7137bae67feca5aa77d51d4a0a14d9c51e1da474ab"
    );
    assert_eq!(*open_bytes(&cipher, &envelope, &aad).unwrap(), plaintext);
}

/// The whole `otv1:` layout for an SSH password, frozen: a change to the
/// associated data, the field order or the encoding fails here before it
/// orphans a saved password.
#[test]
fn ssh_password_envelope_layout_is_frozen() {
    let vault = fixed_vault(&[0x11; 32], [0x22; 16], KdfParams::DEFAULT);
    let aad = CONTEXT.associated_data(SecretPurpose::SshPassword, &vault.header.vault_id);
    let sealed = seal_bytes(
        &vault.cipher,
        &[0x22; 16],
        &[0x33; NONCE_LEN],
        &aad,
        b"hunter2",
    )
    .unwrap();
    assert_eq!(
        format!("{}", serde_json::json!(sealed)),
        FROZEN_ENVELOPE_JSON
    );
    let opened = vault
        .open(SecretPurpose::SshPassword, &CONTEXT, &sealed)
        .unwrap();
    assert_eq!(opened.expose_secret(), "hunter2");
}

/// Cross-checked when frozen against an independent implementation (Python
/// `cryptography` 44 `AESGCM`, the same key, nonce and associated data).
const FROZEN_ENVELOPE_JSON: &str =
    "\"otv1:IiIiIiIiIiIzMzMzMzMzMzMzMzPs9jwyr+BHOpGnRuzAuJsLqjg/AzEKSw==\"";

#[test]
fn seal_then_open_round_trips_and_uses_a_fresh_nonce_each_time() {
    let dir = test_dir();
    let vault = fast_vault(&dir, "master password");
    let password = secret("p@ss wörd ");
    let first = vault
        .seal(SecretPurpose::SshPassword, &CONTEXT, &password)
        .unwrap();
    let second = vault
        .seal(SecretPurpose::SshPassword, &CONTEXT, &password)
        .unwrap();
    assert_ne!(first, second);
    for sealed in [&first, &second] {
        let opened = vault
            .open(SecretPurpose::SshPassword, &CONTEXT, sealed)
            .unwrap();
        assert_eq!(opened.expose_secret(), "p@ss wörd ");
    }
}

#[test]
fn seal_refuses_a_secret_longer_than_its_purpose_allows() {
    let dir = test_dir();
    let vault = fast_vault(&dir, "master password");
    let longest = secret(&"x".repeat(1024));
    let sealed = vault
        .seal(SecretPurpose::SshPassword, &CONTEXT, &longest)
        .unwrap();
    assert!(
        vault
            .open(SecretPurpose::SshPassword, &CONTEXT, &sealed)
            .is_ok()
    );
    assert!(matches!(
        vault.seal(
            SecretPurpose::SshPassword,
            &CONTEXT,
            &secret(&"x".repeat(1025))
        ),
        Err(VaultError::SecretTooLong)
    ));
}

#[test]
fn unlock_accepts_the_right_password_and_rejects_a_wrong_or_empty_one() {
    let dir = test_dir();
    let vault = fast_vault(&dir, "master password");
    let sealed = vault
        .seal(SecretPurpose::SshPassword, &CONTEXT, &secret("pw"))
        .unwrap();
    let header = vault.header().clone();
    drop(vault);

    let reopened = unlock(&header, &secret("master password")).unwrap();
    let opened = reopened
        .open(SecretPurpose::SshPassword, &CONTEXT, &sealed)
        .unwrap();
    assert_eq!(opened.expose_secret(), "pw");
    // Never trimmed: a trailing space is a different password.
    for wrong in ["master password ", "Master password", "x"] {
        assert!(
            matches!(
                unlock(&header, &secret(wrong)),
                Err(VaultError::WrongPassword)
            ),
            "{wrong:?}"
        );
    }
    assert!(matches!(
        unlock(&header, &secret("")),
        Err(VaultError::EmptyPassword)
    ));
    assert!(matches!(
        init_with(&dir, &secret(""), FAST_PARAMS, false),
        Err(VaultError::EmptyPassword)
    ));
}

/// Flip one byte of each part of the payload; each fails with its own error.
#[test]
fn tampered_envelopes_never_open() {
    let dir = test_dir();
    let vault = fast_vault(&dir, "master password");
    let sealed = vault
        .seal(SecretPurpose::SshPassword, &CONTEXT, &secret("pw"))
        .unwrap();
    let envelope = Envelope::decode(&sealed, 1024).unwrap();
    let ciphertext_len = envelope.sealed.len() - envelope::TAG_LEN;

    let flip = |part: &str, index: usize| {
        let mut changed = Envelope {
            kid: envelope.kid,
            nonce: envelope.nonce,
            sealed: envelope.sealed.clone(),
        };
        match part {
            "kid" => changed.kid[index] ^= 1,
            "nonce" => changed.nonce[index] ^= 1,
            _ => changed.sealed[index] ^= 1,
        }
        vault.open(SecretPurpose::SshPassword, &CONTEXT, &changed.encode())
    };
    assert_eq!(flip("kid", 0).err(), Some(OpenError::OtherVault));
    assert_eq!(flip("nonce", 5).err(), Some(OpenError::Rejected));
    assert_eq!(flip("ciphertext", 0).err(), Some(OpenError::Rejected));
    assert_eq!(
        flip("tag", ciphertext_len + 3).err(),
        Some(OpenError::Rejected)
    );
}

#[test]
fn a_sealed_password_opens_only_for_its_own_address_and_vault() {
    let dir = test_dir();
    let vault = fast_vault(&dir, "master password");
    let sealed = vault
        .seal(SecretPurpose::SshPassword, &CONTEXT, &secret("pw"))
        .unwrap();
    let elsewhere = [
        SealContext {
            host: "10.0.0.2",
            ..CONTEXT
        },
        SealContext {
            port: 2222,
            ..CONTEXT
        },
        SealContext {
            username: "root",
            ..CONTEXT
        },
        // Field boundaries are length-prefixed, so moving bytes between fields
        // does not collide.
        SealContext {
            host: "10.0.0.1d",
            username: "eploy",
            ..CONTEXT
        },
    ];
    for context in elsewhere {
        assert_eq!(
            vault
                .open(SecretPurpose::SshPassword, &context, &sealed)
                .err(),
            Some(OpenError::Rejected),
            "{context:?}"
        );
    }

    let other_dir = test_dir();
    let other = fast_vault(&other_dir, "master password");
    assert_eq!(
        other
            .open(SecretPurpose::SshPassword, &CONTEXT, &sealed)
            .err(),
        Some(OpenError::OtherVault)
    );
}

#[test]
fn newer_and_plain_text_values_are_reported_never_used() {
    let dir = test_dir();
    let vault = fast_vault(&dir, "master password");
    let value = |text: &str| -> SealedSecret { serde_json::from_value(text.into()).unwrap() };
    assert_eq!(
        vault
            .open(SecretPurpose::SshPassword, &CONTEXT, &value("otv2:AAAA"))
            .err(),
        Some(OpenError::NewerFormat)
    );
    assert_eq!(
        vault
            .open(SecretPurpose::SshPassword, &CONTEXT, &value("hunter2"))
            .err(),
        Some(OpenError::Malformed)
    );
}

#[test]
fn set_up_writes_a_header_that_loads_and_never_holds_the_password() {
    let dir = test_dir();
    let vault = fast_vault(&dir, "master password");
    let raw = fs::read_to_string(dir.join(DOCUMENT_NAME)).unwrap();
    assert!(!raw.contains("master password"));
    let document: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(document["schema_version"], 1);
    assert_eq!(document["vault_id"], hex(&vault.vault_id()));
    assert_eq!(document["kdf"]["algorithm"], "argon2id");
    assert_eq!(document["kdf"]["version"], 19);
    assert!(
        document["wrapped_key"]
            .as_str()
            .unwrap()
            .starts_with("otv1:")
    );
    // The fast test parameters are below the floor, so this header reads as
    // unsupported; the floor itself is covered by `document_tests`.
    assert!(matches!(load(&dir).unwrap(), VaultStatus::Unsupported(_)));
}

#[test]
fn set_up_is_create_only_and_leaves_an_existing_or_damaged_file_alone() {
    let dir = test_dir();
    fast_vault(&dir, "first");
    let before = fs::read(dir.join(DOCUMENT_NAME)).unwrap();
    assert!(matches!(
        init_with(&dir, &secret("second"), FAST_PARAMS, false),
        Err(VaultError::AlreadyInitialised)
    ));
    assert_eq!(fs::read(dir.join(DOCUMENT_NAME)).unwrap(), before);

    let damaged_dir = test_dir();
    fs::write(damaged_dir.join(DOCUMENT_NAME), b"{ not json").unwrap();
    assert!(matches!(
        init_with(&damaged_dir, &secret("second"), FAST_PARAMS, false),
        Err(VaultError::Io(_))
    ));
    assert_eq!(
        fs::read(damaged_dir.join(DOCUMENT_NAME)).unwrap(),
        b"{ not json"
    );
}

#[test]
fn two_concurrent_set_ups_produce_exactly_one_vault() {
    let dir = test_dir();
    let results: Vec<_> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..2)
            .map(|index| {
                let dir = &*dir;
                scope.spawn(move || {
                    init_with(
                        dir,
                        &secret(&format!("password {index}")),
                        FAST_PARAMS,
                        false,
                    )
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect()
    });
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert!(
        results
            .iter()
            .any(|result| matches!(result, Err(VaultError::AlreadyInitialised)))
    );
}

#[test]
fn change_password_rewraps_the_same_data_key_and_scrubs_the_backup() {
    let dir = test_dir();
    let mut vault = fast_vault(&dir, "old password");
    let sealed = vault
        .seal(SecretPurpose::SshPassword, &CONTEXT, &secret("pw"))
        .unwrap();
    let old_header = vault.header().clone();

    assert!(matches!(
        vault.change_password_with(
            &dir,
            &secret("not the old one"),
            &secret("new password"),
            FAST_PARAMS,
            false
        ),
        Err(VaultError::WrongPassword)
    ));
    vault
        .change_password_with(
            &dir,
            &secret("old password"),
            &secret("new password"),
            FAST_PARAMS,
            false,
        )
        .unwrap();

    let new_header = vault.header().clone();
    assert_eq!(new_header.vault_id, old_header.vault_id);
    assert_ne!(new_header.salt, old_header.salt);
    assert!(matches!(
        unlock(&new_header, &secret("old password")),
        Err(VaultError::WrongPassword)
    ));
    let reopened = unlock(&new_header, &secret("new password")).unwrap();
    assert_eq!(
        reopened
            .open(SecretPurpose::SshPassword, &CONTEXT, &sealed)
            .unwrap()
            .expose_secret(),
        "pw"
    );

    let current = fs::read(dir.join(DOCUMENT_NAME)).unwrap();
    assert_eq!(fs::read(dir.join("vault.bak")).unwrap(), current);
    let old_wrap = serde_json::to_string(&old_header.wrapped_key).unwrap();
    assert!(!String::from_utf8_lossy(&current).contains(old_wrap.trim_matches('"')));
}

#[test]
fn change_password_preserves_fields_this_binary_does_not_know() {
    let dir = test_dir();
    let mut vault = fast_vault(&dir, "old password");
    let path = dir.join(DOCUMENT_NAME);
    let mut document: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    document["added_later"] = serde_json::json!({"keep": true});
    document["kdf"]["hint_later"] = serde_json::json!("keep");
    fs::write(&path, serde_json::to_vec(&document).unwrap()).unwrap();

    vault
        .change_password_with(
            &dir,
            &secret("old password"),
            &secret("new password"),
            FAST_PARAMS,
            false,
        )
        .unwrap();
    let document: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(document["added_later"]["keep"], true);
    assert_eq!(document["kdf"]["hint_later"], "keep");
    assert_eq!(
        document["kdf"]["salt"],
        base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            &vault.header().salt
        )
    );
}

#[test]
fn change_password_refuses_when_the_vault_was_reset_elsewhere() {
    let dir = test_dir();
    let mut vault = fast_vault(&dir, "old password");
    reset_with(&dir, false).unwrap();
    fast_vault(&dir, "someone else");
    let before = fs::read(dir.join(DOCUMENT_NAME)).unwrap();
    assert!(matches!(
        vault.change_password_with(
            &dir,
            &secret("old password"),
            &secret("new password"),
            FAST_PARAMS,
            false
        ),
        Err(VaultError::VaultReplaced)
    ));
    assert_eq!(fs::read(dir.join(DOCUMENT_NAME)).unwrap(), before);
}

#[test]
fn reset_removes_the_header_and_its_backup() {
    let dir = test_dir();
    let mut vault = fast_vault(&dir, "old password");
    vault
        .change_password_with(
            &dir,
            &secret("old password"),
            &secret("new password"),
            FAST_PARAMS,
            false,
        )
        .unwrap();
    assert!(dir.join("vault.bak").exists());

    reset_with(&dir, false).unwrap();
    assert!(!dir.join(DOCUMENT_NAME).exists());
    assert!(!dir.join("vault.bak").exists());
    assert!(matches!(load(&dir).unwrap(), VaultStatus::Uninitialised));
    // Damaged or absent: reset still succeeds.
    fs::write(dir.join(DOCUMENT_NAME), b"garbage").unwrap();
    reset_with(&dir, false).unwrap();
    reset_with(&dir, false).unwrap();
    assert!(!dir.join(DOCUMENT_NAME).exists());
}

/// `DEC-0019` M4: every vault write is refused in an elevated window, with
/// nothing changed on disk.
#[test]
fn an_elevated_process_writes_nothing() {
    let dir = test_dir();
    assert!(matches!(
        init_with(&dir, &secret("password"), FAST_PARAMS, true),
        Err(VaultError::ReadOnly)
    ));
    assert_eq!(fs::read_dir(&*dir).unwrap().count(), 0);

    let mut vault = fast_vault(&dir, "password");
    let before = fs::read(dir.join(DOCUMENT_NAME)).unwrap();
    assert!(matches!(
        vault.change_password_with(
            &dir,
            &secret("password"),
            &secret("new password"),
            FAST_PARAMS,
            true
        ),
        Err(VaultError::ReadOnly)
    ));
    assert!(matches!(reset_with(&dir, true), Err(VaultError::ReadOnly)));
    assert_eq!(fs::read(dir.join(DOCUMENT_NAME)).unwrap(), before);
    // The refused change left the in-memory vault as it was.
    assert!(unlock(vault.header(), &secret("password")).is_ok());
}

/// A locked window verifies against the header it holds; replacing
/// `vault.json` (a passer-by setting up a vault of their own from a second
/// OneTerm) changes nothing for it.
#[test]
fn unlock_never_rereads_the_file() {
    let dir = test_dir();
    let header = fast_vault(&dir, "owner password").header().clone();
    reset_with(&dir, false).unwrap();
    fast_vault(&dir, "passer-by password");

    assert!(matches!(
        unlock(&header, &secret("passer-by password")),
        Err(VaultError::WrongPassword)
    ));
    assert!(unlock(&header, &secret("owner password")).is_ok());
}

#[test]
fn needs_rehash_follows_the_header_parameters() {
    assert!(fixed_vault(&[0; 32], [0; 16], KdfParams::FLOOR).needs_rehash());
    assert!(!fixed_vault(&[0; 32], [0; 16], KdfParams::DEFAULT).needs_rehash());
    let dir = test_dir();
    let mut vault = fast_vault(&dir, "password");
    assert!(vault.needs_rehash());
    // The rehash an unlock triggers: the same password, the current defaults.
    vault
        .change_password_with(
            &dir,
            &secret("password"),
            &secret("password"),
            KdfParams::DEFAULT,
            false,
        )
        .unwrap();
    assert!(!vault.needs_rehash());
}

#[test]
fn debug_output_carries_no_key_material() {
    let dir = test_dir();
    let vault = fast_vault(&dir, "master password");
    let shown = format!("{:?}", vault.header());
    let wrapped = serde_json::to_string(&vault.header().wrapped_key).unwrap();
    assert!(!shown.contains(wrapped.trim_matches('"').trim_start_matches("otv1:")));
    assert!(shown.contains(&hex(&vault.vault_id())));
}

/// The cost contract, as ratios only (`BUG-0075`: never a wall-clock bound):
///
/// - a wrong password costs a full key derivation, like a right one (both
///   fail or succeed only at the GCM tag), so timing reveals nothing;
/// - the default parameters cost clearly more than the floor.
///
/// The measured unlock time at the defaults is printed for the packet record
/// (`cargo test -p oneterm-vault --release unlock_cost_contract -- --nocapture`).
#[test]
fn unlock_cost_contract() {
    let floor_dir = test_dir();
    let floor = init_with(&floor_dir, &secret("right"), KdfParams::FLOOR, false)
        .unwrap()
        .header()
        .clone();
    let default_dir = test_dir();
    let default = init_with(&default_dir, &secret("right"), KdfParams::DEFAULT, false)
        .unwrap()
        .header()
        .clone();

    let (right, wrong) = fastest_interleaved(
        || assert!(unlock(&floor, &secret("right")).is_ok()),
        || {
            assert!(matches!(
                unlock(&floor, &secret("wrong")),
                Err(VaultError::WrongPassword)
            ))
        },
    );
    let ratio = wrong.as_secs_f64() / right.as_secs_f64();
    assert!(
        (0.5..=2.0).contains(&ratio),
        "wrong/right unlock = {ratio:.2} ({wrong:?} vs {right:?})"
    );

    let (at_default, at_floor) = fastest_interleaved(
        || assert!(unlock(&default, &secret("right")).is_ok()),
        || assert!(unlock(&floor, &secret("right")).is_ok()),
    );
    eprintln!(
        "Argon2id unlock: defaults {} ms, floor {} ms (fastest of 3)",
        at_default.as_millis(),
        at_floor.as_millis()
    );
    assert!(
        at_default.as_secs_f64() > 2.0 * at_floor.as_secs_f64(),
        "defaults {at_default:?} vs floor {at_floor:?}"
    );
}

/// The fastest of three runs of each, alternating, so a load change on the
/// machine hits both sides alike.
fn fastest_interleaved(first: impl Fn(), second: impl Fn()) -> (Duration, Duration) {
    let time = |run: &dyn Fn()| {
        let start = Instant::now();
        run();
        start.elapsed()
    };
    (0..3).fold((Duration::MAX, Duration::MAX), |(a, b), _| {
        (a.min(time(&first)), b.min(time(&second)))
    })
}

#[cfg(unix)]
#[test]
fn the_header_is_owner_only() {
    use std::os::unix::fs::PermissionsExt;

    let dir = test_dir();
    fast_vault(&dir, "password");
    let mode = fs::metadata(dir.join(DOCUMENT_NAME))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
}

// Independent verification of US-0156 (`evidence/US-0156-verify.md`): the
// envelope parser, the secret-bearing types and the nonce source, attacked.
mod verify {
    use std::collections::HashSet;

    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD;

    use super::*;

    /// Compile-time proof that `UnlockedVault` is neither `Clone`, `Debug` nor
    /// `Serialize`: if it were any of them, the last line would be ambiguous and
    /// the crate would not compile (the `static_assertions::assert_not_impl_any`
    /// technique).
    const _: fn() = || {
        trait AmbiguousIfImpl<A> {
            fn some_item() {}
        }
        impl<T: ?Sized> AmbiguousIfImpl<()> for T {}
        struct IfClone;
        impl<T: Clone> AmbiguousIfImpl<IfClone> for T {}
        struct IfDebug;
        impl<T: ?Sized + std::fmt::Debug> AmbiguousIfImpl<IfDebug> for T {}
        struct IfSerialize;
        impl<T: ?Sized + serde::Serialize> AmbiguousIfImpl<IfSerialize> for T {}
        let _ = <UnlockedVault as AmbiguousIfImpl<_>>::some_item;
    };

    /// The unlocked vault crosses back from the KDF's background task
    /// (`crypto-and-format.md` § 7).
    #[test]
    fn an_unlocked_vault_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<UnlockedVault>();
    }

    fn value(text: &str) -> SealedSecret {
        serde_json::from_value(text.into()).unwrap()
    }

    /// Every way to spell a v1 envelope other than the one canonical spelling
    /// is refused, and nothing but `otvN:` with N > 1 reads as newer.
    #[test]
    fn the_envelope_parser_accepts_only_the_canonical_spelling() {
        let vault = fixed_vault(&[0x11; 32], [0x22; 16], KdfParams::DEFAULT);
        let canonical = FROZEN_ENVELOPE_JSON.trim_matches('"');
        let open = |text: &str| vault.open(SecretPurpose::SshPassword, &CONTEXT, &value(text));
        assert_eq!(open(canonical).unwrap().expose_secret(), "hunter2");

        let payload = &canonical["otv1:".len()..];
        assert!(payload.contains('+') && payload.contains('/') && payload.ends_with("=="));
        // The same bytes with non-zero trailing bits in the last quantum: a
        // lenient decoder would accept this alias.
        let alias = {
            const ALPHABET: &[u8] =
                b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
            let mut bytes = payload.as_bytes().to_vec();
            let last = bytes.len() - 3;
            let index = ALPHABET.iter().position(|&c| c == bytes[last]).unwrap();
            bytes[last] = ALPHABET[index ^ 1];
            String::from_utf8(bytes).unwrap()
        };
        assert_ne!(alias, payload);
        let huge = format!("otv1:{}", "A".repeat(4 << 20));
        let malformed = [
            format!("otv1:{payload}="),
            format!("otv1:{}", payload.trim_end_matches('=')),
            format!("otv1:{alias}"),
            format!("otv1:{payload}\n"),
            format!("otv1: {payload}"),
            format!(" otv1:{payload}"),
            format!("otv1:{}\r\n{}", &payload[..8], &payload[8..]),
            format!("otv1:{}", payload.replace('+', "-").replace('/', "_")),
            format!("OTV1:{payload}"),
            format!("Otv1:{payload}"),
            format!("otv01:{payload}"),
            format!("otv1x:{payload}"),
            format!("otv1{payload}"),
            "otv1:".to_owned(),
            "otv1:====".to_owned(),
            huge,
        ];
        for text in &malformed {
            let shown: String = text.chars().take(40).collect();
            assert_eq!(open(text).err(), Some(OpenError::Malformed), "{shown:?}");
        }
        for newer in [format!("otv2:{payload}"), format!("otv10:{payload}")] {
            assert_eq!(open(&newer).err(), Some(OpenError::NewerFormat), "{newer}");
        }
    }

    /// The shortest payload the parser takes (kid, nonce, no ciphertext, a
    /// tag) decodes, and only the tag check stops it.
    #[test]
    fn an_empty_ciphertext_is_rejected_at_the_tag() {
        let vault = fixed_vault(&[0x11; 32], [0x22; 16], KdfParams::DEFAULT);
        let mut payload = vec![0x22; KID_LEN];
        payload.extend_from_slice(&[0; NONCE_LEN + envelope::TAG_LEN]);
        let text = format!("otv1:{}", STANDARD.encode(payload));
        assert_eq!(
            vault
                .open(SecretPurpose::SshPassword, &CONTEXT, &value(&text))
                .err(),
            Some(OpenError::Rejected)
        );
    }

    /// The wrapped data key never opens as an SSH password, even though its
    /// kid is this vault's: other key, other purpose.
    #[test]
    fn the_wrapped_key_never_opens_as_an_item() {
        let dir = test_dir();
        let vault = fast_vault(&dir, "master password");
        let wrapped = vault.header().wrapped_key.clone();
        assert_eq!(
            vault
                .open(SecretPurpose::SshPassword, &CONTEXT, &wrapped)
                .err(),
            Some(OpenError::Rejected)
        );
    }

    /// Nonces come from the OS generator: no repeat in a few thousand seals,
    /// and the kid is always this vault's.
    #[test]
    fn nonces_never_repeat_and_an_empty_secret_round_trips() {
        let dir = test_dir();
        let vault = fast_vault(&dir, "master password");
        let mut nonces = HashSet::new();
        for _ in 0..4096 {
            let sealed = vault
                .seal(SecretPurpose::SshPassword, &CONTEXT, &secret("pw"))
                .unwrap();
            let envelope = Envelope::decode(&sealed, 1024).unwrap();
            assert_eq!(envelope.kid, vault.vault_id()[..KID_LEN]);
            assert!(nonces.insert(envelope.nonce));
        }
        let empty = vault
            .seal(SecretPurpose::SshPassword, &CONTEXT, &secret(""))
            .unwrap();
        let opened = vault
            .open(SecretPurpose::SshPassword, &CONTEXT, &empty)
            .unwrap();
        assert_eq!(opened.expose_secret(), "");
    }
}
