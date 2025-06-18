use aes_gcm::{aead::Aead, AeadCore, Aes256Gcm, KeyInit};
use elliptic_curve::point::AffineCoordinates;
use elliptic_curve::PrimeField;
use k256::{AffinePoint, FieldBytes, Scalar};
use log::info;
use rand::{rngs::OsRng, RngCore};

use super::constants_types::{CipherText, Nonce};

#[derive(Debug)]
///Ephemeral secret key holder. Non-clonable as intended for one-time use. Produces ephemeral public keys. Can produce shared secret for sender.
pub struct EphemeralKeyHolder {
    ephemeral_secret_key: Scalar,
}

impl EphemeralKeyHolder {
    pub fn new_os_random() -> Self {
        let mut bytes = FieldBytes::default();

        OsRng.fill_bytes(&mut bytes);

        Self {
            ephemeral_secret_key: Scalar::from_repr(bytes).unwrap(),
        }
    }

    pub fn generate_ephemeral_public_key(&self) -> AffinePoint {
        (AffinePoint::GENERATOR * self.ephemeral_secret_key).into()
    }

    pub fn calculate_shared_secret_sender(
        &self,
        viewing_public_key_receiver: AffinePoint,
    ) -> AffinePoint {
        (viewing_public_key_receiver * self.ephemeral_secret_key).into()
    }

    pub fn encrypt_data(
        &self,
        viewing_public_key_receiver: AffinePoint,
        data: &[u8],
    ) -> (CipherText, Nonce) {
        let shared_secret = self.calculate_shared_secret_sender(viewing_public_key_receiver);
        let cipher = Aes256Gcm::new(&shared_secret.x());
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);

        (cipher.encrypt(&nonce, data).unwrap(), nonce)
    }

    pub fn log(&self) {
        info!(
            "Ephemeral private key is {:?}",
            hex::encode(serde_json::to_vec(&self.ephemeral_secret_key).unwrap())
        );
    }
}
