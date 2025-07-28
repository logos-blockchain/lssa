use std::fmt::Display;

use accounts::account_core::AccountAddress;
use anyhow::Result;
use common::{
    block::{Block, HashableBlockData},
    execution_input::PublicNativeTokenSend,
    merkle_tree_public::TreeHashType,
    nullifier::UTXONullifier,
    transaction::{AuthenticatedTransaction, Transaction, TransactionBody, TxKind},
    utxo_commitment::UTXOCommitment,
};
use config::SequencerConfig;
use mempool::MemPool;
use mempool_transaction::MempoolTransaction;
use sequencer_store::SequecerChainStore;
use serde::{Deserialize, Serialize};
use tiny_keccak::{Hasher, Keccak};

pub mod config;
pub mod mempool_transaction;
pub mod sequencer_store;

pub struct SequencerCore {
    pub store: SequecerChainStore,
    pub mempool: MemPool<MempoolTransaction>,
    pub sequencer_config: SequencerConfig,
    pub chain_height: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransactionMalformationErrorKind {
    PublicTransactionChangedPrivateData { tx: TreeHashType },
    PrivateTransactionChangedPublicData { tx: TreeHashType },
    TxHashAlreadyPresentInTree { tx: TreeHashType },
    NullifierAlreadyPresentInTree { tx: TreeHashType },
    UTXOCommitmentAlreadyPresentInTree { tx: TreeHashType },
    MempoolFullForRound { tx: TreeHashType },
    ChainStateFurtherThanTransactionState { tx: TreeHashType },
    FailedToInsert { tx: TreeHashType, details: String },
    InvalidSignature,
    IncorrectSender,
    BalanceMismatch { tx: TreeHashType },
    FailedToDecode { tx: TreeHashType },
}

impl Display for TransactionMalformationErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:#?}")
    }
}

impl std::error::Error for TransactionMalformationErrorKind {}

impl SequencerCore {
    pub fn start_from_config(config: SequencerConfig) -> Self {
        Self {
            store: SequecerChainStore::new_with_genesis(
                &config.home,
                config.genesis_id,
                config.is_genesis_random,
                &config.initial_accounts,
            ),
            mempool: MemPool::<MempoolTransaction>::default(),
            chain_height: config.genesis_id,
            sequencer_config: config,
        }
    }

    pub fn get_tree_roots(&self) -> [[u8; 32]; 2] {
        [
            self.store
                .utxo_commitments_store
                .get_root()
                .unwrap_or([0; 32]),
            self.store.pub_tx_store.get_root().unwrap_or([0; 32]),
        ]
    }

    pub fn transaction_pre_check(
        &mut self,
        tx: Transaction,
        tx_roots: [[u8; 32]; 2],
    ) -> Result<AuthenticatedTransaction, TransactionMalformationErrorKind> {
        let tx = tx
            .into_authenticated()
            .map_err(|_| TransactionMalformationErrorKind::InvalidSignature)?;

        let TransactionBody {
            tx_kind,
            ref execution_input,
            ref execution_output,
            ref utxo_commitments_created_hashes,
            ref nullifier_created_hashes,
            ..
        } = tx.transaction().body();

        let tx_hash = *tx.hash();

        let mempool_size = self.mempool.len();

        if mempool_size >= self.sequencer_config.max_num_tx_in_block {
            return Err(TransactionMalformationErrorKind::MempoolFullForRound { tx: tx_hash });
        }

        let curr_sequencer_roots = self.get_tree_roots();

        if tx_roots != curr_sequencer_roots {
            return Err(
                TransactionMalformationErrorKind::ChainStateFurtherThanTransactionState {
                    tx: tx_hash,
                },
            );
        }

        //Sanity check
        match tx_kind {
            TxKind::Public => {
                if !utxo_commitments_created_hashes.is_empty()
                    || !nullifier_created_hashes.is_empty()
                {
                    //Public transactions can not make private operations.
                    return Err(
                        TransactionMalformationErrorKind::PublicTransactionChangedPrivateData {
                            tx: tx_hash,
                        },
                    );
                }
            }
            TxKind::Private => {
                if !execution_input.is_empty() || !execution_output.is_empty() {
                    //Not entirely necessary, but useful simplification for a future.
                    //This way only shielded and deshielded transactions can be used for interaction
                    //between public and private state.
                    return Err(
                        TransactionMalformationErrorKind::PrivateTransactionChangedPublicData {
                            tx: tx_hash,
                        },
                    );
                }
            }
            _ => {}
        };

        //Correct sender check
        if let Ok(native_transfer_action) =
            serde_json::from_slice::<PublicNativeTokenSend>(execution_input)
        {
            let mut output = [0; 32];
            let mut keccak_hasher = Keccak::v256();
            keccak_hasher.update(&tx.transaction().public_key.to_sec1_bytes());
            keccak_hasher.finalize(&mut output);

            if native_transfer_action.from != output {
                return Err(TransactionMalformationErrorKind::IncorrectSender);
            }
        }

        //Tree checks
        let tx_tree_check = self.store.pub_tx_store.get_tx(tx_hash).is_some();
        let nullifier_tree_check = nullifier_created_hashes.iter().any(|nullifier_hash| {
            self.store.nullifier_store.contains(&UTXONullifier {
                utxo_hash: *nullifier_hash,
            })
        });
        let utxo_commitments_check =
            utxo_commitments_created_hashes
                .iter()
                .any(|utxo_commitment_hash| {
                    self.store
                        .utxo_commitments_store
                        .get_tx(*utxo_commitment_hash)
                        .is_some()
                });

        if tx_tree_check {
            return Err(
                TransactionMalformationErrorKind::TxHashAlreadyPresentInTree { tx: *tx.hash() },
            );
        }

        if nullifier_tree_check {
            return Err(
                TransactionMalformationErrorKind::NullifierAlreadyPresentInTree { tx: *tx.hash() },
            );
        }

        if utxo_commitments_check {
            return Err(
                TransactionMalformationErrorKind::UTXOCommitmentAlreadyPresentInTree {
                    tx: *tx.hash(),
                },
            );
        }

        Ok(tx)
    }

