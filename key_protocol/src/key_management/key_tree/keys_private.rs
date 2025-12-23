use common::HashType;
use k256::{Scalar, elliptic_curve::PrimeField};
use nssa_core::{NullifierPublicKey, encryption::IncomingViewingPublicKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, digest::FixedOutput};

use crate::key_management::{
    KeyChain,
    key_tree::traits::KeyNode,
    secret_holders::{PrivateKeyHolder, SecretSpendingKey},
};

const TWO_POWER_31: u32 = (2u32).pow(31);

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChildKeysPrivate {
    pub value: (KeyChain, nssa::Account),
    pub ccc: [u8; 32],
    /// Can be [`None`] if root
    pub cci: Option<u32>,
}

impl ChildKeysPrivate {
    fn nth_child_nonharden_hash(&self, cci: u32) -> [u8; 64] {
        /// TODO: logic required
        panic!("Nonharden keys not yet designed for private state")
    }

    fn nth_child_harden_hash(&self, cci: u32) -> [u8; 64] {
        let mut hash_input = vec![];
        //   hash_input.extend_from_slice(self.csk.value());
        //hash_input.extend_from_slice(&(cci - TWO_POWER_31).to_le_bytes());

        hmac_sha512::HMAC::mac(hash_input, self.ccc)
    }
}

