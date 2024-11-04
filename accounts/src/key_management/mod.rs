use aes_gcm::{aead::Aead, Aes256Gcm, Key, KeyInit};
use constants_types::{CipherText, Nonce};
use elliptic_curve::group::GroupEncoding;
use ephemeral_key_holder::EphemeralKeyHolder;
use k256::AffinePoint;
use secret_holders::{SeedHolder, TopSecretKeyHolder, UTXOSecretKeyHolder};
use storage::merkle_tree_public::TreeHashType;

pub mod constants_types;
pub mod ephemeral_key_holder;
pub mod secret_holders;

#[derive(Debug)]
///Entrypoint to key management
pub struct AddressKeyHolder {
    //Will be useful in future
    #[allow(dead_code)]
    top_secret_key_holder: TopSecretKeyHolder,
    utxo_secret_key_holder: UTXOSecretKeyHolder,
    pub address: TreeHashType,
    pub nullifer_public_key: AffinePoint,
    pub viewing_public_key: AffinePoint,
}

impl AddressKeyHolder {
    pub fn new_os_random() -> Self {
        //Currently dropping SeedHolder at the end of initialization.
        //Now entirely sure if we need it in the future.
        let seed_holder = SeedHolder::new_os_random();
        let top_secret_key_holder = seed_holder.produce_top_secret_key_holder();

        let utxo_secret_key_holder = top_secret_key_holder.produce_utxo_secret_holder();

        let address = utxo_secret_key_holder.generate_address();
        let nullifer_public_key = utxo_secret_key_holder.generate_nullifier_public_key();
        let viewing_public_key = utxo_secret_key_holder.generate_viewing_public_key();

        Self {
            top_secret_key_holder,
            utxo_secret_key_holder,
            address,
            nullifer_public_key,
            viewing_public_key,
        }
    }

    pub fn calculate_shared_secret_receiver(
        &self,
        ephemeral_public_key_sender: AffinePoint,
    ) -> AffinePoint {
        (ephemeral_public_key_sender * self.utxo_secret_key_holder.viewing_secret_key).into()
    }

    pub fn produce_ephemeral_key_holder(&self) -> EphemeralKeyHolder {
        EphemeralKeyHolder::new_os_random()
    }

