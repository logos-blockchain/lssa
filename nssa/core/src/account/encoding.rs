use risc0_zkvm::{
    serde::to_vec,
    sha::{Impl, Sha256},
};

use crate::account::{Account, Commitment, NullifierPublicKey};

impl Account {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        for word in &self.program_owner {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        bytes.extend_from_slice(&self.balance.to_le_bytes());
        bytes.extend_from_slice(&self.nonce.to_le_bytes());
        let hashed_data: [u8; 32] = Impl::hash_bytes(&self.data).as_bytes().try_into().unwrap();
        bytes.extend_from_slice(&hashed_data);
        bytes
    }
}

impl Commitment {
    pub(crate) fn to_byte_array(&self) -> [u8; 32] {
        self.0
    }
}


impl NullifierPublicKey {
    pub(crate) fn to_byte_array(&self) -> [u8; 32] {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use crate::account::Account;

    #[test]
    fn test_enconding() {
        let account = Account {
            program_owner: [1, 2, 3, 4, 5, 6, 7, 8],
            balance: 123456789012345678901234567890123456,
            nonce: 42,
            data: b"hola mundo".to_vec(),
        };

        // program owner || balance || nonce || hash(data)
        let expected_bytes = [
            1, 0, 0, 0, 2, 0, 0, 0, 3, 0, 0, 0, 4, 0, 0, 0, 5, 0, 0, 0, 6, 0, 0, 0, 7, 0, 0, 0, 8,
            0, 0, 0, 192, 186, 220, 114, 113, 65, 236, 234, 222, 15, 215, 191, 227, 198, 23, 0, 42,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 11, 137, 65, 102, 211, 51, 100, 53, 200,
            0, 190, 163, 111, 242, 27, 41, 234, 168, 1, 165, 47, 88, 76, 0, 108, 73, 40, 154, 13,
            207, 110, 47,
        ];

        let bytes = account.to_bytes();
        assert_eq!(bytes, expected_bytes);
    }
}
