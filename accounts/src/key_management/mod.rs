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
        let key_raw = key_point.to_bytes();
        let key_raw_adjust: [u8; 32] = key_raw.as_slice().try_into().unwrap();

        let key: Key<Aes256Gcm> = key_raw_adjust.into();

        let cipher = Aes256Gcm::new(&key);

        cipher.decrypt(&nonce, ciphertext.as_slice()).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use constants_types::{NULLIFIER_SECRET_CONST, VIEVING_SECRET_CONST};
    use elliptic_curve::group::GroupEncoding;

    use super::*;

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
