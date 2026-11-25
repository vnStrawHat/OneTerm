//! What a vault is written as (specs/oneterm, "Saved passwords"): the document of OneTerm 0.6's
//! `vault.json`, and its `otv1:` envelopes, byte for byte, so what OneTerm 0.6 sealed OneTerm opens.
//!
//! A document names the vault (16 bytes), the cost of Argon2id, its salt, and the data key
//! sealed under the key the master password derives. An envelope is `otv1:` and the base64 of
//! the vault's first 8 bytes, a nonce of 12, the ciphertext and a tag of 16 (AES-256-GCM).

const TAG: &str = "otv1:";
const KID: usize = 8;
/// A nonce and a tag: what an envelope holds beside the vault's bytes and the ciphertext.
const AROUND: usize = 12 + 16;

/// The cost a new vault is made with, and the least and the most a document may ask for:
/// memory in KiB, passes, lanes. One outside them is not derived with.
pub(crate) const COST: [u32; 3] = [64 * 1024, 3, 1];
const LEAST: [u32; 3] = [19 * 1024, 2, 1];
const MOST: [u32; 3] = [1024 * 1024, 16, 4];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Header {
    pub id: [u8; 16],
    pub cost: [u32; 3],
    pub salt: Vec<u8>,
    /// The data key's envelope.
    pub wrapped: String,
}

impl Header {
    /// A document, if it is one this version reads: schema 1, Argon2id of version 19 within
    /// the bounds, a salt of 16 to 64 bytes, and a wrapped key of this vault.
    pub fn parse(json: &str) -> Option<Header> {
        let all = quadra_data::parse_json(json).ok()?;
        let kdf = all.get("kdf")?;
        let text = |of: &quadra_data::Value, key: &str| match of.get(key)? {
            quadra_data::Value::Text(text) => Some(text.clone()),
            _ => None,
        };
        let number = |of: &quadra_data::Value, key: &str| match of.get(key)? {
            quadra_data::Value::Number(written) => written.parse::<u32>().ok(),
            _ => None,
        };
        let known = number(&all, "schema_version") == Some(1)
            && text(kdf, "algorithm").as_deref() == Some("argon2id")
            && number(kdf, "version") == Some(19);
        let cost = [number(kdf, "memory_kib")?, number(kdf, "iterations")?, number(kdf, "parallelism")?];
        let within = (0..3).all(|at| (LEAST[at]..=MOST[at]).contains(&cost[at]));
        let id: [u8; 16] = super::unhex(&text(&all, "vault_id")?)?.try_into().ok()?;
        let (salt, wrapped) = (unbase64(&text(kdf, "salt")?)?, text(&all, "wrapped_key")?);
        let whole = sealed(&wrapped, &id).is_some_and(|it| it.len() == AROUND + 32);
        (known && within && (16..=64).contains(&salt.len()) && whole).then_some(Header {
            id,
            cost,
            salt,
            wrapped,
        })
    }

    /// Whether an envelope is one of this vault.
    pub fn seals(&self, envelope: &str) -> bool {
        sealed(envelope, &self.id).is_some()
    }

    /// The document, as OneTerm 0.6 writes it.
    pub fn write(&self) -> String {
        format!(
            "{{\"schema_version\":1,\"vault_id\":\"{}\",\"kdf\":{{\"algorithm\":\"argon2id\",\"version\":19,\
             \"memory_kib\":{},\"iterations\":{},\"parallelism\":{},\"salt\":\"{}\"}},\"wrapped_key\":\"{}\"}}",
            super::hex(&self.id),
            self.cost[0],
            self.cost[1],
            self.cost[2],
            base64(&self.salt),
            self.wrapped
        )
    }
}

/// What a seal is bound to: `purpose` ("key-wrap", "ssh-password"), the vault, and each field
/// with its length before it.
pub(crate) fn bound(purpose: &str, id: &[u8; 16], fields: &[&[u8]]) -> Vec<u8> {
    let mut all = b"oneterm-vault\0\x01".to_vec();
    all.push(purpose.len() as u8);
    all.extend_from_slice(purpose.as_bytes());
    all.extend_from_slice(id);
    for field in fields {
        all.extend_from_slice(&(field.len() as u32).to_be_bytes());
        all.extend_from_slice(field);
    }
    all
}

