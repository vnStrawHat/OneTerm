//! Tests for `document.rs`: the load outcomes of `vault.json` and the frozen
//! schema-1 fixture.

use std::fs;

use oneterm_core::SecretString;
use serde_json::{Value, json};

use super::*;
use crate::test_support::test_dir;
use crate::{SealContext, SecretPurpose, unlock};

const V1: &str = include_str!("../tests/fixtures/persistence/vault-v1.json");
const V9: &str = include_str!("../tests/fixtures/persistence/vault-v9.json");
const INVALID_JSON: &str = include_str!("../tests/fixtures/persistence/vault-invalid-json.json");
const INVALID_PARAMS: &str =
    include_str!("../tests/fixtures/persistence/vault-invalid-params.json");

/// The password and an SSH password sealed under the `vault-v1.json` fixture.
/// Fixture and envelope were produced by an implementation independent of
/// OneTerm (Python `cryptography` 44: `Argon2id`, `AESGCM`), so this test pins
/// the whole format, not just OneTerm's agreement with itself.
const V1_PASSWORD: &str = "correct horse battery staple";
const V1_SEALED_SSH_PASSWORD: &str =
    "otv1:P5wKTosdTCdcXFxcXFxcXFxcXFxbpgEB6ylBEMijZRhddr55R0kOe/uq";

fn status_of(raw: &str) -> VaultStatus {
    let dir = test_dir();
    fs::write(dir.join(DOCUMENT_NAME), raw).unwrap();
    let status = load(&dir).unwrap();
    // A load never writes, never quarantines: the bytes are still there.
    assert_eq!(fs::read_to_string(dir.join(DOCUMENT_NAME)).unwrap(), raw);
    assert_eq!(fs::read_dir(&*dir).unwrap().count(), 1);
    status
}

fn v1_with(change: impl FnOnce(&mut Value)) -> String {
    let mut document: Value = serde_json::from_str(V1).unwrap();
    change(&mut document);
    serde_json::to_string(&document).unwrap()
}

#[test]
fn the_v1_fixture_loads_unlocks_and_opens_its_sealed_password() {
    let VaultStatus::Ready(header) = status_of(V1) else {
        panic!("vault-v1.json must load as Ready");
    };
    assert_eq!(header.kdf, KdfParams::FLOOR);
    let vault = unlock(&header, &SecretString::new(V1_PASSWORD)).unwrap();
    assert!(vault.needs_rehash());
    let sealed: SealedSecret = serde_json::from_value(json!(V1_SEALED_SSH_PASSWORD)).unwrap();
    let context = SealContext {
        host: "10.0.0.1",
        port: 22,
        username: "deploy",
    };
    let opened = vault
        .open(SecretPurpose::SshPassword, &context, &sealed)
        .unwrap();
    assert_eq!(opened.expose_secret(), "s3cret");
}

#[test]
fn missing_file_is_uninitialised_and_unreadable_file_is_a_load_error() {
    let dir = test_dir();
    assert!(matches!(load(&dir).unwrap(), VaultStatus::Uninitialised));
    // A directory where the file should be: a read error that is not
    // "not found", reported and left alone.
    fs::create_dir(dir.join(DOCUMENT_NAME)).unwrap();
    assert!(matches!(load(&dir), Err(AppError::ConfigLoad { .. })));
    assert!(dir.join(DOCUMENT_NAME).is_dir());
}

#[test]
fn fixtures_load_as_damaged_or_unsupported_and_are_left_in_place() {
    assert!(matches!(status_of(INVALID_JSON), VaultStatus::Damaged(_)));
    assert!(matches!(status_of(V9), VaultStatus::Unsupported(_)));
    assert!(matches!(
        status_of(INVALID_PARAMS),
        VaultStatus::Unsupported(_)
    ));
}

