use common::{error::ExecutionFailureKind, sequencer_client::json::SendTxResponse};
use nssa::{Account, Address, program::Program};
use nssa_core::{
    MembershipProof, NullifierPublicKey, SharedSecretKey, encryption::IncomingViewingPublicKey,
    program::InstructionData,
};

use crate::WalletCore;

impl WalletCore {
    pub fn token_program_preparation_transfer(
        amount: u128,
    ) -> (
        InstructionData,
        Program,
        impl FnOnce(&Account, &Account) -> Result<(), ExecutionFailureKind>,
    ) {
        // Instruction must be: [0x01 || amount (little-endian 16 bytes) || 0x00 || 0x00 || 0x00 || 0x00 || 0x00 || 0x00].
        let mut instruction = [0; 23];
        instruction[0] = 0x01;
        instruction[1..17].copy_from_slice(&amount.to_le_bytes());
        let instruction_data = Program::serialize_instruction(instruction).unwrap();
        let program = Program::token();
        let tx_pre_check = |_: &Account, _: &Account| Ok(());

        (instruction_data, program, tx_pre_check)
    }

    pub fn token_program_preparation_definition(
        name: [u8; 6],
        total_supply: u128,
    ) -> (
        InstructionData,
        Program,
        impl FnOnce(&Account, &Account) -> Result<(), ExecutionFailureKind>,
    ) {
        // Instruction must be: [0x00 || total_supply (little-endian 16 bytes) || name (6 bytes)]
        let mut instruction = [0; 23];
        instruction[1..17].copy_from_slice(&total_supply.to_le_bytes());
        instruction[17..].copy_from_slice(&name);
        let instruction_data = Program::serialize_instruction(instruction).unwrap();
        let program = Program::token();
        let tx_pre_check = |_: &Account, _: &Account| Ok(());

        (instruction_data, program, tx_pre_check)
    }

    pub async fn send_new_token_definition(
        &self,
        definition_address: Address,
        supply_address: Address,
        name: [u8; 6],
        total_supply: u128,
    ) -> Result<SendTxResponse, ExecutionFailureKind> {
        let addresses = vec![definition_address, supply_address];
        let program_id = nssa::program::Program::token().id();
        // Instruction must be: [0x00 || total_supply (little-endian 16 bytes) || name (6 bytes)]
        let mut instruction = [0; 23];
        instruction[1..17].copy_from_slice(&total_supply.to_le_bytes());
        instruction[17..].copy_from_slice(&name);
        let message =
            nssa::public_transaction::Message::try_new(program_id, addresses, vec![], instruction)
                .unwrap();

        let witness_set = nssa::public_transaction::WitnessSet::for_message(&message, &[]);

        let tx = nssa::PublicTransaction::new(message, witness_set);

        Ok(self.sequencer_client.send_tx_public(tx).await?)
    }

    pub async fn send_new_token_definition_private_owned(
        &self,
        definition_addr: Address,
        supply_addr: Address,
        name: [u8; 6],
        total_supply: u128,
    ) -> Result<(SendTxResponse, [SharedSecretKey; 1]), ExecutionFailureKind> {
        let (instruction_data, program, tx_pre_check) =
            WalletCore::token_program_preparation_definition(name, total_supply);

        // Kind of non-obvious naming
        // Basically this funtion is called because authentication mask is [0, 2]
        self.shielded_two_accs_receiver_uninit(
            definition_addr,
            supply_addr,
            instruction_data,
            tx_pre_check,
            program,
        )
        .await
    }

    pub async fn send_transfer_token_transaction(
        &self,
        sender_address: Address,
        recipient_address: Address,
        amount: u128,
    ) -> Result<SendTxResponse, ExecutionFailureKind> {
        let addresses = vec![sender_address, recipient_address];
        let program_id = nssa::program::Program::token().id();
        // Instruction must be: [0x01 || amount (little-endian 16 bytes) || 0x00 || 0x00 || 0x00 || 0x00 || 0x00 || 0x00].
        let mut instruction = [0; 23];
        instruction[0] = 0x01;
        instruction[1..17].copy_from_slice(&amount.to_le_bytes());
        let Ok(nonces) = self.get_accounts_nonces(vec![sender_address]).await else {
            return Err(ExecutionFailureKind::SequencerError);
        };
        let message =
            nssa::public_transaction::Message::try_new(program_id, addresses, nonces, instruction)
                .unwrap();

        let Some(signing_key) = self
            .storage
            .user_data
            .get_pub_account_signing_key(&sender_address)
        else {
            return Err(ExecutionFailureKind::KeyNotFoundError);
        };
        let witness_set =
            nssa::public_transaction::WitnessSet::for_message(&message, &[signing_key]);

        let tx = nssa::PublicTransaction::new(message, witness_set);

        Ok(self.sequencer_client.send_tx_public(tx).await?)
    }

