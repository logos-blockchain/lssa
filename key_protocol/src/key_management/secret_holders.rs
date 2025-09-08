use bip39::Mnemonic;
use common::merkle_tree_public::TreeHashType;
use elliptic_curve::PrimeField;
use k256::{AffinePoint, Scalar};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, digest::FixedOutput};

#[derive(Debug)]
///Seed holder. Non-clonable to ensure that different holders use different seeds.
/// Produces `TopSecretKeyHolder` objects.
pub struct SeedHolder {
    //ToDo: Needs to be vec as serde derives is not implemented for [u8; 64]
    pub(crate) seed: Vec<u8>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
///Secret spending key holder. Produces `PrivateKeyHolder` objects.
pub struct TopSecretKeyHolder {
    pub(crate) secret_spending_key: [u8; 32],
}

#[derive(Serialize, Deserialize, Debug, Clone)]
///Private key holder. Produces public keys. Can produce address. Can produce shared secret for recepient.
pub struct PrivateKeyHolder {
    pub(crate) nullifier_secret_key: [u8; 32],
    pub(crate) incoming_viewing_secret_key: Scalar,
    pub(crate) outgoing_viewing_secret_key: Scalar,
}

impl SeedHolder {
    pub fn new_os_random() -> Self {
        let mut enthopy_bytes: [u8; 32] = [0; 32];
        OsRng.fill_bytes(&mut enthopy_bytes);

        let mnemonic = Mnemonic::from_entropy(&enthopy_bytes).unwrap();
        let seed_wide = mnemonic.to_seed("mnemonic");

        Self {
            seed: seed_wide.to_vec(),
        }
    }

    pub fn generate_secret_spending_key_hash(&self) -> TreeHashType {
        let mut hash = hmac_sha512::HMAC::mac(&self.seed, "NSSA_seed");

        for _ in 1..2048 {
            hash = hmac_sha512::HMAC::mac(hash, "NSSA_seed");
        }

        //Safe unwrap
        *hash.first_chunk::<32>().unwrap()
    }

    pub fn produce_top_secret_key_holder(&self) -> TopSecretKeyHolder {
        TopSecretKeyHolder {
            secret_spending_key: self.generate_secret_spending_key_hash(),
        }
    }
}

impl TopSecretKeyHolder {
    pub fn generate_nullifier_secret_key(&self) -> [u8; 32] {
        let mut hasher = sha2::Sha256::new();

        hasher.update("NSSA_keys");
        hasher.update(self.secret_spending_key);
        hasher.update([1u8]);
        hasher.update([0u8; 176]);

        <TreeHashType>::from(hasher.finalize_fixed())
    }

    pub fn generate_incloming_viewing_secret_key(&self) -> Scalar {
        let mut hasher = sha2::Sha256::new();

        hasher.update("NSSA_keys");
        hasher.update(self.secret_spending_key);
        hasher.update([2u8]);
        hasher.update([0u8; 176]);

        let hash = <TreeHashType>::from(hasher.finalize_fixed());

        Scalar::from_repr(hash.into()).unwrap()
    }

    pub fn generate_outgoing_viewing_secret_key(&self) -> Scalar {
        let mut hasher = sha2::Sha256::new();

        hasher.update("NSSA_keys");
        hasher.update(self.secret_spending_key);
        hasher.update([3u8]);
        hasher.update([0u8; 176]);

        let hash = <TreeHashType>::from(hasher.finalize_fixed());

        Scalar::from_repr(hash.into()).unwrap()
    }

    pub fn produce_private_key_holder(&self) -> PrivateKeyHolder {
        PrivateKeyHolder {
            nullifier_secret_key: self.generate_nullifier_secret_key(),
            incoming_viewing_secret_key: self.generate_incloming_viewing_secret_key(),
            outgoing_viewing_secret_key: self.generate_outgoing_viewing_secret_key(),
        }
    }
}

impl PrivateKeyHolder {
    pub fn generate_nullifier_public_key(&self) -> [u8; 32] {
        let mut hasher = sha2::Sha256::new();

        hasher.update("NSSA_keys");
        hasher.update(self.nullifier_secret_key);
        hasher.update([7u8]);
        hasher.update([0u8; 176]);

        <TreeHashType>::from(hasher.finalize_fixed())
    }

    pub fn generate_incoming_viewing_public_key(&self) -> AffinePoint {
        (AffinePoint::GENERATOR * self.incoming_viewing_secret_key).into()
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

        let _ = top_secret_key_holder.generate_incloming_viewing_secret_key();
    }

    #[test]
    fn ovs_generation_test() {
        let seed_holder = SeedHolder::new_os_random();

        assert_eq!(seed_holder.seed.len(), 64);

        let top_secret_key_holder = seed_holder.produce_top_secret_key_holder();

        let _ = top_secret_key_holder.generate_outgoing_viewing_secret_key();
    }
}
