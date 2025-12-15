use k256::{Scalar, elliptic_curve::PrimeField};
use nssa_core::{NullifierPublicKey, encryption::IncomingViewingPublicKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, digest::FixedOutput};
use common::HashType;

use crate::key_management::{
    KeyChain,
    key_tree::traits::KeyNode,
    secret_holders::{PrivateKeyHolder, SecretSpendingKey},
};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChildKeysPrivate {
    pub value: (KeyChain, nssa::Account),
    pub ccc: [u8; 32],
    /// Can be [`None`] if root
    pub cci: Option<u32>,
}

impl KeyNode for ChildKeysPrivate {
    fn root(seed: [u8; 64]) -> Self {
        let hash_value = hmac_sha512::HMAC::mac(seed, "NSSA_master_priv");

        let ssk = SecretSpendingKey(
            *hash_value
                .first_chunk::<32>()
                .expect("hash_value is 64 bytes, must be safe to get first 32"),
        );
        let ccc = *hash_value
            .last_chunk::<32>()
            .expect("hash_value is 64 bytes, must be safe to get last 32");

        let nsk = ssk.generate_nullifier_secret_key();
        let isk = ssk.generate_incoming_viewing_secret_key();
        let ovk = ssk.generate_outgoing_viewing_secret_key();


        let npk: NullifierPublicKey = { 
            let mut hasher = sha2::Sha256::new();

            hasher.update("NSSA_keys");
            hasher.update(nsk);
            hasher.update([7u8]);
            hasher.update([0u8; 22]);

            NullifierPublicKey {
            0: <HashType>::from(hasher.finalize_fixed())
            }
        };

        let ipk = IncomingViewingPublicKey::from_scalar(isk);

        Self {
            value: (
                KeyChain {
                    secret_spending_key: ssk,
                    nullifer_public_key: npk,
                    incoming_viewing_public_key: ipk,
                    private_key_holder: PrivateKeyHolder {
                        nullifier_secret_key: nsk,
                        incoming_viewing_secret_key: isk,
                        outgoing_viewing_secret_key: ovk,
                    },
                },
                nssa::Account::default(),
            ),
            ccc,
            cci: None,
        }
    }

    fn nth_child(&self, cci: u32) -> Self {
        // parent_pt = ovk_par + scalar(nsk_par)*isk_par
        let parent_pt = Scalar::from_repr(
            self.value
                .0
                .private_key_holder
                .outgoing_viewing_secret_key
                .into(),
        )
        .expect("Key generated as scalar, must be valid representation")
            + Scalar::from_repr(self.value.0.private_key_holder.nullifier_secret_key.into())
                .expect("Key generated as scalar, must be valid representation")
                * Scalar::from_repr(
                    self.value
                        .0
                        .private_key_holder
                        .incoming_viewing_secret_key
                        .into(),
                )
                .expect("Key generated as scalar, must be valid representation");
        let mut input = vec![];

        input.extend_from_slice(b"NSSA_seed_priv");
        input.extend_from_slice(&parent_pt.to_bytes());
        input.extend_from_slice(&cci.to_le_bytes());

        let hash_value = hmac_sha512::HMAC::mac(input, self.ccc);

        let ssk = SecretSpendingKey(
            *hash_value
                .first_chunk::<32>()
                .expect("hash_value is 64 bytes, must be safe to get first 32"),
        );
        let ccc = *hash_value
            .last_chunk::<32>()
            .expect("hash_value is 64 bytes, must be safe to get last 32");

        let nsk = ssk.generate_child_nullifier_secret_key(cci);
        let isk = ssk.generate_child_incoming_viewing_secret_key(cci);
        let ovk = ssk.generate_child_outgoing_viewing_secret_key(cci);

        //TODO: separate out into its own function
        let npk: NullifierPublicKey = {
            let mut hasher = sha2::Sha256::new();

            hasher.update("NSSAchain");
            hasher.update(nsk);
            hasher.update([7u8]);
            hasher.update([0u8; 22]);

            NullifierPublicKey {
                0: <HashType>::from(hasher.finalize_fixed())
            }
        };

        let ipk = IncomingViewingPublicKey::from_scalar(isk);

        Self {
            value: (
                KeyChain {
                    secret_spending_key: ssk,
                    nullifer_public_key: npk,
                    incoming_viewing_public_key: ipk,
                    private_key_holder: PrivateKeyHolder {
                        nullifier_secret_key: nsk,
                        incoming_viewing_secret_key: isk,
                        outgoing_viewing_secret_key: ovk,
                    },
                },
                nssa::Account::default(),
            ),
            ccc,
            cci: Some(cci),
        }
    }

    fn chain_code(&self) -> &[u8; 32] {
        &self.ccc
    }

    fn child_index(&self) -> Option<u32> {
        self.cci
    }

    fn account_id(&self) -> nssa::AccountId {
        nssa::AccountId::from(&self.value.0.nullifer_public_key)
    }
}

