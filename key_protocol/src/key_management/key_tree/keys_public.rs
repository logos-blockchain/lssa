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
        let hash_value = hmac_sha512::HMAC::mac(seed, "NSSA_master_pub");

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
        let hash_value = self.clone().compute_hash_value(cci);

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
    use super::*;
    use nssa::{PrivateKey, PublicKey};

    #[test]
    fn test_master_keys_generation() {
        let seed = [
            1, 130, 162, 216, 26, 234, 27, 234, 59, 207, 162, 21, 199, 134, 255, 150, 213, 185, 39,
            171, 190, 140, 144, 170, 180, 168, 36, 190, 35, 154, 32, 164, 91, 177, 221, 142, 190,
            150, 128, 72, 118, 124, 182, 223, 137, 172, 6, 133, 220, 55, 27, 24, 133, 23, 37, 193,
            212, 237, 51, 61, 74, 173, 70, 193,
        ];
        let keys = ChildKeysPublic::root(seed);

        let expected_ccc = [
            196, 136, 91, 61, 98, 123, 72, 161, 143, 192, 242, 133, 4, 231, 101, 199, 165, 79, 60,
            121, 165, 234, 179, 205, 227, 195, 116, 180, 114, 104, 63, 193,
        ];
        let expected_csk: PrivateKey = PrivateKey::try_new([
            122, 75, 152, 80, 233, 219, 100, 140, 106, 84, 74, 60, 102, 92, 23, 83, 17, 195, 122,
            33, 12, 39, 154, 247, 68, 132, 125, 236, 182, 123, 129, 91,
        ])
        .unwrap();
        let expected_cpk: PublicKey = PublicKey::try_new([
            91, 160, 2, 88, 187, 86, 42, 53, 237, 131, 141, 208, 218, 40, 81, 209, 221, 89, 134,
            127, 254, 249, 21, 38, 186, 139, 232, 134, 253, 97, 83, 149,
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
            1, 19, 186, 27, 153, 246, 163, 56, 19, 66, 184, 252, 125, 91, 229, 55, 22, 186, 129,
            78, 67, 38, 102, 167, 88, 237, 142, 162, 165, 105, 67, 250,
        ];

        let expected_csk: PrivateKey = PrivateKey::try_new([
            93, 255, 132, 159, 52, 30, 43, 128, 106, 84, 99, 16, 193, 37, 224, 142, 208, 87, 142,
            156, 175, 116, 90, 204, 157, 219, 136, 109, 230, 223, 76, 70,
        ])
        .unwrap();

        let expected_cpk: PublicKey = PublicKey::try_new([
            78, 123, 221, 195, 111, 255, 131, 167, 117, 146, 61, 161, 179, 51, 250, 25, 90, 187,
            190, 163, 30, 145, 212, 87, 88, 127, 86, 5, 45, 236, 184, 223,
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
            189, 224, 117, 5, 91, 65, 195, 166, 97, 192, 203, 11, 254, 170, 159, 146, 234, 238,
            157, 155, 189, 197, 187, 190, 125, 127, 146, 20, 250, 174, 218, 111,
        ];

        let expected_csk: PrivateKey = PrivateKey::try_new([
            56, 213, 27, 113, 20, 234, 174, 150, 186, 32, 151, 177, 118, 37, 83, 181, 30, 71, 183,
            34, 174, 44, 143, 250, 65, 158, 106, 2, 239, 20, 194, 176,
        ])
        .unwrap();

        let expected_cpk: PublicKey = PublicKey::try_new([
            118, 202, 73, 30, 197, 237, 223, 167, 99, 45, 17, 251, 91, 168, 252, 158, 196, 243, 48,
            159, 253, 181, 224, 110, 177, 111, 115, 19, 247, 158, 202, 61,
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
            196, 27, 223, 192, 33, 28, 41, 165, 247, 198, 251, 26, 63, 85, 223, 6, 57, 201, 10, 46,
            189, 152, 39, 69, 28, 30, 112, 167, 211, 175, 170, 75,
        ];

        let expected_csk: PrivateKey = PrivateKey::try_new([
            55, 68, 204, 198, 234, 171, 247, 60, 177, 24, 216, 130, 62, 115, 130, 156, 94, 90, 156,
            8, 160, 126, 14, 33, 214, 184, 79, 127, 88, 87, 95, 217,
        ])
        .unwrap();

        let expected_cpk: PublicKey = PublicKey::try_new([
            222, 5, 102, 223, 97, 95, 83, 100, 114, 154, 15, 248, 164, 117, 209, 125, 193, 19, 64,
            75, 245, 168, 52, 199, 45, 39, 237, 232, 175, 3, 167, 178,
        ])
        .unwrap();

        assert!(expected_ccc == child_keys.ccc);
        assert!(expected_csk == child_keys.csk);
        assert!(expected_cpk == child_keys.cpk);
    }
}
