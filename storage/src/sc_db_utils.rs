use serde::Serialize;

use crate::SC_DATA_BLOB_SIZE;

pub type DataBlob = [u8; SC_DATA_BLOB_SIZE];

#[derive(Debug, Clone, Copy)]
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
    let mut blob: DataBlob = [0; SC_DATA_BLOB_SIZE];

    for (idx, item) in data.into_iter().enumerate() {
        blob[idx] = item
    }

    blob
}

///Creates blob list from generic serializable state
///
///`ToDo`: Find a way to align data in a way, to minimize read and write operations in db
pub fn produce_blob_list_from_sc_public_state<S: Serialize>(
    state: &S,
) -> Result<Vec<DataBlob>, serde_json::Error> {
    let mut blob_list = vec![];

    let ser_data = serde_json::to_vec(state)?;

    //`ToDo` Replace with `next_chunk` usage, when feature stabilizes in Rust
    for i in 0..(ser_data.len() / SC_DATA_BLOB_SIZE) {
        let next_chunk: Vec<u8>;

        if (i + 1) * SC_DATA_BLOB_SIZE < ser_data.len() {
            next_chunk = ser_data[(i * SC_DATA_BLOB_SIZE)..((i + 1) * SC_DATA_BLOB_SIZE)]
                .iter()
                .cloned()
                .collect();
        } else {
            next_chunk = ser_data[(i * SC_DATA_BLOB_SIZE)..(ser_data.len())]
                .iter()
                .cloned()
                .collect();
        }

        blob_list.push(produce_blob_from_fit_vec(next_chunk));
    }

    Ok(blob_list)
}

///Compare two consecutive in time blob lists to produce list of modified ids
pub fn compare_blob_lists(
    blob_list_old: &[DataBlob],
    blob_list_new: &[DataBlob],
) -> Vec<DataBlobChangeVariant> {
    let mut changed_ids = vec![];
    let mut id_end = 0;

    let old_len = blob_list_old.len();
    let new_len = blob_list_new.len();

    if old_len > new_len {
        for id in new_len..old_len {
            changed_ids.push(DataBlobChangeVariant::Deleted { id });
        }
    } else if new_len > old_len {
        for id in old_len..new_len {
            changed_ids.push(DataBlobChangeVariant::Created {
                id,
                blob: blob_list_new[id],
            });
        }
    }

    loop {
        let old_blob = blob_list_old.get(id_end);
        let new_blob = blob_list_new.get(id_end);

        match (old_blob, new_blob) {
            (Some(old), Some(new)) => {
                if old != new {
                    changed_ids.push(DataBlobChangeVariant::Modified {
                        id: id_end,
                        blob_old: *old,
                        blob_new: *new,
                    });
                }
            }
            _ => break,
        }

        id_end += 1;
    }

    changed_ids
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
        assert_eq!(blob[..4], [0, 1, 2, 3]);
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
        let new_list: Vec<DataBlob> = vec![[1; SC_DATA_BLOB_SIZE]];

        let changes = compare_blob_lists(&old_list, &new_list);
        assert_eq!(changes.len(), 1);
        assert!(matches!(changes[0], DataBlobChangeVariant::Created { .. }));
    }

    #[test]
    fn test_compare_blob_lists_deleted() {
        let old_list: Vec<DataBlob> = vec![[1; SC_DATA_BLOB_SIZE]];
        let new_list: Vec<DataBlob> = vec![];

        let changes = compare_blob_lists(&old_list, &new_list);
        assert_eq!(changes.len(), 1);
        assert!(matches!(changes[0], DataBlobChangeVariant::Deleted { .. }));
    }

    #[test]
    fn test_compare_blob_lists_modified() {
        let old_list: Vec<DataBlob> = vec![[1; SC_DATA_BLOB_SIZE]];
        let new_list: Vec<DataBlob> = vec![[2; SC_DATA_BLOB_SIZE]];

        let changes = compare_blob_lists(&old_list, &new_list);
        assert_eq!(changes.len(), 1);
        assert!(matches!(changes[0], DataBlobChangeVariant::Modified { .. }));
    }
}
