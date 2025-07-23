use accounts::{account_core::Account, key_management::ephemeral_key_holder::EphemeralKeyHolder};
use anyhow::Result;
use common::transaction::{TransactionBody, TxKind};
use rand::thread_rng;
use risc0_zkvm::Receipt;
use secp256k1_zkp::{CommitmentSecrets, PedersenCommitment, Tweak};
use utxo::utxo_core::UTXO;

use crate::proofs_circuits::{commit, generate_nullifiers, tag_random};

pub fn create_public_transaction_payload(
    execution_input: Vec<u8>,
    commitment: Vec<PedersenCommitment>,
    tweak: Tweak,
    secret_r: [u8; 32],
    sc_addr: String,
    state_changes: (serde_json::Value, usize),
) -> TransactionBody {
    TransactionBody {
        tx_kind: TxKind::Public,
        execution_input,
        execution_output: vec![],
        utxo_commitments_spent_hashes: vec![],
        utxo_commitments_created_hashes: vec![],
        nullifier_created_hashes: vec![],
        execution_proof_private: "".to_string(),
        encoded_data: vec![],
        ephemeral_pub_key: vec![],
        commitment,
        tweak,
        secret_r,
        sc_addr,
        state_changes,
    }
}

pub fn encode_utxos_to_receivers(
    utxos_receivers: Vec<(UTXO, &Account)>,
) -> Vec<(Vec<u8>, Vec<u8>)> {
    let mut all_encoded_data = vec![];

    for (utxo, receiver) in utxos_receivers {
        let ephm_key_holder = EphemeralKeyHolder::new_os_random();

        let encoded_data = Account::encrypt_data(
            &ephm_key_holder,
            receiver.key_holder.viewing_public_key,
            &serde_json::to_vec(&utxo).unwrap(),
        );

        let encoded_data_vec = (encoded_data.0, encoded_data.1.to_vec());

        all_encoded_data.push(encoded_data_vec);
    }

    all_encoded_data
}

pub fn generate_nullifiers_spent_utxos(utxos_spent: Vec<(UTXO, &Account)>) -> Vec<Vec<u8>> {
    let mut all_nullifiers = vec![];

    for (utxo, spender) in utxos_spent {
        let nullifier = generate_nullifiers(
            &utxo,
            &spender
                .key_holder
                .utxo_secret_key_holder
                .nullifier_secret_key
                .to_bytes(),
        );

        all_nullifiers.push(nullifier);
    }

    all_nullifiers
}

pub fn encode_receipt(receipt: Receipt) -> Result<String> {
    Ok(hex::encode(serde_json::to_vec(&receipt)?))
}

pub fn generate_secret_random_commitment(
    value: u64,
    account: &Account,
) -> Result<PedersenCommitment> {
    let commitment_secrets = CommitmentSecrets {
        value,
        value_blinding_factor: Tweak::from_slice(
            &account
                .key_holder
                .utxo_secret_key_holder
                .viewing_secret_key
                .to_bytes(),
        )?,
        generator_blinding_factor: Tweak::new(&mut thread_rng()),
    };

    let tag = tag_random();
    let commitment = commit(&commitment_secrets, tag);

    Ok(commitment)
}
