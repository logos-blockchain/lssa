use secp256k1::Scalar;
use serde::{Deserialize, Serialize};

use crate::key_management::key_tree::traits::KeyNode;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChildKeysPublic {
    pub csk: nssa::PrivateKey,
    pub cpk: nssa::PublicKey,
    pub ccc: [u8; 32],
    /// Can be [`None`] if root
    pub cci: Option<u32>,
}

impl ChildKeysPublic {
    fn compute_hash_value(&self, cci: u32) -> [u8; 64] {
        let mut hash_input = vec![];

        match ((2u32).pow(31)).cmp(&cci) {
            // Harden
            std::cmp::Ordering::Less => {
                hash_input.extend_from_slice(self.csk.value());
                hash_input.extend_from_slice(&(cci).to_le_bytes());

                hmac_sha512::HMAC::mac(hash_input, self.ccc)
            }
            // Non-harden
            _ => {
                hash_input.extend_from_slice(self.cpk.value());
                hash_input.extend_from_slice(&cci.to_le_bytes());

                hmac_sha512::HMAC::mac(hash_input, self.ccc)
            }
        }
    }
}

impl KeyNode for ChildKeysPublic {
    fn root(seed: [u8; 64]) -> Self {
        let hash_value = hmac_sha512::HMAC::mac(seed, "LEE_master_pub");

        let csk = nssa::PrivateKey::try_new(*hash_value.first_chunk::<32>().unwrap()).unwrap();
        let ccc = *hash_value.last_chunk::<32>().unwrap();
        let cpk = nssa::PublicKey::new_from_private_key(&csk);

        Self {
            csk,
            cpk,
            ccc,
            cci: None,
        }
    }

    fn nth_child(&self, cci: u32) -> Self {
        let hash_value = self.compute_hash_value(cci);

        let csk = secp256k1::SecretKey::from_byte_array(
            *hash_value
                .first_chunk::<32>()
                .expect("hash_value is 64 bytes, must be safe to get first 32"),
        )
        .unwrap();

        let csk = nssa::PrivateKey::try_new(
            csk.add_tweak(&Scalar::from_le_bytes(*self.csk.value()).unwrap())
                .expect("Expect a valid Scalar")
                .secret_bytes(),
        )
        .unwrap();

        if secp256k1::constants::CURVE_ORDER < *csk.value() {
            panic!("Secret key cannot exceed curve order");
        }

        let ccc = *hash_value
            .last_chunk::<32>()
            .expect("hash_value is 64 bytes, must be safe to get last 32");

        let cpk = nssa::PublicKey::new_from_private_key(&csk);

        Self {
            csk,
            cpk,
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
        nssa::AccountId::from(&self.cpk)
    }
}

impl<'a> From<&'a ChildKeysPublic> for &'a nssa::PrivateKey {
    fn from(value: &'a ChildKeysPublic) -> Self {
        &value.csk
    }
}

#[cfg(test)]
mod tests {
    use nssa::{PrivateKey, PublicKey};

    use super::*;

    #[test]
    fn test_master_keys_generation() {
        let seed = [
            88, 189, 37, 237, 199, 125, 151, 226, 69, 153, 165, 113, 191, 69, 188, 221, 9, 34, 173,
            134, 61, 109, 34, 103, 121, 39, 237, 14, 107, 194, 24, 194, 191, 14, 237, 185, 12, 87,
            22, 227, 38, 71, 17, 144, 251, 118, 217, 115, 33, 222, 201, 61, 203, 246, 121, 214, 6,
            187, 148, 92, 44, 253, 210, 37,
        ];
        let keys = ChildKeysPublic::root(seed);

        let expected_ccc = [
            238, 94, 84, 154, 56, 224, 80, 218, 133, 249, 179, 222, 9, 24, 17, 252, 120, 127, 222,
            13, 146, 126, 232, 239, 113, 9, 194, 219, 190, 48, 187, 155,
        ];

        let expected_csk: PrivateKey = PrivateKey::try_new([
            40, 35, 239, 19, 53, 178, 250, 55, 115, 12, 34, 3, 153, 153, 72, 170, 190, 36, 172, 36,
            202, 148, 181, 228, 35, 222, 58, 84, 156, 24, 146, 86,
        ])
        .unwrap();
        let expected_cpk: PublicKey = PublicKey::try_new([
            219, 141, 130, 105, 11, 203, 187, 124, 112, 75, 223, 22, 11, 164, 153, 127, 59, 247,
            244, 166, 75, 66, 242, 224, 35, 156, 161, 75, 41, 51, 76, 245,
        ])
        .unwrap();

        assert!(expected_ccc == keys.ccc);
        assert!(expected_csk == keys.csk);
        assert!(expected_cpk == keys.cpk);
    }

