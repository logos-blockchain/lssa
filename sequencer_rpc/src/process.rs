use actix_web::Error as HttpError;
use sequencer_core::config::AccountInitialData;
use serde_json::Value;

use common::{
    merkle_tree_public::TreeHashType,
    rpc_primitives::{
        errors::RpcError,
        message::{Message, Request},
        parser::RpcRequest,
        requests::{
            GetAccountBalanceRequest, GetAccountBalanceResponse, GetInitialTestnetAccountsRequest,
            GetTransactionByHashRequest, GetTransactionByHashResponse,
        },
    },
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
pub const GET_ACCOUNT_BALANCE: &str = "get_account_balance";
pub const GET_TRANSACTION_BY_HASH: &str = "get_transaction_by_hash";

pub const HELLO_FROM_SEQUENCER: &str = "HELLO_FROM_SEQUENCER";

pub const SUCCESS: &str = "Success";

pub const GET_INITIAL_TESTNET_ACCOUNTS: &str = "get_initial_testnet_accounts";

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

            state.push_tx_into_mempool_pre_check(send_tx_req.transaction, send_tx_req.tx_roots)?;
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

    /// Returns the initial accounts for testnet
    /// ToDo: Useful only for testnet and needs to be removed later
    async fn get_initial_testnet_accounts(&self, request: Request) -> Result<Value, RpcErr> {
        let _get_initial_testnet_accounts_request =
            GetInitialTestnetAccountsRequest::parse(Some(request.params))?;

        let initial_accounts: Vec<AccountInitialData> = {
            let state = self.sequencer_state.lock().await;

            state.sequencer_config.initial_accounts.clone()
        };

        respond(initial_accounts)
    }

    /// Returns the balance of the account at the given address.
    /// The address must be a valid hex string of the correct length.
    async fn process_get_account_balance(&self, request: Request) -> Result<Value, RpcErr> {
        let get_account_req = GetAccountBalanceRequest::parse(Some(request.params))?;
        let address_bytes = hex::decode(get_account_req.address)
            .map_err(|_| RpcError::invalid_params("invalid hex".to_string()))?;
        let address = address_bytes
            .try_into()
            .map_err(|_| RpcError::invalid_params("invalid length".to_string()))?;

        let balance = {
            let state = self.sequencer_state.lock().await;
            state.store.acc_store.get_account_balance(&address)
        };

        let helperstruct = GetAccountBalanceResponse { balance };

        respond(helperstruct)
    }

    /// Returns the transaction corresponding to the given hash, if it exists in the blockchain.
    /// The hash must be a valid hex string of the correct length.
    async fn process_get_transaction_by_hash(&self, request: Request) -> Result<Value, RpcErr> {
        let get_transaction_req = GetTransactionByHashRequest::parse(Some(request.params))?;
        let bytes: Vec<u8> = hex::decode(get_transaction_req.hash)
            .map_err(|_| RpcError::invalid_params("invalid hex".to_string()))?;
        let hash: TreeHashType = bytes
            .try_into()
            .map_err(|_| RpcError::invalid_params("invalid length".to_string()))?;

        let transaction = {
            let state = self.sequencer_state.lock().await;
            state.store.block_store.get_transaction_by_hash(hash)
        };
        let helperstruct = GetTransactionByHashResponse { transaction };
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
            GET_INITIAL_TESTNET_ACCOUNTS => self.get_initial_testnet_accounts(request).await,
            GET_ACCOUNT_BALANCE => self.process_get_account_balance(request).await,
            GET_TRANSACTION_BY_HASH => self.process_get_transaction_by_hash(request).await,
            _ => Err(RpcErr(RpcError::method_not_found(request.method))),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::{rpc_handler, JsonHandler};
    use common::{
        rpc_primitives::RpcPollingConfig,
        transaction::{SignaturePrivateKey, Transaction, TransactionBody},
    };
    use sequencer_core::{
        config::{AccountInitialData, SequencerConfig},
        SequencerCore,
    };
    use serde_json::Value;
    use tempfile::tempdir;
    use tokio::sync::Mutex;

    fn sequencer_config_for_tests() -> SequencerConfig {
        let tempdir = tempdir().unwrap();
        let home = tempdir.path().to_path_buf();
        let acc1_addr = vec![
            13, 150, 223, 204, 65, 64, 25, 56, 12, 157, 222, 12, 211, 220, 229, 170, 201, 15, 181,
            68, 59, 248, 113, 16, 135, 65, 174, 175, 222, 85, 42, 215,
        ];

        let acc2_addr = vec![
            151, 72, 112, 233, 190, 141, 10, 192, 138, 168, 59, 63, 199, 167, 166, 134, 41, 29,
            135, 50, 80, 138, 186, 152, 179, 96, 128, 243, 156, 44, 243, 100,
        ];

        let initial_acc1 = AccountInitialData {
            addr: hex::encode(acc1_addr),
            balance: 10000,
        };

        let initial_acc2 = AccountInitialData {
            addr: hex::encode(acc2_addr),
            balance: 20000,
        };

        let initial_accounts = vec![initial_acc1, initial_acc2];

        SequencerConfig {
            home,
            override_rust_log: Some("info".to_string()),
            genesis_id: 1,
            is_genesis_random: false,
            max_num_tx_in_block: 10,
            block_create_timeout_millis: 1000,
            port: 8080,
            initial_accounts,
        }
    }

    fn json_handler_for_tests() -> (JsonHandler, Vec<AccountInitialData>) {
        let config = sequencer_config_for_tests();

        let mut sequencer_core = SequencerCore::start_from_config(config);

        let initial_accounts = sequencer_core.sequencer_config.initial_accounts.clone();

        let tx_body = TransactionBody {
            tx_kind: common::transaction::TxKind::Shielded,
            execution_input: Default::default(),
            execution_output: Default::default(),
            utxo_commitments_spent_hashes: Default::default(),
            utxo_commitments_created_hashes: Default::default(),
            nullifier_created_hashes: Default::default(),
            execution_proof_private: Default::default(),
            encoded_data: Default::default(),
            ephemeral_pub_key: Default::default(),
            commitment: Default::default(),
            tweak: Default::default(),
            secret_r: Default::default(),
            sc_addr: Default::default(),
            state_changes: Default::default(),
        };
        let tx = Transaction::new(tx_body, SignaturePrivateKey::from_slice(&[1; 32]).unwrap());

        sequencer_core
            .push_tx_into_mempool_pre_check(tx, [[0; 32]; 2])
            .unwrap();
        sequencer_core
            .produce_new_block_with_mempool_transactions()
            .unwrap();

        let sequencer_core = Arc::new(Mutex::new(sequencer_core));

        (
            JsonHandler {
                polling_config: RpcPollingConfig::default(),
                sequencer_state: sequencer_core,
            },
            initial_accounts,
        )
    }

    async fn call_rpc_handler_with_json(handler: JsonHandler, request_json: Value) -> Value {
        use actix_web::{test, web, App};

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(handler))
                .route("/", web::post().to(rpc_handler)),
        )
        .await;

        let req = test::TestRequest::post()
            .uri("/")
            .set_json(request_json)
            .to_request();

        let resp = test::call_service(&app, req).await;
        let body = test::read_body(resp).await;

        serde_json::from_slice(&body).unwrap()
    }

    #[actix_web::test]
    async fn test_get_account_balance_for_non_existent_account() {
        let (json_handler, _) = json_handler_for_tests();
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "get_account_balance",
            "params": { "address": "efac".repeat(16) },
            "id": 1
        });
        let expected_response = serde_json::json!({
            "id": 1,
            "jsonrpc": "2.0",
            "result": {
                "balance": 0
            }
        });

        let response = call_rpc_handler_with_json(json_handler, request).await;

        assert_eq!(response, expected_response);
    }

    #[actix_web::test]
    async fn test_get_account_balance_for_invalid_hex() {
        let (json_handler, _) = json_handler_for_tests();
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "get_account_balance",
            "params": { "address": "not_a_valid_hex" },
            "id": 1
        });
        let expected_response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "error": {
                "code": -32602,
                "message": "Invalid params",
                "data": "invalid hex"
            }
        });
        let response = call_rpc_handler_with_json(json_handler, request).await;

        assert_eq!(response, expected_response);
    }

    #[actix_web::test]
    async fn test_get_account_balance_for_invalid_length() {
        let (json_handler, _) = json_handler_for_tests();
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "get_account_balance",
            "params": { "address": "cafecafe" },
            "id": 1
        });
        let expected_response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "error": {
                "code": -32602,
                "message": "Invalid params",
                "data": "invalid length"
            }
        });
        let response = call_rpc_handler_with_json(json_handler, request).await;

        assert_eq!(response, expected_response);
    }

    #[actix_web::test]
    async fn test_get_account_balance_for_existing_account() {
        let (json_handler, initial_accounts) = json_handler_for_tests();

        let acc1_addr = initial_accounts[0].addr.clone();

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "get_account_balance",
            "params": { "address": acc1_addr },
            "id": 1
        });
        let expected_response = serde_json::json!({
            "id": 1,
            "jsonrpc": "2.0",
            "result": {
                "balance": 10000
            }
        });

        let response = call_rpc_handler_with_json(json_handler, request).await;

        assert_eq!(response, expected_response);
    }

    #[actix_web::test]
    async fn test_get_transaction_by_hash_for_non_existent_hash() {
        let (json_handler, _) = json_handler_for_tests();
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "get_transaction_by_hash",
            "params": { "hash": "cafe".repeat(16) },
            "id": 1
        });
        let expected_response = serde_json::json!({
            "id": 1,
            "jsonrpc": "2.0",
            "result": {
                "transaction": null
            }
        });

        let response = call_rpc_handler_with_json(json_handler, request).await;

        assert_eq!(response, expected_response);
    }

    #[actix_web::test]
    async fn test_get_transaction_by_hash_for_invalid_hex() {
        let (json_handler, _) = json_handler_for_tests();
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "get_transaction_by_hash",
            "params": { "hash": "not_a_valid_hex" },
            "id": 1
        });
        let expected_response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "error": {
                "code": -32602,
                "message": "Invalid params",
                "data": "invalid hex"
            }
        });

        let response = call_rpc_handler_with_json(json_handler, request).await;

        assert_eq!(response, expected_response);
    }

    #[actix_web::test]
    async fn test_get_transaction_by_hash_for_invalid_length() {
        let (json_handler, _) = json_handler_for_tests();
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "get_transaction_by_hash",
            "params": { "hash": "cafecafe" },
            "id": 1
        });
        let expected_response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "error": {
                "code": -32602,
                "message": "Invalid params",
                "data": "invalid length"
            }
        });

        let response = call_rpc_handler_with_json(json_handler, request).await;

        assert_eq!(response, expected_response);
    }

    #[actix_web::test]
    async fn test_get_transaction_by_hash_for_existing_transaction() {
        let (json_handler, _) = json_handler_for_tests();
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "get_transaction_by_hash",
            "params": { "hash": "a5210ef33912a448cfe6eda43878c144df81f7bdf51d19b5ddf97be11806a6ed"},
            "id": 1
        });

        let expected_response = serde_json::json!({
            "id": 1,
            "jsonrpc": "2.0",
            "result": {
                "transaction": {
                    "body": {
                        "commitment": [],
                        "encoded_data": [],
                        "ephemeral_pub_key": [],
                        "execution_input": [],
                        "execution_output": [],
                        "execution_proof_private": "",
                        "nullifier_created_hashes": [],
                        "sc_addr": "",
                        "secret_r": vec![0; 32],
                        "state_changes": [null, 0],
                        "tweak": "0".repeat(64),
                        "tx_kind": "Shielded",
                        "utxo_commitments_created_hashes": [],
                        "utxo_commitments_spent_hashes": [],
                    },
                    "public_key": "3056301006072A8648CE3D020106052B8104000A034200041B84C5567B126440995D3ED5AABA0565D71E1834604819FF9C17F5E9D5DD078F70BEAF8F588B541507FED6A642C5AB42DFDF8120A7F639DE5122D47A69A8E8D1",
                    "signature": "A4E0D6A25BE829B006124F0DFD766427967AA3BEA96C29219E79BB2CC871891F384748C27E28718A4450AA78709FBF1A57DB33BCD575A2C819D2A439C2D878E6"
                }
            }
        });

        let response = call_rpc_handler_with_json(json_handler, request).await;

        assert_eq!(response, expected_response);
    }
}
