//! Argon2id: the master password to the key-encryption key (`crypto-and-format.md` § 3).

use argon2::{Algorithm, Argon2, Params, Version};
use oneterm_core::SecretString;
use zeroize::Zeroizing;

use crate::VaultError;

/// Argon2id cost parameters, stored in `vault.json` beside the salt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KdfParams {
    /// Memory cost in KiB.
    pub memory_kib: u32,
    /// Number of passes.
    pub iterations: u32,
    /// Number of lanes.
    pub parallelism: u32,
}

impl KdfParams {
    /// What set-up and a password change write: 64 MiB, 3 passes, 1 lane
    /// (RFC 9106 § 4's second option, one lane because the KDF runs its
    /// lanes sequentially).
    pub const DEFAULT: Self = Self {
        memory_kib: 64 * 1024,
        iterations: 3,
        parallelism: 1,
    };
    /// The least a header may ask for (OWASP's 19 MiB, 2 passes); below it
    /// the header reads as unsupported.
    pub const FLOOR: Self = Self {
        memory_kib: 19 * 1024,
        iterations: 2,
        parallelism: 1,
    };
    /// The most a header may ask for, so a tampered header cannot turn an
    /// unlock into a memory bomb.
    pub const CEILING: Self = Self {
        memory_kib: 1024 * 1024,
        iterations: 16,
        parallelism: 4,
    };

    /// Whether every parameter lies within [`Self::FLOOR`]..=[`Self::CEILING`].
    pub(crate) fn is_within_bounds(&self) -> bool {
        (Self::FLOOR.memory_kib..=Self::CEILING.memory_kib).contains(&self.memory_kib)
            && (Self::FLOOR.iterations..=Self::CEILING.iterations).contains(&self.iterations)
            && (Self::FLOOR.parallelism..=Self::CEILING.parallelism).contains(&self.parallelism)
    }

    /// Whether a header written with these parameters is cheaper to attack
    /// than one written now, so it should be rewrapped after an unlock.
    pub(crate) fn is_below_default(&self) -> bool {
        self.memory_kib < Self::DEFAULT.memory_kib || self.iterations < Self::DEFAULT.iterations
    }
}

/// Salt length written at set-up.
pub(crate) const SALT_LEN: usize = 16;
/// Salt lengths a header may carry.
pub(crate) const SALT_LEN_RANGE: std::ops::RangeInclusive<usize> = SALT_LEN..=64;
/// Length of the key-encryption key and of the data key.
pub(crate) const KEY_LEN: usize = 32;

/// The Argon2id identifier and version `vault.json` records.
pub(crate) const ALGORITHM_NAME: &str = "argon2id";
pub(crate) const ALGORITHM_VERSION: u32 = 0x13;

/// Derive the key-encryption key from the password bytes exactly as typed.
pub(crate) fn derive_kek(
    password: &SecretString,
    salt: &[u8],
    params: KdfParams,
) -> Result<Zeroizing<[u8; KEY_LEN]>, VaultError> {
    let params = Params::new(
        params.memory_kib,
        params.iterations,
        params.parallelism,
        Some(KEY_LEN),
    )
    .map_err(|error| VaultError::Unsupported(format!("Argon2id parameters: {error}")))?;
    let mut kek = Zeroizing::new([0u8; KEY_LEN]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(password.expose_secret().as_bytes(), salt, kek.as_mut())
        .map_err(|error| VaultError::Unsupported(format!("Argon2id: {error}")))?;
    Ok(kek)
}

#[cfg(test)]
mod tests {
    use argon2::{AssociatedData, ParamsBuilder};

    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    /// RFC 9106 § 5.3 Argon2id v0x13 vector, against the crate directly: the
    /// vector uses a secret and associated data, which OneTerm's wrapper
    /// never passes.
    #[test]
    fn argon2_crate_matches_rfc_9106_argon2id_vector() {
        let params = ParamsBuilder::new()
            .m_cost(32)
            .t_cost(3)
            .p_cost(4)
            .data(AssociatedData::new(&[0x04; 12]).unwrap())
            .build()
            .unwrap();
        let argon2 =
            Argon2::new_with_secret(&[0x03; 8], Algorithm::Argon2id, Version::V0x13, params)
                .unwrap();
        let mut out = [0u8; 32];
        argon2
            .hash_password_into(&[0x01; 32], &[0x02; 16], &mut out)
            .unwrap();
        assert_eq!(
            hex(&out),
            "0d640df58d78766c08c037a34a8b53c9d01ef0452d75b65eb52520e96b01e659"
        );
    }

    /// A frozen vector through OneTerm's own wrapper: if a crate update ever
    /// changed what a password derives, every existing vault would stop
    /// unlocking, and this test says so first.
    #[test]
    fn oneterm_wrapper_derivation_is_frozen() {
        let params = KdfParams {
            memory_kib: 64,
            iterations: 2,
            parallelism: 1,
        };
        let kek = derive_kek(
            &SecretString::new("correct horse battery staple"),
            &[0x02; SALT_LEN],
            params,
        )
        .unwrap();
        assert_eq!(hex(kek.as_ref()), FROZEN_KEK_HEX);
    }

    /// Cross-checked when frozen against an independent implementation
    /// (Python `cryptography` 44, `Argon2id(salt, length=32, iterations=2,
    /// lanes=1, memory_cost=64)`).
    const FROZEN_KEK_HEX: &str = "7c177df272a1b705e396ed8da3b16f8892b204ec523d65abfd310e2b2b927177";

    #[test]
    fn bounds_cover_floor_default_and_ceiling_only() {
        assert!(KdfParams::FLOOR.is_within_bounds());
        assert!(KdfParams::DEFAULT.is_within_bounds());
        assert!(KdfParams::CEILING.is_within_bounds());
        let below = KdfParams {
            memory_kib: KdfParams::FLOOR.memory_kib - 1,
            ..KdfParams::DEFAULT
        };
        let too_few_passes = KdfParams {
            iterations: 1,
            ..KdfParams::DEFAULT
        };
        let above = KdfParams {
            memory_kib: KdfParams::CEILING.memory_kib + 1,
            ..KdfParams::DEFAULT
        };
        let too_many_lanes = KdfParams {
            parallelism: 5,
            ..KdfParams::DEFAULT
        };
        for params in [below, too_few_passes, above, too_many_lanes] {
            assert!(!params.is_within_bounds(), "{params:?}");
        }
        assert!(KdfParams::FLOOR.is_below_default());
        assert!(!KdfParams::DEFAULT.is_below_default());
    }
}