    pub fn push_tx_into_mempool_pre_check(
        &mut self,
        transaction: Transaction,
        tx_roots: [[u8; 32]; 2],
    ) -> Result<(), TransactionMalformationErrorKind> {
        let mempool_size = self.mempool.len();
        if mempool_size >= self.sequencer_config.max_num_tx_in_block {
            return Err(TransactionMalformationErrorKind::MempoolFullForRound {
                tx: transaction.body().hash(),
            });
        }

        let authenticated_tx = self.transaction_pre_check(transaction, tx_roots)?;

        self.mempool.push_item(authenticated_tx.into());

        Ok(())
    }

    fn execute_check_transaction_on_state(
        &mut self,
        mempool_tx: &MempoolTransaction,
    ) -> Result<(), TransactionMalformationErrorKind> {
        let TransactionBody {
            ref utxo_commitments_created_hashes,
            ref nullifier_created_hashes,
            execution_input,
            ..
        } = mempool_tx.auth_tx.transaction().body();

        let tx_hash = *mempool_tx.auth_tx.hash();

        //Balance check
        if let Ok(native_transfer_action) =
            serde_json::from_slice::<PublicNativeTokenSend>(execution_input)
        {
            let from_balance = self
                .store
                .acc_store
                .get_account_balance(&native_transfer_action.from);
            let to_balance = self
                .store
                .acc_store
                .get_account_balance(&native_transfer_action.to);

            if from_balance >= native_transfer_action.balance_to_move {
                self.store.acc_store.set_account_balance(
                    &native_transfer_action.from,
                    from_balance - native_transfer_action.balance_to_move,
                );
                self.store.acc_store.set_account_balance(
                    &native_transfer_action.to,
                    to_balance + native_transfer_action.balance_to_move,
                );
            } else {
                return Err(TransactionMalformationErrorKind::BalanceMismatch { tx: tx_hash });
            }
        }

        for utxo_comm in utxo_commitments_created_hashes {
            self.store
                .utxo_commitments_store
                .add_tx(&UTXOCommitment { hash: *utxo_comm });
        }

        for nullifier in nullifier_created_hashes.iter() {
            self.store.nullifier_store.insert(UTXONullifier {
                utxo_hash: *nullifier,
            });
        }

        self.store
            .pub_tx_store
            .add_tx(mempool_tx.auth_tx.transaction());

        Ok(())
    }

    pub fn register_account(&mut self, account_addr: AccountAddress) {
        self.store.acc_store.register_account(account_addr);
    }

