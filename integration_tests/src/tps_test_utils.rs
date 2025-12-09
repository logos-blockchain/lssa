use std::time::Duration;

use key_protocol::key_management::ephemeral_key_holder::EphemeralKeyHolder;
use nssa::{
    Account, AccountId, PrivacyPreservingTransaction, PrivateKey, PublicKey, PublicTransaction,
    privacy_preserving_transaction::{self as pptx, circuit},
    program::Program,
    public_transaction as putx,
};
use nssa_core::{
    MembershipProof, NullifierPublicKey,
    account::{AccountWithMetadata, data::Data},
    encryption::IncomingViewingPublicKey,
};
use sequencer_core::config::{AccountInitialData, CommitmentsInitialData, SequencerConfig};

pub(crate) struct TpsTestManager {
    public_keypairs: Vec<(PrivateKey, AccountId)>,
    target_tps: u64,
}

impl TpsTestManager {
    /// Generates public account keypairs. These are used to populate the config and to generate
    /// valid public transactions for the tps test.
    pub(crate) fn new(target_tps: u64, number_transactions: usize) -> Self {
        let public_keypairs = (1..(number_transactions + 2))
            .map(|i| {
                let mut private_key_bytes = [0u8; 32];
                private_key_bytes[..8].copy_from_slice(&i.to_le_bytes());
                let private_key = PrivateKey::try_new(private_key_bytes).unwrap();
                let public_key = PublicKey::new_from_private_key(&private_key);
                let account_id = AccountId::from(&public_key);
                (private_key, account_id)
            })
            .collect::<Vec<_>>();
        Self {
            public_keypairs,
            target_tps,
        }
    }

    pub(crate) fn target_time(&self) -> Duration {
        let number_transactions = (self.public_keypairs.len() - 1) as u64;
        Duration::from_secs_f64(number_transactions as f64 / self.target_tps as f64)
    }

    /// Build a batch of public transactions to submit to the node.
    pub fn build_public_txs(&self) -> Vec<PublicTransaction> {
        // Create valid public transactions
        let program = Program::authenticated_transfer_program();
        let public_txs: Vec<PublicTransaction> = self
            .public_keypairs
            .windows(2)
            .map(|pair| {
                let amount: u128 = 1;
                let message = putx::Message::try_new(
                    program.id(),
                    [pair[0].1, pair[1].1].to_vec(),
                    [0u128].to_vec(),
                    amount,
                )
                .unwrap();
                let witness_set =
                    nssa::public_transaction::WitnessSet::for_message(&message, &[&pair[0].0]);
                PublicTransaction::new(message, witness_set)
            })
            .collect();

        public_txs
    }

    /// Generates a sequencer configuration with initial balance in a number of public accounts.
    /// The transactions generated with the function `build_public_txs` will be valid in a node
    /// started with the config from this method.
    pub(crate) fn generate_tps_test_config(&self) -> SequencerConfig {
        // Create public public keypairs
        let initial_public_accounts = self
            .public_keypairs
            .iter()
            .map(|(_, account_id)| AccountInitialData {
                account_id: account_id.to_string(),
                balance: 10,
            })
            .collect();

        // Generate an initial commitment to be used with the privacy preserving transaction
        // created with the `build_privacy_transaction` function.
        let sender_nsk = [1; 32];
        let sender_npk = NullifierPublicKey::from(&sender_nsk);
        let account = Account {
            balance: 100,
            nonce: 0xdeadbeef,
            program_owner: Program::authenticated_transfer_program().id(),
            data: Data::default(),
        };
        let initial_commitment = CommitmentsInitialData {
            npk: sender_npk,
            account,
        };

        SequencerConfig {
            home: ".".into(),
            override_rust_log: None,
            genesis_id: 1,
            is_genesis_random: true,
            max_num_tx_in_block: 300,
            mempool_max_size: 10000,
            block_create_timeout_millis: 12000,
            port: 3040,
            initial_accounts: initial_public_accounts,
            initial_commitments: vec![initial_commitment],
            signing_key: [37; 32],
        }
    }
}

/// Builds a single privacy transaction to use in stress tests. This involves generating a proof so
/// it may take a while to run. In normal execution of the node this transaction will be accepted
/// only once. Disabling the node's nullifier uniqueness check allows to submit this transaction
/// multiple times with the purpose of testing the node's processing performance.
#[allow(unused)]
fn build_privacy_transaction() -> PrivacyPreservingTransaction {
    let program = Program::authenticated_transfer_program();
    let sender_nsk = [1; 32];
    let sender_isk = [99; 32];
    let sender_ipk = IncomingViewingPublicKey::from_scalar(sender_isk);
    let sender_npk = NullifierPublicKey::from(&sender_nsk);
    let sender_pre = AccountWithMetadata::new(
        Account {
            balance: 100,
            nonce: 0xdeadbeef,
            program_owner: program.id(),
            data: Data::default(),
        },
        true,
        AccountId::from(&sender_npk),
    );
    let recipient_nsk = [2; 32];
    let recipient_isk = [99; 32];
    let recipient_ipk = IncomingViewingPublicKey::from_scalar(recipient_isk);
    let recipient_npk = NullifierPublicKey::from(&recipient_nsk);
    let recipient_pre =
        AccountWithMetadata::new(Account::default(), false, AccountId::from(&recipient_npk));

    let eph_holder_from = EphemeralKeyHolder::new(&sender_npk);
    let sender_ss = eph_holder_from.calculate_shared_secret_sender(&sender_ipk);
    let sender_epk = eph_holder_from.generate_ephemeral_public_key();

    let eph_holder_to = EphemeralKeyHolder::new(&recipient_npk);
    let recipient_ss = eph_holder_to.calculate_shared_secret_sender(&recipient_ipk);
    let recipient_epk = eph_holder_from.generate_ephemeral_public_key();

    let balance_to_move: u128 = 1;
    let proof: MembershipProof = (
        1,
        vec![[
            170, 10, 217, 228, 20, 35, 189, 177, 238, 235, 97, 129, 132, 89, 96, 247, 86, 91, 222,
            214, 38, 194, 216, 67, 56, 251, 208, 226, 0, 117, 149, 39,
        ]],
    );
    let (output, proof) = circuit::execute_and_prove(
        &[sender_pre, recipient_pre],
        &Program::serialize_instruction(balance_to_move).unwrap(),
        &[1, 2],
        &[0xdeadbeef1, 0xdeadbeef2],
        &[
            (sender_npk.clone(), sender_ss),
            (recipient_npk.clone(), recipient_ss),
        ],
        &[(sender_nsk, proof)],
        &program,
    )
    .unwrap();
    let message = pptx::message::Message::try_from_circuit_output(
        vec![],
        vec![],
        vec![
            (sender_npk, sender_ipk, sender_epk),
            (recipient_npk, recipient_ipk, recipient_epk),
        ],
        output,
    )
    .unwrap();
    let witness_set = pptx::witness_set::WitnessSet::for_message(&message, proof, &[]);
    pptx::PrivacyPreservingTransaction::new(message, witness_set)
}
