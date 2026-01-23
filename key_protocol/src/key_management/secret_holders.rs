use bip39::Mnemonic;
use common::HashType;
use nssa_core::{
    NullifierPublicKey, NullifierSecretKey,
    encryption::{IncomingViewingPublicKey, Scalar},
};
use rand::{RngCore, rngs::OsRng};
use serde::{Deserialize, Serialize};
use sha2::{Digest, digest::FixedOutput};

const NSSA_ENTROPY_BYTES: [u8; 32] = [0; 32];

#[derive(Debug)]
/// Seed holder. Non-clonable to ensure that different holders use different seeds.
/// Produces `TopSecretKeyHolder` objects.
pub struct SeedHolder {
    // ToDo: Needs to be vec as serde derives is not implemented for [u8; 64]
    pub(crate) seed: Vec<u8>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
/// Secret spending key object. Can produce `PrivateKeyHolder` objects.
pub struct SecretSpendingKey(pub(crate) [u8; 32]);

pub type IncomingViewingSecretKey = Scalar;
pub type OutgoingViewingSecretKey = Scalar;

#[derive(Serialize, Deserialize, Debug, Clone)]
/// Private key holder. Produces public keys. Can produce account_id. Can produce shared secret for
/// recepient.
pub struct PrivateKeyHolder {
    pub nullifier_secret_key: NullifierSecretKey,
    pub(crate) incoming_viewing_secret_key: IncomingViewingSecretKey,
}

impl SeedHolder {
    pub fn new_os_random() -> Self {
        let mut enthopy_bytes: [u8; 32] = [0; 32];
        OsRng.fill_bytes(&mut enthopy_bytes);

        let mnemonic = Mnemonic::from_entropy(&enthopy_bytes)
            .expect("Enthropy must be a multiple of 32 bytes");
        let seed_wide = mnemonic.to_seed("mnemonic");

        Self {
            seed: seed_wide.to_vec(),
        }
    }

    pub fn new_mnemonic(passphrase: String) -> Self {
        let mnemonic = Mnemonic::from_entropy(&NSSA_ENTROPY_BYTES)
            .expect("Enthropy must be a multiple of 32 bytes");
        let seed_wide = mnemonic.to_seed(passphrase);

        Self {
            seed: seed_wide.to_vec(),
        }
    }

    pub fn generate_secret_spending_key_hash(&self) -> HashType {
        let mut hash = hmac_sha512::HMAC::mac(&self.seed, "NSSA_seed");

        for _ in 1..2048 {
            hash = hmac_sha512::HMAC::mac(hash, "NSSA_seed");
        }

        // Safe unwrap
        *hash.first_chunk::<32>().unwrap()
    }

    pub fn produce_top_secret_key_holder(&self) -> SecretSpendingKey {
        SecretSpendingKey(self.generate_secret_spending_key_hash())
    }
}

impl SecretSpendingKey {
    pub fn generate_nullifier_secret_key(&self) -> NullifierSecretKey {
        let mut hasher = sha2::Sha256::new();

        hasher.update("LEE/keys");
        hasher.update(self.0);
        hasher.update([1u8]);
        hasher.update([0u8; 23]);

        <NullifierSecretKey>::from(hasher.finalize_fixed())
    }

    pub fn generate_viewing_secret_key(&self) -> IncomingViewingSecretKey {
        let mut hasher = sha2::Sha256::new();

        hasher.update("LEE/keys");
        hasher.update(self.0);
        hasher.update([2u8]);
        hasher.update([0u8; 23]);

        <HashType>::from(hasher.finalize_fixed())
    }

    pub fn generate_outgoing_viewing_secret_key(&self) -> OutgoingViewingSecretKey {
        let mut hasher = sha2::Sha256::new();

        hasher.update("NSSA_keys");
        hasher.update(self.0);
        hasher.update([3u8]);
        hasher.update([0u8; 22]);

        <HashType>::from(hasher.finalize_fixed())
    }

    pub fn produce_private_key_holder(&self) -> PrivateKeyHolder {
        PrivateKeyHolder {
            nullifier_secret_key: self.generate_nullifier_secret_key(),
            incoming_viewing_secret_key: self.generate_viewing_secret_key(),
        }
    }

    pub fn generate_child_nullifier_secret_key(&self, cci: u32) -> NullifierSecretKey {
        let mut key = vec![];
        key.extend_from_slice(b"LEE/chain");

        let mut input = vec![];

        input.extend_from_slice(&self.0);
        input.extend_from_slice(&[1u8]);
        input.extend_from_slice(&cci.to_le_bytes());
        input.extend_from_slice(&[0u8; 22]);

        let hash_value = hmac_sha512::HMAC::mac(input, key);

        *hash_value
            .first_chunk::<32>()
            .expect("hash_value is 64 bytes, must be safe to get first 32")
    }

    pub fn generate_child_viewing_secret_key(&self, cci: u32) -> IncomingViewingSecretKey {
        let mut key = vec![];
        key.extend_from_slice(b"LEE/chain");

        let mut input = vec![];

        input.extend_from_slice(&self.0);
        input.extend_from_slice(&[2u8]);
        input.extend_from_slice(&cci.to_le_bytes());
        input.extend_from_slice(&[0u8; 22]);

        let hash_value = hmac_sha512::HMAC::mac(input, key);

        *hash_value
            .first_chunk::<32>()
            .expect("hash_value is 64 bytes, must be safe to get first 32")
    }
}

impl PrivateKeyHolder {
    pub fn generate_nullifier_public_key(&self) -> NullifierPublicKey {
        (&self.nullifier_secret_key).into()
    }

    pub fn generate_incoming_viewing_public_key(&self) -> IncomingViewingPublicKey {
        IncomingViewingPublicKey::from_scalar(self.incoming_viewing_secret_key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_generation_test() {
        let seed_holder = SeedHolder::new_os_random();

        assert_eq!(seed_holder.seed.len(), 64);
    }

    #[test]
    fn ssk_generation_test() {
        let seed_holder = SeedHolder::new_os_random();

        assert_eq!(seed_holder.seed.len(), 64);

        let _ = seed_holder.generate_secret_spending_key_hash();
    }

    #[test]
    fn ivs_generation_test() {
        let seed_holder = SeedHolder::new_os_random();

        assert_eq!(seed_holder.seed.len(), 64);

        let top_secret_key_holder = seed_holder.produce_top_secret_key_holder();

        let _ = top_secret_key_holder.generate_viewing_secret_key();
    }

    #[test]
    fn ovs_generation_test() {
        let seed_holder = SeedHolder::new_os_random();

        assert_eq!(seed_holder.seed.len(), 64);

        let top_secret_key_holder = seed_holder.produce_top_secret_key_holder();

        let _ = top_secret_key_holder.generate_outgoing_viewing_secret_key();
    }

    #[test]
    fn two_seeds_generated_same_from_same_mnemonic() {
        let mnemonic = "test_pass";

        let seed_holder1 = SeedHolder::new_mnemonic(mnemonic.to_string());
        let seed_holder2 = SeedHolder::new_mnemonic(mnemonic.to_string());

        assert_eq!(seed_holder1.seed, seed_holder2.seed);
    }
}
