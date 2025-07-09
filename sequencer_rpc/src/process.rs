use actix_web::Error as HttpError;
use serde_json::Value;

use common::rpc_primitives::{
    errors::RpcError,
    message::{Message, Request},
    parser::RpcRequest,
};

use common::rpc_primitives::requests::{
    GetBlockDataRequest, GetBlockDataResponse, GetGenesisIdRequest, GetGenesisIdResponse,
    GetLastBlockRequest, GetLastBlockResponse, HelloRequest, HelloResponse, RegisterAccountRequest,
    RegisterAccountResponse, SendTxRequest, SendTxResponse,
};

use super::{respond, types::err_rpc::RpcErr, JsonHandler};

pub const HELLO: &str = "hello";
pub const REGISTER_ACCOUNT: &str = "register_account";
pub const SEND_TX: &str = "send_tx";
pub const GET_BLOCK: &str = "get_block";
pub const GET_GENESIS: &str = "get_genesis";
pub const GET_LAST_BLOCK: &str = "get_last_block";

pub const HELLO_FROM_SEQUENCER: &str = "HELLO_FROM_SEQUENCER";

pub const SUCCESS: &str = "Success";

impl JsonHandler {
    pub async fn process(&self, message: Message) -> Result<Message, HttpError> {
        let id = message.id();
        if let Message::Request(request) = message {
            let message_inner = self
                .process_request_internal(request)
                .await
                .map_err(|e| e.0);
            Ok(Message::response(id, message_inner))
        } else {
            Ok(Message::error(RpcError::parse_error(
                "JSON RPC Request format was expected".to_owned(),
            )))
        }
    }

    #[allow(clippy::unused_async)]
    ///Example of request processing
    async fn process_temp_hello(&self, request: Request) -> Result<Value, RpcErr> {
        let _hello_request = HelloRequest::parse(Some(request.params))?;

        let helperstruct = HelloResponse {
            greeting: HELLO_FROM_SEQUENCER.to_string(),
        };

        respond(helperstruct)
    }

    async fn process_register_account_request(&self, request: Request) -> Result<Value, RpcErr> {
        let acc_req = RegisterAccountRequest::parse(Some(request.params))?;

        {
            let mut acc_store = self.sequencer_state.lock().await;

            acc_store.register_account(acc_req.address);
        }

        let helperstruct = RegisterAccountResponse {
            status: SUCCESS.to_string(),
        };

        respond(helperstruct)
    }

    async fn process_send_tx(&self, request: Request) -> Result<Value, RpcErr> {
        let send_tx_req = SendTxRequest::parse(Some(request.params))?;

        {
            let mut state = self.sequencer_state.lock().await;

            state.push_tx_into_mempool_pre_check(
                send_tx_req.transaction.into(),
                send_tx_req.tx_roots,
            )?;
        }

        let helperstruct = SendTxResponse {
            status: SUCCESS.to_string(),
        };

        respond(helperstruct)
    }

    async fn process_get_block_data(&self, request: Request) -> Result<Value, RpcErr> {
        let get_block_req = GetBlockDataRequest::parse(Some(request.params))?;

        let block = {
            let state = self.sequencer_state.lock().await;

            state
                .store
                .block_store
                .get_block_at_id(get_block_req.block_id)?
        };

        let helperstruct = GetBlockDataResponse { block };

        respond(helperstruct)
    }

    async fn process_get_genesis(&self, request: Request) -> Result<Value, RpcErr> {
        let _get_genesis_req = GetGenesisIdRequest::parse(Some(request.params))?;

        let genesis_id = {
            let state = self.sequencer_state.lock().await;

            state.store.block_store.genesis_id
        };

        let helperstruct = GetGenesisIdResponse { genesis_id };

        respond(helperstruct)
    }

    async fn process_get_last_block(&self, request: Request) -> Result<Value, RpcErr> {
        let _get_last_block_req = GetLastBlockRequest::parse(Some(request.params))?;

        let last_block = {
            let state = self.sequencer_state.lock().await;

            state.chain_height
        };

        let helperstruct = GetLastBlockResponse { last_block };

        respond(helperstruct)
    }

    pub async fn process_request_internal(&self, request: Request) -> Result<Value, RpcErr> {
        match request.method.as_ref() {
            HELLO => self.process_temp_hello(request).await,
            REGISTER_ACCOUNT => self.process_register_account_request(request).await,
            SEND_TX => self.process_send_tx(request).await,
            GET_BLOCK => self.process_get_block_data(request).await,
            GET_GENESIS => self.process_get_genesis(request).await,
            GET_LAST_BLOCK => self.process_get_last_block(request).await,
            _ => Err(RpcErr(RpcError::method_not_found(request.method))),
        }
    }
}
