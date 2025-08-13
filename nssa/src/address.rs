use anyhow::anyhow;
use serde::{Deserialize, Serialize, de::Visitor};

use crate::signature::PublicKey;

pub const LENGTH_MISMATCH_ERROR_MESSAGE: &str = "Slice length != 32 ";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Address {
    value: [u8; 32],
}

impl Address {
    pub fn new(value: [u8; 32]) -> Self {
        Self { value }
    }

    pub fn tag(&self) -> u8 {
        self.value[0]
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

impl TryFrom<Vec<u8>> for Address {
    type Error = anyhow::Error;

    fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
        let addr_val: [u8; 32] = value
            .try_into()
            .map_err(|_| anyhow!(LENGTH_MISMATCH_ERROR_MESSAGE))?;

        Ok(Address::new(addr_val))
    }
}

impl From<&PublicKey> for Address {
    fn from(value: &PublicKey) -> Self {
        // TODO: Check specs
        Self::new(*value.value())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HexString(String);

impl HexString {
    pub fn inner(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, thiserror::Error)]
pub enum HexStringConsistencyError {
    #[error("Hex decode error")]
    HexError(#[from] hex::FromHexError),
    #[error("Decode slice does not fit 32 bytes")]
    SizeError(#[from] anyhow::Error),
}

impl TryFrom<&str> for HexString {
    type Error = HexStringConsistencyError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let decoded_str = hex::decode(value)?;
        let _: Address = decoded_str.try_into()?;

        Ok(Self(value.to_string()))
    }
}

impl Serialize for HexString {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

struct HexStringVisitor;

impl<'de> Visitor<'de> for HexStringVisitor {
    type Value = String;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("expected a valid string")
    }

    fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(v)
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(v.to_string())
    }
}

impl<'de> Deserialize<'de> for HexString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let str_cand = deserializer.deserialize_string(HexStringVisitor)?;

        let hex_string =
            HexString::try_from(str_cand.as_str()).map_err(|err| serde::de::Error::custom(err))?;

        Ok(hex_string)
    }
}

impl From<HexString> for Address {
    fn from(value: HexString) -> Self {
        Address::try_from(hex::decode(value.inner()).unwrap()).unwrap()
    }
}

impl From<Address> for HexString {
    fn from(value: Address) -> Self {
        HexString::try_from(hex::encode(value).as_str()).unwrap()
    }
}

impl Serialize for Address {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let hex_string: HexString = (*self).into();

        hex_string.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Address {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let hex_sring = HexString::deserialize(deserializer)?;

        Ok(hex_sring.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Serialize, Deserialize)]
    struct Ser1 {
        f1: String,
    }

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct Ser2 {
        f1: HexString,
    }

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct Ser3 {
        f1: Address,
    }

    #[test]
    fn test_hex_ser() {
        let str_for_tests = hex::encode([42; 32]);

        let hex_str_for_tests = HexString::try_from(str_for_tests.as_str()).unwrap();

        let ser1_str = Ser1 { f1: str_for_tests };

        let ser2_str = Ser2 {
            f1: hex_str_for_tests,
        };

        let ser1_str_ser = serde_json::to_string(&ser1_str).unwrap();
        let ser2_str_ser = serde_json::to_string(&ser2_str).unwrap();

        println!("{ser2_str_ser:#?}");

        assert_eq!(ser1_str_ser, ser2_str_ser);
    }

    #[test]
    fn test_hex_deser() {
        let raw_json = r#"{
            "f1": "2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a"
        }"#;

        let str_for_tests = hex::encode([42; 32]);

        let hex_str_for_tests = HexString::try_from(str_for_tests.as_str()).unwrap();

        let ser2_str = Ser2 {
            f1: hex_str_for_tests,
        };

        let ser1_str: Ser2 = serde_json::from_str(raw_json).unwrap();

        assert_eq!(ser1_str, ser2_str);
    }

    #[test]
    fn test_addr_deser() {
        let raw_json = r#"{
            "f1": "2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a"
        }"#;

        let addr_for_tests = Address::new([42; 32]);

        let ser2_str = Ser3 {
            f1: addr_for_tests,
        };

        let ser1_str: Ser3 = serde_json::from_str(raw_json).unwrap();

        assert_eq!(ser1_str, ser2_str);
    }
}
