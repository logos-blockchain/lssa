use secp256k1::{Scalar, SecretKey};
use serde::{Deserialize, Serialize};

use crate::key_management::key_tree::traits::KeyNode;

const TWO_POWER_31: u32 = (2u32).pow(31);

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChildKeysPublic {
    pub csk: nssa::PrivateKey,
    pub cpk: nssa::PublicKey,
    pub ccc: [u8; 32],
    /// Can be [`None`] if root
    pub cci: Option<u32>,
}

impl ChildKeysPublic {
    fn nth_child_nonharden_hash(&self, cci: u32) -> [u8; 64] {
        let mut hash_input = vec![];
        hash_input.extend_from_slice(self.cpk.value());
        hash_input.extend_from_slice(&cci.to_le_bytes());

        hmac_sha512::HMAC::mac(hash_input, self.ccc)
    }

    fn nth_child_harden_hash(&self, cci: u32) -> [u8; 64] {
        let mut hash_input = vec![];
        hash_input.extend_from_slice(self.csk.value());
        hash_input.extend_from_slice(&(cci - TWO_POWER_31).to_le_bytes());

        hmac_sha512::HMAC::mac(hash_input, self.ccc)
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
        let mut hash_input = vec![];
        hash_input.extend_from_slice(self.cpk.value());
        hash_input.extend_from_slice(&cci.to_le_bytes());

        let hash_value = match ((2u32).pow(31)).cmp(&cci) {
            // Harden
            std::cmp::Ordering::Less => self.nth_child_harden_hash(cci),
            // Non-harden
            std::cmp::Ordering::Greater => self.nth_child_nonharden_hash(cci),
            std::cmp::Ordering::Equal => self.nth_child_nonharden_hash(cci),
        };

        let mut csk = secp256k1::SecretKey::from_byte_array(
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
            84, 37, 139, 254, 228, 162, 42, 156, 65, 175, 48, 210, 234, 18, 153, 90, 203, 87, 194,
            213, 17, 80, 170, 211, 99, 192, 133, 85, 120, 188, 130, 6,
        ];

        let expected_csk: PrivateKey = PrivateKey::try_new([
            210, 196, 140, 88, 194, 249, 219, 180, 242, 207, 206, 205, 41, 160, 89, 179, 221, 155,
            134, 237, 180, 175, 84, 102, 216, 219, 128, 135, 89, 180, 13, 177,
        ])
        .unwrap();

        let expected_cpk: PublicKey = PublicKey::try_new([
            130, 250, 109, 211, 83, 152, 36, 53, 47, 197, 199, 161, 150, 96, 126, 20, 39, 43, 36,
            75, 132, 30, 50, 245, 206, 61, 83, 103, 193, 223, 83, 147,
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
}

/*
[2, 14, 243, 116, 96, 0, 81, 219, 86, 228, 188, 116, 201, 71, 176, 107, 84, 4, 196, 176, 100, 140, 111, 57, 126, 38, 84, 91, 40, 154, 53, 12, 54]
Secret child key
[194, 38, 83, 68, 93, 201, 23, 245, 127, 216, 162, 139, 59, 19, 119, 40, 105, 126, 19, 219, 246, 219, 74, 217, 152, 159, 177, 235, 109, 237, 171, 194]
Child chain code
[84, 37, 139, 254, 228, 162, 42, 156, 65, 175, 48, 210, 234, 18, 153, 90, 203, 87, 194, 213, 17, 80, 170, 211, 99, 192, 133, 85, 120, 188, 130, 6]

*/
