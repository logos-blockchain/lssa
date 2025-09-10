use elliptic_curve::PrimeField;
use k256::{AffinePoint, Scalar};
use log::info;
use sha2::Digest;

#[derive(Debug)]
///Ephemeral secret key holder. Non-clonable as intended for one-time use. Produces ephemeral public keys. Can produce shared secret for sender.
pub struct EphemeralKeyHolder {
    ephemeral_secret_key: Scalar,
}

impl EphemeralKeyHolder {
    pub fn new(
        receiver_nullifier_public_key: [u8; 32],
        sender_outgoing_viewing_secret_key: Scalar,
        nonce: u64,
    ) -> Self {
        let mut hasher = sha2::Sha256::new();
        hasher.update(receiver_nullifier_public_key);
        hasher.update(nonce.to_le_bytes());
        hasher.update([0; 192]);

        let hash_recepient = hasher.finalize();

        let mut hasher = sha2::Sha256::new();
        hasher.update(sender_outgoing_viewing_secret_key.to_bytes());
        hasher.update(hash_recepient);

        Self {
            ephemeral_secret_key: Scalar::from_repr(hasher.finalize()).unwrap(),
        }
    }

    pub fn generate_ephemeral_public_key(&self) -> AffinePoint {
        (AffinePoint::GENERATOR * self.ephemeral_secret_key).into()
    }

    pub fn calculate_shared_secret_sender(
        &self,
        receiver_incoming_viewing_public_key: Scalar,
    ) -> Scalar {
        receiver_incoming_viewing_public_key * self.ephemeral_secret_key
    }

    pub fn log(&self) {
        info!(
            "Ephemeral private key is {:?}",
            hex::encode(serde_json::to_vec(&self.ephemeral_secret_key).unwrap())
        );
    }
}
