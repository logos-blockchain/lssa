use std::fmt::Display;

use anyhow::Result;
use common::{block::HashableBlockData, merkle_tree_public::TreeHashType};
use config::SequencerConfig;
use log::warn;
use mempool::MemPool;
use sequencer_store::SequecerChainStore;
use serde::{Deserialize, Serialize};

pub mod config;
pub mod sequencer_store;

pub struct SequencerCore {
    pub store: SequecerChainStore,
    pub mempool: MemPool<nssa::PublicTransaction>,
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
    MempoolFullForRound,
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
            mempool: MemPool::default(),
            chain_height: config.genesis_id,
            sequencer_config: config,
        }
    }

    pub fn transaction_pre_check(
        &mut self,
        tx: nssa::PublicTransaction,
    ) -> Result<nssa::PublicTransaction, TransactionMalformationErrorKind> {
        // Stateless checks here
        if tx.witness_set().is_valid_for(tx.message()) {
            Ok(tx)
        } else {
            Err(TransactionMalformationErrorKind::InvalidSignature)
        }
    }

    pub fn push_tx_into_mempool_pre_check(
        &mut self,
        transaction: nssa::PublicTransaction,
    ) -> Result<(), TransactionMalformationErrorKind> {
        let mempool_size = self.mempool.len();
        if mempool_size >= self.sequencer_config.max_num_tx_in_block {
            return Err(TransactionMalformationErrorKind::MempoolFullForRound);
        }

        let authenticated_tx = self
            .transaction_pre_check(transaction)
            .inspect_err(|err| warn!("Error at pre_check {err:#?}"))?;

        self.mempool.push_item(authenticated_tx);

        Ok(())
    }

    fn execute_check_transaction_on_state(
        &mut self,
        tx: nssa::PublicTransaction,
    ) -> Result<nssa::PublicTransaction, nssa::error::NssaError> {
        self.store
            .state
            .transition_from_public_transaction(&tx)
            .inspect_err(|err| warn!("Error at transition {err:#?}"))?;

        Ok(tx)
    }

    ///Produces new block from transactions in mempool
    pub fn produce_new_block_with_mempool_transactions(&mut self) -> Result<u64> {
        let new_block_height = self.chain_height + 1;

        let transactions = self
            .mempool
            .pop_size(self.sequencer_config.max_num_tx_in_block);

        let valid_transactions: Vec<_> = transactions
            .into_iter()
            .filter_map(|tx| self.execute_check_transaction_on_state(tx).ok())
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
            prev_block_hash,
        };

        let block = hashable_data.into();

        self.store.block_store.put_block_at_id(block)?;

        self.chain_height = new_block_height;

        Ok(self.chain_height)
    }
}

#[cfg(test)]
mod tests {
    use crate::config::AccountInitialData;

