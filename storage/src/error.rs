#[derive(thiserror::Error, Debug)]
pub enum DbError {
    #[error("RocksDb error")]
    RocksDbError {
        error: rocksdb::Error,
        additional_info: Option<String>,
    },
    #[error("Serialization error")]
    SerializationError {
        error: borsh::io::Error,
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

    pub fn borsh_cast_message(berr: borsh::io::Error, message: Option<String>) -> Self {
        Self::SerializationError {
            error: berr,
            additional_info: message,
        }
    }

    pub fn db_interaction_error(message: String) -> Self {
        Self::DbInteractionError {
            additional_info: message,
        }
    }
}
