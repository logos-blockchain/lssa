use std::sync::atomic::Ordering;

use actix_web::Error as HttpError;
use serde_json::Value;

use common::rpc_primitives::{
    errors::RpcError,
    message::{Message, Request},
    parser::RpcRequest,
};
use common::transaction::ActionData;

use common::rpc_primitives::requests::{GetLastBlockRequest, GetLastBlockResponse};

use crate::types::rpc_structs::{
    CreateAccountRequest, CreateAccountResponse, ShowAccountPublicBalanceRequest,
    ShowAccountPublicBalanceResponse, ShowAccountUTXORequest, ShowAccountUTXOResponse,
    ShowTransactionRequest, ShowTransactionResponse,
};

pub const CREATE_ACCOUNT: &str = "create_account";
pub const EXECUTE_SUBSCENARIO: &str = "execute_subscenario";
pub const GET_BLOCK: &str = "get_block";
pub const GET_LAST_BLOCK: &str = "get_last_block";
pub const EXECUTE_SCENARIO_SPLIT: &str = "execute_scenario_split";
pub const EXECUTE_SCENARIO_MULTIPLE_SEND: &str = "execute_scenario_multiple_send";
pub const SHOW_ACCOUNT_PUBLIC_BALANCE: &str = "show_account_public_balance";
pub const SHOW_ACCOUNT_UTXO: &str = "show_account_utxo";
pub const SHOW_TRANSACTION: &str = "show_transaction";
pub const WRITE_MINT_UTXO: &str = "write_mint_utxo";
pub const WRITE_MINT_UTXO_MULTIPLE_ASSETS: &str = "write_mint_utxo_multiple_assets";
pub const WRITE_SEND_UTXO_PRIVATE: &str = "write_send_utxo_private";
pub const WRITE_SEND_UTXO_SHIELDED: &str = "write_send_utxo_shielded";
pub const WRITE_SEND_UTXO_DESHIELDED: &str = "write_send_utxo_deshielded";
pub const WRITE_SPLIT_UTXO: &str = "write_split_utxo";

pub const SUCCESS: &str = "success";

pub const ACCOUNT_NOT_FOUND: &str = "Account not found";
pub const TRANSACTION_NOT_FOUND: &str = "Transaction not found";

