pub mod net_utils;
pub mod process;
pub mod types;

use std::sync::Arc;

use common::{
    rpc_primitives::errors::{RpcError, RpcErrorKind},
    transaction::EncodedTransaction,
};
use mempool::MemPoolHandle;
pub use net_utils::*;
use sequencer_core::SequencerCore;
use serde::Serialize;
use serde_json::Value;
use tokio::sync::Mutex;

use self::types::err_rpc::RpcErr;

// ToDo: Add necessary fields
pub struct JsonHandler {
    sequencer_state: Arc<Mutex<SequencerCore>>,
    mempool_handle: MemPoolHandle<EncodedTransaction>,
}

fn respond<T: Serialize>(val: T) -> Result<Value, RpcErr> {
    Ok(serde_json::to_value(val)?)
}

pub fn rpc_error_responce_inverter(err: RpcError) -> RpcError {
    let mut content: Option<Value> = None;
    if err.error_struct.is_some() {
        content = match err.error_struct.clone().unwrap() {
            RpcErrorKind::HandlerError(val) | RpcErrorKind::InternalError(val) => Some(val),
            RpcErrorKind::RequestValidationError(vall) => Some(serde_json::to_value(vall).unwrap()),
        };
    }
    RpcError {
        error_struct: None,
        code: err.code,
        message: err.message,
        data: content,
    }
}