impl KeyNode for ChildKeysPrivate {
    fn root(seed: [u8; 64]) -> Self {
        let hash_value = hmac_sha512::HMAC::mac(seed, b"NSSA_master_priv");

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
                0: <HashType>::from(hasher.finalize_fixed()),
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
                0: <HashType>::from(hasher.finalize_fixed()),
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
    use std::process::Child;

    use k256::Secp256k1;
    use nssa_core::{NullifierSecretKey, encryption::shared_key_derivation::Secp256k1Point};

    use crate::key_management::{
        self,
        secret_holders::{IncomingViewingSecretKey, OutgoingViewingSecretKey},
    };

    use super::*;

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
            189, 102, 14, 63, 41, 116, 229, 119, 41, 59, 225, 169, 205, 21, 50, 214, 222, 67, 109,
            126, 107, 153, 57, 118, 29, 239, 79, 162, 95, 13, 197, 170,
        ]);

        let expected_ccc = [
            5, 205, 75, 227, 45, 88, 53, 168, 99, 138, 145, 94, 195, 176, 178, 118, 213, 129, 64,
            70, 105, 60, 27, 230, 73, 86, 110, 203, 28, 60, 191, 172,
        ];
        let expected_nsk: NullifierSecretKey = [
            181, 144, 216, 101, 27, 177, 89, 140, 223, 128, 200, 3, 208, 144, 250, 242, 145, 25,
            197, 107, 74, 187, 99, 58, 253, 254, 82, 16, 221, 9, 202, 99,
        ];
        let expected_npk: NullifierPublicKey = nssa_core::NullifierPublicKey([
            161, 65, 163, 239, 194, 99, 30, 5, 6, 117, 116, 154, 218, 50, 72, 221, 222, 187, 36,
            25, 18, 98, 242, 140, 117, 18, 183, 150, 235, 207, 150, 205,
        ]);
        let expected_isk: IncomingViewingSecretKey = [
            153, 108, 251, 220, 218, 41, 212, 54, 175, 61, 198, 247, 82, 127, 215, 160, 226, 26,
            154, 96, 41, 126, 247, 136, 206, 187, 233, 193, 47, 159, 169, 71,
        ];
        let expected_ovk: OutgoingViewingSecretKey = [
            169, 133, 157, 26, 10, 196, 45, 254, 82, 146, 180, 151, 193, 152, 84, 92, 252, 249,
            166, 192, 43, 93, 79, 153, 205, 56, 208, 5, 116, 151, 252, 78,
        ];
        let expected_ipk_as_bytes: [u8; 33] = [
            2, 14, 226, 128, 146, 254, 56, 61, 3, 24, 211, 151, 194, 41, 166, 67, 146, 0, 73, 4,
            140, 184, 244, 200, 43, 159, 141, 234, 90, 90, 145, 53, 251,
        ];

        assert!(expected_ssk == keys.value.0.secret_spending_key);
        assert!(expected_ccc == keys.ccc);
        assert!(expected_nsk == keys.value.0.private_key_holder.nullifier_secret_key);
        assert!(expected_npk == keys.value.0.nullifer_public_key);
        assert!(expected_isk == keys.value.0.private_key_holder.incoming_viewing_secret_key);
        assert!(expected_ovk == keys.value.0.private_key_holder.outgoing_viewing_secret_key);
        assert!(expected_ipk_as_bytes == keys.value.0.incoming_viewing_public_key.to_bytes());
    }

    #[test]
    fn test_child_keys_generation() {
        let seed: [u8; 64] = [
            88, 189, 37, 237, 199, 125, 151, 226, 69, 153, 165, 113, 191, 69, 188, 221, 9, 34, 173,
            134, 61, 109, 34, 103, 121, 39, 237, 14, 107, 194, 24, 194, 191, 14, 237, 185, 12, 87,
            22, 227, 38, 71, 17, 144, 251, 118, 217, 115, 33, 222, 201, 61, 203, 246, 121, 214, 6,
            187, 148, 92, 44, 253, 210, 37,
        ];

        let root_node = ChildKeysPrivate::root(seed);
        let child_node = ChildKeysPrivate::nth_child(&root_node, 42u32);

        let expected_ccc: [u8; 32] = [
            131, 6, 100, 230, 202, 63, 5, 206, 158, 3, 81, 177, 221, 107, 27, 194, 192, 38, 104,
            87, 23, 98, 107, 1, 78, 19, 216, 195, 63, 66, 13, 172,
        ];

        let expected_nsk: NullifierSecretKey = [
            118, 91, 48, 86, 184, 103, 178, 151, 169, 126, 198, 254, 177, 130, 48, 175, 250, 255,
            19, 89, 122, 133, 216, 80, 101, 155, 243, 186, 104, 161, 35, 208,
        ];

        let expected_npk: NullifierPublicKey = nssa_core::NullifierPublicKey([
            122, 13, 105, 25, 155, 46, 105, 20, 93, 112, 97, 78, 198, 186, 227, 74, 13, 213, 135,
            215, 254, 96, 115, 228, 137, 139, 35, 73, 67, 123, 48, 48,
        ]);

        let expected_isk: IncomingViewingSecretKey = [
            38, 172, 52, 226, 190, 69, 120, 123, 231, 65, 88, 97, 125, 56, 120, 225, 253, 198, 133,
            145, 84, 118, 182, 80, 188, 210, 146, 91, 197, 48, 39, 36,
        ];
        let expected_ovk: OutgoingViewingSecretKey = [
            105, 26, 128, 5, 91, 183, 81, 224, 125, 217, 93, 173, 162, 129, 46, 85, 164, 215, 169,
            236, 202, 12, 49, 31, 199, 130, 108, 159, 68, 196, 58, 96,
        ];
        let expected_ipk_as_bytes: [u8; 33] = [
            2, 249, 186, 99, 214, 150, 242, 196, 122, 68, 237, 126, 129, 14, 189, 238, 132, 31,
            228, 94, 224, 25, 248, 77, 250, 198, 122, 24, 232, 38, 147, 225, 158,
        ];

        assert!(expected_ccc == child_node.ccc);
        assert!(expected_nsk == child_node.value.0.private_key_holder.nullifier_secret_key);
        assert!(expected_npk == child_node.value.0.nullifer_public_key);
        assert!(
            expected_isk
                == child_node
                    .value
                    .0
                    .private_key_holder
                    .incoming_viewing_secret_key
        );
        assert!(expected_ipk_as_bytes == child_node.value.0.incoming_viewing_public_key.to_bytes());
        assert!(
            expected_ovk
                == child_node
                    .value
                    .0
                    .private_key_holder
                    .outgoing_viewing_secret_key
        );
    }
}