use super::{respond, types::err_rpc::RpcErr, JsonHandler};

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

    async fn process_create_account(&self, request: Request) -> Result<Value, RpcErr> {
        let _req = CreateAccountRequest::parse(Some(request.params))?;

        let acc_addr = {
            let mut guard = self.node_chain_store.lock().await;

            guard.create_new_account().await
        };

        let helperstruct = CreateAccountResponse {
            status: hex::encode(acc_addr),
        };

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

    async fn process_show_account_public_balance(&self, request: Request) -> Result<Value, RpcErr> {
        let req = ShowAccountPublicBalanceRequest::parse(Some(request.params))?;

        let acc_addr_hex_dec = hex::decode(req.account_addr.clone()).map_err(|_| {
            RpcError::parse_error("Failed to decode account address from hex string".to_string())
        })?;

        let acc_addr: [u8; 32] = acc_addr_hex_dec.try_into().map_err(|_| {
            RpcError::parse_error("Failed to parse account address from bytes".to_string())
        })?;

        let balance = {
            let cover_guard = self.node_chain_store.lock().await;

            {
                let under_guard = cover_guard.storage.read().await;

                let acc = under_guard
                    .acc_map
                    .get(&acc_addr)
                    .ok_or(RpcError::new_internal_error(None, ACCOUNT_NOT_FOUND))?;

                acc.balance
            }
        };

        let helperstruct = ShowAccountPublicBalanceResponse {
            addr: req.account_addr,
            balance,
        };

        respond(helperstruct)
    }

    async fn process_show_account_utxo_request(&self, request: Request) -> Result<Value, RpcErr> {
        let req = ShowAccountUTXORequest::parse(Some(request.params))?;

        let acc_addr_hex_dec = hex::decode(req.account_addr.clone()).map_err(|_| {
            RpcError::parse_error("Failed to decode account address from hex string".to_string())
        })?;

        let acc_addr: [u8; 32] = acc_addr_hex_dec.try_into().map_err(|_| {
            RpcError::parse_error("Failed to parse account address from bytes".to_string())
        })?;

        let utxo_hash_hex_dec = hex::decode(req.utxo_hash.clone()).map_err(|_| {
            RpcError::parse_error("Failed to decode hash from hex string".to_string())
        })?;

        let utxo_hash: [u8; 32] = utxo_hash_hex_dec
            .try_into()
            .map_err(|_| RpcError::parse_error("Failed to parse hash from bytes".to_string()))?;

        let (asset, amount) = {
            let cover_guard = self.node_chain_store.lock().await;

            {
                let mut under_guard = cover_guard.storage.write().await;

                let acc = under_guard
                    .acc_map
                    .get_mut(&acc_addr)
                    .ok_or(RpcError::new_internal_error(None, ACCOUNT_NOT_FOUND))?;

                let utxo = acc
                    .utxos
                    .get(&utxo_hash)
                    .ok_or(RpcError::new_internal_error(
                        None,
                        "UTXO does not exist in the tree",
                    ))?;

                (utxo.asset.clone(), utxo.amount)
            }
        };

        let helperstruct = ShowAccountUTXOResponse {
            hash: req.utxo_hash,
            asset,
            amount,
        };

        respond(helperstruct)
    }

    async fn process_show_transaction(&self, request: Request) -> Result<Value, RpcErr> {
        let req = ShowTransactionRequest::parse(Some(request.params))?;

        let tx_hash_hex_dec = hex::decode(req.tx_hash.clone()).map_err(|_| {
            RpcError::parse_error("Failed to decode hash from hex string".to_string())
        })?;

        let tx_hash: [u8; 32] = tx_hash_hex_dec
            .try_into()
            .map_err(|_| RpcError::parse_error("Failed to parse hash from bytes".to_string()))?;

        let helperstruct = {
            let cover_guard = self.node_chain_store.lock().await;

            {
                let under_guard = cover_guard.storage.read().await;

                let tx = under_guard
                    .pub_tx_store
                    .get_tx(tx_hash)
                    .ok_or(RpcError::new_internal_error(None, TRANSACTION_NOT_FOUND))?;

                ShowTransactionResponse {
                    hash: req.tx_hash,
                    tx_kind: tx.body().tx_kind,
                    public_input: if let Ok(action) =
                        serde_json::from_slice::<ActionData>(&tx.body().execution_input)
                    {
                        action.into_hexed_print()
                    } else {
                        "".to_string()
                    },
                    public_output: if let Ok(action) =
                        serde_json::from_slice::<ActionData>(&tx.body().execution_output)
                    {
                        action.into_hexed_print()
                    } else {
                        "".to_string()
                    },
                    utxo_commitments_created_hashes: tx
                        .body()
                        .utxo_commitments_created_hashes
                        .iter()
                        .map(hex::encode)
                        .collect::<Vec<_>>(),
                    utxo_commitments_spent_hashes: tx
                        .body()
                        .utxo_commitments_spent_hashes
                        .iter()
                        .map(hex::encode)
                        .collect::<Vec<_>>(),
                    utxo_nullifiers_created_hashes: tx
                        .body()
                        .nullifier_created_hashes
                        .iter()
                        .map(hex::encode)
                        .collect::<Vec<_>>(),
                    encoded_data: tx
                        .body()
                        .encoded_data
                        .iter()
                        .map(|val| (hex::encode(val.0.clone()), hex::encode(val.1.clone())))
                        .collect::<Vec<_>>(),
                    ephemeral_pub_key: hex::encode(tx.body().ephemeral_pub_key.clone()),
                }
            }
        };

        respond(helperstruct)
    }

    pub async fn process_request_internal(&self, request: Request) -> Result<Value, RpcErr> {
        match request.method.as_ref() {
            //Todo : Add handling of more JSON RPC methods
            CREATE_ACCOUNT => self.process_create_account(request).await,
            GET_LAST_BLOCK => self.process_get_last_block(request).await,
            SHOW_ACCOUNT_PUBLIC_BALANCE => self.process_show_account_public_balance(request).await,
            SHOW_ACCOUNT_UTXO => self.process_show_account_utxo_request(request).await,
            SHOW_TRANSACTION => self.process_show_transaction(request).await,
            _ => Err(RpcErr(RpcError::method_not_found(request.method))),
        }
    }
}
