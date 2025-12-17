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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChildKeysPrivate {
    pub value: (KeyChain, nssa::Account),
    pub ccc: [u8; 32],
    /// Can be [`None`] if root
    pub cci: Option<u32>,
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

        panic!("{}", parent_pt.to_bytes()[0]);

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
        let seed: [u8; 64] = [88, 189, 37, 237, 199, 125, 151, 226, 69, 153, 165, 113, 191, 69, 188, 221, 9, 34, 173, 134, 61, 109, 34, 103, 121, 39, 237, 14, 107, 194, 24, 194, 191, 14, 237, 185, 12, 87, 22, 227, 38, 71, 17, 144, 251, 118, 217, 115, 33, 222, 201, 61, 203, 246, 121, 214, 6, 187, 148, 92, 44, 253, 210, 37];

        let root_node = ChildKeysPrivate::root(seed);
        let child_node = ChildKeysPrivate::nth_child(&root_node, 42u32);

        let expected_ccc: [u8;32] = 
[131, 6, 100, 230, 202, 63, 5, 206, 158, 3, 81, 177, 221, 107, 27, 194, 192, 38, 104, 87, 23, 98, 107, 1, 78, 19, 216, 195, 63, 66, 13, 172];

        /* 
        let expected_nsk: NullifierSecretKey = [
            88, 186, 150, 238, 56, 44, 107, 53, 97, 59, 42, 62, 175, 63, 222, 11, 231, 223, 174,
            39, 168, 52, 18, 14, 38, 83, 11, 86, 172, 48, 66, 201,
        ];*/
        /*
        let expected_npk: NullifierPublicKey = nssa_core::NullifierPublicKey([
            246, 214, 170, 117, 73, 240, 82, 143, 201, 193, 24, 218, 75, 226, 140, 78, 10, 45, 4,
            5, 184, 164, 127, 172, 24, 26, 241, 205, 13, 179, 91, 232,
        ]);*/
        /*
        let expected_isk: IncomingViewingSecretKey = [
            182, 238, 179, 119, 236, 79, 86, 2, 3, 225, 143, 237, 86, 139, 183, 108, 23, 223, 49,
            69, 23, 208, 136, 65, 139, 92, 240, 106, 46, 172, 222, 247,
        ];*/
        
        let expected_ovk: OutgoingViewingSecretKey = [185, 67, 59, 18, 95, 73, 48, 122, 255, 221, 165, 100, 254, 226, 243, 111, 10, 3, 107, 64, 128, 122, 6, 240, 41, 232, 105, 235, 212, 133, 43, 9];
        /*
        let expected_ipk_as_bytes: [u8; 33] = [
            3, 10, 247, 74, 120, 6, 174, 60, 163, 22, 150, 206, 196, 66, 233, 216, 66, 3, 150, 24,
            20, 120, 29, 70, 178, 26, 125, 253, 75, 166, 114, 128, 34,
        ];*/

        
        assert!(expected_ccc == child_node.ccc);
        /*
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
    */
        assert!(
            expected_ovk
                == child_node
                    .value
                    .0
                    .private_key_holder
                    .outgoing_viewing_secret_key
        );
    /*
        assert!(expected_ipk_as_bytes == child_node.value.0.incoming_viewing_public_key.to_bytes());
*/
        /*Child nsk
Child nsk
[35, 218, 71, 160, 145, 129, 143, 216, 174, 178, 215, 92, 182, 249, 121, 153, 146, 124, 172, 70, 131, 184, 150, 46, 175, 201, 101, 86, 203, 25, 189, 175]
Child Npk
[196, 98, 217, 101, 101, 93, 1, 11, 253, 204, 128, 139, 198, 71, 19, 189, 37, 178, 0, 18, 211, 199, 56, 211, 199, 179, 126, 184, 151, 94, 140, 63]     
Child isk
[9, 6, 246, 146, 108, 119, 185, 109, 36, 205, 35, 176, 196, 196, 153, 246, 215, 127, 89, 39, 174, 3, 86, 197, 231, 181, 33, 75, 47, 29, 18, 2]
Child Ipk
[2, 165, 98, 11, 43, 108, 222, 0, 21, 41, 156, 217, 67, 122, 150, 142, 45, 156, 31, 164, 134, 241, 59, 71, 245, 44, 45, 96, 18, 118, 167, 249, 228]    
Child ovk
[185, 67, 59, 18, 95, 73, 48, 122, 255, 221, 165, 100, 254, 226, 243, 111, 10, 3, 107, 64, 128, 122, 6, 240, 41, 232, 105, 235, 212, 133, 43, 9]       
Child chain code
[252, 165, 63, 74, 148, 28, 14, 197, 76, 240, 82, 1, 213, 179, 149, 190, 174, 49, 47, 94, 84, 246, 219, 189, 125, 190, 86, 120, 206, 159, 36, 172]     
PS C:\Users\jones\OneDrive\Desktop\key_python>

        */
    }
}