/// The envelope of what `quadra_shell::seal` gave (the nonce, the ciphertext, the tag).
pub(crate) fn envelope(id: &[u8; 16], sealed: &[u8]) -> String {
    format!("{TAG}{}", base64(&[&id[..KID], sealed].concat()))
}

/// What an envelope of this vault holds for `quadra_shell::open`; none of another vault's, of
/// another version's, or of anything else.
pub(crate) fn sealed(envelope: &str, id: &[u8; 16]) -> Option<Vec<u8>> {
    let all = unbase64(envelope.strip_prefix(TAG)?)?;
    (all.len() >= KID + AROUND && all[..KID] == id[..KID]).then(|| all[KID..].to_vec())
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Base64 as RFC 4648 says, padded.
pub(crate) fn base64(bytes: &[u8]) -> String {
    let mut out = String::new();
    for three in bytes.chunks(3) {
        let n = three.iter().fold(0u32, |n, b| n << 8 | u32::from(*b)) << (8 * (3 - three.len()));
        for at in 0..4 {
            let sextet = (n >> (18 - 6 * at)) & 63;
            out.push(if at <= three.len() { ALPHABET[sextet as usize] as char } else { '=' });
        }
    }
    out
}

/// The bytes of padded base64; none of anything else.
pub(crate) fn unbase64(text: &str) -> Option<Vec<u8>> {
    let text = text.as_bytes();
    if !text.len().is_multiple_of(4) {
        return None;
    }
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    for (at, four) in text.chunks(4).enumerate() {
        let pads = four.iter().rev().take_while(|b| **b == b'=').count();
        if pads > 2 || (pads > 0 && at + 1 != text.len() / 4) {
            return None;
        }
        let mut n = 0u32;
        for b in &four[..4 - pads] {
            n = n << 6 | ALPHABET.iter().position(|it| it == b)? as u32;
        }
        n <<= 6 * pads;
        out.extend_from_slice(&n.to_be_bytes()[1..4 - pads]);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_as_rfc_4648_says_both_ways() {
        for (plain, coded) in [("", ""), ("f", "Zg=="), ("fo", "Zm8="), ("foo", "Zm9v"), ("foob", "Zm9vYg==")]
        {
            assert_eq!(base64(plain.as_bytes()), coded);
            assert_eq!(unbase64(coded).as_deref(), Some(plain.as_bytes()));
        }
        for wrong in ["Zg", "Zg=", "Z===", "Zg==Zg==", "Zm9!"] {
            assert_eq!(unbase64(wrong), None, "{wrong}");
        }
    }

    #[test]
    fn what_a_seal_is_bound_to_is_laid_out_as_oneterm_lays_it() {
        let all = bound("ssh-password", &[0xab; 16], &[b"h", &22u16.to_be_bytes(), b""]);
        let mut wanted = b"oneterm-vault\0\x01\x0cssh-password".to_vec();
        wanted.extend_from_slice(&[0xab; 16]);
        wanted.extend_from_slice(&[0, 0, 0, 1, b'h', 0, 0, 0, 2, 0, 22, 0, 0, 0, 0]);
        assert_eq!(all, wanted);
    }

    #[test]
    fn a_document_is_read_within_the_bounds_and_written_back() {
        let header = Header::parse(crate::tests::ONETERM_S).expect("OneTerm 0.6's document");
        assert_eq!((header.cost, header.salt.as_slice()), ([19456, 2, 1], &b"defghijklmnopqrs"[..]));
        assert_eq!(Header::parse(&header.write()), Some(header.clone()));
        let with = |was: &str, now: &str| Header::parse(&crate::tests::ONETERM_S.replace(was, now));
        assert!(with("19456", "19455").is_none(), "less memory than the least");
        assert!(with("\"iterations\": 2", "\"iterations\": 17").is_none());
        assert!(with("\"schema_version\": 1", "\"schema_version\": 9").is_none());
        assert!(with("argon2id", "argon2i").is_none());
        assert!(with("3f9c0a4e", "00000000").is_none(), "a wrapped key of another vault");
        assert!(with("otv1:", "otv2:").is_none() && Header::parse("{}").is_none());
    }
}
