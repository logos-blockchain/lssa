use std::{fmt::Display, time::Instant};

use anyhow::Result;
#[cfg(feature = "testnet")]
use common::PINATA_BASE58;
use common::{
    HashType,
    block::HashableBlockData,
    transaction::{EncodedTransaction, NSSATransaction},
};
use config::SequencerConfig;
use log::warn;
use mempool::{MemPool, MemPoolHandle};
use serde::{Deserialize, Serialize};

use crate::{block_settlement_client::BlockSettlementClient, block_store::SequencerBlockStore};

mod block_settlement_client;
pub mod block_store;
pub mod config;

pub struct SequencerCore {
    state: nssa::V02State,
    block_store: SequencerBlockStore,
    mempool: MemPool<EncodedTransaction>,
    sequencer_config: SequencerConfig,
    chain_height: u64,
    block_settlement_client: Option<BlockSettlementClient>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TransactionMalformationError {
    InvalidSignature,
    FailedToDecode { tx: HashType },
}

impl Display for TransactionMalformationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:#?}")
    }
}

impl std::error::Error for TransactionMalformationError {}

impl SequencerCore {
    /// Start Sequencer from configuration and construct transaction sender
    pub fn start_from_config(config: SequencerConfig) -> (Self, MemPoolHandle<EncodedTransaction>) {
        let hashable_data = HashableBlockData {
            block_id: config.genesis_id,
            transactions: vec![],
            prev_block_hash: [0; 32],
            timestamp: 0,
        };

        let signing_key = nssa::PrivateKey::try_new(config.signing_key).unwrap();
        let genesis_block = hashable_data.into_pending_block(&signing_key);

        // Sequencer should panic if unable to open db,
        // as fixing this issue may require actions non-native to program scope
        let block_store = SequencerBlockStore::open_db_with_genesis(
            &config.home.join("rocksdb"),
            Some(genesis_block),
            signing_key,
        )
        .unwrap();
        let mut initial_commitments = vec![];

        for init_comm_data in config.initial_commitments.clone() {
            let npk = init_comm_data.npk;

            let mut acc = init_comm_data.account;

            acc.program_owner = nssa::program::Program::authenticated_transfer_program().id();

            let comm = nssa_core::Commitment::new(&npk, &acc);

            initial_commitments.push(comm);
        }

        let init_accs: Vec<(nssa::AccountId, u128)> = config
            .initial_accounts
            .iter()
            .map(|acc_data| (acc_data.account_id.parse().unwrap(), acc_data.balance))
            .collect();

        let mut state = nssa::V02State::new_with_genesis_accounts(&init_accs, &initial_commitments);

        #[cfg(feature = "testnet")]
        state.add_pinata_program(PINATA_BASE58.parse().unwrap());

        let (mempool, mempool_handle) = MemPool::new(config.mempool_max_size);
        let block_settlement_client = config.bedrock_config.as_ref().map(|bedrock_config| {
            BlockSettlementClient::try_new(&config.home, bedrock_config)
                .expect("Block settlement client should be constructible")
        });

        let mut this = Self {
            state,
            block_store,
            mempool,
            chain_height: config.genesis_id,
            sequencer_config: config,
            block_settlement_client,
        };

        this.sync_state_with_stored_blocks();

        (this, mempool_handle)
    }

    /// If there are stored blocks ahead of the current height, this method will load and process
    /// all transaction in them in the order they are stored. The NSSA state will be updated
    /// accordingly.
    fn sync_state_with_stored_blocks(&mut self) {
        let mut next_block_id = self.sequencer_config.genesis_id + 1;
        while let Ok(block) = self.block_store.get_block_at_id(next_block_id) {
            for encoded_transaction in block.body.transactions {
                let transaction = NSSATransaction::try_from(&encoded_transaction).unwrap();
                // Process transaction and update state
                self.execute_check_transaction_on_state(transaction)
                    .unwrap();
                // Update the tx hash to block id map.
                self.block_store.insert(&encoded_transaction, next_block_id);
            }
            self.chain_height = next_block_id;
            next_block_id += 1;
        }
    }

