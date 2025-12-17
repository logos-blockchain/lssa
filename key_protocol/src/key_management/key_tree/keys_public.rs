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

        let hash_value = hmac_sha512::HMAC::mac(hash_input, self.ccc);

        
        /*
        let csk = nssa::PrivateKey::try_new(
            *hash_value
                .first_chunk::<32>()
                .expect("hash_value is 64 bytes, must be safe to get first 32"),
        )
        .unwrap();
    */
        let csk = *hash_value
                .first_chunk::<32>()
                .expect("hash_value is 64 bytes, must be safe to get first 32");

        let csk = secp256k1::SecretKey::from_byte_array(csk)
            .unwrap()
            .add_tweak(&secp256k1::Scalar::from_be_bytes(*self.csk.value()).unwrap()).unwrap();
        let csk = nssa::PrivateKey::try_new(*csk.as_ref()).unwrap();
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
    fn test_keys_deterministic_generation() {
        let root_keys = ChildKeysPublic::root([42; 64]);
        let child_keys = root_keys.nth_child(5);

        assert_eq!(root_keys.cci, None);
        assert_eq!(child_keys.cci, Some(5));

        assert_eq!(
            root_keys.ccc,
            [
                61, 30, 91, 26, 133, 91, 236, 192, 231, 53, 186, 139, 11, 221, 202, 11, 178, 215,
                254, 103, 191, 60, 117, 112, 1, 226, 31, 156, 83, 104, 150, 224
            ]
        );
        assert_eq!(
            child_keys.ccc,
            [
                67, 26, 102, 68, 189, 155, 102, 80, 199, 188, 112, 142, 207, 157, 36, 210, 48, 224,
                35, 6, 112, 180, 11, 190, 135, 218, 9, 14, 84, 231, 58, 98
            ]
        );

        assert_eq!(
            root_keys.csk.value(),
            &[
                241, 82, 246, 237, 62, 130, 116, 47, 189, 112, 99, 67, 178, 40, 115, 245, 141, 193,
                77, 164, 243, 76, 222, 64, 50, 146, 23, 145, 91, 164, 92, 116
            ]
        );
        assert_eq!(
            child_keys.csk.value(),
            &[
                11, 151, 27, 212, 167, 26, 77, 234, 103, 145, 53, 191, 184, 25, 240, 191, 156, 25,
                60, 144, 65, 22, 193, 163, 246, 227, 212, 81, 49, 170, 33, 158
            ]
        );

        assert_eq!(
            root_keys.cpk.value(),
            &[
                220, 170, 95, 177, 121, 37, 86, 166, 56, 238, 232, 72, 21, 106, 107, 217, 158, 74,
                133, 91, 143, 244, 155, 15, 2, 230, 223, 169, 13, 20, 163, 138
            ]
        );
        assert_eq!(
            child_keys.cpk.value(),
            &[
                152, 249, 236, 111, 132, 96, 184, 122, 21, 179, 240, 15, 234, 155, 164, 144, 108,
                110, 120, 74, 176, 147, 196, 168, 243, 186, 203, 79, 97, 17, 194, 52
            ]
        );
    }

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
    fn test_child_keys_generation() {
        let seed = [88, 189, 37, 237, 199, 125, 151, 226, 69, 153, 165, 113, 191, 69, 188, 221, 9, 34, 173, 134, 61, 109, 34, 103, 121, 39, 237, 14, 107, 194, 24, 194, 191, 14, 237, 185, 12, 87, 22, 227, 38, 71, 17, 144, 251, 118, 217, 115, 33, 222, 201, 61, 203, 246, 121, 214, 6, 187, 148, 92, 44, 253, 210, 37];
        
        let root_keys = ChildKeysPublic::root(seed);
        let child_keys = ChildKeysPublic::nth_child(&root_keys, 13u32);

        let expected_ccc = [189, 224, 117, 5, 91, 65, 195, 166, 97, 192, 203, 11, 254, 170, 159, 146, 234, 238, 157, 155, 189, 197, 187, 190, 125, 127, 146, 20, 250, 174, 218, 111];

        let expected_csk: PrivateKey = PrivateKey::try_new([40, 54, 226, 92, 175, 185, 234, 215, 71, 41, 107, 111, 135, 152, 113, 41, 170, 42, 68, 16, 240, 88, 134, 109, 1, 98, 155, 103, 3, 78, 96, 193])
        .unwrap();
        let expected_cpk: PublicKey = PublicKey::try_new([144, 0, 43, 242, 24, 163, 240, 224, 176, 212, 59, 165, 7, 250, 50, 218, 169, 219, 121, 137, 31, 202, 205, 61, 93, 160, 135, 245, 60, 105, 94, 173]).unwrap();

        assert!(expected_ccc == child_keys.ccc);
        assert!(expected_csk == child_keys.csk);
        assert!(expected_cpk == child_keys.cpk);
    }
}
