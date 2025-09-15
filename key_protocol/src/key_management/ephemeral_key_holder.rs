use elliptic_curve::PrimeField;
use k256::Scalar;
use log::info;
use nssa_core::{
    NullifierPublicKey, SharedSecretKey,
    encryption::{EphemeralPublicKey, EphemeralSecretKey, IncomingViewingPublicKey},
};
use sha2::Digest;

use crate::key_management::secret_holders::OutgoingViewingSecretKey;

#[derive(Debug)]
///Ephemeral secret key holder. Non-clonable as intended for one-time use. Produces ephemeral public keys. Can produce shared secret for sender.
pub struct EphemeralKeyHolder {
    ephemeral_secret_key: EphemeralSecretKey,
}

impl EphemeralKeyHolder {
    pub fn new(
        receiver_nullifier_public_key: NullifierPublicKey,
        sender_outgoing_viewing_secret_key: OutgoingViewingSecretKey,
        nonce: u64,
    ) -> Self {
        let mut hasher = sha2::Sha256::new();
        hasher.update(receiver_nullifier_public_key);
        hasher.update(nonce.to_le_bytes());
        hasher.update([0; 24]);

        let hash_recepient = hasher.finalize();

        let mut hasher = sha2::Sha256::new();
        hasher.update(sender_outgoing_viewing_secret_key.to_bytes());
        hasher.update(hash_recepient);

        Self {
            ephemeral_secret_key: Scalar::from_repr(hasher.finalize()).unwrap(),
        }
    }

    pub fn generate_ephemeral_public_key(&self) -> EphemeralPublicKey {
        EphemeralPublicKey::from_scalar(self.ephemeral_secret_key)
    }

    pub fn calculate_shared_secret_sender(
        &self,
        receiver_incoming_viewing_public_key: IncomingViewingPublicKey,
    ) -> SharedSecretKey {
        SharedSecretKey::new(
            &self.ephemeral_secret_key,
            &receiver_incoming_viewing_public_key,
        )
    }

    pub fn log(&self) {
        info!(
            "Ephemeral private key is {:?}",
            hex::encode(serde_json::to_vec(&self.ephemeral_secret_key).unwrap())
        );
    }
}