    fn execute_check_transaction_on_state(
        &mut self,
        tx: NSSATransaction,
    ) -> Result<NSSATransaction, nssa::error::NssaError> {
        match &tx {
            NSSATransaction::Public(tx) => self.state.transition_from_public_transaction(tx),
            NSSATransaction::PrivacyPreserving(tx) => self
                .state
                .transition_from_privacy_preserving_transaction(tx),
            NSSATransaction::ProgramDeployment(tx) => self
                .state
                .transition_from_program_deployment_transaction(tx),
        }
        .inspect_err(|err| warn!("Error at transition {err:#?}"))?;

        Ok(tx)
    }

    pub async fn produce_new_block_and_post_to_settlement_layer(&mut self) -> Result<u64> {
        let block_data = self.produce_new_block_with_mempool_transactions()?;

        if let Some(block_settlement) = self.block_settlement_client.as_mut() {
            block_settlement.post_and_wait(&block_data).await?;
            log::info!("Posted block data to Bedrock");
        }

        Ok(self.chain_height)
    }

    /// Produces new block from transactions in mempool
    pub fn produce_new_block_with_mempool_transactions(&mut self) -> Result<HashableBlockData> {
        let now = Instant::now();

        let new_block_height = self.chain_height + 1;

        let mut valid_transactions = vec![];

        while let Some(tx) = self.mempool.pop() {
            let nssa_transaction = NSSATransaction::try_from(&tx)
                .map_err(|_| TransactionMalformationError::FailedToDecode { tx: tx.hash() })?;

            if let Ok(valid_tx) = self.execute_check_transaction_on_state(nssa_transaction) {
                valid_transactions.push(valid_tx.into());

                if valid_transactions.len() >= self.sequencer_config.max_num_tx_in_block {
                    break;
                }
            } else {
                // Probably need to handle unsuccessful transaction execution?
            }
        }

        let prev_block_hash = self
            .block_store
            .get_block_at_id(self.chain_height)?
            .header
            .hash;

        let curr_time = chrono::Utc::now().timestamp_millis() as u64;

        let hashable_data = HashableBlockData {
            block_id: new_block_height,
            transactions: valid_transactions,
            prev_block_hash,
            timestamp: curr_time,
        };

        let block = hashable_data
            .clone()
            .into_pending_block(self.block_store.signing_key());

        self.block_store.put_block_at_id(block)?;

        self.chain_height = new_block_height;

        // TODO: Consider switching to `tracing` crate to have more structured and consistent logs
        // e.g.
        //
        // ```
        // info!(
        //     num_txs = num_txs_in_block,
        //     time = now.elapsed(),
        //     "Created block"
        // );
        // ```
        log::info!(
            "Created block with {} transactions in {} seconds",
            hashable_data.transactions.len(),
            now.elapsed().as_secs()
        );
        Ok(hashable_data)
    }

    pub fn state(&self) -> &nssa::V02State {
        &self.state
    }

    pub fn block_store(&self) -> &SequencerBlockStore {
        &self.block_store
    }

    pub fn chain_height(&self) -> u64 {
        self.chain_height
    }

    pub fn sequencer_config(&self) -> &SequencerConfig {
        &self.sequencer_config
    }
}

// TODO: Introduce type-safe wrapper around checked transaction, e.g. AuthenticatedTransaction
pub fn transaction_pre_check(
    tx: NSSATransaction,
) -> Result<NSSATransaction, TransactionMalformationError> {
    // Stateless checks here
    match tx {
        NSSATransaction::Public(tx) => {
            if tx.witness_set().is_valid_for(tx.message()) {
                Ok(NSSATransaction::Public(tx))
            } else {
                Err(TransactionMalformationError::InvalidSignature)
            }
        }
        NSSATransaction::PrivacyPreserving(tx) => {
            if tx.witness_set().signatures_are_valid_for(tx.message()) {
                Ok(NSSATransaction::PrivacyPreserving(tx))
            } else {
                Err(TransactionMalformationError::InvalidSignature)
            }
        }
        NSSATransaction::ProgramDeployment(tx) => Ok(NSSATransaction::ProgramDeployment(tx)),
    }
}

