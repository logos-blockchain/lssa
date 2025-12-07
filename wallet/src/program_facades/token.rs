use common::{error::ExecutionFailureKind, rpc_primitives::requests::SendTxResponse};
use nssa::{AccountId, program::Program};
use nssa_core::{
    NullifierPublicKey, SharedSecretKey, encryption::IncomingViewingPublicKey,
    program::InstructionData,
};

use crate::{PrivacyPreservingAccount, WalletCore};

pub struct Token<'w>(pub &'w WalletCore);

impl Token<'_> {
    pub async fn send_new_definition(
        &self,
        definition_account_id: AccountId,
        supply_account_id: AccountId,
        name: [u8; 6],
        total_supply: u128,
    ) -> Result<SendTxResponse, ExecutionFailureKind> {
        let account_ids = vec![definition_account_id, supply_account_id];
        let program_id = nssa::program::Program::token().id();
        // Instruction must be: [0x00 || total_supply (little-endian 16 bytes) || name (6 bytes)]
        let mut instruction = [0; 23];
        instruction[1..17].copy_from_slice(&total_supply.to_le_bytes());
        instruction[17..].copy_from_slice(&name);
        let message = nssa::public_transaction::Message::try_new(
            program_id,
            account_ids,
            vec![],
            instruction,
        )
        .unwrap();

        let witness_set = nssa::public_transaction::WitnessSet::for_message(&message, &[]);

        let tx = nssa::PublicTransaction::new(message, witness_set);

        Ok(self.0.sequencer_client.send_tx_public(tx).await?)
    }

    pub async fn send_new_definition_private_owned(
        &self,
        definition_account_id: AccountId,
        supply_account_id: AccountId,
        name: [u8; 6],
        total_supply: u128,
    ) -> Result<(SendTxResponse, SharedSecretKey), ExecutionFailureKind> {
        let (instruction_data, program) = token_program_preparation_definition(name, total_supply);

        self.0
            .send_privacy_preserving_tx(
                vec![
                    PrivacyPreservingAccount::Public(definition_account_id),
                    PrivacyPreservingAccount::PrivateOwned(supply_account_id),
                ],
                &instruction_data,
                &program,
            )
            .await
            .map(|(resp, secrets)| {
                let first = secrets
                    .into_iter()
                    .next()
                    .expect("expected recipient's secret");
                (resp, first)
            })
    }

    pub async fn send_transfer_transaction(
        &self,
        sender_account_id: AccountId,
        recipient_account_id: AccountId,
        amount: u128,
    ) -> Result<SendTxResponse, ExecutionFailureKind> {
        let account_ids = vec![sender_account_id, recipient_account_id];
        let program_id = nssa::program::Program::token().id();
        // Instruction must be: [0x01 || amount (little-endian 16 bytes) || 0x00 || 0x00 || 0x00 ||
        // 0x00 || 0x00 || 0x00].
        let mut instruction = [0; 23];
        instruction[0] = 0x01;
        instruction[1..17].copy_from_slice(&amount.to_le_bytes());
        let Ok(nonces) = self.0.get_accounts_nonces(vec![sender_account_id]).await else {
            return Err(ExecutionFailureKind::SequencerError);
        };
        let message = nssa::public_transaction::Message::try_new(
            program_id,
            account_ids,
            nonces,
            instruction,
        )
        .unwrap();

        let Some(signing_key) = self
            .0
            .storage
            .user_data
            .get_pub_account_signing_key(&sender_account_id)
        else {
            return Err(ExecutionFailureKind::KeyNotFoundError);
        };
        let witness_set =
            nssa::public_transaction::WitnessSet::for_message(&message, &[signing_key]);

        let tx = nssa::PublicTransaction::new(message, witness_set);

        Ok(self.0.sequencer_client.send_tx_public(tx).await?)
    }

    pub async fn send_transfer_transaction_private_owned_account(
        &self,
        sender_account_id: AccountId,
        recipient_account_id: AccountId,
        amount: u128,
    ) -> Result<(SendTxResponse, [SharedSecretKey; 2]), ExecutionFailureKind> {
        let (instruction_data, program) = token_program_preparation_transfer(amount);

        self.0
            .send_privacy_preserving_tx(
                vec![
                    PrivacyPreservingAccount::PrivateOwned(sender_account_id),
                    PrivacyPreservingAccount::PrivateOwned(recipient_account_id),
                ],
                &instruction_data,
                &program,
            )
            .await
            .map(|(resp, secrets)| {
                let mut iter = secrets.into_iter();
                let first = iter.next().expect("expected sender's secret");
                let second = iter.next().expect("expected recipient's secret");
                (resp, [first, second])
            })
    }

    pub async fn send_transfer_transaction_private_foreign_account(
        &self,
        sender_account_id: AccountId,
        recipient_npk: NullifierPublicKey,
        recipient_ipk: IncomingViewingPublicKey,
        amount: u128,
    ) -> Result<(SendTxResponse, [SharedSecretKey; 2]), ExecutionFailureKind> {
        let (instruction_data, program) = token_program_preparation_transfer(amount);

        self.0
            .send_privacy_preserving_tx(
                vec![
                    PrivacyPreservingAccount::PrivateOwned(sender_account_id),
                    PrivacyPreservingAccount::PrivateForeign {
                        npk: recipient_npk,
                        ipk: recipient_ipk,
                    },
                ],
                &instruction_data,
                &program,
            )
            .await
            .map(|(resp, secrets)| {
                let mut iter = secrets.into_iter();
                let first = iter.next().expect("expected sender's secret");
                let second = iter.next().expect("expected recipient's secret");
                (resp, [first, second])
            })
    }

    pub async fn send_transfer_transaction_deshielded(
        &self,
        sender_account_id: AccountId,
        recipient_account_id: AccountId,
        amount: u128,
    ) -> Result<(SendTxResponse, SharedSecretKey), ExecutionFailureKind> {
        let (instruction_data, program) = token_program_preparation_transfer(amount);

        self.0
            .send_privacy_preserving_tx(
                vec![
                    PrivacyPreservingAccount::PrivateOwned(sender_account_id),
                    PrivacyPreservingAccount::Public(recipient_account_id),
                ],
                &instruction_data,
                &program,
            )
            .await
            .map(|(resp, secrets)| {
                let first = secrets
                    .into_iter()
                    .next()
                    .expect("expected sender's secret");
                (resp, first)
            })
    }

    pub async fn send_transfer_transaction_shielded_owned_account(
        &self,
        sender_account_id: AccountId,
        recipient_account_id: AccountId,
        amount: u128,
    ) -> Result<(SendTxResponse, SharedSecretKey), ExecutionFailureKind> {
        let (instruction_data, program) = token_program_preparation_transfer(amount);

        self.0
            .send_privacy_preserving_tx(
                vec![
                    PrivacyPreservingAccount::Public(sender_account_id),
                    PrivacyPreservingAccount::PrivateOwned(recipient_account_id),
                ],
                &instruction_data,
                &program,
            )
            .await
            .map(|(resp, secrets)| {
                let first = secrets
                    .into_iter()
                    .next()
                    .expect("expected recipient's secret");
                (resp, first)
            })
    }

    pub async fn send_transfer_transaction_shielded_foreign_account(
        &self,
        sender_account_id: AccountId,
        recipient_npk: NullifierPublicKey,
        recipient_ipk: IncomingViewingPublicKey,
        amount: u128,
    ) -> Result<(SendTxResponse, SharedSecretKey), ExecutionFailureKind> {
        let (instruction_data, program) = token_program_preparation_transfer(amount);

        self.0
            .send_privacy_preserving_tx(
                vec![
                    PrivacyPreservingAccount::Public(sender_account_id),
                    PrivacyPreservingAccount::PrivateForeign {
                        npk: recipient_npk,
                        ipk: recipient_ipk,
                    },
                ],
                &instruction_data,
                &program,
            )
            .await
            .map(|(resp, secrets)| {
                let first = secrets
                    .into_iter()
                    .next()
                    .expect("expected recipient's secret");
                (resp, first)
            })
    }
}

fn token_program_preparation_transfer(amount: u128) -> (InstructionData, Program) {
    // Instruction must be: [0x01 || amount (little-endian 16 bytes) || 0x00 || 0x00 || 0x00 ||
    // 0x00 || 0x00 || 0x00].
    let mut instruction = [0; 23];
    instruction[0] = 0x01;
    instruction[1..17].copy_from_slice(&amount.to_le_bytes());
    let instruction_data = Program::serialize_instruction(instruction).unwrap();
    let program = Program::token();

    (instruction_data, program)
}

fn token_program_preparation_definition(
    name: [u8; 6],
    total_supply: u128,
) -> (InstructionData, Program) {
    // Instruction must be: [0x00 || total_supply (little-endian 16 bytes) || name (6 bytes)]
    let mut instruction = [0; 23];
    instruction[1..17].copy_from_slice(&total_supply.to_le_bytes());
    instruction[17..].copy_from_slice(&name);
    let instruction_data = Program::serialize_instruction(instruction).unwrap();
    let program = Program::token();

    (instruction_data, program)
}