#[test]
fn every_field_rule_gives_its_outcome() {
    let floor = KdfParams::FLOOR;
    let cases: Vec<(&str, String, bool)> = vec![
        // (case, document, expect Unsupported (true) or Damaged (false))
        (
            "no schema_version",
            v1_with(|d| drop(d.as_object_mut().unwrap().remove("schema_version"))),
            false,
        ),
        (
            "schema_version text",
            v1_with(|d| d["schema_version"] = json!("1")),
            false,
        ),
        (
            "schema_version 2",
            v1_with(|d| d["schema_version"] = json!(2)),
            true,
        ),
        ("not an object", "[1, 2]".to_owned(), false),
        (
            "missing wrapped_key",
            v1_with(|d| drop(d.as_object_mut().unwrap().remove("wrapped_key"))),
            false,
        ),
        (
            "memory as text",
            v1_with(|d| d["kdf"]["memory_kib"] = json!("19456")),
            false,
        ),
        (
            "argon2i",
            v1_with(|d| d["kdf"]["algorithm"] = json!("argon2i")),
            true,
        ),
        (
            "argon2 v0x10",
            v1_with(|d| d["kdf"]["version"] = json!(16)),
            true,
        ),
        (
            "memory below floor",
            v1_with(|d| d["kdf"]["memory_kib"] = json!(floor.memory_kib - 1)),
            true,
        ),
        (
            "one pass",
            v1_with(|d| d["kdf"]["iterations"] = json!(1)),
            true,
        ),
        (
            "17 passes",
            v1_with(|d| d["kdf"]["iterations"] = json!(17)),
            true,
        ),
        (
            "5 lanes",
            v1_with(|d| d["kdf"]["parallelism"] = json!(5)),
            true,
        ),
        (
            "uppercase vault_id",
            v1_with(|d| d["vault_id"] = json!("3F9C0A4E8B1D4C27A6E2F0B9C1D2E3F4")),
            false,
        ),
        (
            "short vault_id",
            v1_with(|d| d["vault_id"] = json!("3f9c")),
            false,
        ),
        (
            "salt not base64",
            v1_with(|d| d["kdf"]["salt"] = json!("not base64!")),
            false,
        ),
        (
            "8-byte salt",
            v1_with(|d| d["kdf"]["salt"] = json!("AAAAAAAAAAA=")),
            true,
        ),
        (
            "wrapped_key otv2",
            v1_with(|d| d["wrapped_key"] = json!("otv2:AAAA")),
            true,
        ),
        (
            "wrapped_key plain",
            v1_with(|d| d["wrapped_key"] = json!("plain")),
            false,
        ),
        (
            "kid of another vault",
            v1_with(|d| d["vault_id"] = json!("00000000000000000000000000000000")),
            false,
        ),
    ];
    for (case, raw, expect_unsupported) in cases {
        match status_of(&raw) {
            VaultStatus::Unsupported(_) if expect_unsupported => {}
            VaultStatus::Damaged(_) if !expect_unsupported => {}
            other => panic!("{case}: unexpected {other:?}"),
        }
    }
}

#[test]
fn unknown_fields_are_ignored_on_read() {
    let raw = v1_with(|d| {
        d["added_by_a_later_version"] = json!({"anything": [1, 2, 3]});
        d["kdf"]["also_later"] = json!(true);
    });
    assert!(matches!(status_of(&raw), VaultStatus::Ready(_)));
}

#[test]
fn hex_ids_round_trip() {
    let id = [
        0x3f, 0x9c, 0x0a, 0x4e, 0x8b, 0x1d, 0x4c, 0x27, 0xa6, 0xe2, 0xf0, 0xb9, 0xc1, 0xd2, 0xe3,
        0xf4,
    ];
    assert_eq!(hex(&id), "3f9c0a4e8b1d4c27a6e2f0b9c1d2e3f4");
    assert_eq!(parse_hex_id(&hex(&id)), Some(id));
    assert_eq!(parse_hex_id("3f9c0a4e8b1d4c27a6e2f0b9c1d2e3fg"), None);
}

// Independent verification of US-0156 (`evidence/US-0156-verify.md`): attacks on
// the one write function. D1-D3 recorded there are fixed; their tests assert
// the fixed behaviour.
mod verify {
    use super::*;
    use crate::UnlockedVault;
    use crate::test_support::FAST_PARAMS;
    use crate::vault::init_with;

    fn backup(dir: &Path) -> std::path::PathBuf {
        dir.join("vault.bak")
    }

    fn change(vault: &mut UnlockedVault, dir: &Path) -> Result<(), VaultError> {
        vault.change_password_with(
            dir,
            &SecretString::new("pw"),
            &SecretString::new("new"),
            FAST_PARAMS,
            false,
        )
    }

