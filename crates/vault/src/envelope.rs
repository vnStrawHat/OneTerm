//! The persisted envelope (`crypto-and-format.md` § 4-5):
//! `otv1:` + base64( kid[8] | nonce[12] | ciphertext | tag[16] ), and the
//! associated data every seal authenticates.

use std::fmt;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Serialize};

use crate::OpenError;

/// The only version tag this binary writes and decrypts.
const V1_PREFIX: &str = "otv1:";
pub(crate) const KID_LEN: usize = 8;
pub(crate) const NONCE_LEN: usize = 12;
pub(crate) const TAG_LEN: usize = 16;
/// Bytes every v1 payload carries besides the ciphertext.
const OVERHEAD: usize = KID_LEN + NONCE_LEN + TAG_LEN;

/// A sealed value exactly as it is persisted (`otv1:...`).
///
/// It is opaque: any string deserializes, and it serializes back byte for
/// byte, so an envelope from a newer OneTerm (`otv2:`) or a hand-edited value
/// survives every load and save untouched. Whether it can be opened is decided
/// only by [`crate::UnlockedVault::open`], never by parsing on load. It has no
/// `Display`, and its `Debug` shows the version tag only.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SealedSecret(String);

impl fmt::Debug for SealedSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match version_tag(&self.0) {
            Some(tag) => write!(f, "{tag}:<sealed>"),
            None => f.write_str("<sealed>"),
        }
    }
}

/// `otvN` when the value starts with a canonical `otvN:` tag (`N >= 1`, no
/// leading zero).
fn version_tag(value: &str) -> Option<&str> {
    let (tag, _) = value.split_once(':')?;
    let digits = tag.strip_prefix("otv")?;
    let canonical = !digits.is_empty()
        && digits.bytes().all(|byte| byte.is_ascii_digit())
        && !digits.starts_with('0');
    canonical.then_some(tag)
}

/// A v1 envelope split into its parts.
pub(crate) struct Envelope {
    pub(crate) kid: [u8; KID_LEN],
    pub(crate) nonce: [u8; NONCE_LEN],
    /// Ciphertext followed by the 16-byte tag.
    pub(crate) sealed: Vec<u8>,
}

impl Envelope {
    pub(crate) fn encode(&self) -> SealedSecret {
        let mut payload = Vec::with_capacity(KID_LEN + NONCE_LEN + self.sealed.len());
        payload.extend_from_slice(&self.kid);
        payload.extend_from_slice(&self.nonce);
        payload.extend_from_slice(&self.sealed);
        SealedSecret(format!("{V1_PREFIX}{}", STANDARD.encode(payload)))
    }

    /// Parse strictly: exact `otv1:` prefix, padded base64, and a payload
    /// holding at least the overhead and at most `max_plaintext` bytes of
    /// ciphertext.
    pub(crate) fn decode(value: &SealedSecret, max_plaintext: usize) -> Result<Self, OpenError> {
        let Some(payload) = value.0.strip_prefix(V1_PREFIX) else {
            return Err(match version_tag(&value.0) {
                Some(_) => OpenError::NewerFormat,
                None => OpenError::Malformed,
            });
        };
        let payload = STANDARD.decode(payload).map_err(|_| OpenError::Malformed)?;
        if payload.len() < OVERHEAD || payload.len() > OVERHEAD + max_plaintext {
            return Err(OpenError::Malformed);
        }
        let (kid, rest) = payload.split_at(KID_LEN);
        let (nonce, sealed) = rest.split_at(NONCE_LEN);
        Ok(Self {
            kid: kid.try_into().map_err(|_| OpenError::Malformed)?,
            nonce: nonce.try_into().map_err(|_| OpenError::Malformed)?,
            sealed: sealed.to_vec(),
        })
    }
}

/// Format version of the associated-data layout below.
const AAD_LAYOUT_VERSION: u8 = 0x01;