    pub async fn send_transfer_token_transaction_private_owned_account_already_initialized(
        &self,
        sender_address: Address,
        recipient_address: Address,
        amount: u128,
        recipient_proof: MembershipProof,
    ) -> Result<(SendTxResponse, [SharedSecretKey; 2]), ExecutionFailureKind> {
        let (instruction_data, program, tx_pre_check) =
            WalletCore::token_program_preparation_transfer(amount);

        self.private_tx_two_accs_all_init(
            sender_address,
            recipient_address,
            instruction_data,
            tx_pre_check,
            program,
            recipient_proof,
        )
        .await
    }

    pub async fn send_transfer_token_transaction_private_owned_account_not_initialized(
        &self,
        sender_address: Address,
        recipient_address: Address,
        amount: u128,
    ) -> Result<(SendTxResponse, [SharedSecretKey; 2]), ExecutionFailureKind> {
        let (instruction_data, program, tx_pre_check) =
            WalletCore::token_program_preparation_transfer(amount);

        self.private_tx_two_accs_receiver_uninit(
            sender_address,
            recipient_address,
            instruction_data,
            tx_pre_check,
            program,
        )
        .await
    }

    pub async fn send_transfer_token_transaction_private_foreign_account(
        &self,
        sender_address: Address,
        recipient_npk: NullifierPublicKey,
        recipient_ipk: IncomingViewingPublicKey,
        amount: u128,
    ) -> Result<(SendTxResponse, [SharedSecretKey; 2]), ExecutionFailureKind> {
        let (instruction_data, program, tx_pre_check) =
            WalletCore::token_program_preparation_transfer(amount);

        self.private_tx_two_accs_receiver_outer(
            sender_address,
            recipient_npk,
            recipient_ipk,
            instruction_data,
            tx_pre_check,
            program,
        )
        .await
    }

    pub async fn send_transfer_token_transaction_deshielded(
        &self,
        sender_address: Address,
        recipient_address: Address,
        amount: u128,
    ) -> Result<(SendTxResponse, [SharedSecretKey; 1]), ExecutionFailureKind> {
        let (instruction_data, program, tx_pre_check) =
            WalletCore::token_program_preparation_transfer(amount);

        self.deshielded_tx_two_accs(
            sender_address,
            recipient_address,
            instruction_data,
            tx_pre_check,
            program,
        )
        .await
    }

    pub async fn send_transfer_token_transaction_shielded_owned_account_already_initialized(
        &self,
        sender_address: Address,
        recipient_address: Address,
        amount: u128,
        recipient_proof: MembershipProof,
    ) -> Result<(SendTxResponse, [SharedSecretKey; 1]), ExecutionFailureKind> {
        let (instruction_data, program, tx_pre_check) =
            WalletCore::token_program_preparation_transfer(amount);

        self.shielded_two_accs_all_init(
            sender_address,
            recipient_address,
            instruction_data,
            tx_pre_check,
            program,
            recipient_proof,
        )
        .await
    }

    pub async fn send_transfer_token_transaction_shielded_owned_account_not_initialized(
        &self,
        sender_address: Address,
        recipient_address: Address,
        amount: u128,
    ) -> Result<(SendTxResponse, [SharedSecretKey; 1]), ExecutionFailureKind> {
        let (instruction_data, program, tx_pre_check) =
            WalletCore::token_program_preparation_transfer(amount);

        self.shielded_two_accs_receiver_uninit(
            sender_address,
            recipient_address,
            instruction_data,
            tx_pre_check,
            program,
        )
        .await
    }

    pub async fn send_transfer_token_transaction_shielded_foreign_account(
        &self,
        sender_address: Address,
        recipient_npk: NullifierPublicKey,
        recipient_ipk: IncomingViewingPublicKey,
        amount: u128,
    ) -> Result<SendTxResponse, ExecutionFailureKind> {
        let (instruction_data, program, tx_pre_check) =
            WalletCore::token_program_preparation_transfer(amount);

        self.shielded_two_accs_receiver_outer(
            sender_address,
            recipient_npk,
            recipient_ipk,
            instruction_data,
            tx_pre_check,
            program,
        )
        .await
    }
}
