use serde::{de::Error, Deserialize, Serialize};

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

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
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
    use serde::Serialize;

    const TEST_BLOB_SIZE: usize = 256; // Define a test blob size for simplicity
    static SC_DATA_BLOB_SIZE: usize = TEST_BLOB_SIZE;

    #[test]
    fn test_produce_blob_from_fit_vec() {
        let data = (0..0 + 255).collect();
        let blob = produce_blob_from_fit_vec(data);
        assert_eq!(blob.0[..4], [0, 1, 2, 3]);
    }

    #[test]
    #[should_panic]
    fn test_produce_blob_from_fit_vec_panic() {
        let data = vec![0; SC_DATA_BLOB_SIZE + 1];
        let _ = produce_blob_from_fit_vec(data);
    }

    #[derive(Serialize)]
    struct TestState {
        a: u32,
        b: u32,
    }

    #[test]
    fn test_produce_blob_list_from_sc_public_state() {
        let state = TestState { a: 42, b: 99 };
        let result = produce_blob_list_from_sc_public_state(&state).unwrap();
        assert!(!result.is_empty());
    }

    #[test]
    fn test_compare_blob_lists_created() {
        let old_list: Vec<DataBlob> = vec![];
        let new_list: Vec<DataBlob> = vec![[1; SC_DATA_BLOB_SIZE].into()];

        let changes = compare_blob_lists(&old_list, &new_list);
        assert_eq!(changes.len(), 1);
        assert!(matches!(changes[0], DataBlobChangeVariant::Created { .. }));
    }

    #[test]
    fn test_compare_blob_lists_deleted() {
        let old_list: Vec<DataBlob> = vec![[1; SC_DATA_BLOB_SIZE].into()];
        let new_list: Vec<DataBlob> = vec![];

        let changes = compare_blob_lists(&old_list, &new_list);
        assert_eq!(changes.len(), 1);
        assert!(matches!(changes[0], DataBlobChangeVariant::Deleted { .. }));
    }

    #[test]
    fn test_compare_blob_lists_modified() {
        let old_list: Vec<DataBlob> = vec![[1; SC_DATA_BLOB_SIZE].into()];
        let new_list: Vec<DataBlob> = vec![[2; SC_DATA_BLOB_SIZE].into()];

        let changes = compare_blob_lists(&old_list, &new_list);
        assert_eq!(changes.len(), 1);
        assert!(matches!(changes[0], DataBlobChangeVariant::Modified { .. }));
    }
}