    ///Produces new block from transactions in mempool
    pub fn produce_new_block_with_mempool_transactions(&mut self) -> Result<u64> {
        let new_block_height = self.chain_height + 1;

        let transactions = self
            .mempool
            .pop_size(self.sequencer_config.max_num_tx_in_block);

        for tx in &transactions {
            self.execute_check_transaction_on_state(tx)?;
        }

        let prev_block_hash = self
            .store
            .block_store
            .get_block_at_id(self.chain_height)?
            .hash;

        let hashable_data = HashableBlockData {
            block_id: new_block_height,
            prev_block_id: self.chain_height,
            transactions: transactions
                .into_iter()
                .map(|tx_mem| tx_mem.auth_tx.transaction().clone())
                .collect(),
            data: vec![],
            prev_block_hash,
        };

        let block = Block::produce_block_from_hashable_data(hashable_data);

        self.store.block_store.put_block_at_id(block)?;

        self.chain_height = new_block_height;

        Ok(self.chain_height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use accounts::account_core::AccountForSerialization;
    use common::transaction::{SignaturePrivateKey, Transaction, TransactionBody, TxKind};
    use mempool_transaction::MempoolTransaction;
    use rand::Rng;
    use secp256k1_zkp::Tweak;

    fn setup_sequencer_config_variable_initial_accounts(
        initial_accounts: Vec<AccountForSerialization>,
    ) -> SequencerConfig {
        let mut rng = rand::thread_rng();
        let random_u8: u8 = rng.gen();

        let path_str = format!("/tmp/sequencer_{random_u8:?}");

        SequencerConfig {
            home: PathBuf::from(path_str),
            override_rust_log: Some("info".to_string()),
            genesis_id: 1,
            is_genesis_random: false,
            max_num_tx_in_block: 10,
            block_create_timeout_millis: 1000,
            port: 8080,
            initial_accounts,
        }
    }

    fn setup_sequencer_config() -> SequencerConfig {
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
            "balance": 10,
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
            "balance": 100,
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

        setup_sequencer_config_variable_initial_accounts(initial_accounts)
    }

    fn create_dummy_transaction(
        nullifier_created_hashes: Vec<[u8; 32]>,
        utxo_commitments_spent_hashes: Vec<[u8; 32]>,
        utxo_commitments_created_hashes: Vec<[u8; 32]>,
    ) -> Transaction {
        let mut rng = rand::thread_rng();

        let body = TransactionBody {
            tx_kind: TxKind::Private,
            execution_input: vec![],
            execution_output: vec![],
            utxo_commitments_spent_hashes,
            utxo_commitments_created_hashes,
            nullifier_created_hashes,
            execution_proof_private: "dummy_proof".to_string(),
            encoded_data: vec![],
            ephemeral_pub_key: vec![10, 11, 12],
            commitment: vec![],
            tweak: Tweak::new(&mut rng),
            secret_r: [0; 32],
            sc_addr: "sc_addr".to_string(),
            state_changes: (serde_json::Value::Null, 0),
        };
        Transaction::new(body, SignaturePrivateKey::random(&mut rng))
    }

    fn common_setup(sequencer: &mut SequencerCore) {
        let tx = create_dummy_transaction(vec![[9; 32]], vec![[7; 32]], vec![[8; 32]]);
        let mempool_tx = MempoolTransaction {
            auth_tx: tx.into_authenticated().unwrap(),
        };
        sequencer.mempool.push_item(mempool_tx);

        sequencer
            .produce_new_block_with_mempool_transactions()
            .unwrap();
    }

    #[test]
    fn test_start_from_config() {
        let config = setup_sequencer_config();
        let sequencer = SequencerCore::start_from_config(config.clone());

        assert_eq!(sequencer.chain_height, config.genesis_id);
        assert_eq!(sequencer.sequencer_config.max_num_tx_in_block, 10);
        assert_eq!(sequencer.sequencer_config.port, 8080);

        let acc1_addr = config.initial_accounts[0].address;
        let acc2_addr = config.initial_accounts[1].address;

        assert!(sequencer.store.acc_store.contains_account(&acc1_addr));
        assert!(sequencer.store.acc_store.contains_account(&acc2_addr));

        assert_eq!(
            10,
            sequencer.store.acc_store.get_account_balance(&acc1_addr)
        );
        assert_eq!(
            100,
            sequencer.store.acc_store.get_account_balance(&acc2_addr)
        );
    }

    #[test]
    fn test_start_different_intial_accounts_balances() {
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
            "balance": 1000,
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
            "balance": 1000,
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

        let intial_accounts_len = initial_accounts.len();

        let config = setup_sequencer_config_variable_initial_accounts(initial_accounts);
        let sequencer = SequencerCore::start_from_config(config.clone());

        let acc1_addr = config.initial_accounts[0].address;
        let acc2_addr = config.initial_accounts[1].address;

        assert!(sequencer.store.acc_store.contains_account(&acc1_addr));
        assert!(sequencer.store.acc_store.contains_account(&acc2_addr));

        assert_eq!(sequencer.store.acc_store.len(), intial_accounts_len);

        assert_eq!(
            1000,
            sequencer.store.acc_store.get_account_balance(&acc1_addr)
        );
        assert_eq!(
            1000,
            sequencer.store.acc_store.get_account_balance(&acc2_addr)
        );
    }

    #[test]
    fn test_get_tree_roots() {
        let config = setup_sequencer_config();
        let mut sequencer = SequencerCore::start_from_config(config);

        common_setup(&mut sequencer);

        let roots = sequencer.get_tree_roots();
        assert_eq!(roots.len(), 2); // Should return two roots
    }

    #[test]
    fn test_transaction_pre_check_pass() {
        let config = setup_sequencer_config();
        let mut sequencer = SequencerCore::start_from_config(config);

        common_setup(&mut sequencer);

        let tx = create_dummy_transaction(vec![[91; 32]], vec![[71; 32]], vec![[81; 32]]);
        let tx_roots = sequencer.get_tree_roots();
        let result = sequencer.transaction_pre_check(tx, tx_roots);

        assert!(result.is_ok());
    }

    #[test]
    fn test_push_tx_into_mempool_fails_mempool_full() {
        let config = SequencerConfig {
            max_num_tx_in_block: 1,
            ..setup_sequencer_config()
        };
        let mut sequencer = SequencerCore::start_from_config(config);

        common_setup(&mut sequencer);

        let tx = create_dummy_transaction(vec![[92; 32]], vec![[72; 32]], vec![[82; 32]]);
        let tx_roots = sequencer.get_tree_roots();

        // Fill the mempool
        let dummy_tx = MempoolTransaction {
            auth_tx: tx.clone().into_authenticated().unwrap(),
        };
        sequencer.mempool.push_item(dummy_tx);

        let result = sequencer.push_tx_into_mempool_pre_check(tx, tx_roots);

        assert!(matches!(
            result,
            Err(TransactionMalformationErrorKind::MempoolFullForRound { .. })
        ));
    }

    #[test]
    fn test_push_tx_into_mempool_pre_check() {
        let config = setup_sequencer_config();
        let mut sequencer = SequencerCore::start_from_config(config);

        common_setup(&mut sequencer);

        let tx = create_dummy_transaction(vec![[93; 32]], vec![[73; 32]], vec![[83; 32]]);
        let tx_roots = sequencer.get_tree_roots();

        let result = sequencer.push_tx_into_mempool_pre_check(tx, tx_roots);
        assert!(result.is_ok());
        assert_eq!(sequencer.mempool.len(), 1);
    }

    #[test]
    fn test_produce_new_block_with_mempool_transactions() {
        let config = setup_sequencer_config();
        let mut sequencer = SequencerCore::start_from_config(config);
        let genesis_height = sequencer.chain_height;

        let tx = create_dummy_transaction(vec![[94; 32]], vec![[7; 32]], vec![[8; 32]]);
        let tx_mempool = MempoolTransaction {
            auth_tx: tx.into_authenticated().unwrap(),
        };
        sequencer.mempool.push_item(tx_mempool);

        let block_id = sequencer.produce_new_block_with_mempool_transactions();
        assert!(block_id.is_ok());
        assert_eq!(block_id.unwrap(), genesis_height + 1);
    }

    #[test]
    fn test_replay_transactions_are_rejected() {
        let config = setup_sequencer_config();
        let mut sequencer = SequencerCore::start_from_config(config);

        let tx = create_dummy_transaction(vec![[94; 32]], vec![[7; 32]], vec![[8; 32]]);

        // The transaction should be included the first time
        let tx_mempool_original = MempoolTransaction {
            auth_tx: tx.clone().into_authenticated().unwrap(),
        };
        sequencer.mempool.push_item(tx_mempool_original);
        let current_height = sequencer
            .produce_new_block_with_mempool_transactions()
            .unwrap();
        let block = sequencer
            .store
            .block_store
            .get_block_at_id(current_height)
            .unwrap();
        assert_eq!(block.transactions, vec![tx.clone()]);

        // Add same transaction should fail
        let tx_mempool_replay = MempoolTransaction {
            auth_tx: tx.into_authenticated().unwrap(),
        };
        sequencer.mempool.push_item(tx_mempool_replay);
        let current_height = sequencer
            .produce_new_block_with_mempool_transactions()
            .unwrap();
        let block = sequencer
            .store
            .block_store
            .get_block_at_id(current_height)
            .unwrap();
        assert!(block.transactions.is_empty());
    }
}
