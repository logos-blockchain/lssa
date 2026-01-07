#[derive(Clone, Debug, Eq, PartialEq, Copy, Hash, PartialOrd, Ord)]
pub struct HeaderId([u8; 32]);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Invalid header id size: {0}")]
    InvalidHeaderIdSize(usize),
}

impl From<[u8; 32]> for HeaderId {
    fn from(id: [u8; 32]) -> Self {
        Self(id)
    }
}

impl From<HeaderId> for [u8; 32] {
    fn from(id: HeaderId) -> Self {
        id.0
    }
}

impl TryFrom<&[u8]> for HeaderId {
    type Error = Error;

    fn try_from(slice: &[u8]) -> Result<Self, Self::Error> {
        if slice.len() != 32 {
            return Err(Error::InvalidHeaderIdSize(slice.len()));
        }
        let mut id = [0u8; 32];
        id.copy_from_slice(slice);
        Ok(Self::from(id))
    }
}

impl AsRef<[u8]> for HeaderId {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Copy, Hash)]
pub struct ContentId([u8; 32]);

impl From<ContentId> for [u8; 32] {
    fn from(id: ContentId) -> Self {
        id.0
    }
}

macro_rules! display_hex_bytes_newtype {
    ($newtype:ty) => {
        impl core::fmt::Display for $newtype {
            fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
                write!(f, "0x")?;
                for v in self.0 {
                    write!(f, "{:02x}", v)?;
                }
                Ok(())
            }
        }
    };
}

macro_rules! serde_bytes_newtype {
    ($newtype:ty, $len:expr) => {
        impl serde::Serialize for $newtype {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                if serializer.is_human_readable() {
                    const_hex::const_encode::<$len, false>(&self.0)
                        .as_str()
                        .serialize(serializer)
                } else {
                    self.0.serialize(serializer)
                }
            }
        }

        impl<'de> serde::Deserialize<'de> for $newtype {
            fn deserialize<D>(deserializer: D) -> Result<$newtype, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                if deserializer.is_human_readable() {
                    let s = <String>::deserialize(deserializer)?;
                    const_hex::decode_to_array(s)
                        .map(Self)
                        .map_err(serde::de::Error::custom)
                } else {
                    <[u8; $len] as serde::Deserialize>::deserialize(deserializer).map(Self)
                }
            }
        }
    };
}

display_hex_bytes_newtype!(HeaderId);
display_hex_bytes_newtype!(ContentId);

serde_bytes_newtype!(HeaderId, 32);
serde_bytes_newtype!(ContentId, 32);