/// The associated data of one seal:
///
/// ```text
/// "oneterm-vault" 0x00 | 0x01 | u8 len(purpose) | purpose | vault_id[16]
///     | for each context field: u32 big-endian length | bytes
/// ```
pub(crate) fn associated_data(purpose: &str, vault_id: &[u8; 16], context: &[&[u8]]) -> Vec<u8> {
    let mut aad = Vec::with_capacity(64);
    aad.extend_from_slice(b"oneterm-vault\0");
    aad.push(AAD_LAYOUT_VERSION);
    // Purposes are short constants of this crate, never user input.
    aad.push(purpose.len() as u8);
    aad.extend_from_slice(purpose.as_bytes());
    aad.extend_from_slice(vault_id);
    for field in context {
        aad.extend_from_slice(&(field.len() as u32).to_be_bytes());
        aad.extend_from_slice(field);
    }
    aad
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sealed(value: &str) -> SealedSecret {
        SealedSecret(value.to_owned())
    }

    fn valid_payload(ciphertext_len: usize) -> String {
        STANDARD.encode(vec![0u8; OVERHEAD + ciphertext_len])
    }

    #[test]
    fn parser_table() {
        let ok = format!("otv1:{}", valid_payload(4));
        assert!(Envelope::decode(&sealed(&ok), 1024).is_ok());

        let cases = [
            ("plain text password", OpenError::Malformed),
            ("", OpenError::Malformed),
            (&*format!("OTV1:{}", valid_payload(4)), OpenError::Malformed),
            (
                &*format!("otv01:{}", valid_payload(4)),
                OpenError::Malformed,
            ),
            (&*format!("otv0:{}", valid_payload(4)), OpenError::Malformed),
            ("otv1:not base64!", OpenError::Malformed),
            // Missing padding: strict decode refuses it.
            (
                &*format!("otv1:{}", valid_payload(4).trim_end_matches('=')),
                OpenError::Malformed,
            ),
            (
                &*format!("otv1:{}", STANDARD.encode([0u8; OVERHEAD - 1])),
                OpenError::Malformed,
            ),
            (
                &*format!("otv1:{}", valid_payload(1025)),
                OpenError::Malformed,
            ),
            (
                &*format!("otv2:{}", valid_payload(4)),
                OpenError::NewerFormat,
            ),
            ("otv12:anything", OpenError::NewerFormat),
        ];
        for (value, expected) in cases {
            assert_eq!(
                Envelope::decode(&sealed(value), 1024).err(),
                Some(expected),
                "{value:?}"
            );
        }
    }

    #[test]
    fn encode_then_decode_keeps_every_part() {
        let envelope = Envelope {
            kid: [1; KID_LEN],
            nonce: [2; NONCE_LEN],
            sealed: vec![3; 20],
        };
        let decoded = Envelope::decode(&envelope.encode(), 1024).unwrap();
        assert_eq!(decoded.kid, envelope.kid);
        assert_eq!(decoded.nonce, envelope.nonce);
        assert_eq!(decoded.sealed, envelope.sealed);
    }

    #[test]
    fn unknown_versions_round_trip_through_serde_untouched() {
        for value in ["otv2:AAAA", "otv99:whatever", "hand-edited plain text"] {
            let json = serde_json::to_string(value).unwrap();
            let parsed: SealedSecret = serde_json::from_str(&json).unwrap();
            assert_eq!(serde_json::to_string(&parsed).unwrap(), json);
        }
    }

    #[test]
    fn debug_never_shows_the_payload() {
        let value = sealed("otv1:c2VjcmV0LXBheWxvYWQ=");
        let shown = format!("{value:?}");
        assert_eq!(shown, "otv1:<sealed>");
        assert_eq!(format!("{:?}", sealed("plain")), "<sealed>");
    }

    #[test]
    fn associated_data_layout_is_frozen() {
        let aad = associated_data(
            "ssh-password",
            &[0xab; 16],
            &[b"h", &22u16.to_be_bytes(), b""],
        );
        let mut expected = b"oneterm-vault\0\x01\x0cssh-password".to_vec();
        expected.extend_from_slice(&[0xab; 16]);
        expected.extend_from_slice(&[0, 0, 0, 1, b'h', 0, 0, 0, 2, 0, 22, 0, 0, 0, 0]);
        assert_eq!(aad, expected);
    }
}
