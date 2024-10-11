#[derive(thiserror::Error, Debug)]
pub enum DbError {
    #[error("RocksDb error")]
    RocksDbError {
        error: rocksdb::Error,
        additional_info: Option<String>,
    },
    #[error("Serialization error")]
    SerializationError {
        error: serde_json::Error,
        additional_info: Option<String>,
    },
    #[error("Logic Error")]
    DbInteractionError { additional_info: String },
}

impl DbError {
    pub fn rocksdb_cast_message(rerr: rocksdb::Error, message: Option<String>) -> Self {
        Self::RocksDbError {
            error: rerr,
            additional_info: message,
        }
    }

    pub fn serde_cast_message(serr: serde_json::Error, message: Option<String>) -> Self {
        Self::SerializationError {
            error: serr,
            additional_info: message,
        }
    }

    pub fn db_interaction_error(message: String) -> Self {
        Self::DbInteractionError {
            additional_info: message,
        }
    }
}