impl<'a> From<&'a ChildKeysPrivate> for &'a (KeyChain, nssa::Account) {
    fn from(value: &'a ChildKeysPrivate) -> Self {
        &value.value
    }
}

impl<'a> From<&'a mut ChildKeysPrivate> for &'a mut (KeyChain, nssa::Account) {
    fn from(value: &'a mut ChildKeysPrivate) -> Self {
        &mut value.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keys_deterministic_generation() {
        let root_keys = ChildKeysPrivate::root([42; 64]);
        let child_keys = root_keys.nth_child(5);

        assert_eq!(root_keys.cci, None);
        assert_eq!(child_keys.cci, Some(5));

        assert_eq!(
            root_keys.value.0.secret_spending_key.0,
            [
                249, 83, 253, 32, 174, 204, 185, 44, 253, 167, 61, 92, 128, 5, 152, 4, 220, 21, 88,
                84, 167, 180, 154, 249, 44, 77, 33, 136, 59, 131, 203, 152
            ]
        );
        assert_eq!(
            child_keys.value.0.secret_spending_key.0,
            [
                16, 242, 229, 242, 252, 158, 153, 210, 234, 120, 70, 85, 83, 196, 5, 53, 28, 26,
                187, 230, 22, 193, 146, 232, 237, 3, 166, 184, 122, 1, 233, 93
            ]
        );

        assert_eq!(
            root_keys.value.0.private_key_holder.nullifier_secret_key,
            [
                38, 195, 52, 182, 16, 66, 167, 156, 9, 14, 65, 100, 17, 93, 166, 71, 27, 148, 93,
                85, 116, 109, 130, 8, 195, 222, 159, 214, 141, 41, 124, 57
            ]
        );
        assert_eq!(
            child_keys.value.0.private_key_holder.nullifier_secret_key,
            [
                215, 46, 2, 151, 174, 60, 86, 154, 5, 3, 175, 245, 12, 176, 220, 58, 250, 118, 236,
                49, 254, 221, 229, 58, 40, 1, 170, 145, 175, 108, 23, 170
            ]
        );

        assert_eq!(
            root_keys
                .value
                .0
                .private_key_holder
                .incoming_viewing_secret_key,
            [
                153, 161, 15, 34, 96, 184, 165, 165, 27, 244, 155, 40, 70, 5, 241, 133, 78, 40, 61,
                118, 48, 148, 226, 5, 97, 18, 201, 128, 82, 248, 163, 72
            ]
        );
        assert_eq!(
            child_keys
                .value
                .0
                .private_key_holder
                .incoming_viewing_secret_key,
            [
                192, 155, 55, 43, 164, 115, 71, 145, 227, 225, 21, 57, 55, 12, 226, 44, 10, 103,
                39, 73, 230, 173, 60, 69, 69, 122, 110, 241, 164, 3, 192, 57
            ]
        );

        assert_eq!(
            root_keys
                .value
                .0
                .private_key_holder
                .outgoing_viewing_secret_key,
            [
                205, 87, 71, 129, 90, 242, 217, 200, 140, 252, 124, 46, 207, 7, 33, 156, 83, 166,
                150, 81, 98, 131, 182, 156, 110, 92, 78, 140, 125, 218, 152, 154
            ]
        );
        assert_eq!(
            child_keys
                .value
                .0
                .private_key_holder
                .outgoing_viewing_secret_key,
            [
                131, 202, 219, 172, 219, 29, 48, 120, 226, 209, 209, 10, 216, 173, 48, 167, 233,
                17, 35, 155, 30, 217, 176, 120, 72, 146, 250, 226, 165, 178, 255, 90
            ]
        );

        assert_eq!(
            root_keys.value.0.nullifer_public_key.0,
            [
                65, 176, 149, 243, 192, 45, 216, 177, 169, 56, 229, 7, 28, 66, 204, 87, 109, 83,
                152, 64, 14, 188, 179, 210, 147, 60, 22, 251, 203, 70, 89, 215
            ]
        );
        assert_eq!(
            child_keys.value.0.nullifer_public_key.0,
            [
                69, 104, 130, 115, 48, 134, 19, 188, 67, 148, 163, 54, 155, 237, 57, 27, 136, 228,
                111, 233, 205, 158, 149, 31, 84, 11, 241, 176, 243, 12, 138, 249
            ]
        );

        assert_eq!(
            root_keys.value.0.incoming_viewing_public_key.0,
            &[
                3, 174, 56, 136, 244, 179, 18, 122, 38, 220, 36, 50, 200, 41, 104, 167, 70, 18, 60,
                202, 93, 193, 29, 16, 125, 252, 96, 51, 199, 152, 47, 233, 178
            ]
        );
        assert_eq!(
            child_keys.value.0.incoming_viewing_public_key.0,
            &[
                3, 18, 202, 246, 79, 141, 169, 51, 55, 202, 120, 169, 244, 201, 156, 162, 216, 115,
                126, 53, 46, 94, 235, 125, 114, 178, 215, 81, 171, 93, 93, 88, 117
            ]
        );
    }
}
