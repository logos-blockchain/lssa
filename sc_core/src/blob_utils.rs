use serde::Serialize;
use storage::{
    sc_db_utils::{produce_blob_from_fit_vec, DataBlob, DataBlobChangeVariant},
    SC_DATA_BLOB_SIZE,
};

///Creates blob list from generic serializable state
///
///`ToDo`: Find a way to align data in a way, to minimize read and write operations in db
pub fn produce_blob_list_from_sc_public_state<S: Serialize>(
    state: &S,
) -> Result<Vec<DataBlob>, serde_json::Error> {
    let mut blob_list = vec![];

    let ser_data = serde_json::to_vec(state)?;

    //`ToDo` Replace with `next_chunk` usage, when feature stabilizes in Rust
    for i in 0..=(ser_data.len() / SC_DATA_BLOB_SIZE) {
        let next_chunk: Vec<u8> = if (i + 1) * SC_DATA_BLOB_SIZE < ser_data.len() {
            ser_data[(i * SC_DATA_BLOB_SIZE)..((i + 1) * SC_DATA_BLOB_SIZE)].to_vec()
        } else {
            ser_data[(i * SC_DATA_BLOB_SIZE)..(ser_data.len())].to_vec()
        };

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
        for (id, blob) in blob_list_new.iter().enumerate().take(new_len).skip(old_len) {
            changed_ids.push(DataBlobChangeVariant::Created { id, blob: *blob });
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