    pub fn decrypt_data(
        &self,
        ephemeral_public_key_sender: AffinePoint,
        ciphertext: CipherText,
        nonce: Nonce,
    ) -> Vec<u8> {
        let key_point = self.calculate_shared_secret_receiver(ephemeral_public_key_sender);
        let binding = key_point.to_bytes();
        let key_raw = &binding.as_slice()[..32];
        let key_raw_adjust: [u8; 32] = key_raw.try_into().unwrap();

        let key: Key<Aes256Gcm> = key_raw_adjust.into();

        let cipher = Aes256Gcm::new(&key);

        cipher.decrypt(&nonce, ciphertext.as_slice()).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use aes_gcm::{
        aead::{Aead, KeyInit, OsRng},
        Aes256Gcm,
    };
    use constants_types::{CipherText, Nonce};
    use constants_types::{NULLIFIER_SECRET_CONST, VIEVING_SECRET_CONST};
    use elliptic_curve::ff::Field;
    use elliptic_curve::group::prime::PrimeCurveAffine;
    use elliptic_curve::group::GroupEncoding;
    use k256::{AffinePoint, ProjectivePoint, Scalar};

    use super::*;

    #[test]
    fn test_new_os_random() {
        // Ensure that a new AddressKeyHolder instance can be created without errors.
        let address_key_holder = AddressKeyHolder::new_os_random();

        // Check that key holder fields are initialized with expected types
        assert!(!Into::<bool>::into(
            address_key_holder.nullifer_public_key.is_identity()
        ));
        assert!(!Into::<bool>::into(
            address_key_holder.viewing_public_key.is_identity()
        ));
    }

    #[test]
    fn test_calculate_shared_secret_receiver() {
        let address_key_holder = AddressKeyHolder::new_os_random();

        // Generate a random ephemeral public key sender
        let scalar = Scalar::random(&mut OsRng);
        let ephemeral_public_key_sender = (ProjectivePoint::generator() * scalar).to_affine();

        // Calculate shared secret
        let shared_secret =
            address_key_holder.calculate_shared_secret_receiver(ephemeral_public_key_sender);

        // Ensure the shared secret is not an identity point (suggesting non-zero output)
        assert!(!Into::<bool>::into(shared_secret.is_identity()));
    }

    #[test]
    fn test_decrypt_data() {
        let address_key_holder = AddressKeyHolder::new_os_random();

        // Generate an ephemeral key and shared secret
        let scalar = Scalar::random(OsRng);
        let ephemeral_public_key_sender = address_key_holder
            .produce_ephemeral_key_holder()
            .generate_ephemeral_public_key();
        let shared_secret =
            address_key_holder.calculate_shared_secret_receiver(ephemeral_public_key_sender);

        // Prepare the encryption key from shared secret
        let key_raw = shared_secret.to_bytes();
        let key_raw_adjust_pre = &key_raw.as_slice()[..32];
        let key_raw_adjust: [u8; 32] = key_raw_adjust_pre.try_into().unwrap();
        let key: Key<Aes256Gcm> = key_raw_adjust.into();

        let cipher = Aes256Gcm::new(&key);

        // Encrypt sample data
        let nonce = Nonce::from_slice(b"unique nonce");
        let plaintext = b"Sensitive data";
        let ciphertext = cipher
            .encrypt(nonce, plaintext.as_ref())
            .expect("encryption failure");

        // Attempt decryption
        let decrypted_data: Vec<u8> = address_key_holder.decrypt_data(
            ephemeral_public_key_sender,
            CipherText::from(ciphertext),
            nonce.clone(),
        );

        // Verify decryption is successful and matches original plaintext
        assert_eq!(decrypted_data, plaintext);
    }

    #[test]
    fn test_new_os_random_initialization() {
        // Ensure that AddressKeyHolder is initialized correctly
        let address_key_holder = AddressKeyHolder::new_os_random();

        // Check that key holder fields are initialized with expected types and values
        assert!(!Into::<bool>::into(
            address_key_holder.nullifer_public_key.is_identity()
        ));
        assert!(!Into::<bool>::into(
            address_key_holder.viewing_public_key.is_identity()
        ));
        assert!(address_key_holder.address.as_slice().len() > 0); // Assume TreeHashType has non-zero length for a valid address
    }

    #[test]
    fn test_calculate_shared_secret_with_identity_point() {
        let address_key_holder = AddressKeyHolder::new_os_random();

        // Use identity point as ephemeral public key
        let identity_point = AffinePoint::identity();

        // Calculate shared secret
        let shared_secret = address_key_holder.calculate_shared_secret_receiver(identity_point);

        // The shared secret with the identity point should also result in the identity point
        assert!(Into::<bool>::into(shared_secret.is_identity()));
    }

    #[test]
    #[should_panic]
    fn test_decrypt_data_with_incorrect_nonce() {
        let address_key_holder = AddressKeyHolder::new_os_random();

        // Generate ephemeral public key and shared secret
        let scalar = Scalar::random(OsRng);
        let ephemeral_public_key_sender = (ProjectivePoint::generator() * scalar).to_affine();
        let shared_secret =
            address_key_holder.calculate_shared_secret_receiver(ephemeral_public_key_sender);

        // Prepare the encryption key from shared secret
        let key_raw = shared_secret.to_bytes();
        let key_raw_adjust_pre = &key_raw.as_slice()[..32];
        let key_raw_adjust: [u8; 32] = key_raw_adjust_pre.try_into().unwrap();
        let key: Key<Aes256Gcm> = key_raw_adjust.into();

        let cipher = Aes256Gcm::new(&key);

        // Encrypt sample data with a specific nonce
        let nonce = Nonce::from_slice(b"unique nonce");
        let plaintext = b"Sensitive data";
        let ciphertext = cipher
            .encrypt(nonce, plaintext.as_ref())
            .expect("encryption failure");

        // Attempt decryption with an incorrect nonce
        let incorrect_nonce = Nonce::from_slice(b"wrong nonce");
        let decrypted_data = address_key_holder.decrypt_data(
            ephemeral_public_key_sender,
            CipherText::from(ciphertext.clone()),
            incorrect_nonce.clone(),
        );

        // The decryption should fail or produce incorrect output due to nonce mismatch
        assert_ne!(decrypted_data, plaintext);
    }

    #[test]
    #[should_panic]
    fn test_decrypt_data_with_incorrect_ciphertext() {
        let address_key_holder = AddressKeyHolder::new_os_random();

        // Generate ephemeral public key and shared secret
        let scalar = Scalar::random(OsRng);
        let ephemeral_public_key_sender = (ProjectivePoint::generator() * scalar).to_affine();
        let shared_secret =
            address_key_holder.calculate_shared_secret_receiver(ephemeral_public_key_sender);

        // Prepare the encryption key from shared secret
        let key_raw = shared_secret.to_bytes();
        let key_raw_adjust_pre = &key_raw.as_slice()[..32];
        let key_raw_adjust: [u8; 32] = key_raw_adjust_pre.try_into().unwrap();
        let key: Key<Aes256Gcm> = key_raw_adjust.into();

        let cipher = Aes256Gcm::new(&key);

        // Encrypt sample data
        let nonce = Nonce::from_slice(b"unique nonce");
        let plaintext = b"Sensitive data";
        let ciphertext = cipher
            .encrypt(nonce, plaintext.as_ref())
            .expect("encryption failure");

        // Tamper with the ciphertext to simulate corruption
        let mut corrupted_ciphertext = ciphertext.clone();
        corrupted_ciphertext[0] ^= 1; // Flip a bit in the ciphertext

        // Attempt decryption
        let result = address_key_holder.decrypt_data(
            ephemeral_public_key_sender,
            CipherText::from(corrupted_ciphertext),
            nonce.clone(),
        );

        // The decryption should fail or produce incorrect output due to tampered ciphertext
        assert_ne!(result, plaintext);
    }

    #[test]
    fn test_encryption_decryption_round_trip() {
        let address_key_holder = AddressKeyHolder::new_os_random();

        // Generate ephemeral key and shared secret
        let scalar = Scalar::random(OsRng);
        let ephemeral_public_key_sender = (ProjectivePoint::generator() * scalar).to_affine();

        // Encrypt sample data
        let plaintext = b"Round-trip test data";
        let nonce = Nonce::from_slice(b"unique nonce");

        let shared_secret =
            address_key_holder.calculate_shared_secret_receiver(ephemeral_public_key_sender);
        // Prepare the encryption key from shared secret
        let key_raw = shared_secret.to_bytes();
        let key_raw_adjust_pre = &key_raw.as_slice()[..32];
        let key_raw_adjust: [u8; 32] = key_raw_adjust_pre.try_into().unwrap();
        let key: Key<Aes256Gcm> = key_raw_adjust.into();
        let cipher = Aes256Gcm::new(&key);

        let ciphertext = cipher
            .encrypt(nonce, plaintext.as_ref())
            .expect("encryption failure");

        // Decrypt the data using the `AddressKeyHolder` instance
        let decrypted_data = address_key_holder.decrypt_data(
            ephemeral_public_key_sender,
            CipherText::from(ciphertext),
            nonce.clone(),
        );

        // Verify the decrypted data matches the original plaintext
        assert_eq!(decrypted_data, plaintext);
    }

    #[test]
    fn key_generation_test() {
        let seed_holder = SeedHolder::new_os_random();
        let top_secret_key_holder = seed_holder.produce_top_secret_key_holder();

        let utxo_secret_key_holder = top_secret_key_holder.produce_utxo_secret_holder();

        let address = utxo_secret_key_holder.generate_address();
        let nullifer_public_key = utxo_secret_key_holder.generate_nullifier_public_key();
        let viewing_public_key = utxo_secret_key_holder.generate_viewing_public_key();

        println!("======Prerequisites======");
        println!();

        println!(
            "Group generator {:?}",
            hex::encode(AffinePoint::GENERATOR.to_bytes())
        );
        println!(
            "Nullifier constant {:?}",
            hex::encode(NULLIFIER_SECRET_CONST)
        );
        println!("Viewing constatnt {:?}", hex::encode(VIEVING_SECRET_CONST));
        println!();

        println!("======Holders======");
        println!();

        println!("{seed_holder:?}");
        println!("{top_secret_key_holder:?}");
        println!("{utxo_secret_key_holder:?}");
        println!();

        println!("======Public data======");
        println!();
        println!("Address{:?}", hex::encode(address));
        println!(
            "Nulifier public key {:?}",
            hex::encode(nullifer_public_key.to_bytes())
        );
        println!(
            "Viewing public key {:?}",
            hex::encode(viewing_public_key.to_bytes())
        );
    }
}