    /// Every file `load` reports as `Damaged` or `Unsupported` survives a
    /// set-up byte for byte, gains no `vault.bak`, and is refused with
    /// `AlreadyInitialised`, JSON or not (N1).
    #[test]
    fn set_up_never_overwrites_a_damaged_or_unsupported_file() {
        let other_id = v1_with(|d| d["vault_id"] = json!("00000000000000000000000000000000"));
        let argon2i = v1_with(|d| d["kdf"]["algorithm"] = json!("argon2i"));
        let cases: Vec<(&str, String, bool)> = vec![
            // (case, file, is JSON)
            ("not JSON", INVALID_JSON.to_owned(), false),
            ("empty file", String::new(), false),
            ("array", "[1, 2]".to_owned(), true),
            ("null", "null".to_owned(), true),
            (
                "text schema_version",
                r#"{"schema_version":"1"}"#.to_owned(),
                true,
            ),
            ("kid of another vault", other_id, true),
            ("schema 9", V9.to_owned(), true),
            ("memory above ceiling", INVALID_PARAMS.to_owned(), true),
            ("argon2i", argon2i, true),
        ];
        for (case, raw, is_json) in cases {
            let dir = test_dir();
            fs::write(dir.join(DOCUMENT_NAME), &raw).unwrap();
            assert!(
                matches!(
                    load(&dir).unwrap(),
                    VaultStatus::Damaged(_) | VaultStatus::Unsupported(_)
                ),
                "{case}"
            );
            match init_with(&dir, &SecretString::new("pw"), FAST_PARAMS, false) {
                Err(VaultError::AlreadyInitialised) => {}
                Err(other) => panic!("{case} (JSON: {is_json}): unexpected {other:?}"),
                Ok(_) => panic!("{case}: set-up overwrote the file"),
            }
            assert_eq!(
                fs::read_to_string(dir.join(DOCUMENT_NAME)).unwrap(),
                raw,
                "{case}"
            );
            assert!(!backup(&dir).exists(), "{case}");
        }
    }

    /// D1 (fixed): a `vault.json` holding exactly `{}` loads as `Damaged("no
    /// schema_version")`, and set-up no longer mistakes it for a missing file.
    #[test]
    fn set_up_refuses_an_empty_object_file_d1() {
        let dir = test_dir();
        fs::write(dir.join(DOCUMENT_NAME), "{ }").unwrap();
        assert!(matches!(load(&dir).unwrap(), VaultStatus::Damaged(_)));
        assert!(matches!(
            init_with(&dir, &SecretString::new("pw"), FAST_PARAMS, false),
            Err(VaultError::AlreadyInitialised)
        ));
        assert_eq!(fs::read_to_string(dir.join(DOCUMENT_NAME)).unwrap(), "{ }");
        assert!(!backup(&dir).exists());
    }

    /// The rewrap refuses, writes nothing and leaves `vault.bak` alone when the
    /// file is gone (no file is created), not JSON (`Io`), or from a newer
    /// schema with the same id (`VaultReplaced`).
    #[test]
    fn rewrap_refuses_a_missing_non_json_or_newer_schema_file() {
        let dir = test_dir();
        let mut vault = init_with(&dir, &SecretString::new("pw"), FAST_PARAMS, false).unwrap();
        let path = dir.join(DOCUMENT_NAME);
        let bak = fs::read(backup(&dir)).unwrap();

        fs::remove_file(&path).unwrap();
        assert!(matches!(
            change(&mut vault, &dir),
            Err(VaultError::VaultReplaced)
        ));
        assert!(!path.exists());

        fs::write(&path, INVALID_JSON).unwrap();
        assert!(matches!(change(&mut vault, &dir), Err(VaultError::Io(_))));
        assert_eq!(fs::read_to_string(&path).unwrap(), INVALID_JSON);

        let newer = json!({"schema_version": 2, "vault_id": hex(&vault.vault_id()), "kdf": {}});
        let newer = newer.to_string();
        fs::write(&path, &newer).unwrap();
        assert!(matches!(
            change(&mut vault, &dir),
            Err(VaultError::VaultReplaced)
        ));
        assert_eq!(fs::read_to_string(&path).unwrap(), newer);

        assert_eq!(fs::read(backup(&dir)).unwrap(), bak);
        // Every refusal left the in-memory vault on the old password.
        assert!(unlock(vault.header(), &SecretString::new("pw")).is_ok());
    }

