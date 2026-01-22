use common::HashType;
use k256::{Scalar, elliptic_curve::PrimeField};
use nssa_core::{NullifierPublicKey, encryption::ViewingPublicKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, digest::FixedOutput};

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
        let hash_value = hmac_sha512::HMAC::mac(seed, b"LEE_master_priv");

        let ssk = SecretSpendingKey(
            *hash_value
                .first_chunk::<32>()
                .expect("hash_value is 64 bytes, must be safe to get first 32"),
        );
        let ccc = *hash_value
            .last_chunk::<32>()
            .expect("hash_value is 64 bytes, must be safe to get last 32");

        // TODO: check these generations
        let nsk = ssk.generate_nullifier_secret_key();
        let vsk = ssk.generate_viewing_secret_key();

        let npk: NullifierPublicKey = {
            let mut hasher = sha2::Sha256::new();

            hasher.update("LEE/keys");
            hasher.update(nsk);
            hasher.update([7u8]);
            hasher.update([0u8; 23]);

            NullifierPublicKey(<HashType>::from(hasher.finalize_fixed()))
        };

        let vpk = ViewingPublicKey::from_scalar(vsk);

        Self {
            value: (
                KeyChain {
                    secret_spending_key: ssk,
                    nullifer_public_key: npk,
                    viewing_public_key: vpk,
                    private_key_holder: PrivateKeyHolder {
                        nullifier_secret_key: nsk,
                        viewing_secret_key: vsk,
                    },
                },
                nssa::Account::default(),
            ),
            ccc,
            cci: None,
        }
    }

    fn nth_child(&self, cci: u32) -> Self {
        let parent_pt =
            Scalar::from_repr(self.value.0.private_key_holder.nullifier_secret_key.into())
                .expect("Key generated as scalar, must be valid representation")
                * Scalar::from_repr(self.value.0.private_key_holder.viewing_secret_key.into())
                    .expect("Key generated as scalar, must be valid representation");
        let mut input = vec![];

        input.extend_from_slice(b"LEE_seed_priv");
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
        let vsk = ssk.generate_child_viewing_secret_key(cci);

        let npk: NullifierPublicKey = {
            let mut hasher = sha2::Sha256::new();

            hasher.update("LEE/chain");
            hasher.update(nsk);
            hasher.update([7u8]);
            hasher.update([0u8; 22]);

            NullifierPublicKey {
                0: <HashType>::from(hasher.finalize_fixed()),
            }
        };

        let vpk = ViewingPublicKey::from_scalar(vsk);

        Self {
            value: (
                KeyChain {
                    secret_spending_key: ssk,
                    nullifer_public_key: npk,
                    viewing_public_key: vpk,
                    private_key_holder: PrivateKeyHolder {
                        nullifier_secret_key: nsk,
                        viewing_secret_key: vsk,
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
    use nssa_core::{NullifierPublicKey, NullifierSecretKey};

    use super::*;
    use crate::key_management::{self, secret_holders::ViewingSecretKey};

    #[test]
    fn test_master_key_generation() {
        let seed: [u8; 64] = [
            252, 56, 204, 83, 232, 123, 209, 188, 187, 167, 39, 213, 71, 39, 58, 65, 125, 134, 255,
            49, 43, 108, 92, 53, 173, 164, 94, 142, 150, 74, 21, 163, 43, 144, 226, 87, 199, 18,
            129, 223, 176, 198, 5, 150, 157, 70, 210, 254, 14, 105, 89, 191, 246, 27, 52, 170, 56,
            114, 39, 38, 118, 197, 205, 225,
        ];

        let keys = ChildKeysPrivate::root(seed);

        let expected_ssk: SecretSpendingKey = key_management::secret_holders::SecretSpendingKey([
            246, 79, 26, 124, 135, 95, 52, 51, 201, 27, 48, 194, 2, 144, 51, 219, 245, 128, 139,
            222, 42, 195, 105, 33, 115, 97, 186, 0, 97, 14, 218, 191,
        ]);

        let expected_ccc = [
            56, 114, 70, 249, 67, 169, 206, 9, 192, 11, 180, 168, 149, 129, 42, 95, 43, 157, 130,
            111, 13, 5, 195, 75, 20, 255, 162, 85, 40, 251, 8, 168,
        ];

        let expected_nsk: NullifierSecretKey = [
            154, 102, 103, 5, 34, 235, 227, 13, 22, 182, 226, 11, 7, 67, 110, 162, 99, 193, 174,
            34, 234, 19, 222, 2, 22, 12, 163, 252, 88, 11, 0, 163,
        ];

        let expected_npk: NullifierPublicKey = nssa_core::NullifierPublicKey([
            7, 123, 125, 191, 233, 183, 201, 4, 20, 214, 155, 210, 45, 234, 27, 240, 194, 111, 97,
            247, 155, 113, 122, 246, 192, 0, 70, 61, 76, 71, 70, 2,
        ]);
        let expected_vsk: ViewingSecretKey = [
            155, 90, 54, 75, 228, 130, 68, 201, 129, 251, 180, 195, 250, 64, 34, 230, 241, 204,
            216, 50, 149, 156, 10, 67, 208, 74, 9, 10, 47, 59, 50, 202,
        ];

        let expected_vpk_as_bytes: [u8; 33] = [
            2, 191, 99, 102, 114, 40, 131, 109, 166, 8, 222, 186, 107, 29, 156, 106, 206, 96, 127,
            80, 170, 66, 217, 79, 38, 80, 11, 74, 147, 123, 221, 159, 166,
        ];

        assert!(expected_ssk == keys.value.0.secret_spending_key);
        assert!(expected_ccc == keys.ccc);
        assert!(expected_nsk == keys.value.0.private_key_holder.nullifier_secret_key);
        assert!(expected_npk == keys.value.0.nullifer_public_key);
        assert!(expected_vsk == keys.value.0.private_key_holder.viewing_secret_key);
        assert!(expected_vpk_as_bytes == keys.value.0.viewing_public_key.to_bytes());
    }

    #[test]
    fn test_child_keys_generation() {
        let seed: [u8; 64] = [
            252, 56, 204, 83, 232, 123, 209, 188, 187, 167, 39, 213, 71, 39, 58, 65, 125, 134, 255,
            49, 43, 108, 92, 53, 173, 164, 94, 142, 150, 74, 21, 163, 43, 144, 226, 87, 199, 18,
            129, 223, 176, 198, 5, 150, 157, 70, 210, 254, 14, 105, 89, 191, 246, 27, 52, 170, 56,
            114, 39, 38, 118, 197, 205, 225,
        ];

        let root_node = ChildKeysPrivate::root(seed);
        let child_node = ChildKeysPrivate::nth_child(&root_node, 42u32);

        let expected_ccc: [u8; 32] = [
            145, 59, 225, 32, 54, 168, 14, 45, 60, 253, 57, 202, 31, 86, 142, 234, 51, 57, 154, 88,
            132, 200, 92, 191, 220, 144, 42, 184, 108, 35, 226, 146,
        ];

        let expected_nsk: NullifierSecretKey = [
            82, 238, 58, 161, 96, 201, 25, 193, 53, 101, 100, 173, 183, 167, 165, 141, 252, 214,
            214, 3, 176, 186, 62, 112, 56, 54, 6, 197, 29, 178, 88, 214,
        ];

        let expected_npk: NullifierPublicKey = nssa_core::NullifierPublicKey([
            40, 104, 183, 124, 101, 11, 61, 45, 140, 53, 3, 155, 139, 134, 105, 108, 60, 229, 165,
            195, 187, 246, 14, 88, 76, 69, 137, 154, 29, 113, 205, 153,
        ]);

        let expected_vsk: ViewingSecretKey = [
            14, 114, 31, 116, 147, 114, 62, 111, 176, 100, 211, 68, 38, 47, 250, 34, 224, 249, 25,
            40, 35, 37, 237, 224, 161, 58, 228, 154, 44, 162, 128, 138,
        ];
        let expected_vpk_as_bytes: [u8; 33] = [
            3, 243, 200, 219, 91, 171, 128, 76, 173, 117, 255, 212, 233, 71, 205, 204, 89, 104, 92,
            187, 249, 154, 197, 102, 241, 66, 15, 55, 194, 189, 16, 124, 176,
        ];

        assert!(expected_ccc == child_node.ccc);
        assert!(expected_nsk == child_node.value.0.private_key_holder.nullifier_secret_key);
        assert!(expected_npk == child_node.value.0.nullifer_public_key);
        assert!(expected_vsk == child_node.value.0.private_key_holder.viewing_secret_key);
        assert!(expected_vpk_as_bytes == child_node.value.0.viewing_public_key.to_bytes());
    }
}