    use super::*;

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
            27, 132, 197, 86, 123, 18, 100, 64, 153, 93, 62, 213, 170, 186, 5, 101, 215, 30, 24,
            52, 96, 72, 25, 255, 156, 23, 245, 233, 213, 221, 7, 143,
        ];

        let acc2_addr = vec![
            77, 75, 108, 209, 54, 16, 50, 202, 155, 210, 174, 185, 217, 0, 170, 77, 69, 217, 234,
            216, 10, 201, 66, 51, 116, 196, 81, 167, 37, 77, 7, 102,
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

    fn create_signing_key_for_account1() -> nssa::PrivateKey {
        nssa::PrivateKey::try_new([1; 32]).unwrap()
    }

    fn create_signing_key_for_account2() -> nssa::PrivateKey {
        nssa::PrivateKey::try_new([2; 32]).unwrap()
    }

    fn common_setup(sequencer: &mut SequencerCore) {
        let tx = common::test_utils::produce_dummy_empty_transaction();
        sequencer.mempool.push_item(tx);

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

        let balance_acc_1 = sequencer
            .store
            .state
            .get_account_by_address(&nssa::Address::new(acc1_addr))
            .balance;
        let balance_acc_2 = sequencer
            .store
            .state
            .get_account_by_address(&nssa::Address::new(acc2_addr))
            .balance;

        assert_eq!(10000, balance_acc_1);
        assert_eq!(20000, balance_acc_2);
    }

    #[test]
    fn test_start_different_intial_accounts_balances() {
        let acc1_addr = vec![
            27, 132, 197, 86, 123, 18, 100, 64, 153, 93, 62, 213, 170, 186, 5, 101, 215, 30, 24,
            52, 96, 72, 25, 255, 156, 23, 245, 233, 213, 221, 7, 143,
        ];

        let acc2_addr = vec![
            77, 75, 108, 209, 54, 16, 50, 202, 155, 210, 174, 185, 217, 0, 170, 77, 69, 217, 234,
            216, 10, 201, 66, 51, 116, 196, 81, 167, 37, 77, 7, 102,
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

        assert_eq!(
            10000,
            sequencer
                .store
                .state
                .get_account_by_address(&nssa::Address::new(acc1_addr))
                .balance
        );
        assert_eq!(
            20000,
            sequencer
                .store
                .state
                .get_account_by_address(&nssa::Address::new(acc2_addr))
                .balance
        );
    }

    #[test]
    fn test_transaction_pre_check_pass() {
        let config = setup_sequencer_config();
        let mut sequencer = SequencerCore::start_from_config(config);

        common_setup(&mut sequencer);

        let tx = common::test_utils::produce_dummy_empty_transaction();
        let result = sequencer.transaction_pre_check(tx);

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

        let tx = common::test_utils::create_transaction_native_token_transfer(
            acc1, 0, acc2, 10, sign_key1,
        );
        let result = sequencer.transaction_pre_check(tx);

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

        let tx = common::test_utils::create_transaction_native_token_transfer(
            acc1, 0, acc2, 10, sign_key2,
        );

        // Signature is valid, stateless check pass
        let tx = sequencer.transaction_pre_check(tx).unwrap();

        // Signature is not from sender. Execution fails
        let result = sequencer.execute_check_transaction_on_state(tx);

        assert!(matches!(
            result,
            Err(nssa::error::NssaError::ProgramExecutionFailed(_))
        ));
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

        let tx = common::test_utils::create_transaction_native_token_transfer(
            acc1, 0, acc2, 10000000, sign_key1,
        );

        let result = sequencer.transaction_pre_check(tx);

        //Passed pre-check
        assert!(result.is_ok());

        let result = sequencer.execute_check_transaction_on_state(result.unwrap());
        let is_failed_at_balance_mismatch = matches!(
            result.err().unwrap(),
            nssa::error::NssaError::ProgramExecutionFailed(_)
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

        let tx = common::test_utils::create_transaction_native_token_transfer(
            acc1, 0, acc2, 100, sign_key1,
        );

        sequencer.execute_check_transaction_on_state(tx).unwrap();

        let bal_from = sequencer
            .store
            .state
            .get_account_by_address(&nssa::Address::new(acc1))
            .balance;
        let bal_to = sequencer
            .store
            .state
            .get_account_by_address(&nssa::Address::new(acc2))
            .balance;

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

        let tx = common::test_utils::produce_dummy_empty_transaction();

        // Fill the mempool
        sequencer.mempool.push_item(tx.clone());

        let result = sequencer.push_tx_into_mempool_pre_check(tx);

        assert!(matches!(
            result,
            Err(TransactionMalformationErrorKind::MempoolFullForRound)
        ));
    }

    #[test]
    fn test_push_tx_into_mempool_pre_check() {
        let config = setup_sequencer_config();
        let mut sequencer = SequencerCore::start_from_config(config);

        common_setup(&mut sequencer);

        let tx = common::test_utils::produce_dummy_empty_transaction();

        let result = sequencer.push_tx_into_mempool_pre_check(tx);
        assert!(result.is_ok());
        assert_eq!(sequencer.mempool.len(), 1);
    }

    #[test]
    fn test_produce_new_block_with_mempool_transactions() {
        let config = setup_sequencer_config();
        let mut sequencer = SequencerCore::start_from_config(config);
        let genesis_height = sequencer.chain_height;

        let tx = common::test_utils::produce_dummy_empty_transaction();
        sequencer.mempool.push_item(tx);

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

        let tx = common::test_utils::create_transaction_native_token_transfer(
            acc1, 0, acc2, 100, sign_key1,
        );

        let tx_original = tx.clone();
        let tx_replay = tx.clone();
        // Pushing two copies of the same tx to the mempool
        sequencer.mempool.push_item(tx_original);
        sequencer.mempool.push_item(tx_replay);

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

        let tx = common::test_utils::create_transaction_native_token_transfer(
            acc1, 0, acc2, 100, sign_key1,
        );

        // The transaction should be included the first time
        sequencer.mempool.push_item(tx.clone());
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
        sequencer.mempool.push_item(tx);
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
