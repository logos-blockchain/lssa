// TODO: Consider switching to deriving Borsh
#[cfg(feature = "host")]
use std::io::Cursor;

#[cfg(feature = "host")]
use std::io::Read;

use crate::account::Account;

#[cfg(feature = "host")]
use crate::encryption::shared_key_derivation::Secp256k1Point;

use crate::encryption::Ciphertext;

#[cfg(feature = "host")]
use crate::error::NssaCoreError;

use crate::Commitment;
#[cfg(feature = "host")]
use crate::Nullifier;
use crate::NullifierPublicKey;

impl Account {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        for word in &self.program_owner {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        bytes.extend_from_slice(&self.balance.to_le_bytes());
        bytes.extend_from_slice(&self.nonce.to_le_bytes());
        let data_length: u32 = self.data.len() as u32;
        bytes.extend_from_slice(&data_length.to_le_bytes());
        bytes.extend_from_slice(self.data.as_slice());
        bytes
    }

    #[cfg(feature = "host")]
    pub fn from_cursor(cursor: &mut Cursor<&[u8]>) -> Result<Self, NssaCoreError> {
        let mut u32_bytes = [0u8; 4];
        let mut u128_bytes = [0u8; 16];

        // program owner
        let mut program_owner = [0u32; 8];
        for word in &mut program_owner {
            cursor.read_exact(&mut u32_bytes)?;
            *word = u32::from_le_bytes(u32_bytes);
        }

        // balance
        cursor.read_exact(&mut u128_bytes)?;
        let balance = u128::from_le_bytes(u128_bytes);

        // nonce
        cursor.read_exact(&mut u128_bytes)?;
        let nonce = u128::from_le_bytes(u128_bytes);

        //data
        cursor.read_exact(&mut u32_bytes)?;
        let data_length = u32::from_le_bytes(u32_bytes);
        let mut data = vec![0; data_length as usize];
        cursor.read_exact(&mut data)?;

        Ok(Self {
            program_owner,
            balance,
            data,
            nonce,
        })
    }
}

impl Commitment {
    pub fn to_byte_array(&self) -> [u8; 32] {
        self.0
    }

    #[cfg(feature = "host")]
    pub fn from_cursor(cursor: &mut Cursor<&[u8]>) -> Result<Self, NssaCoreError> {
        let mut bytes = [0u8; 32];
        cursor.read_exact(&mut bytes)?;
        Ok(Self(bytes))
    }
}

impl NullifierPublicKey {
    pub fn to_byte_array(&self) -> [u8; 32] {
        self.0
    }
}

#[cfg(feature = "host")]
impl Nullifier {
    pub fn to_byte_array(&self) -> [u8; 32] {
        self.0
    }

    pub fn from_cursor(cursor: &mut Cursor<&[u8]>) -> Result<Self, NssaCoreError> {
        let mut bytes = [0u8; 32];
        cursor.read_exact(&mut bytes)?;
        Ok(Self(bytes))
    }
}

impl Ciphertext {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        let ciphertext_length: u32 = self.0.len() as u32;
        bytes.extend_from_slice(&ciphertext_length.to_le_bytes());
        bytes.extend_from_slice(&self.0);

        bytes
    }

    #[cfg(feature = "host")]
    pub fn from_cursor(cursor: &mut Cursor<&[u8]>) -> Result<Self, NssaCoreError> {
        let mut u32_bytes = [0; 4];

        cursor.read_exact(&mut u32_bytes)?;
        let ciphertext_lenght = u32::from_le_bytes(u32_bytes);
        let mut ciphertext = vec![0; ciphertext_lenght as usize];
        cursor.read_exact(&mut ciphertext)?;

        Ok(Self(ciphertext))
    }
}

#[cfg(feature = "host")]
impl Secp256k1Point {
    pub fn to_bytes(&self) -> [u8; 33] {
        self.0.clone().try_into().unwrap()
    }

    pub fn from_cursor(cursor: &mut Cursor<&[u8]>) -> Result<Self, NssaCoreError> {
        let mut value = vec![0; 33];
        cursor.read_exact(&mut value)?;
        Ok(Self(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enconding() {
        let account = Account {
            program_owner: [1, 2, 3, 4, 5, 6, 7, 8],
            balance: 123456789012345678901234567890123456,
            nonce: 42,
            data: b"hola mundo".to_vec(),
        };

        // program owner || balance || nonce || data_len || data
        let expected_bytes = [
            1, 0, 0, 0, 2, 0, 0, 0, 3, 0, 0, 0, 4, 0, 0, 0, 5, 0, 0, 0, 6, 0, 0, 0, 7, 0, 0, 0, 8,
            0, 0, 0, 192, 186, 220, 114, 113, 65, 236, 234, 222, 15, 215, 191, 227, 198, 23, 0, 42,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 10, 0, 0, 0, 104, 111, 108, 97, 32, 109,
            117, 110, 100, 111,
        ];

        let bytes = account.to_bytes();
        assert_eq!(bytes, expected_bytes);
    }

    #[test]
    fn test_commitment_to_bytes() {
        let commitment = Commitment((0..32).collect::<Vec<u8>>().try_into().unwrap());
        let expected_bytes: [u8; 32] = (0..32).collect::<Vec<u8>>().try_into().unwrap();

        let bytes = commitment.to_byte_array();
        assert_eq!(expected_bytes, bytes);
    }

    #[cfg(feature = "host")]
    #[test]
    fn test_nullifier_to_bytes() {
        let nullifier = Nullifier((0..32).collect::<Vec<u8>>().try_into().unwrap());
        let expected_bytes: [u8; 32] = (0..32).collect::<Vec<u8>>().try_into().unwrap();

        let bytes = nullifier.to_byte_array();
        assert_eq!(expected_bytes, bytes);
    }

    #[cfg(feature = "host")]
    #[test]
    fn test_commitment_to_bytes_roundtrip() {
        let commitment = Commitment((0..32).collect::<Vec<u8>>().try_into().unwrap());
        let bytes = commitment.to_byte_array();
        let mut cursor = Cursor::new(bytes.as_ref());
        let commitment_from_cursor = Commitment::from_cursor(&mut cursor).unwrap();
        assert_eq!(commitment, commitment_from_cursor);
    }

    #[cfg(feature = "host")]
    #[test]
    fn test_nullifier_to_bytes_roundtrip() {
        let nullifier = Nullifier((0..32).collect::<Vec<u8>>().try_into().unwrap());
        let bytes = nullifier.to_byte_array();
        let mut cursor = Cursor::new(bytes.as_ref());
        let nullifier_from_cursor = Nullifier::from_cursor(&mut cursor).unwrap();
        assert_eq!(nullifier, nullifier_from_cursor);
    }

    #[cfg(feature = "host")]
    #[test]
    fn test_account_to_bytes_roundtrip() {
        let account = Account {
            program_owner: [1, 2, 3, 4, 5, 6, 7, 8],
            balance: 123456789012345678901234567890123456,
            nonce: 42,
            data: b"hola mundo".to_vec(),
        };
        let bytes = account.to_bytes();
        let mut cursor = Cursor::new(bytes.as_ref());
        let account_from_cursor = Account::from_cursor(&mut cursor).unwrap();
        assert_eq!(account, account_from_cursor);
    }
}