    /// D2 (fixed): a file with this vault's id that `load` reports as
    /// `Damaged` or `Unsupported` (another envelope version, parameters above
    /// the ceiling, another algorithm, a mangled `kdf`) is refused by a password
    /// change or rehash: the file and `vault.bak` stay as they are.
    #[test]
    fn rewrap_refuses_a_damaged_or_unsupported_file_of_the_same_vault_d2() {
        let edits: [(&str, fn(&mut Value)); 4] = [
            ("kdf not an object", |d| d["kdf"] = json!("garbage")),
            ("wrapped_key otv2", |d| {
                d["wrapped_key"] = json!("otv2:AAAA")
            }),
            ("memory above ceiling", |d| {
                d["kdf"]["memory_kib"] = json!(KdfParams::CEILING.memory_kib + 1)
            }),
            ("argon2i", |d| d["kdf"]["algorithm"] = json!("argon2i")),
        ];
        for (case, edit) in edits {
            let dir = test_dir();
            let mut vault = init_with(&dir, &SecretString::new("pw"), FAST_PARAMS, false).unwrap();
            let path = dir.join(DOCUMENT_NAME);
            let mut document: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            // Floor parameters, so `load` judges only the edited field.
            document["kdf"]["memory_kib"] = json!(KdfParams::FLOOR.memory_kib);
            document["kdf"]["iterations"] = json!(KdfParams::FLOOR.iterations);
            edit(&mut document);
            let raw = document.to_string();
            fs::write(&path, &raw).unwrap();
            let bak = fs::read(backup(&dir)).unwrap();
            assert!(
                matches!(
                    load(&dir).unwrap(),
                    VaultStatus::Damaged(_) | VaultStatus::Unsupported(_)
                ),
                "{case}"
            );

            match change(&mut vault, &dir) {
                Err(VaultError::VaultReplaced) => {}
                other => panic!("{case}: unexpected {other:?}"),
            }
            assert_eq!(fs::read_to_string(&path).unwrap(), raw, "{case}");
            assert_eq!(fs::read(backup(&dir)).unwrap(), bak, "{case}");
        }
    }

    /// O1 / D2 (fixed): a process still unlocked from an older header cannot
    /// override a password another process set since. Its rewrap is refused
    /// with `ChangedElsewhere`, and the newer header stays.
    #[test]
    fn rewrap_refuses_a_header_rewrapped_by_another_process() {
        let dir = test_dir();
        let mut first = init_with(&dir, &SecretString::new("pw"), FAST_PARAMS, false).unwrap();
        let mut second = unlock(first.header(), &SecretString::new("pw")).unwrap();
        change(&mut first, &dir).unwrap();
        let after_first = fs::read(dir.join(DOCUMENT_NAME)).unwrap();

        assert!(matches!(
            change(&mut second, &dir),
            Err(VaultError::ChangedElsewhere)
        ));
        assert_eq!(fs::read(dir.join(DOCUMENT_NAME)).unwrap(), after_first);
        assert_eq!(fs::read(backup(&dir)).unwrap(), after_first);
        assert!(unlock(first.header(), &SecretString::new("new")).is_ok());
    }

    /// D3 (fixed): a file written by a newer OneTerm is not reset by this one;
    /// a damaged file still is.
    #[test]
    fn reset_refuses_an_unsupported_file_and_resets_a_damaged_one_d3() {
        let dir = test_dir();
        fs::write(dir.join(DOCUMENT_NAME), V9).unwrap();
        assert!(matches!(
            crate::vault::reset_with(&dir, false),
            Err(VaultError::Unsupported(_))
        ));
        assert_eq!(fs::read_to_string(dir.join(DOCUMENT_NAME)).unwrap(), V9);

        fs::write(dir.join(DOCUMENT_NAME), INVALID_JSON).unwrap();
        crate::vault::reset_with(&dir, false).unwrap();
        assert!(!dir.join(DOCUMENT_NAME).exists());
    }

    /// A refusal is never confused with an I/O failure: `vault.json` being a
    /// directory is an `Io` error for set-up, not `AlreadyInitialised`.
    #[test]
    fn an_io_failure_is_not_reported_as_a_refusal() {
        let dir = test_dir();
        fs::create_dir(dir.join(DOCUMENT_NAME)).unwrap();
        assert!(matches!(
            init_with(&dir, &SecretString::new("pw"), FAST_PARAMS, false),
            Err(VaultError::Io(_))
        ));
        assert!(dir.join(DOCUMENT_NAME).is_dir());
    }

