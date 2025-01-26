use log::debug;

use node_core::sequencer_client::SequencerClientError;
use rpc_primitives::errors::{RpcError, RpcParseError};

pub struct RpcErr(pub RpcError);

pub type RpcErrInternal = anyhow::Error;

pub trait RpcErrKind: 'static {
    fn into_rpc_err(self) -> RpcError;
}

impl<T: RpcErrKind> From<T> for RpcErr {
    fn from(e: T) -> Self {
        Self(e.into_rpc_err())
    }
}

macro_rules! standard_rpc_err_kind {
    ($type_name:path) => {
        impl RpcErrKind for $type_name {
            fn into_rpc_err(self) -> RpcError {
                self.into()
            }
        }
    };
}
standard_rpc_err_kind!(RpcError);
standard_rpc_err_kind!(RpcParseError);

impl RpcErrKind for serde_json::Error {
    fn into_rpc_err(self) -> RpcError {
        RpcError::serialization_error(&self.to_string())
    }
}

impl RpcErrKind for RpcErrInternal {
    fn into_rpc_err(self) -> RpcError {
        RpcError::new_internal_error(None, &format!("{self:#?}"))
    }
}

#[allow(clippy::needless_pass_by_value)]
pub fn from_rpc_err_into_anyhow_err(rpc_err: RpcError) -> anyhow::Error {
    debug!("Rpc error cast to anyhow error : err {rpc_err:?}");
    anyhow::anyhow!(format!("{rpc_err:#?}"))
}

pub fn cast_seq_client_error_into_rpc_error(seq_cli_err: SequencerClientError) -> RpcError {
    let error_string = seq_cli_err.to_string();

    match seq_cli_err {
        SequencerClientError::SerdeError(_) => RpcError::serialization_error(&error_string),
        SequencerClientError::HTTPError(_) => RpcError::new_internal_error(None, &error_string),
        SequencerClientError::InternalError(err) => RpcError::new_internal_error(
            err.error.data,
            &serde_json::to_string(&err.error.error_struct).unwrap_or(String::default()),
        ),
    }
}
