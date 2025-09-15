use common::TreeHashType;
use log::info;
use nssa_core::{
    NullifierPublicKey, SharedSecretKey,
    encryption::{EphemeralPublicKey, IncomingViewingPublicKey},
};
use secret_holders::{PrivateKeyHolder, SecretSpendingKey, SeedHolder};
use serde::{Deserialize, Serialize};
use sha2::{Digest, digest::FixedOutput};

pub type PublicAccountSigningKey = [u8; 32];

pub mod ephemeral_key_holder;
pub mod secret_holders;

#[derive(Serialize, Deserialize, Clone, Debug)]
///Entrypoint to key management
pub struct KeyChain {
    secret_spending_key: SecretSpendingKey,
    pub private_key_holder: PrivateKeyHolder,
    pub nullifer_public_key: NullifierPublicKey,
    pub incoming_viewing_public_key: IncomingViewingPublicKey,
}

impl KeyChain {
    pub fn new_os_random() -> Self {
        //Currently dropping SeedHolder at the end of initialization.
        //Now entirely sure if we need it in the future.
        let seed_holder = SeedHolder::new_os_random();
        let secret_spending_key = seed_holder.produce_top_secret_key_holder();

        let private_key_holder = secret_spending_key.produce_private_key_holder();

        let nullifer_public_key = private_key_holder.generate_nullifier_public_key();
        let incoming_viewing_public_key = private_key_holder.generate_incoming_viewing_public_key();

        Self {
            secret_spending_key,
            private_key_holder,
            nullifer_public_key,
            incoming_viewing_public_key,
        }
    }

    pub fn produce_user_address(&self) -> [u8; 32] {
        let mut hasher = sha2::Sha256::new();

        hasher.update(&self.nullifer_public_key);
        hasher.update(self.incoming_viewing_public_key.to_bytes());

        <TreeHashType>::from(hasher.finalize_fixed())
    }

    pub fn calculate_shared_secret_receiver(
        &self,
        ephemeral_public_key_sender: EphemeralPublicKey,
    ) -> SharedSecretKey {
        SharedSecretKey::new(
            &self
                .secret_spending_key
                .generate_incoming_viewing_secret_key(),
            &ephemeral_public_key_sender,
        )
    }

    pub fn log(&self) {
        info!(
            "Secret spending key is {:?}",
            hex::encode(serde_json::to_vec(&self.secret_spending_key).unwrap()),
        );
        info!(
            "Nulifier secret key is {:?}",
            hex::encode(serde_json::to_vec(&self.private_key_holder.nullifier_secret_key).unwrap()),
        );
        info!(
            "Viewing secret key is {:?}",
            hex::encode(
                serde_json::to_vec(&self.private_key_holder.incoming_viewing_secret_key).unwrap()
            ),
        );
        info!(
            "Viewing secret key is {:?}",
            hex::encode(
                serde_json::to_vec(&self.private_key_holder.outgoing_viewing_secret_key).unwrap()
            ),
        );
        info!(
            "Nullifier public key is {:?}",
            hex::encode(serde_json::to_vec(&self.nullifer_public_key).unwrap()),
        );
        info!(
            "Viewing public key is {:?}",
            hex::encode(serde_json::to_vec(&self.incoming_viewing_public_key).unwrap()),
        );
    }
}

#[cfg(test)]
mod tests {
    use aes_gcm::aead::OsRng;
    use elliptic_curve::ff::Field;
    use k256::{AffinePoint, Scalar};

    use super::*;

    #[test]
    fn test_new_os_random() {
        // Ensure that a new KeyChain instance can be created without errors.
        let address_key_holder = KeyChain::new_os_random();

        // Check that key holder fields are initialized with expected types
        assert_ne!(address_key_holder.nullifer_public_key.as_ref(), &[0u8; 32]);
    }

    #[test]
    fn test_calculate_shared_secret_receiver() {
        let address_key_holder = KeyChain::new_os_random();

        // Generate a random ephemeral public key sender
        let scalar = Scalar::random(&mut OsRng);
        let ephemeral_public_key_sender = EphemeralPublicKey::from_scalar(scalar);

        // Calculate shared secret
        let _shared_secret =
            address_key_holder.calculate_shared_secret_receiver(ephemeral_public_key_sender);
    }

    #[test]
    fn key_generation_test() {
        let seed_holder = SeedHolder::new_os_random();
        let top_secret_key_holder = seed_holder.produce_top_secret_key_holder();

        let utxo_secret_key_holder = top_secret_key_holder.produce_private_key_holder();

        let nullifer_public_key = utxo_secret_key_holder.generate_nullifier_public_key();
        let viewing_public_key = utxo_secret_key_holder.generate_incoming_viewing_public_key();

        let pub_account_signing_key = nssa::PrivateKey::new_os_random();

        let public_key = nssa::PublicKey::new_from_private_key(&pub_account_signing_key);

        let address = nssa::Address::from(&public_key);

        println!("======Prerequisites======");
        println!();

        println!(
            "Group generator {:?}",
            hex::encode(serde_json::to_vec(&AffinePoint::GENERATOR).unwrap())
        );
        println!();

        println!("======Holders======");
        println!();

        println!("{seed_holder:?}");
        println!("{top_secret_key_holder:?}");
        println!("{utxo_secret_key_holder:?}");
        println!();

        println!("======Public data======");
        println!();
        println!("Address{:?}", hex::encode(address.value()));
        println!(
            "Nulifier public key {:?}",
            hex::encode(serde_json::to_vec(&nullifer_public_key).unwrap())
        );
        println!(
            "Viewing public key {:?}",
            hex::encode(serde_json::to_vec(&viewing_public_key).unwrap())
        );
    }
}
