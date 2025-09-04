use common::merkle_tree_public::TreeHashType;
use elliptic_curve::PrimeField;
use k256::{AffinePoint, FieldBytes, Scalar};
use rand::{RngCore, rngs::OsRng};
use serde::{Deserialize, Serialize};
use sha2::{Digest, digest::FixedOutput};

use super::constants_types::{NULLIFIER_SECRET_CONST, VIEWING_SECRET_CONST};

#[derive(Debug)]
///Seed holder. Non-clonable to ensure that different holders use different seeds.
/// Produces `TopSecretKeyHolder` objects.
pub struct SeedHolder {
    seed: Scalar,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
///Secret spending key holder. Produces `UTXOSecretKeyHolder` objects.
pub struct TopSecretKeyHolder {
    pub secret_spending_key: Scalar,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
///Nullifier secret key and viewing secret key holder. Produces public keys. Can produce address. Can produce shared secret for recepient.
pub struct UTXOSecretKeyHolder {
    pub nullifier_secret_key: Scalar,
    pub viewing_secret_key: Scalar,
}

impl SeedHolder {
    pub fn new_os_random() -> Self {
        let mut bytes = FieldBytes::default();

        OsRng.fill_bytes(&mut bytes);

        Self {
            seed: Scalar::from_repr(bytes).unwrap(),
        }
    }

    pub fn generate_secret_spending_key_hash(&self) -> TreeHashType {
        let mut hasher = sha2::Sha256::new();

        hasher.update(self.seed.to_bytes());

        <TreeHashType>::from(hasher.finalize_fixed())
    }

    pub fn generate_secret_spending_key_scalar(&self) -> Scalar {
        let hash = self.generate_secret_spending_key_hash();

        Scalar::from_repr(hash.into()).unwrap()
    }

    pub fn produce_top_secret_key_holder(&self) -> TopSecretKeyHolder {
        TopSecretKeyHolder {
            secret_spending_key: self.generate_secret_spending_key_scalar(),
        }
    }
}

impl TopSecretKeyHolder {
    pub fn generate_nullifier_secret_key(&self) -> Scalar {
        let mut hasher = sha2::Sha256::new();

        hasher.update(self.secret_spending_key.to_bytes());
        hasher.update(*NULLIFIER_SECRET_CONST);

        let hash = <TreeHashType>::from(hasher.finalize_fixed());

        Scalar::from_repr(hash.into()).unwrap()
    }

    pub fn generate_viewing_secret_key(&self) -> Scalar {
        let mut hasher = sha2::Sha256::new();

        hasher.update(self.secret_spending_key.to_bytes());
        hasher.update(*VIEWING_SECRET_CONST);

        let hash = <TreeHashType>::from(hasher.finalize_fixed());

        Scalar::from_repr(hash.into()).unwrap()
    }

    pub fn produce_utxo_secret_holder(&self) -> UTXOSecretKeyHolder {
        UTXOSecretKeyHolder {
            nullifier_secret_key: self.generate_nullifier_secret_key(),
            viewing_secret_key: self.generate_viewing_secret_key(),
        }
    }
}

impl UTXOSecretKeyHolder {
    pub fn generate_nullifier_public_key(&self) -> AffinePoint {
        (AffinePoint::GENERATOR * self.nullifier_secret_key).into()
    }

    pub fn generate_viewing_public_key(&self) -> AffinePoint {
        (AffinePoint::GENERATOR * self.viewing_secret_key).into()
    }
}
