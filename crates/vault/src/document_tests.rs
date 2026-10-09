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
