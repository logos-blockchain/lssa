use serde::{Deserialize, Serialize};

use crate::key_management::key_tree::traits::KeyNode;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChildKeysPublic {
    pub csk: nssa::PrivateKey,
    pub cpk: nssa::PublicKey,
    pub ccc: [u8; 32],
    ///Can be None if root
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

    fn n_th_child(&self, cci: u32) -> Self {
        let mut hash_input = vec![];
        hash_input.extend_from_slice(self.csk.value());
        hash_input.extend_from_slice(&cci.to_le_bytes());

        let hash_value = hmac_sha512::HMAC::mac(&hash_input, self.ccc);

        let csk = nssa::PrivateKey::try_new(*hash_value.first_chunk::<32>().unwrap()).unwrap();
        let ccc = *hash_value.last_chunk::<32>().unwrap();
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

    fn child_index(&self) -> &Option<u32> {
        &self.cci
    }

    fn address(&self) -> nssa::Address {
        nssa::Address::from(&self.cpk)
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

    #[test]
    fn test_keys_deterministic_generation() {
        let root_keys = ChildKeysPublic::root([42; 64]);
        let child_keys = root_keys.n_th_child(5);

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
}
