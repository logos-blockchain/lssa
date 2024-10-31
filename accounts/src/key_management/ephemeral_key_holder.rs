use aes_gcm::{aead::Aead, AeadCore, Aes256Gcm, Key, KeyInit};
use elliptic_curve::group::GroupEncoding;
use elliptic_curve::PrimeField;
use k256::{AffinePoint, FieldBytes, Scalar};
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
        let key_point = self.calculate_shared_secret_sender(viewing_public_key_receiver);
        let key_raw = key_point.to_bytes();
        let key_raw_adjust: [u8; 32] = key_raw.as_slice().try_into().unwrap();

        let key: Key<Aes256Gcm> = key_raw_adjust.into();

        let cipher = Aes256Gcm::new(&key);
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);

        (cipher.encrypt(&nonce, data).unwrap(), nonce)
    }
}
