use common::{error::ExecutionFailureKind, sequencer_client::json::SendTxResponse};
use nssa::{Account, AccountId, program::Program};
use nssa_core::{
    NullifierPublicKey, SharedSecretKey, encryption::IncomingViewingPublicKey,
    program::InstructionData,
};

use crate::{PrivacyPreservingAccount, WalletCore};

impl WalletCore {
    pub fn token_program_preparation_transfer(
        amount: u128,
    ) -> (
        InstructionData,
        Program,
        impl FnOnce(&[&Account]) -> Result<(), ExecutionFailureKind>,
    ) {
        // Instruction must be: [0x01 || amount (little-endian 16 bytes) || 0x00 || 0x00 || 0x00 ||
        // 0x00 || 0x00 || 0x00].
        let mut instruction = [0; 23];
        instruction[0] = 0x01;
        instruction[1..17].copy_from_slice(&amount.to_le_bytes());
        let instruction_data = Program::serialize_instruction(instruction).unwrap();
        let program = Program::token();
        let tx_pre_check = |_: &[&Account]| Ok(());

        (instruction_data, program, tx_pre_check)
    }

    pub fn token_program_preparation_definition(
        name: [u8; 6],
        total_supply: u128,
    ) -> (
        InstructionData,
        Program,
        impl FnOnce(&[&Account]) -> Result<(), ExecutionFailureKind>,
    ) {
        // Instruction must be: [0x00 || total_supply (little-endian 16 bytes) || name (6 bytes)]
        let mut instruction = [0; 23];
        instruction[1..17].copy_from_slice(&total_supply.to_le_bytes());
        instruction[17..].copy_from_slice(&name);
        let instruction_data = Program::serialize_instruction(instruction).unwrap();
        let program = Program::token();
        let tx_pre_check = |_: &[&Account]| Ok(());

        (instruction_data, program, tx_pre_check)
    }

    pub async fn send_new_token_definition(
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

        Ok(self.sequencer_client.send_tx_public(tx).await?)
    }

    pub async fn send_new_token_definition_private_owned(
        &self,
        definition_account_id: AccountId,
        supply_account_id: AccountId,
        name: [u8; 6],
        total_supply: u128,
    ) -> Result<(SendTxResponse, SharedSecretKey), ExecutionFailureKind> {
        let (instruction_data, program, tx_pre_check) =
            WalletCore::token_program_preparation_definition(name, total_supply);

        self.send_privacy_preserving_tx(
            vec![
                PrivacyPreservingAccount::Public(definition_account_id),
                PrivacyPreservingAccount::PrivateLocal(supply_account_id),
            ],
            instruction_data,
            tx_pre_check,
            program,
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

    pub async fn send_transfer_token_transaction(
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
        let Ok(nonces) = self.get_accounts_nonces(vec![sender_account_id]).await else {
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
            .storage
            .user_data
            .get_pub_account_signing_key(&sender_account_id)
        else {
            return Err(ExecutionFailureKind::KeyNotFoundError);
        };
        let witness_set =
            nssa::public_transaction::WitnessSet::for_message(&message, &[signing_key]);

        let tx = nssa::PublicTransaction::new(message, witness_set);

        Ok(self.sequencer_client.send_tx_public(tx).await?)
    }

    pub async fn send_transfer_token_transaction_private_owned_account(
        &self,
        sender_account_id: AccountId,
        recipient_account_id: AccountId,
        amount: u128,
    ) -> Result<(SendTxResponse, [SharedSecretKey; 2]), ExecutionFailureKind> {
        let (instruction_data, program, tx_pre_check) =
            WalletCore::token_program_preparation_transfer(amount);

        self.send_privacy_preserving_tx(
            vec![
                PrivacyPreservingAccount::PrivateLocal(sender_account_id),
                PrivacyPreservingAccount::PrivateLocal(recipient_account_id),
            ],
            instruction_data,
            tx_pre_check,
            program,
        )
        .await
        .map(|(resp, secrets)| {
            let mut iter = secrets.into_iter();
            let first = iter.next().expect("expected sender's secret");
            let second = iter.next().expect("expected recipient's secret");
            (resp, [first, second])
        })
    }

    pub async fn send_transfer_token_transaction_private_foreign_account(
        &self,
        sender_account_id: AccountId,
        recipient_npk: NullifierPublicKey,
        recipient_ipk: IncomingViewingPublicKey,
        amount: u128,
    ) -> Result<(SendTxResponse, [SharedSecretKey; 2]), ExecutionFailureKind> {
        let (instruction_data, program, tx_pre_check) =
            WalletCore::token_program_preparation_transfer(amount);

        self.send_privacy_preserving_tx(
            vec![
                PrivacyPreservingAccount::PrivateLocal(sender_account_id),
                PrivacyPreservingAccount::PrivateForeign {
                    npk: recipient_npk,
                    ipk: recipient_ipk,
                },
            ],
            instruction_data,
            tx_pre_check,
            program,
        )
        .await
        .map(|(resp, secrets)| {
            let mut iter = secrets.into_iter();
            let first = iter.next().expect("expected sender's secret");
            let second = iter.next().expect("expected recipient's secret");
            (resp, [first, second])
        })
    }

    pub async fn send_transfer_token_transaction_deshielded(
        &self,
        sender_account_id: AccountId,
        recipient_account_id: AccountId,
        amount: u128,
    ) -> Result<(SendTxResponse, SharedSecretKey), ExecutionFailureKind> {
        let (instruction_data, program, tx_pre_check) =
            WalletCore::token_program_preparation_transfer(amount);

        self.send_privacy_preserving_tx(
            vec![
                PrivacyPreservingAccount::PrivateLocal(sender_account_id),
                PrivacyPreservingAccount::Public(recipient_account_id),
            ],
            instruction_data,
            tx_pre_check,
            program,
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

    pub async fn send_transfer_token_transaction_shielded_owned_account(
        &self,
        sender_account_id: AccountId,
        recipient_account_id: AccountId,
        amount: u128,
    ) -> Result<(SendTxResponse, SharedSecretKey), ExecutionFailureKind> {
        let (instruction_data, program, tx_pre_check) =
            WalletCore::token_program_preparation_transfer(amount);

        self.send_privacy_preserving_tx(
            vec![
                PrivacyPreservingAccount::Public(sender_account_id),
                PrivacyPreservingAccount::PrivateLocal(recipient_account_id),
            ],
            instruction_data,
            tx_pre_check,
            program,
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

    pub async fn send_transfer_token_transaction_shielded_foreign_account(
        &self,
        sender_account_id: AccountId,
        recipient_npk: NullifierPublicKey,
        recipient_ipk: IncomingViewingPublicKey,
        amount: u128,
    ) -> Result<(SendTxResponse, SharedSecretKey), ExecutionFailureKind> {
        let (instruction_data, program, tx_pre_check) =
            WalletCore::token_program_preparation_transfer(amount);

        self.send_privacy_preserving_tx(
            vec![
                PrivacyPreservingAccount::Public(sender_account_id),
                PrivacyPreservingAccount::PrivateForeign {
                    npk: recipient_npk,
                    ipk: recipient_ipk,
                },
            ],
            instruction_data,
            tx_pre_check,
            program,
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
