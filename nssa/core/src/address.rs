use serde::{Deserialize, Serialize};

#[cfg(feature = "host")]
use std::{fmt::Display, str::FromStr};

#[cfg(feature = "host")]
use base58::{FromBase58, ToBase58};

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(
    any(feature = "host", test),
    derive(Debug, Copy, PartialOrd, Ord, Default)
)]
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

#[cfg(feature = "host")]
#[derive(Debug, thiserror::Error)]
pub enum AddressError {
    #[error("invalid base58")]
    InvalidBase58(#[from] anyhow::Error),
    #[error("invalid length: expected 32 bytes, got {0}")]
    InvalidLength(usize),
}

#[cfg(feature = "host")]
impl FromStr for Address {
    type Err = AddressError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bytes = s
            .from_base58()
            .map_err(|err| anyhow::anyhow!("Invalid base58 err {err:?}"))?;
        if bytes.len() != 32 {
            return Err(AddressError::InvalidLength(bytes.len()));
        }
        let mut value = [0u8; 32];
        value.copy_from_slice(&bytes);
        Ok(Address { value })
    }
}

#[cfg(feature = "host")]
impl Display for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.value.to_base58())
    }
}

#[cfg(test)]
mod tests {

    use super::{Address, AddressError};

    #[test]
    fn parse_valid_address() {
        let base58_str = "11111111111111111111111111111111";
        let addr: Address = base58_str.parse().unwrap();
        assert_eq!(addr.value, [0u8; 32]);
    }

    #[test]
    fn parse_invalid_base58() {
        let base58_str = "00".repeat(32); // invalid base58 chars
        let result = base58_str.parse::<Address>().unwrap_err();
        assert!(matches!(result, AddressError::InvalidBase58(_)));
    }

    #[test]
    fn parse_wrong_length_short() {
        let base58_str = "11".repeat(31); // 62 chars = 31 bytes
        let result = base58_str.parse::<Address>().unwrap_err();
        assert!(matches!(result, AddressError::InvalidLength(_)));
    }

    #[test]
    fn parse_wrong_length_long() {
        let base58_str = "11".repeat(33); // 66 chars = 33 bytes
        let result = base58_str.parse::<Address>().unwrap_err();
        assert!(matches!(result, AddressError::InvalidLength(_)));
    }
}
