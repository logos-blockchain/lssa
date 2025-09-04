use serde::{Deserialize, Serialize, de::Error};

use crate::SC_DATA_BLOB_SIZE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataBlob(pub [u8; SC_DATA_BLOB_SIZE]);

impl From<[u8; SC_DATA_BLOB_SIZE]> for DataBlob {
    fn from(value: [u8; SC_DATA_BLOB_SIZE]) -> Self {
        Self(value)
    }
}

impl Serialize for DataBlob {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let data_vec = self.0.to_vec();
        data_vec.serialize(serializer)
    }
}

impl AsRef<[u8]> for DataBlob {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl<'de> Deserialize<'de> for DataBlob {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let data_vec = Vec::<u8>::deserialize(deserializer)?;
        let chunk: [u8; SC_DATA_BLOB_SIZE] = data_vec
            .try_into()
            .map_err(|data| {
                anyhow::anyhow!("failed to fit vec {data:?} to {:?}", SC_DATA_BLOB_SIZE)
            })
            .map_err(D::Error::custom)?;
        Ok(Self(chunk))
    }
}

impl DataBlob {
    pub fn to_vec(&self) -> Vec<u8> {
        self.0.to_vec()
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum DataBlobChangeVariant {
    Created {
        id: usize,
        blob: DataBlob,
    },
    Modified {
        id: usize,
        blob_old: DataBlob,
        blob_new: DataBlob,
    },
    Deleted {
        id: usize,
    },
}

///Produce `DataBlob` from vector of size <= `SC_DATA_BLOB_SIZE`
///
///Extends to `SC_DATA_BLOB_SIZE`, if necessary.
///
///Panics, if size > `SC_DATA_BLOB_SIZE`
pub fn produce_blob_from_fit_vec(data: Vec<u8>) -> DataBlob {
    let data_len = data.len();

    assert!(data_len <= SC_DATA_BLOB_SIZE);
    let mut blob: DataBlob = [0; SC_DATA_BLOB_SIZE].into();

    for (idx, item) in data.into_iter().enumerate() {
        blob.0[idx] = item
    }

    blob
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    const TEST_BLOB_SIZE: usize = 256; // Define a test blob size for simplicity
    static SC_DATA_BLOB_SIZE: usize = TEST_BLOB_SIZE;

    fn sample_vec() -> Vec<u8> {
        (0..SC_DATA_BLOB_SIZE)
            .collect::<Vec<usize>>()
            .iter()
            .map(|&x| x as u8)
            .collect()
    }

    fn sample_data_blob() -> DataBlob {
        let vec: Vec<u8> = sample_vec();
        produce_blob_from_fit_vec(vec)
    }

    #[test]
    fn test_serialize_data_blob() {
        let blob = sample_data_blob();
        let json = serde_json::to_string(&blob).unwrap();

        let expected_json = serde_json::to_string(&sample_vec()).unwrap();
        assert_eq!(json, expected_json);
    }

    #[test]
    fn test_deserialize_data_blob() {
        let data = sample_vec();
        let json = serde_json::to_string(&data).unwrap();
        let deserialized: DataBlob = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.to_vec(), data);
    }

    #[test]
    fn test_serialize_deserialize_data_blob_change_variant() {
        let blob1 = sample_data_blob();
        let blob2 = produce_blob_from_fit_vec((50..50 + SC_DATA_BLOB_SIZE as u8).collect());

        let variants = vec![
            DataBlobChangeVariant::Created { id: 1, blob: blob1 },
            DataBlobChangeVariant::Modified {
                id: 2,
                blob_old: blob1,
                blob_new: blob2,
            },
            DataBlobChangeVariant::Deleted { id: 3 },
        ];

        for variant in variants {
            let json = serde_json::to_string(&variant).unwrap();
            let deserialized: DataBlobChangeVariant = serde_json::from_str(&json).unwrap();
            assert_eq!(variant, deserialized);
        }
    }

    #[test]
    fn test_produce_blob_from_fit_vec() {
        let data = (0..255).collect();
        let blob = produce_blob_from_fit_vec(data);
        assert_eq!(blob.0[..4], [0, 1, 2, 3]);
    }

    #[test]
    #[should_panic]
    fn test_produce_blob_from_fit_vec_panic() {
        let data = vec![0; SC_DATA_BLOB_SIZE + 1];
        let _ = produce_blob_from_fit_vec(data);
    }
}
