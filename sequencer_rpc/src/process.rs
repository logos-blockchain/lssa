use accounts::account_core::AccountForSerialization;
use actix_web::Error as HttpError;
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

        let accounts_for_serialization: Vec<AccountForSerialization> = {
            let state = self.sequencer_state.lock().await;

            state.sequencer_config.initial_accounts.clone()
        };

        respond(accounts_for_serialization)
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
    use accounts::account_core::Account;
    use common::{
        rpc_primitives::RpcPollingConfig,
        transaction::{SignaturePrivateKey, Transaction, TransactionBody},
    };
    use sequencer_core::{config::SequencerConfig, SequencerCore};
    use serde_json::Value;
    use tempfile::tempdir;
    use tokio::sync::Mutex;

    fn sequencer_config_for_tests() -> SequencerConfig {
        let tempdir = tempdir().unwrap();
        let home = tempdir.path().to_path_buf();
        let initial_acc1 = serde_json::from_str(r#"{
            "address": [
                244,
                55,
                238,
                205,
                74,
                115,
                179,
                192,
                65,
                186,
                166,
                169,
                221,
                45,
                6,
                57,
                200,
                65,
                195,
                70,
                118,
                252,
                206,
                100,
                215,
                250,
                72,
                230,
                19,
                71,
                217,
                249
            ],
            "balance": 100,
            "key_holder": {
                "address": [
                    244,
                    55,
                    238,
                    205,
                    74,
                    115,
                    179,
                    192,
                    65,
                    186,
                    166,
                    169,
                    221,
                    45,
                    6,
                    57,
                    200,
                    65,
                    195,
                    70,
                    118,
                    252,
                    206,
                    100,
                    215,
                    250,
                    72,
                    230,
                    19,
                    71,
                    217,
                    249
                ],
                "nullifer_public_key": "03A340BECA9FAAB444CED0140681D72EA1318B5C611704FEE017DA9836B17DB718",
                "pub_account_signing_key": [
                    244,
                    88,
                    134,
                    61,
                    35,
                    209,
                    229,
                    101,
                    85,
                    35,
                    140,
                    140,
                    192,
                    226,
                    83,
                    83,
                    190,
                    189,
                    110,
                    8,
                    89,
                    127,
                    147,
                    142,
                    157,
                    204,
                    51,
                    109,
                    189,
                    92,
                    144,
                    68
                ],
                "top_secret_key_holder": {
                    "secret_spending_key": "7BC46784DB1BC67825D8F029436846712BFDF9B5D79EA3AB11D39A52B9B229D4"
                },
                "utxo_secret_key_holder": {
                    "nullifier_secret_key": "BB54A8D3C9C51B82C431082D1845A74677B0EF829A11B517E1D9885DE3139506",
                    "viewing_secret_key": "AD923E92F6A5683E30140CEAB2702AFB665330C1EE4EFA70FAF29767B6B52BAF"
                },
                "viewing_public_key": "0361220C5D277E7A1709340FD31A52600C1432B9C45B9BCF88A43581D58824A8B6"
            },
            "utxos": {}
        }"#).unwrap();

        let initial_acc2 = serde_json::from_str(r#"{
            "address": [
                72,
                169,
                70,
                237,
                1,
                96,
                35,
                157,
                25,
                15,
                83,
                18,
                52,
                206,
                202,
                63,
                48,
                59,
                173,
                76,
                78,
                7,
                254,
                229,
                28,
                45,
                194,
                79,
                6,
                89,
                58,
                85
            ],
            "balance": 200,
            "key_holder": {
                "address": [
                    72,
                    169,
                    70,
                    237,
                    1,
                    96,
                    35,
                    157,
                    25,
                    15,
                    83,
                    18,
                    52,
                    206,
                    202,
                    63,
                    48,
                    59,
                    173,
                    76,
                    78,
                    7,
                    254,
                    229,
                    28,
                    45,
                    194,
                    79,
                    6,
                    89,
                    58,
                    85
                ],
                "nullifer_public_key": "02172F50274DE67C4087C344F5D58E11DF761D90285B095060E0994FAA6BCDE271",
                "pub_account_signing_key": [
                    136,
                    105,
                    9,
                    53,
                    180,
                    145,
                    64,
                    5,
                    235,
                    174,
                    62,
                    211,
                    206,
                    116,
                    185,
                    24,
                    214,
                    62,
                    244,
                    64,
                    224,
                    59,
                    120,
                    150,
                    30,
                    249,
                    160,
                    46,
                    189,
                    254,
                    47,
                    244
                ],
                "top_secret_key_holder": {
                    "secret_spending_key": "80A186737C8D38B4288A03F0F589957D9C040D79C19F3E0CC4BA80F8494E5179"
                },
                "utxo_secret_key_holder": {
                    "nullifier_secret_key": "746928E63F0984F6F4818933493CE9C067562D9CB932FDC06D82C86CDF6D7122",
                    "viewing_secret_key": "89176CF4BC9E673807643FD52110EF99D4894335AFB10D881AC0B5041FE1FCB7"
                },
                "viewing_public_key": "026072A8F83FEC3472E30CDD4767683F30B91661D25B1040AD9A5FC2E01D659F99"
            },
            "utxos": {}
        }"#).unwrap();

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

    fn json_handler_for_tests() -> (JsonHandler, Vec<Account>) {
        let config = sequencer_config_for_tests();

        let mut sequencer_core = SequencerCore::start_from_config(config);
        let initial_accounts = sequencer_core
            .sequencer_config
            .initial_accounts
            .iter()
            .map(|acc_ser| acc_ser.clone().into())
            .collect();

        let tx_body = TransactionBody {
            tx_kind: common::transaction::TxKind::Public,
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

        let acc1_addr = hex::encode(initial_accounts[0].address);

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
                "balance": 100
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
            "params": { "hash": "ca8e38269c0137d27cbe7c55d240a834b46e86e236578b9a1a3a25b3dabc5709" },
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
                        "tx_kind": "Public",
                        "utxo_commitments_created_hashes": [],
                        "utxo_commitments_spent_hashes": []
                    },
                    "public_key": "3056301006072A8648CE3D020106052B8104000A034200041B84C5567B126440995D3ED5AABA0565D71E1834604819FF9C17F5E9D5DD078F70BEAF8F588B541507FED6A642C5AB42DFDF8120A7F639DE5122D47A69A8E8D1",
                    "signature": "28CB6CA744864340A3441CB48D5700690F90130DE0760EE5C640F85F4285C5FD2BD7D0E270EC2AC82E4124999E63659AA9C33CF378F959EDF4E50F2626EA3B99"
                }
            }
        });

        let response = call_rpc_handler_with_json(json_handler, request).await;

        assert_eq!(response, expected_response);
    }
}