#[cfg(test)]
mod tests {
    use std::pin::pin;

    use base58::{FromBase58, ToBase58};
    use common::test_utils::sequencer_sign_key_for_testing;
    use nssa::PrivateKey;

    use super::*;
    use crate::config::AccountInitialData;

    fn parse_unwrap_tx_body_into_nssa_tx(tx_body: EncodedTransaction) -> NSSATransaction {
        NSSATransaction::try_from(&tx_body)
            .map_err(|_| TransactionMalformationError::FailedToDecode { tx: tx_body.hash() })
            .unwrap()
    }

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
            mempool_max_size: 10000,
            block_create_timeout_millis: 1000,
            port: 8080,
            initial_accounts,
            initial_commitments: vec![],
            signing_key: *sequencer_sign_key_for_testing().value(),
            bedrock_config: None,
        }
    }

    fn setup_sequencer_config() -> SequencerConfig {
        let acc1_account_id: Vec<u8> = vec![
            148, 179, 206, 253, 199, 51, 82, 86, 232, 2, 152, 122, 80, 243, 54, 207, 237, 112, 83,
            153, 44, 59, 204, 49, 128, 84, 160, 227, 216, 149, 97, 102,
        ];

        let acc2_account_id: Vec<u8> = vec![
            30, 145, 107, 3, 207, 73, 192, 230, 160, 63, 238, 207, 18, 69, 54, 216, 103, 244, 92,
            94, 124, 248, 42, 16, 141, 19, 119, 18, 14, 226, 140, 204,
        ];

        let initial_acc1 = AccountInitialData {
            account_id: acc1_account_id.to_base58(),
            balance: 10000,
        };

        let initial_acc2 = AccountInitialData {
            account_id: acc2_account_id.to_base58(),
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

    async fn common_setup() -> (SequencerCore, MemPoolHandle<EncodedTransaction>) {
        let config = setup_sequencer_config();
        common_setup_with_config(config).await
    }

    async fn common_setup_with_config(
        config: SequencerConfig,
    ) -> (SequencerCore, MemPoolHandle<EncodedTransaction>) {
        let (mut sequencer, mempool_handle) = SequencerCore::start_from_config(config);

        let tx = common::test_utils::produce_dummy_empty_transaction();
        mempool_handle.push(tx).await.unwrap();

        sequencer
            .produce_new_block_with_mempool_transactions()
            .unwrap();

        (sequencer, mempool_handle)
    }

    #[test]
    fn test_start_from_config() {
        let config = setup_sequencer_config();
        let (sequencer, _mempool_handle) = SequencerCore::start_from_config(config.clone());

        assert_eq!(sequencer.chain_height, config.genesis_id);
        assert_eq!(sequencer.sequencer_config.max_num_tx_in_block, 10);
        assert_eq!(sequencer.sequencer_config.port, 8080);

        let acc1_account_id = config.initial_accounts[0]
            .account_id
            .clone()
            .from_base58()
            .unwrap()
            .try_into()
            .unwrap();
        let acc2_account_id = config.initial_accounts[1]
            .account_id
            .clone()
            .from_base58()
            .unwrap()
            .try_into()
            .unwrap();

        let balance_acc_1 = sequencer
            .state
            .get_account_by_id(&nssa::AccountId::new(acc1_account_id))
            .balance;
        let balance_acc_2 = sequencer
            .state
            .get_account_by_id(&nssa::AccountId::new(acc2_account_id))
            .balance;

        assert_eq!(10000, balance_acc_1);
        assert_eq!(20000, balance_acc_2);
    }

    #[test]
    fn test_start_different_intial_accounts_balances() {
        let acc1_account_id: Vec<u8> = vec![
            27, 132, 197, 86, 123, 18, 100, 64, 153, 93, 62, 213, 170, 186, 5, 101, 215, 30, 24,
            52, 96, 72, 25, 255, 156, 23, 245, 233, 213, 221, 7, 143,
        ];

        let acc2_account_id: Vec<u8> = vec![
            77, 75, 108, 209, 54, 16, 50, 202, 155, 210, 174, 185, 217, 0, 170, 77, 69, 217, 234,
            216, 10, 201, 66, 51, 116, 196, 81, 167, 37, 77, 7, 102,
        ];

        let initial_acc1 = AccountInitialData {
            account_id: acc1_account_id.to_base58(),
            balance: 10000,
        };

        let initial_acc2 = AccountInitialData {
            account_id: acc2_account_id.to_base58(),
            balance: 20000,
        };

        let initial_accounts = vec![initial_acc1, initial_acc2];

        let config = setup_sequencer_config_variable_initial_accounts(initial_accounts);
        let (sequencer, _mempool_handle) = SequencerCore::start_from_config(config.clone());

        let acc1_account_id = config.initial_accounts[0]
            .account_id
            .clone()
            .from_base58()
            .unwrap()
            .try_into()
            .unwrap();
        let acc2_account_id = config.initial_accounts[1]
            .account_id
            .clone()
            .from_base58()
            .unwrap()
            .try_into()
            .unwrap();

        assert_eq!(
            10000,
            sequencer
                .state
                .get_account_by_id(&nssa::AccountId::new(acc1_account_id))
                .balance
        );
        assert_eq!(
            20000,
            sequencer
                .state
                .get_account_by_id(&nssa::AccountId::new(acc2_account_id))
                .balance
        );
    }

    #[test]
    fn test_transaction_pre_check_pass() {
        let tx = common::test_utils::produce_dummy_empty_transaction();
        let result = transaction_pre_check(parse_unwrap_tx_body_into_nssa_tx(tx));

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_transaction_pre_check_native_transfer_valid() {
        let (sequencer, _mempool_handle) = common_setup().await;

        let acc1 = sequencer.sequencer_config.initial_accounts[0]
            .account_id
            .clone()
            .from_base58()
            .unwrap()
            .try_into()
            .unwrap();
        let acc2 = sequencer.sequencer_config.initial_accounts[1]
            .account_id
            .clone()
            .from_base58()
            .unwrap()
            .try_into()
            .unwrap();

        let sign_key1 = create_signing_key_for_account1();

        let tx = common::test_utils::create_transaction_native_token_transfer(
            acc1, 0, acc2, 10, sign_key1,
        );
        let result = transaction_pre_check(parse_unwrap_tx_body_into_nssa_tx(tx));

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_transaction_pre_check_native_transfer_other_signature() {
        let (mut sequencer, _mempool_handle) = common_setup().await;

        let acc1 = sequencer.sequencer_config.initial_accounts[0]
            .account_id
            .clone()
            .from_base58()
            .unwrap()
            .try_into()
            .unwrap();
        let acc2 = sequencer.sequencer_config.initial_accounts[1]
            .account_id
            .clone()
            .from_base58()
            .unwrap()
            .try_into()
            .unwrap();

        let sign_key2 = create_signing_key_for_account2();

        let tx = common::test_utils::create_transaction_native_token_transfer(
            acc1, 0, acc2, 10, sign_key2,
        );

        // Signature is valid, stateless check pass
        let tx = transaction_pre_check(parse_unwrap_tx_body_into_nssa_tx(tx)).unwrap();

        // Signature is not from sender. Execution fails
        let result = sequencer.execute_check_transaction_on_state(tx);

        assert!(matches!(
            result,
            Err(nssa::error::NssaError::ProgramExecutionFailed(_))
        ));
    }

    #[tokio::test]
    async fn test_transaction_pre_check_native_transfer_sent_too_much() {
        let (mut sequencer, _mempool_handle) = common_setup().await;

        let acc1 = sequencer.sequencer_config.initial_accounts[0]
            .account_id
            .clone()
            .from_base58()
            .unwrap()
            .try_into()
            .unwrap();
        let acc2 = sequencer.sequencer_config.initial_accounts[1]
            .account_id
            .clone()
            .from_base58()
            .unwrap()
            .try_into()
            .unwrap();

        let sign_key1 = create_signing_key_for_account1();

        let tx = common::test_utils::create_transaction_native_token_transfer(
            acc1, 0, acc2, 10000000, sign_key1,
        );

        let result = transaction_pre_check(parse_unwrap_tx_body_into_nssa_tx(tx));

        // Passed pre-check
        assert!(result.is_ok());

        let result = sequencer.execute_check_transaction_on_state(result.unwrap());
        let is_failed_at_balance_mismatch = matches!(
            result.err().unwrap(),
            nssa::error::NssaError::ProgramExecutionFailed(_)
        );

        assert!(is_failed_at_balance_mismatch);
    }

    #[tokio::test]
    async fn test_transaction_execute_native_transfer() {
        let (mut sequencer, _mempool_handle) = common_setup().await;

        let acc1 = sequencer.sequencer_config.initial_accounts[0]
            .account_id
            .clone()
            .from_base58()
            .unwrap()
            .try_into()
            .unwrap();
        let acc2 = sequencer.sequencer_config.initial_accounts[1]
            .account_id
            .clone()
            .from_base58()
            .unwrap()
            .try_into()
            .unwrap();

        let sign_key1 = create_signing_key_for_account1();

        let tx = common::test_utils::create_transaction_native_token_transfer(
            acc1, 0, acc2, 100, sign_key1,
        );

        sequencer
            .execute_check_transaction_on_state(parse_unwrap_tx_body_into_nssa_tx(tx))
            .unwrap();

        let bal_from = sequencer
            .state
            .get_account_by_id(&nssa::AccountId::new(acc1))
            .balance;
        let bal_to = sequencer
            .state
            .get_account_by_id(&nssa::AccountId::new(acc2))
            .balance;

        assert_eq!(bal_from, 9900);
        assert_eq!(bal_to, 20100);
    }

    #[tokio::test]
    async fn test_push_tx_into_mempool_blocks_until_mempool_is_full() {
        let config = SequencerConfig {
            mempool_max_size: 1,
            ..setup_sequencer_config()
        };
        let (mut sequencer, mempool_handle) = common_setup_with_config(config).await;

        let tx = common::test_utils::produce_dummy_empty_transaction();

        // Fill the mempool
        mempool_handle.push(tx.clone()).await.unwrap();

        // Check that pushing another transaction will block
        let mut push_fut = pin!(mempool_handle.push(tx.clone()));
        let poll = futures::poll!(push_fut.as_mut());
        assert!(poll.is_pending());

        // Empty the mempool by producing a block
        sequencer
            .produce_new_block_with_mempool_transactions()
            .unwrap();

        // Resolve the pending push
        assert!(push_fut.await.is_ok());
    }

    #[tokio::test]
    async fn test_produce_new_block_with_mempool_transactions() {
        let (mut sequencer, mempool_handle) = common_setup().await;
        let genesis_height = sequencer.chain_height;

        let tx = common::test_utils::produce_dummy_empty_transaction();
        mempool_handle.push(tx).await.unwrap();

        let block = sequencer.produce_new_block_with_mempool_transactions();
        assert!(block.is_ok());
        assert_eq!(block.unwrap().block_id, genesis_height + 1);
    }

    #[tokio::test]
    async fn test_replay_transactions_are_rejected_in_the_same_block() {
        let (mut sequencer, mempool_handle) = common_setup().await;

        let acc1 = sequencer.sequencer_config.initial_accounts[0]
            .account_id
            .clone()
            .from_base58()
            .unwrap()
            .try_into()
            .unwrap();
        let acc2 = sequencer.sequencer_config.initial_accounts[1]
            .account_id
            .clone()
            .from_base58()
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
        mempool_handle.push(tx_original).await.unwrap();
        mempool_handle.push(tx_replay).await.unwrap();

        // Create block
        let current_height = sequencer
            .produce_new_block_with_mempool_transactions()
            .unwrap()
            .block_id;
        let block = sequencer
            .block_store
            .get_block_at_id(current_height)
            .unwrap();

        // Only one should be included in the block
        assert_eq!(block.body.transactions, vec![tx.clone()]);
    }

    #[tokio::test]
    async fn test_replay_transactions_are_rejected_in_different_blocks() {
        let (mut sequencer, mempool_handle) = common_setup().await;

        let acc1 = sequencer.sequencer_config.initial_accounts[0]
            .account_id
            .clone()
            .from_base58()
            .unwrap()
            .try_into()
            .unwrap();
        let acc2 = sequencer.sequencer_config.initial_accounts[1]
            .account_id
            .clone()
            .from_base58()
            .unwrap()
            .try_into()
            .unwrap();

        let sign_key1 = create_signing_key_for_account1();

        let tx = common::test_utils::create_transaction_native_token_transfer(
            acc1, 0, acc2, 100, sign_key1,
        );

        // The transaction should be included the first time
        mempool_handle.push(tx.clone()).await.unwrap();
        let current_height = sequencer
            .produce_new_block_with_mempool_transactions()
            .unwrap()
            .block_id;
        let block = sequencer
            .block_store
            .get_block_at_id(current_height)
            .unwrap();
        assert_eq!(block.body.transactions, vec![tx.clone()]);

        // Add same transaction should fail
        mempool_handle.push(tx.clone()).await.unwrap();
        let current_height = sequencer
            .produce_new_block_with_mempool_transactions()
            .unwrap()
            .block_id;
        let block = sequencer
            .block_store
            .get_block_at_id(current_height)
            .unwrap();
        assert!(block.body.transactions.is_empty());
    }

    #[tokio::test]
    async fn test_restart_from_storage() {
        let config = setup_sequencer_config();
        let acc1_account_id: nssa::AccountId =
            config.initial_accounts[0].account_id.parse().unwrap();
        let acc2_account_id: nssa::AccountId =
            config.initial_accounts[1].account_id.parse().unwrap();
        let balance_to_move = 13;

        // In the following code block a transaction will be processed that moves `balance_to_move`
        // from `acc_1` to `acc_2`. The block created with that transaction will be kept stored in
        // the temporary directory for the block storage of this test.
        {
            let (mut sequencer, mempool_handle) = SequencerCore::start_from_config(config.clone());
            let signing_key = PrivateKey::try_new([1; 32]).unwrap();

            let tx = common::test_utils::create_transaction_native_token_transfer(
                *acc1_account_id.value(),
                0,
                *acc2_account_id.value(),
                balance_to_move,
                signing_key,
            );

            mempool_handle.push(tx.clone()).await.unwrap();
            let current_height = sequencer
                .produce_new_block_with_mempool_transactions()
                .unwrap()
                .block_id;
            let block = sequencer
                .block_store
                .get_block_at_id(current_height)
                .unwrap();
            assert_eq!(block.body.transactions, vec![tx.clone()]);
        }

        // Instantiating a new sequencer from the same config. This should load the existing block
        // with the above transaction and update the state to reflect that.
        let (sequencer, _mempool_handle) = SequencerCore::start_from_config(config.clone());
        let balance_acc_1 = sequencer.state.get_account_by_id(&acc1_account_id).balance;
        let balance_acc_2 = sequencer.state.get_account_by_id(&acc2_account_id).balance;

        // Balances should be consistent with the stored block
        assert_eq!(
            balance_acc_1,
            config.initial_accounts[0].balance - balance_to_move
        );
        assert_eq!(
            balance_acc_2,
            config.initial_accounts[1].balance + balance_to_move
        );
    }
}
