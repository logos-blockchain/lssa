use std::{fmt::Display, str::FromStr};

use nssa_core::account::AccountId;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::signature::PublicKey;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Address {
    value: [u8; 32],
}

impl Address {
    pub fn new(value: [u8; 32]) -> Self {
        Self { value }
    }

    pub fn value(&self) -> &[u8; 32] {
        &self.value
    }
}

impl AsRef<[u8]> for Address {
    fn as_ref(&self) -> &[u8] {
        &self.value
    }
}

impl From<&PublicKey> for Address {
    fn from(value: &PublicKey) -> Self {
        // TODO: Check specs
        Self::new(*value.value())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AddressError {
    #[error("invalid hex")]
    InvalidHex(#[from] hex::FromHexError),
    #[error("invalid length: expected 32 bytes, got {0}")]
    InvalidLength(usize),
}

impl FromStr for Address {
    type Err = AddressError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bytes = hex::decode(s)?;
        if bytes.len() != 32 {
            return Err(AddressError::InvalidLength(bytes.len()));
        }
        let mut value = [0u8; 32];
        value.copy_from_slice(&bytes);
        Ok(Address { value })
    }
}

impl Display for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", hex::encode(self.value))
    }
}

impl Serialize for Address {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let hex_string = self.to_string();

        hex_string.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Address {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let hex_string = String::deserialize(deserializer)?;

        Address::from_str(&hex_string).map_err(serde::de::Error::custom)
    }
}

impl From<&Address> for AccountId {
    fn from(address: &Address) -> Self {
        const PUBLIC_ACCOUNT_ID_PREFIX: &[u8; 32] = b"/NSSA/v0.1/AccountId/Public/\x00\x00\x00\x00";

        let mut hasher = Sha256::new();
        hasher.update(PUBLIC_ACCOUNT_ID_PREFIX);
        hasher.update(address.value);
        AccountId::new(hasher.finalize().into())
    }
}

#[cfg(test)]
mod tests {
    use nssa_core::account::AccountId;

    use crate::{Address, address::AddressError};

    #[test]
    fn parse_valid_address() {
        let hex_str = "00".repeat(32); // 64 hex chars = 32 bytes
        let addr: Address = hex_str.parse().unwrap();
        assert_eq!(addr.value, [0u8; 32]);
    }

    #[test]
    fn parse_invalid_hex() {
        let hex_str = "zz".repeat(32); // invalid hex chars
        let result = hex_str.parse::<Address>().unwrap_err();
        assert!(matches!(result, AddressError::InvalidHex(_)));
    }

    #[test]
    fn parse_wrong_length_short() {
        let hex_str = "00".repeat(31); // 62 chars = 31 bytes
        let result = hex_str.parse::<Address>().unwrap_err();
        assert!(matches!(result, AddressError::InvalidLength(_)));
    }

    #[test]
    fn parse_wrong_length_long() {
        let hex_str = "00".repeat(33); // 66 chars = 33 bytes
        let result = hex_str.parse::<Address>().unwrap_err();
        assert!(matches!(result, AddressError::InvalidLength(_)));
    }

    #[test]
    fn test_account_id_from_address() {
        let address: Address = "37".repeat(32).parse().unwrap();
        let expected_account_id = AccountId::new([
            93, 223, 66, 245, 78, 230, 157, 188, 110, 161, 134, 255, 137, 177, 220, 88, 37, 44,
            243, 91, 236, 4, 36, 147, 185, 112, 21, 49, 234, 4, 107, 185,
        ]);

        let account_id = AccountId::from(&address);

        assert_eq!(account_id, expected_account_id);
    }
}