    #[test]
    fn test_harden_child_keys_generation() {
        let seed = [
            88, 189, 37, 237, 199, 125, 151, 226, 69, 153, 165, 113, 191, 69, 188, 221, 9, 34, 173,
            134, 61, 109, 34, 103, 121, 39, 237, 14, 107, 194, 24, 194, 191, 14, 237, 185, 12, 87,
            22, 227, 38, 71, 17, 144, 251, 118, 217, 115, 33, 222, 201, 61, 203, 246, 121, 214, 6,
            187, 148, 92, 44, 253, 210, 37,
        ];
        let root_keys = ChildKeysPublic::root(seed);
        let cci = (2u32).pow(31) + 13;
        let child_keys = ChildKeysPublic::nth_child(&root_keys, cci);

        print!(
            "{} {}",
            child_keys.csk.value()[0],
            child_keys.csk.value()[1]
        );

        let expected_ccc = [
            126, 175, 244, 41, 41, 173, 134, 103, 139, 140, 195, 86, 194, 147, 116, 48, 71, 107,
            253, 235, 114, 139, 60, 115, 226, 205, 215, 248, 240, 190, 196, 6,
        ];

        let expected_csk: PrivateKey = PrivateKey::try_new([
            128, 148, 53, 165, 222, 155, 163, 108, 186, 182, 124, 67, 90, 86, 59, 123, 95, 224,
            171, 4, 51, 131, 254, 57, 241, 178, 82, 161, 204, 206, 79, 107,
        ])
        .unwrap();

        let expected_cpk: PublicKey = PublicKey::try_new([
            149, 240, 55, 15, 178, 67, 245, 254, 44, 141, 95, 223, 238, 62, 85, 11, 248, 9, 11, 40,
            69, 211, 116, 13, 189, 35, 8, 95, 233, 154, 129, 58,
        ])
        .unwrap();

        assert!(expected_ccc == child_keys.ccc);
        assert!(expected_csk == child_keys.csk);
        assert!(expected_cpk == child_keys.cpk);
    }

    #[test]
    fn test_nonharden_child_keys_generation() {
        let seed = [
            88, 189, 37, 237, 199, 125, 151, 226, 69, 153, 165, 113, 191, 69, 188, 221, 9, 34, 173,
            134, 61, 109, 34, 103, 121, 39, 237, 14, 107, 194, 24, 194, 191, 14, 237, 185, 12, 87,
            22, 227, 38, 71, 17, 144, 251, 118, 217, 115, 33, 222, 201, 61, 203, 246, 121, 214, 6,
            187, 148, 92, 44, 253, 210, 37,
        ];
        let root_keys = ChildKeysPublic::root(seed);
        let cci = 13;
        let child_keys = ChildKeysPublic::nth_child(&root_keys, cci);

        print!(
            "{} {}",
            child_keys.csk.value()[0],
            child_keys.csk.value()[1]
        );

        let expected_ccc = [
            50, 29, 113, 102, 49, 130, 64, 0, 247, 95, 135, 187, 118, 162, 65, 65, 194, 53, 189,
            242, 66, 178, 168, 2, 51, 193, 155, 72, 209, 2, 207, 251,
        ];

        let expected_csk: PrivateKey = PrivateKey::try_new([
            162, 32, 211, 190, 180, 74, 151, 246, 189, 93, 8, 57, 182, 239, 125, 245, 192, 255, 24,
            186, 251, 23, 194, 186, 252, 121, 190, 54, 147, 199, 1, 109,
        ])
        .unwrap();

        let expected_cpk: PublicKey = PublicKey::try_new([
            183, 48, 207, 170, 221, 111, 118, 9, 40, 67, 123, 162, 159, 169, 34, 157, 23, 37, 232,
            102, 231, 187, 199, 191, 205, 146, 159, 22, 79, 100, 10, 223,
        ])
        .unwrap();

        assert!(expected_ccc == child_keys.ccc);
        assert!(expected_csk == child_keys.csk);
        assert!(expected_cpk == child_keys.cpk);
    }

    #[test]
    fn test_edge_case_child_keys_generation_2_power_32() {
        let seed = [
            88, 189, 37, 237, 199, 125, 151, 226, 69, 153, 165, 113, 191, 69, 188, 221, 9, 34, 173,
            134, 61, 109, 34, 103, 121, 39, 237, 14, 107, 194, 24, 194, 191, 14, 237, 185, 12, 87,
            22, 227, 38, 71, 17, 144, 251, 118, 217, 115, 33, 222, 201, 61, 203, 246, 121, 214, 6,
            187, 148, 92, 44, 253, 210, 37,
        ];
        let root_keys = ChildKeysPublic::root(seed);
        let cci = (2u32).pow(32); //equivant to 0, thus non-harden.
        let child_keys = ChildKeysPublic::nth_child(&root_keys, cci);

        print!(
            "{} {}",
            child_keys.csk.value()[0],
            child_keys.csk.value()[1]
        );

        let expected_ccc = [
            210, 59, 14, 158, 52, 215, 216, 93, 160, 246, 178, 122, 211, 252, 0, 33, 248, 197, 242,
            243, 78, 210, 67, 55, 212, 147, 3, 12, 80, 132, 245, 139,
        ];

        let expected_csk: PrivateKey = PrivateKey::try_new([
            167, 114, 72, 184, 108, 61, 60, 219, 210, 140, 142, 224, 253, 204, 175, 165, 147, 135,
            43, 42, 9, 224, 164, 196, 179, 178, 169, 162, 41, 56, 161, 146,
        ])
        .unwrap();

        let expected_cpk: PublicKey = PublicKey::try_new([
            127, 243, 89, 67, 37, 211, 15, 39, 31, 110, 175, 89, 9, 102, 209, 99, 105, 194, 172, 8,
            194, 11, 207, 250, 1, 9, 6, 173, 49, 155, 130, 29,
        ])
        .unwrap();

        assert!(expected_ccc == child_keys.ccc);
        assert!(expected_csk == child_keys.csk);
        assert!(expected_cpk == child_keys.cpk);
    }
}
