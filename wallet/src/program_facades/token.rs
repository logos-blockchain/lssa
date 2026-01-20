use common::{error::ExecutionFailureKind, rpc_primitives::requests::SendTxResponse};
use nssa::{AccountId, program::Program};
use nssa_core::program::InstructionData;

use crate::{WalletCore, program_facades::ProgramArgs};

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
        let mut instruction = vec![0u8; 23];
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
        let mut instruction = vec![0u8; 23];
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

    pub async fn send_burn_transaction(
        &self,
        definition_account_id: AccountId,
        holder_account_id: AccountId,
        amount: u128,
    ) -> Result<SendTxResponse, ExecutionFailureKind> {
        let account_ids = vec![definition_account_id, holder_account_id];
        let (instruction, program, _) = TokenBurnArgs { amount }.prepare_private_transfer();

        let Ok(nonces) = self.0.get_accounts_nonces(vec![holder_account_id]).await else {
            return Err(ExecutionFailureKind::SequencerError);
        };
        let message = nssa::public_transaction::Message::new_preserialized(
            program.id(),
            account_ids,
            nonces,
            instruction,
        );

        let signing_key = self
            .0
            .storage
            .user_data
            .get_pub_account_signing_key(&holder_account_id)
            .ok_or(ExecutionFailureKind::KeyNotFoundError)?;
        let witness_set =
            nssa::public_transaction::WitnessSet::for_message(&message, &[signing_key]);

        let tx = nssa::PublicTransaction::new(message, witness_set);

        Ok(self.0.sequencer_client.send_tx_public(tx).await?)
    }

    pub async fn send_mint_transaction(
        &self,
        definition_account_id: AccountId,
        holder_account_id: AccountId,
        amount: u128,
    ) -> Result<SendTxResponse, ExecutionFailureKind> {
        let account_ids = vec![definition_account_id, holder_account_id];
        let (instruction, program, _) = TokenMintArgs { amount }.prepare_private_transfer();

        let Ok(nonces) = self
            .0
            .get_accounts_nonces(vec![definition_account_id])
            .await
        else {
            return Err(ExecutionFailureKind::SequencerError);
        };
        let message = nssa::public_transaction::Message::new_preserialized(
            program.id(),
            account_ids,
            nonces,
            instruction,
        );

        let Some(signing_key) = self
            .0
            .storage
            .user_data
            .get_pub_account_signing_key(&definition_account_id)
        else {
            return Err(ExecutionFailureKind::KeyNotFoundError);
        };
        let witness_set =
            nssa::public_transaction::WitnessSet::for_message(&message, &[signing_key]);

        let tx = nssa::PublicTransaction::new(message, witness_set);

        Ok(self.0.sequencer_client.send_tx_public(tx).await?)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TokenDefinitionArgs {
    pub name: [u8; 6],
    pub total_supply: u128,
}

impl ProgramArgs for TokenDefinitionArgs {
    fn prepare_private_transfer(
        &self,
    ) -> (
        InstructionData,
        Program,
        impl FnOnce(&[&nssa::Account]) -> Result<(), ExecutionFailureKind>,
    ) {
        // Instruction must be: [0x00 || total_supply (little-endian 16 bytes) || name (6 bytes)]
        let mut instruction = [0; 23];
        instruction[1..17].copy_from_slice(&self.total_supply.to_le_bytes());
        instruction[17..].copy_from_slice(&self.name);
        let instruction_data = Program::serialize_instruction(instruction.to_vec()).unwrap();
        let program = Program::token();

        (instruction_data, program, |_| Ok(()))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TokenTransferArgs {
    pub amount: u128,
}

impl ProgramArgs for TokenTransferArgs {
    fn prepare_private_transfer(
        &self,
    ) -> (
        InstructionData,
        Program,
        impl FnOnce(&[&nssa::Account]) -> Result<(), ExecutionFailureKind>,
    ) {
        // Instruction must be: [0x01 || amount (little-endian 16 bytes) || 0x00 || 0x00 || 0x00 ||
        // 0x00 || 0x00 || 0x00].
        let mut instruction = [0; 23];
        instruction[0] = 0x01;
        instruction[1..17].copy_from_slice(&self.amount.to_le_bytes());
        let instruction_data = Program::serialize_instruction(instruction.to_vec()).unwrap();
        let program = Program::token();

        (instruction_data, program, |_| Ok(()))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TokenBurnArgs {
    pub amount: u128,
}

impl ProgramArgs for TokenBurnArgs {
    fn prepare_private_transfer(
        &self,
    ) -> (
        InstructionData,
        Program,
        impl FnOnce(&[&nssa::Account]) -> Result<(), ExecutionFailureKind>,
    ) {
        // Instruction must be: [0x03 || amount (little-endian 16 bytes) || 0x00 || 0x00 || 0x00 ||
        // 0x00 || 0x00 || 0x00].
        let mut instruction = [0; 23];
        instruction[0] = 0x03;
        instruction[1..17].copy_from_slice(&self.amount.to_le_bytes());
        let instruction_data = Program::serialize_instruction(instruction.to_vec()).unwrap();
        let program = Program::token();

        (instruction_data, program, |_| Ok(()))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TokenMintArgs {
    pub amount: u128,
}

impl ProgramArgs for TokenMintArgs {
    fn prepare_private_transfer(
        &self,
    ) -> (
        InstructionData,
        Program,
        impl FnOnce(&[&nssa::Account]) -> Result<(), ExecutionFailureKind>,
    ) {
        // Instruction must be: [0x04 || amount (little-endian 16 bytes) || 0x00 || 0x00 || 0x00 ||
        // 0x00 || 0x00 || 0x00].
        let mut instruction = [0; 23];
        instruction[0] = 0x04;
        instruction[1..17].copy_from_slice(&self.amount.to_le_bytes());
        let instruction_data = Program::serialize_instruction(instruction.to_vec()).unwrap();
        let program = Program::token();

        (instruction_data, program, |_| Ok(()))
    }
}
