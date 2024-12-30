use std::sync::atomic::Ordering;

use actix_web::Error as HttpError;
use serde_json::Value;

use rpc_primitives::{
    errors::RpcError,
    message::{Message, Request},
    parser::RpcRequest,
};

use crate::{
    rpc_error_responce_inverter,
    types::{
        err_rpc::cast_seq_client_error_into_rpc_error,
        rpc_structs::{
            ExecuteSubscenarioRequest, ExecuteSubscenarioResponse, GetBlockDataRequest,
            GetBlockDataResponse, GetLastBlockRequest, GetLastBlockResponse,
            RegisterAccountRequest, RegisterAccountResponse, SendTxRequest,
        },
    },
};

use super::{respond, types::err_rpc::RpcErr, JsonHandler};

impl JsonHandler {
    pub async fn process(&self, message: Message) -> Result<Message, HttpError> {
        let id = message.id();
        if let Message::Request(request) = message {
            let message_inner = self
                .process_request_internal(request)
                .await
                .map_err(|e| e.0)
                .map_err(rpc_error_responce_inverter);
            Ok(Message::response(id, message_inner))
        } else {
            Ok(Message::error(RpcError::parse_error(
                "JSON RPC Request format was expected".to_owned(),
            )))
        }
    }

    async fn process_request_execute_subscenario(&self, request: Request) -> Result<Value, RpcErr> {
        let req = ExecuteSubscenarioRequest::parse(Some(request.params))?;

        {
            let mut store = self.node_chain_store.lock().await;

            match req.scenario_id {
                1 => store.subscenario_1().await,
                2 => store.subscenario_2().await,
                3 => store.subscenario_3().await,
                4 => store.subscenario_4().await,
                5 => store.subscenario_5().await,
                _ => return Err(RpcErr(RpcError::invalid_params("Scenario id not found"))),
            }
        }

        let helperstruct = ExecuteSubscenarioResponse {
            scenario_result: "success".to_string(),
        };

        respond(helperstruct)
    }

    async fn process_register_account(&self, request: Request) -> Result<Value, RpcErr> {
        let _req = RegisterAccountRequest::parse(Some(request.params))?;

        let acc_addr = {
            let mut guard = self.node_chain_store.lock().await;

            guard.create_new_account().await
        };

        let helperstruct = RegisterAccountResponse {
            status: hex::encode(acc_addr),
        };

        respond(helperstruct)
    }

    async fn process_send_tx(&self, request: Request) -> Result<Value, RpcErr> {
        let req = SendTxRequest::parse(Some(request.params))?;

        {
            let guard = self.node_chain_store.lock().await;

            guard
                .sequencer_client
                .send_tx(req.transaction)
                .await
                .map_err(cast_seq_client_error_into_rpc_error)?;
        }

        let helperstruct = RegisterAccountResponse {
            status: "success".to_string(),
        };

        respond(helperstruct)
    }

    async fn process_get_block_data(&self, request: Request) -> Result<Value, RpcErr> {
        let req = GetBlockDataRequest::parse(Some(request.params))?;

        let block = {
            let guard = self.node_chain_store.lock().await;

            {
                let read_guard = guard.storage.read().await;

                read_guard.block_store.get_block_at_id(req.block_id)?
            }
        };

        let helperstruct = GetBlockDataResponse { block };

        respond(helperstruct)
    }

    async fn process_get_last_block(&self, request: Request) -> Result<Value, RpcErr> {
        let _req = GetLastBlockRequest::parse(Some(request.params))?;

        let last_block = {
            let guard = self.node_chain_store.lock().await;

            guard.curr_height.load(Ordering::Relaxed)
        };

        let helperstruct = GetLastBlockResponse { last_block };

        respond(helperstruct)
    }

    pub async fn process_request_internal(&self, request: Request) -> Result<Value, RpcErr> {
        match request.method.as_ref() {
            //Todo : Add handling of more JSON RPC methods
            "register_account" => self.process_register_account(request).await,
            "execute_subscenario" => self.process_request_execute_subscenario(request).await,
            "send_tx" => self.process_send_tx(request).await,
            "get_block" => self.process_get_block_data(request).await,
            "get_last_block" => self.process_get_last_block(request).await,
            _ => Err(RpcErr(RpcError::method_not_found(request.method))),
        }
    }
}
