use serde::Serialize;
use storage::{sc_db_utils::{produce_blob_from_fit_vec, DataBlob, DataBlobChangeVariant}, SC_DATA_BLOB_SIZE};

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

