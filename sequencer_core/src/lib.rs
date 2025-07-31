use std::fmt::Display;

use accounts::account_core::address::{self, AccountAddress};
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

pub mod config;
pub mod mempool_transaction;
pub mod sequencer_store;

pub struct SequencerCore {
    pub store: SequecerChainStore,
    pub mempool: MemPool<MempoolTransaction>,
    pub sequencer_config: SequencerConfig,
    pub chain_height: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
    NonceMismatch { tx: TreeHashType },
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

        //Native transfers checks
        if let Ok(native_transfer_action) =
            serde_json::from_slice::<PublicNativeTokenSend>(execution_input)
        {
            let signer_address = address::from_public_key(&tx.transaction().public_key);

            //Correct sender check
            if native_transfer_action.from != signer_address {
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

        //Balance move
        if let Ok(native_transfer_action) =
            serde_json::from_slice::<PublicNativeTokenSend>(execution_input)
        {
            // Nonce check
            let signer_addres =
                address::from_public_key(&mempool_tx.auth_tx.transaction().public_key);
            if self.store.acc_store.get_account_nonce(&signer_addres)
                != native_transfer_action.nonce
            {
                return Err(TransactionMalformationErrorKind::NonceMismatch { tx: tx_hash });
            }

            let from_balance = self
                .store
                .acc_store
                .get_account_balance(&native_transfer_action.from);
            let to_balance = self
                .store
                .acc_store
                .get_account_balance(&native_transfer_action.to);

            //Balance check
            if from_balance < native_transfer_action.balance_to_move {
                return Err(TransactionMalformationErrorKind::BalanceMismatch { tx: tx_hash });
            }

            self.store.acc_store.set_account_balance(
                &native_transfer_action.from,
                from_balance - native_transfer_action.balance_to_move,
            );
            self.store.acc_store.set_account_balance(
                &native_transfer_action.to,
                to_balance + native_transfer_action.balance_to_move,
            );

            self.store.acc_store.increase_nonce(&signer_addres);
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

        let valid_transactions = transactions
            .into_iter()
            .filter_map(|mempool_tx| {
                if self.execute_check_transaction_on_state(&mempool_tx).is_ok() {
                    Some(mempool_tx.auth_tx.into_transaction())
                } else {
                    None
                }
            })
            .collect();

        let prev_block_hash = self
            .store
            .block_store
            .get_block_at_id(self.chain_height)?
            .hash;

        let hashable_data = HashableBlockData {
            block_id: new_block_height,
            prev_block_id: self.chain_height,
            transactions: valid_transactions,
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
    use crate::config::AccountInitialData;

    use super::*;

    use common::transaction::{SignaturePrivateKey, Transaction, TransactionBody, TxKind};
    use k256::{ecdsa::SigningKey, FieldBytes};
    use mempool_transaction::MempoolTransaction;
    use secp256k1_zkp::Tweak;

    fn setup_sequencer_config_variable_initial_accounts(
        initial_accounts: Vec<AccountInitialData>,
    ) -> SequencerConfig {
        let tempdir = tempfile::tempdir().unwrap();
        let home = tempdir.path().to_path_buf();

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

    fn setup_sequencer_config() -> SequencerConfig {
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

    fn create_dummy_transaction_native_token_transfer(
        from: [u8; 32],
        nonce: u64,
        to: [u8; 32],
        balance_to_move: u64,
        signing_key: SigningKey,
    ) -> Transaction {
        let mut rng = rand::thread_rng();

        let native_token_transfer = PublicNativeTokenSend {
            from,
            nonce,
            to,
            balance_to_move,
        };

        let body = TransactionBody {
            tx_kind: TxKind::Public,
            execution_input: serde_json::to_vec(&native_token_transfer).unwrap(),
            execution_output: vec![],
            utxo_commitments_spent_hashes: vec![],
            utxo_commitments_created_hashes: vec![],
            nullifier_created_hashes: vec![],
            execution_proof_private: "".to_string(),
            encoded_data: vec![],
            ephemeral_pub_key: vec![10, 11, 12],
            commitment: vec![],
            tweak: Tweak::new(&mut rng),
            secret_r: [0; 32],
            sc_addr: "sc_addr".to_string(),
            state_changes: (serde_json::Value::Null, 0),
        };
        Transaction::new(body, signing_key)
    }

    fn create_signing_key_for_account1() -> SigningKey {
        let pub_sign_key_acc1 = [
            133, 143, 177, 187, 252, 66, 237, 236, 234, 252, 244, 138, 5, 151, 3, 99, 217, 231,
            112, 217, 77, 211, 58, 218, 176, 68, 99, 53, 152, 228, 198, 190,
        ];

        let field_bytes = FieldBytes::from_slice(&pub_sign_key_acc1);
        SigningKey::from_bytes(field_bytes).unwrap()
    }

    fn create_signing_key_for_account2() -> SigningKey {
        let pub_sign_key_acc2 = [
            54, 90, 62, 225, 71, 225, 228, 148, 143, 53, 210, 23, 137, 158, 171, 156, 48, 7, 139,
            52, 117, 242, 214, 7, 99, 29, 122, 184, 59, 116, 144, 107,
        ];

        let field_bytes = FieldBytes::from_slice(&pub_sign_key_acc2);
        SigningKey::from_bytes(field_bytes).unwrap()
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

        let acc1_addr = hex::decode(config.initial_accounts[0].addr.clone())
            .unwrap()
            .try_into()
            .unwrap();
        let acc2_addr = hex::decode(config.initial_accounts[1].addr.clone())
            .unwrap()
            .try_into()
            .unwrap();

        assert!(sequencer.store.acc_store.contains_account(&acc1_addr));
        assert!(sequencer.store.acc_store.contains_account(&acc2_addr));

        assert_eq!(
            10000,
            sequencer.store.acc_store.get_account_balance(&acc1_addr)
        );
        assert_eq!(
            20000,
            sequencer.store.acc_store.get_account_balance(&acc2_addr)
        );
    }

    #[test]
    fn test_start_different_intial_accounts_balances() {
        let acc1_addr = vec![
            13, 150, 223, 204, 65, 64, 25, 56, 12, 157, 222, 12, 211, 220, 229, 170, 201, 15, 181,
            68, 59, 248, 113, 16, 135, 65, 174, 175, 222, 42, 42, 42,
        ];

        let acc2_addr = vec![
            151, 72, 112, 233, 190, 141, 10, 192, 138, 168, 59, 63, 199, 167, 166, 134, 41, 29,
            135, 50, 80, 138, 186, 152, 179, 96, 128, 243, 156, 42, 42, 42,
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

        let intial_accounts_len = initial_accounts.len();

        let config = setup_sequencer_config_variable_initial_accounts(initial_accounts);
        let sequencer = SequencerCore::start_from_config(config.clone());

        let acc1_addr = hex::decode(config.initial_accounts[0].addr.clone())
            .unwrap()
            .try_into()
            .unwrap();
        let acc2_addr = hex::decode(config.initial_accounts[1].addr.clone())
            .unwrap()
            .try_into()
            .unwrap();

        assert!(sequencer.store.acc_store.contains_account(&acc1_addr));
        assert!(sequencer.store.acc_store.contains_account(&acc2_addr));

        assert_eq!(sequencer.store.acc_store.len(), intial_accounts_len);

        assert_eq!(
            10000,
            sequencer.store.acc_store.get_account_balance(&acc1_addr)
        );
        assert_eq!(
            20000,
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
    fn test_transaction_pre_check_native_transfer_valid() {
        let config = setup_sequencer_config();
        let mut sequencer = SequencerCore::start_from_config(config);

        common_setup(&mut sequencer);

        let acc1 = hex::decode(sequencer.sequencer_config.initial_accounts[0].addr.clone())
            .unwrap()
            .try_into()
            .unwrap();
        let acc2 = hex::decode(sequencer.sequencer_config.initial_accounts[1].addr.clone())
            .unwrap()
            .try_into()
            .unwrap();

        let sign_key1 = create_signing_key_for_account1();

        let tx = create_dummy_transaction_native_token_transfer(acc1, 0, acc2, 10, sign_key1);
        let tx_roots = sequencer.get_tree_roots();
        let result = sequencer.transaction_pre_check(tx, tx_roots);

        assert!(result.is_ok());
    }

    #[test]
    fn test_transaction_pre_check_native_transfer_other_signature() {
        let config = setup_sequencer_config();
        let mut sequencer = SequencerCore::start_from_config(config);

        common_setup(&mut sequencer);

        let acc1 = hex::decode(sequencer.sequencer_config.initial_accounts[0].addr.clone())
            .unwrap()
            .try_into()
            .unwrap();
        let acc2 = hex::decode(sequencer.sequencer_config.initial_accounts[1].addr.clone())
            .unwrap()
            .try_into()
            .unwrap();

        let sign_key2 = create_signing_key_for_account2();

        let tx = create_dummy_transaction_native_token_transfer(acc1, 0, acc2, 10, sign_key2);
        let tx_roots = sequencer.get_tree_roots();
        let result = sequencer.transaction_pre_check(tx, tx_roots);

        assert_eq!(
            result.err().unwrap(),
            TransactionMalformationErrorKind::IncorrectSender
        );
    }

    #[test]
    fn test_transaction_pre_check_native_transfer_sent_too_much() {
        let config = setup_sequencer_config();
        let mut sequencer = SequencerCore::start_from_config(config);

        common_setup(&mut sequencer);

        let acc1 = hex::decode(sequencer.sequencer_config.initial_accounts[0].addr.clone())
            .unwrap()
            .try_into()
            .unwrap();
        let acc2 = hex::decode(sequencer.sequencer_config.initial_accounts[1].addr.clone())
            .unwrap()
            .try_into()
            .unwrap();

        let sign_key1 = create_signing_key_for_account1();

        let tx = create_dummy_transaction_native_token_transfer(acc1, 0, acc2, 10000000, sign_key1);
        let tx_roots = sequencer.get_tree_roots();
        let result = sequencer.transaction_pre_check(tx, tx_roots);

        //Passed pre-check
        assert!(result.is_ok());

        let result = sequencer.execute_check_transaction_on_state(&result.unwrap().into());
        let is_failed_at_balance_mismatch = matches!(
            result.err().unwrap(),
            TransactionMalformationErrorKind::BalanceMismatch { tx: _ }
        );

        assert!(is_failed_at_balance_mismatch);
    }

    #[test]
    fn test_transaction_execute_native_transfer() {
        let config = setup_sequencer_config();
        let mut sequencer = SequencerCore::start_from_config(config);

        common_setup(&mut sequencer);

        let acc1 = hex::decode(sequencer.sequencer_config.initial_accounts[0].addr.clone())
            .unwrap()
            .try_into()
            .unwrap();
        let acc2 = hex::decode(sequencer.sequencer_config.initial_accounts[1].addr.clone())
            .unwrap()
            .try_into()
            .unwrap();

        let sign_key1 = create_signing_key_for_account1();

        let tx = create_dummy_transaction_native_token_transfer(acc1, 0, acc2, 100, sign_key1);

        sequencer
            .execute_check_transaction_on_state(&tx.into_authenticated().unwrap().into())
            .unwrap();

        let bal_from = sequencer.store.acc_store.get_account_balance(&acc1);
        let bal_to = sequencer.store.acc_store.get_account_balance(&acc2);

        assert_eq!(bal_from, 9900);
        assert_eq!(bal_to, 20100);
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
    fn test_replay_transactions_are_rejected_in_the_same_block() {
        let config = setup_sequencer_config();
        let mut sequencer = SequencerCore::start_from_config(config);

        common_setup(&mut sequencer);

        let acc1 = hex::decode(sequencer.sequencer_config.initial_accounts[0].addr.clone())
            .unwrap()
            .try_into()
            .unwrap();
        let acc2 = hex::decode(sequencer.sequencer_config.initial_accounts[1].addr.clone())
            .unwrap()
            .try_into()
            .unwrap();

        let sign_key1 = create_signing_key_for_account1();

        let tx = create_dummy_transaction_native_token_transfer(acc1, 0, acc2, 100, sign_key1);

        let tx_mempool_original = MempoolTransaction {
            auth_tx: tx.clone().into_authenticated().unwrap(),
        };
        let tx_mempool_replay = MempoolTransaction {
            auth_tx: tx.clone().into_authenticated().unwrap(),
        };

        // Pushing two copies of the same tx to the mempool
        sequencer.mempool.push_item(tx_mempool_original);
        sequencer.mempool.push_item(tx_mempool_replay);

        // Create block
        let current_height = sequencer
            .produce_new_block_with_mempool_transactions()
            .unwrap();
        let block = sequencer
            .store
            .block_store
            .get_block_at_id(current_height)
            .unwrap();

        // Only one should be included in the block
        assert_eq!(block.transactions, vec![tx.clone()]);
    }

    #[test]
    fn test_replay_transactions_are_rejected_in_different_blocks() {
        let config = setup_sequencer_config();
        let mut sequencer = SequencerCore::start_from_config(config);

        common_setup(&mut sequencer);

        let acc1 = hex::decode(sequencer.sequencer_config.initial_accounts[0].addr.clone())
            .unwrap()
            .try_into()
            .unwrap();
        let acc2 = hex::decode(sequencer.sequencer_config.initial_accounts[1].addr.clone())
            .unwrap()
            .try_into()
            .unwrap();

        let sign_key1 = create_signing_key_for_account1();

        let tx = create_dummy_transaction_native_token_transfer(acc1, 0, acc2, 100, sign_key1);

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