    /// Re-verification of the rework: the rewrap compares the salt and the
    /// wrapped key one by one. A file that differs from the unlocked header in
    /// only one of them (still `Ready`, same id) is refused with
    /// `ChangedElsewhere`, and `vault.json` and `vault.bak` stay as they are.
    #[test]
    fn rewrap_refuses_a_header_that_differs_only_in_salt_or_only_in_wrapped_key() {
        let other_wrap = |vault: &UnlockedVault| {
            let mut kid = [0u8; KID_LEN];
            kid.copy_from_slice(&vault.vault_id()[..KID_LEN]);
            let envelope = Envelope {
                kid,
                nonce: [9; crate::envelope::NONCE_LEN],
                sealed: vec![7; crate::kdf::KEY_LEN + TAG_LEN],
            };
            json!(envelope.encode())
        };
        for salt_only in [true, false] {
            let case = if salt_only {
                "salt only"
            } else {
                "wrapped key only"
            };
            let dir = test_dir();
            let mut vault = init_with(&dir, &SecretString::new("pw"), FAST_PARAMS, false).unwrap();
            let path = dir.join(DOCUMENT_NAME);
            let mut document: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            if salt_only {
                document["kdf"]["salt"] = json!(STANDARD.encode([0x5a; 16]));
            } else {
                document["wrapped_key"] = other_wrap(&vault);
            }
            let raw = document.to_string();
            fs::write(&path, &raw).unwrap();
            let bak = fs::read(backup(&dir)).unwrap();
            assert!(
                matches!(load(&dir).unwrap(), VaultStatus::Ready(_)),
                "{case}"
            );

            assert!(
                matches!(change(&mut vault, &dir), Err(VaultError::ChangedElsewhere)),
                "{case}"
            );
            assert_eq!(fs::read_to_string(&path).unwrap(), raw, "{case}");
            assert_eq!(fs::read(backup(&dir)).unwrap(), bak, "{case}");
        }
    }

    /// Re-verification of the rework, documented behaviour (not a defect):
    /// the rewrap does not compare the Argon2id parameters. A `Ready` file with
    /// this vault's id, salt and wrapped key but other parameters (which no
    /// password can unlock: the KEK differs) is rewritten into a valid header.
    #[test]
    fn rewrap_over_a_parameters_only_edit_rewrites_it() {
        let dir = test_dir();
        let mut vault = init_with(&dir, &SecretString::new("pw"), FAST_PARAMS, false).unwrap();
        let path = dir.join(DOCUMENT_NAME);
        let mut document: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        document["kdf"]["iterations"] = json!(FAST_PARAMS.iterations + 1);
        let raw = document.to_string();
        fs::write(&path, &raw).unwrap();
        assert!(matches!(load(&dir).unwrap(), VaultStatus::Ready(_)));

        change(&mut vault, &dir).unwrap();
        let VaultStatus::Ready(header) = load(&dir).unwrap() else {
            panic!("the rewritten header must load as Ready");
        };
        assert!(unlock(&header, &SecretString::new("new")).is_ok());
    }

    /// `Debug` of a whole load outcome carries neither the wrapped key nor the
    /// salt.
    #[test]
    fn debug_of_a_ready_status_carries_no_wrapped_key_or_salt() {
        let status = status_of(V1);
        assert!(matches!(status, VaultStatus::Ready(_)));
        let shown = format!("{status:?}");
        let document: Value = serde_json::from_str(V1).unwrap();
        let wrapped = document["wrapped_key"].as_str().unwrap();
        assert!(!shown.contains(&wrapped["otv1:".len()..]));
        assert!(!shown.contains(document["kdf"]["salt"].as_str().unwrap()));
    }

    /// The whole path through `load`: a password change at the floor is
    /// written to disk, the reloaded header refuses the old password, accepts
    /// the new one and still opens items sealed before.
    #[test]
    fn change_password_survives_a_reload_from_disk() {
        let dir = test_dir();
        let mut vault =
            init_with(&dir, &SecretString::new("old"), KdfParams::FLOOR, false).unwrap();
        let context = SealContext {
            host: "h",
            port: 22,
            username: "u",
        };
        let sealed = vault
            .seal(
                SecretPurpose::SshPassword,
                &context,
                &SecretString::new("pw"),
            )
            .unwrap();
        vault
            .change_password_with(
                &dir,
                &SecretString::new("old"),
                &SecretString::new("new"),
                KdfParams::FLOOR,
                false,
            )
            .unwrap();
        drop(vault);

        let VaultStatus::Ready(header) = load(&dir).unwrap() else {
            panic!("the rewrapped header must load as Ready");
        };
        assert!(matches!(
            unlock(&header, &SecretString::new("old")),
            Err(VaultError::WrongPassword)
        ));
        let reopened = unlock(&header, &SecretString::new("new")).unwrap();
        let opened = reopened
            .open(SecretPurpose::SshPassword, &context, &sealed)
            .unwrap();
        assert_eq!(opened.expose_secret(), "pw");
        assert_eq!(
            fs::read(backup(&dir)).unwrap(),
            fs::read(dir.join(DOCUMENT_NAME)).unwrap()
        );
    }
}
