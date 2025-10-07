use nssa_core::{
    NullifierPublicKey, SharedSecretKey,
    encryption::{EphemeralPublicKey, EphemeralSecretKey, IncomingViewingPublicKey},
};
use rand::{RngCore, rngs::OsRng};
use sha2::Digest;

#[derive(Debug)]
///Ephemeral secret key holder. Non-clonable as intended for one-time use. Produces ephemeral public keys. Can produce shared secret for sender.
pub struct EphemeralKeyHolder {
    ephemeral_secret_key: EphemeralSecretKey,
}

pub fn produce_one_sided_shared_secret_receiver(
    ipk: &IncomingViewingPublicKey,
) -> (SharedSecretKey, EphemeralPublicKey) {
    let mut esk = [0; 32];
    OsRng.fill_bytes(&mut esk);
    (
        SharedSecretKey::new(&esk, ipk),
        EphemeralPublicKey::from_scalar(esk),
    )
}

impl EphemeralKeyHolder {
    pub fn new(receiver_nullifier_public_key: &NullifierPublicKey) -> Self {
        let mut nonce_bytes = [0; 16];
        OsRng.fill_bytes(&mut nonce_bytes);
        let mut hasher = sha2::Sha256::new();
        hasher.update(receiver_nullifier_public_key);
        hasher.update(nonce_bytes);

        Self {
            ephemeral_secret_key: hasher.finalize().into(),
        }
    }

    pub fn generate_ephemeral_public_key(&self) -> EphemeralPublicKey {
        EphemeralPublicKey::from_scalar(self.ephemeral_secret_key)
    }

    pub fn calculate_shared_secret_sender(
        &self,
        receiver_incoming_viewing_public_key: &IncomingViewingPublicKey,
    ) -> SharedSecretKey {
        SharedSecretKey::new(
            &self.ephemeral_secret_key,
            receiver_incoming_viewing_public_key,
        )
    }
}
