use common::{error::ExecutionFailureKind, sequencer_client::json::SendTxResponse};
use key_protocol::key_management::ephemeral_key_holder::EphemeralKeyHolder;
use nssa::{Address, privacy_preserving_transaction::circuit, program::Program};
use nssa_core::{
    Commitment, MembershipProof, NullifierPublicKey, SharedSecretKey, account::AccountWithMetadata,
    encryption::IncomingViewingPublicKey,
};

use crate::{WalletCore, helperfunctions::produce_random_nonces};

impl WalletCore {
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
        let Some((supply_keys, supply_acc)) = self
            .storage
            .user_data
            .get_private_account(&supply_addr)
            .cloned()
        else {
            return Err(ExecutionFailureKind::KeyNotFoundError);
        };

        //It makes more sence to have definition acc as public
        let definition_acc = self.get_account_public(definition_addr).await.unwrap();

        let supply_npk = supply_keys.nullifer_public_key;
        let supply_ipk = supply_keys.incoming_viewing_public_key;

        let program = nssa::program::Program::token();

        let definition_pre =
            AccountWithMetadata::new(definition_acc.clone(), false, definition_addr);
        let supply_pre = AccountWithMetadata::new(supply_acc.clone(), false, &supply_npk);

        let eph_holder_supply = EphemeralKeyHolder::new(&supply_npk);
        let shared_secret_supply = eph_holder_supply.calculate_shared_secret_sender(&supply_ipk);

        // Instruction must be: [0x00 || total_supply (little-endian 16 bytes) || name (6 bytes)]
        let mut instruction = [0; 23];
        instruction[1..17].copy_from_slice(&total_supply.to_le_bytes());
        instruction[17..].copy_from_slice(&name);

        let (output, proof) = circuit::execute_and_prove(
            &[definition_pre, supply_pre],
            &nssa::program::Program::serialize_instruction(instruction).unwrap(),
            &[0, 2],
            &produce_random_nonces(1),
            &[(supply_npk.clone(), shared_secret_supply.clone())],
            &[],
            &program,
        )
        .unwrap();

        let message =
            nssa::privacy_preserving_transaction::message::Message::try_from_circuit_output(
                vec![definition_addr],
                vec![],
                vec![(
                    supply_npk.clone(),
                    supply_ipk.clone(),
                    eph_holder_supply.generate_ephemeral_public_key(),
                )],
                output,
            )
            .unwrap();

        let witness_set =
            nssa::privacy_preserving_transaction::witness_set::WitnessSet::for_message(
                &message,
                proof,
                &[],
            );
        let tx = nssa::PrivacyPreservingTransaction::new(message, witness_set);

        Ok((
            self.sequencer_client.send_tx_private(tx).await?,
            [shared_secret_supply],
        ))
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
        let Some((sender_keys, sender_acc)) = self
            .storage
            .user_data
            .get_private_account(&sender_address)
            .cloned()
        else {
            return Err(ExecutionFailureKind::KeyNotFoundError);
        };

        let Some((recipient_keys, recipient_acc)) = self
            .storage
            .user_data
            .get_private_account(&recipient_address)
            .cloned()
        else {
            return Err(ExecutionFailureKind::KeyNotFoundError);
        };

        let sender_npk = sender_keys.nullifer_public_key;
        let sender_ipk = sender_keys.incoming_viewing_public_key;
        let recipient_npk = recipient_keys.nullifer_public_key.clone();
        let recipient_ipk = recipient_keys.incoming_viewing_public_key.clone();

        let program = Program::token();

        let sender_commitment = Commitment::new(&sender_npk, &sender_acc);

        let sender_pre = AccountWithMetadata::new(sender_acc.clone(), true, &sender_npk);
        let recipient_pre = AccountWithMetadata::new(recipient_acc.clone(), true, &recipient_npk);

        let eph_holder_sender = EphemeralKeyHolder::new(&sender_npk);
        let shared_secret_sender = eph_holder_sender.calculate_shared_secret_sender(&sender_ipk);

        let eph_holder_recipient = EphemeralKeyHolder::new(&recipient_npk);
        let shared_secret_recipient =
            eph_holder_recipient.calculate_shared_secret_sender(&recipient_ipk);

        // Instruction must be: [0x01 || amount (little-endian 16 bytes) || 0x00 || 0x00 || 0x00 || 0x00 || 0x00 || 0x00].
        let mut instruction = [0; 23];
        instruction[0] = 0x01;
        instruction[1..17].copy_from_slice(&amount.to_le_bytes());

        let (output, proof) = circuit::execute_and_prove(
            &[sender_pre, recipient_pre],
            &Program::serialize_instruction(instruction).unwrap(),
            &[1, 1],
            &produce_random_nonces(2),
            &[
                (sender_npk.clone(), shared_secret_sender.clone()),
                (recipient_npk.clone(), shared_secret_recipient.clone()),
            ],
            &[
                (
                    sender_keys.private_key_holder.nullifier_secret_key,
                    self.sequencer_client
                        .get_proof_for_commitment(sender_commitment)
                        .await
                        .unwrap()
                        .unwrap(),
                ),
                (
                    recipient_keys.private_key_holder.nullifier_secret_key,
                    recipient_proof,
                ),
            ],
            &program,
        )
        .unwrap();

        let message =
            nssa::privacy_preserving_transaction::message::Message::try_from_circuit_output(
                vec![],
                vec![],
                vec![
                    (
                        sender_npk.clone(),
                        sender_ipk.clone(),
                        eph_holder_sender.generate_ephemeral_public_key(),
                    ),
                    (
                        recipient_npk.clone(),
                        recipient_ipk.clone(),
                        eph_holder_recipient.generate_ephemeral_public_key(),
                    ),
                ],
                output,
            )
            .unwrap();

        let witness_set =
            nssa::privacy_preserving_transaction::witness_set::WitnessSet::for_message(
                &message,
                proof,
                &[],
            );
        let tx = nssa::PrivacyPreservingTransaction::new(message, witness_set);

        Ok((
            self.sequencer_client.send_tx_private(tx).await?,
            [shared_secret_sender, shared_secret_recipient],
        ))
    }

    pub async fn send_transfer_token_transaction_private_owned_account_not_initialized(
        &self,
        sender_address: Address,
        recipient_address: Address,
        amount: u128,
    ) -> Result<(SendTxResponse, [SharedSecretKey; 2]), ExecutionFailureKind> {
        let Some((sender_keys, sender_acc)) = self
            .storage
            .user_data
            .get_private_account(&sender_address)
            .cloned()
        else {
            return Err(ExecutionFailureKind::KeyNotFoundError);
        };

        let Some((recipient_keys, recipient_acc)) = self
            .storage
            .user_data
            .get_private_account(&recipient_address)
            .cloned()
        else {
            return Err(ExecutionFailureKind::KeyNotFoundError);
        };

        let sender_npk = sender_keys.nullifer_public_key;
        let sender_ipk = sender_keys.incoming_viewing_public_key;
        let recipient_npk = recipient_keys.nullifer_public_key.clone();
        let recipient_ipk = recipient_keys.incoming_viewing_public_key.clone();

        let program = Program::token();

        let sender_commitment = Commitment::new(&sender_npk, &sender_acc);

        let sender_pre = AccountWithMetadata::new(sender_acc.clone(), true, &sender_npk);
        let recipient_pre = AccountWithMetadata::new(recipient_acc.clone(), false, &recipient_npk);

        let eph_holder_sender = EphemeralKeyHolder::new(&sender_npk);
        let shared_secret_sender = eph_holder_sender.calculate_shared_secret_sender(&sender_ipk);

        let eph_holder_recipient = EphemeralKeyHolder::new(&recipient_npk);
        let shared_secret_recipient =
            eph_holder_recipient.calculate_shared_secret_sender(&recipient_ipk);

        // Instruction must be: [0x01 || amount (little-endian 16 bytes) || 0x00 || 0x00 || 0x00 || 0x00 || 0x00 || 0x00].
        let mut instruction = [0; 23];
        instruction[0] = 0x01;
        instruction[1..17].copy_from_slice(&amount.to_le_bytes());

        let (output, proof) = circuit::execute_and_prove(
            &[sender_pre, recipient_pre],
            &Program::serialize_instruction(instruction).unwrap(),
            &[1, 2],
            &produce_random_nonces(2),
            &[
                (sender_npk.clone(), shared_secret_sender.clone()),
                (recipient_npk.clone(), shared_secret_recipient.clone()),
            ],
            &[(
                sender_keys.private_key_holder.nullifier_secret_key,
                self.sequencer_client
                    .get_proof_for_commitment(sender_commitment)
                    .await
                    .unwrap()
                    .unwrap(),
            )],
            &program,
        )
        .unwrap();

        let message =
            nssa::privacy_preserving_transaction::message::Message::try_from_circuit_output(
                vec![],
                vec![],
                vec![
                    (
                        sender_npk.clone(),
                        sender_ipk.clone(),
                        eph_holder_sender.generate_ephemeral_public_key(),
                    ),
                    (
                        recipient_npk.clone(),
                        recipient_ipk.clone(),
                        eph_holder_recipient.generate_ephemeral_public_key(),
                    ),
                ],
                output,
            )
            .unwrap();

        let witness_set =
            nssa::privacy_preserving_transaction::witness_set::WitnessSet::for_message(
                &message,
                proof,
                &[],
            );
        let tx = nssa::PrivacyPreservingTransaction::new(message, witness_set);

        Ok((
            self.sequencer_client.send_tx_private(tx).await?,
            [shared_secret_sender, shared_secret_recipient],
        ))
    }

    pub async fn send_transfer_token_transaction_private_foreign_account(
        &self,
        sender_address: Address,
        recipient_npk: NullifierPublicKey,
        recipient_ipk: IncomingViewingPublicKey,
        amount: u128,
    ) -> Result<(SendTxResponse, [SharedSecretKey; 2]), ExecutionFailureKind> {
        let Some((sender_keys, sender_acc)) = self
            .storage
            .user_data
            .get_private_account(&sender_address)
            .cloned()
        else {
            return Err(ExecutionFailureKind::KeyNotFoundError);
        };

        let recipient_acc = nssa_core::account::Account::default();

        let sender_npk = sender_keys.nullifer_public_key;
        let sender_ipk = sender_keys.incoming_viewing_public_key;

        let program = Program::token();

        let sender_commitment = Commitment::new(&sender_npk, &sender_acc);

        let sender_pre = AccountWithMetadata::new(sender_acc.clone(), true, &sender_npk);
        let recipient_pre = AccountWithMetadata::new(recipient_acc.clone(), false, &recipient_npk);

        let eph_holder_sender = EphemeralKeyHolder::new(&sender_npk);
        let shared_secret_sender = eph_holder_sender.calculate_shared_secret_sender(&sender_ipk);

        let eph_holder_recipient = EphemeralKeyHolder::new(&recipient_npk);
        let shared_secret_recipient =
            eph_holder_recipient.calculate_shared_secret_sender(&recipient_ipk);

        // Instruction must be: [0x01 || amount (little-endian 16 bytes) || 0x00 || 0x00 || 0x00 || 0x00 || 0x00 || 0x00].
        let mut instruction = [0; 23];
        instruction[0] = 0x01;
        instruction[1..17].copy_from_slice(&amount.to_le_bytes());

        let (output, proof) = circuit::execute_and_prove(
            &[sender_pre, recipient_pre],
            &Program::serialize_instruction(instruction).unwrap(),
            &[1, 2],
            &produce_random_nonces(2),
            &[
                (sender_npk.clone(), shared_secret_sender.clone()),
                (recipient_npk.clone(), shared_secret_recipient.clone()),
            ],
            &[(
                sender_keys.private_key_holder.nullifier_secret_key,
                self.sequencer_client
                    .get_proof_for_commitment(sender_commitment)
                    .await
                    .unwrap()
                    .unwrap(),
            )],
            &program,
        )
        .unwrap();

        let message =
            nssa::privacy_preserving_transaction::message::Message::try_from_circuit_output(
                vec![],
                vec![],
                vec![
                    (
                        sender_npk.clone(),
                        sender_ipk.clone(),
                        eph_holder_sender.generate_ephemeral_public_key(),
                    ),
                    (
                        recipient_npk.clone(),
                        recipient_ipk.clone(),
                        eph_holder_recipient.generate_ephemeral_public_key(),
                    ),
                ],
                output,
            )
            .unwrap();

        let witness_set =
            nssa::privacy_preserving_transaction::witness_set::WitnessSet::for_message(
                &message,
                proof,
                &[],
            );
        let tx = nssa::PrivacyPreservingTransaction::new(message, witness_set);

        Ok((
            self.sequencer_client.send_tx_private(tx).await?,
            [shared_secret_sender, shared_secret_recipient],
        ))
    }

    pub async fn send_transfer_token_transaction_deshielded(
        &self,
        sender_address: Address,
        recipient_address: Address,
        amount: u128,
    ) -> Result<(SendTxResponse, [SharedSecretKey; 1]), ExecutionFailureKind> {
        let Some((sender_keys, sender_acc)) = self
            .storage
            .user_data
            .get_private_account(&sender_address)
            .cloned()
        else {
            return Err(ExecutionFailureKind::KeyNotFoundError);
        };

        let Ok(recipient_acc) = self.get_account_public(recipient_address).await else {
            return Err(ExecutionFailureKind::KeyNotFoundError);
        };

        let sender_npk = sender_keys.nullifer_public_key;
        let sender_ipk = sender_keys.incoming_viewing_public_key;

        let program = Program::token();

        let sender_commitment = Commitment::new(&sender_npk, &sender_acc);

        let sender_pre = AccountWithMetadata::new(sender_acc.clone(), true, &sender_npk);
        let recipient_pre =
            AccountWithMetadata::new(recipient_acc.clone(), false, recipient_address);

        let eph_holder_sender = EphemeralKeyHolder::new(&sender_npk);
        let shared_secret_sender = eph_holder_sender.calculate_shared_secret_sender(&sender_ipk);

        // Instruction must be: [0x01 || amount (little-endian 16 bytes) || 0x00 || 0x00 || 0x00 || 0x00 || 0x00 || 0x00].
        let mut instruction = [0; 23];
        instruction[0] = 0x01;
        instruction[1..17].copy_from_slice(&amount.to_le_bytes());

        let (output, proof) = circuit::execute_and_prove(
            &[sender_pre, recipient_pre],
            &Program::serialize_instruction(instruction).unwrap(),
            &[1, 0],
            &produce_random_nonces(1),
            &[(sender_npk.clone(), shared_secret_sender.clone())],
            &[(
                sender_keys.private_key_holder.nullifier_secret_key,
                self.sequencer_client
                    .get_proof_for_commitment(sender_commitment)
                    .await
                    .unwrap()
                    .unwrap(),
            )],
            &program,
        )
        .unwrap();

        let message =
            nssa::privacy_preserving_transaction::message::Message::try_from_circuit_output(
                vec![recipient_address],
                vec![],
                vec![(
                    sender_npk.clone(),
                    sender_ipk.clone(),
                    eph_holder_sender.generate_ephemeral_public_key(),
                )],
                output,
            )
            .unwrap();

        let witness_set =
            nssa::privacy_preserving_transaction::witness_set::WitnessSet::for_message(
                &message,
                proof,
                &[],
            );
        let tx = nssa::PrivacyPreservingTransaction::new(message, witness_set);

        Ok((
            self.sequencer_client.send_tx_private(tx).await?,
            [shared_secret_sender],
        ))
    }

    pub async fn send_transfer_token_transaction_shielded_owned_account_already_initialized(
        &self,
        sender_address: Address,
        recipient_address: Address,
        amount: u128,
        recipient_proof: MembershipProof,
    ) -> Result<(SendTxResponse, [SharedSecretKey; 1]), ExecutionFailureKind> {
        let Ok(sender_acc) = self.get_account_public(sender_address).await else {
            return Err(ExecutionFailureKind::KeyNotFoundError);
        };

        let Some(sender_priv_key) = self
            .storage
            .user_data
            .get_pub_account_signing_key(&sender_address)
        else {
            return Err(ExecutionFailureKind::KeyNotFoundError);
        };

        let Some((recipient_keys, recipient_acc)) = self
            .storage
            .user_data
            .get_private_account(&recipient_address)
            .cloned()
        else {
            return Err(ExecutionFailureKind::KeyNotFoundError);
        };

        let recipient_npk = recipient_keys.nullifer_public_key.clone();
        let recipient_ipk = recipient_keys.incoming_viewing_public_key.clone();

        let program = Program::token();

        let sender_pre = AccountWithMetadata::new(sender_acc.clone(), true, sender_address);
        let recipient_pre = AccountWithMetadata::new(recipient_acc.clone(), true, &recipient_npk);

        let eph_holder_recipient = EphemeralKeyHolder::new(&recipient_npk);
        let shared_secret_recipient =
            eph_holder_recipient.calculate_shared_secret_sender(&recipient_ipk);

        // Instruction must be: [0x01 || amount (little-endian 16 bytes) || 0x00 || 0x00 || 0x00 || 0x00 || 0x00 || 0x00].
        let mut instruction = [0; 23];
        instruction[0] = 0x01;
        instruction[1..17].copy_from_slice(&amount.to_le_bytes());

        let (output, proof) = circuit::execute_and_prove(
            &[sender_pre, recipient_pre],
            &Program::serialize_instruction(instruction).unwrap(),
            &[0, 1],
            &produce_random_nonces(1),
            &[(recipient_npk.clone(), shared_secret_recipient.clone())],
            &[(
                recipient_keys.private_key_holder.nullifier_secret_key,
                recipient_proof,
            )],
            &program,
        )
        .unwrap();

        let message =
            nssa::privacy_preserving_transaction::message::Message::try_from_circuit_output(
                vec![sender_address],
                vec![sender_acc.nonce],
                vec![(
                    recipient_npk.clone(),
                    recipient_ipk.clone(),
                    eph_holder_recipient.generate_ephemeral_public_key(),
                )],
                output,
            )
            .unwrap();

        let witness_set =
            nssa::privacy_preserving_transaction::witness_set::WitnessSet::for_message(
                &message,
                proof,
                &[sender_priv_key],
            );
        let tx = nssa::PrivacyPreservingTransaction::new(message, witness_set);

        Ok((
            self.sequencer_client.send_tx_private(tx).await?,
            [shared_secret_recipient],
        ))
    }

    pub async fn send_transfer_token_transaction_shielded_owned_account_not_initialized(
        &self,
        sender_address: Address,
        recipient_address: Address,
        amount: u128,
    ) -> Result<(SendTxResponse, [SharedSecretKey; 1]), ExecutionFailureKind> {
        let Ok(sender_acc) = self.get_account_public(sender_address).await else {
            return Err(ExecutionFailureKind::KeyNotFoundError);
        };

        let Some(sender_priv_key) = self
            .storage
            .user_data
            .get_pub_account_signing_key(&sender_address)
        else {
            return Err(ExecutionFailureKind::KeyNotFoundError);
        };

        let Some((recipient_keys, recipient_acc)) = self
            .storage
            .user_data
            .get_private_account(&recipient_address)
            .cloned()
        else {
            return Err(ExecutionFailureKind::KeyNotFoundError);
        };

        let recipient_npk = recipient_keys.nullifer_public_key.clone();
        let recipient_ipk = recipient_keys.incoming_viewing_public_key.clone();

        let program = Program::token();

        let sender_pre = AccountWithMetadata::new(sender_acc.clone(), true, sender_address);
        let recipient_pre = AccountWithMetadata::new(recipient_acc.clone(), false, &recipient_npk);

        let eph_holder_recipient = EphemeralKeyHolder::new(&recipient_npk);
        let shared_secret_recipient =
            eph_holder_recipient.calculate_shared_secret_sender(&recipient_ipk);

        // Instruction must be: [0x01 || amount (little-endian 16 bytes) || 0x00 || 0x00 || 0x00 || 0x00 || 0x00 || 0x00].
        let mut instruction = [0; 23];
        instruction[0] = 0x01;
        instruction[1..17].copy_from_slice(&amount.to_le_bytes());

        let (output, proof) = circuit::execute_and_prove(
            &[sender_pre, recipient_pre],
            &Program::serialize_instruction(instruction).unwrap(),
            &[0, 2],
            &produce_random_nonces(1),
            &[(recipient_npk.clone(), shared_secret_recipient.clone())],
            &[],
            &program,
        )
        .unwrap();

        let message =
            nssa::privacy_preserving_transaction::message::Message::try_from_circuit_output(
                vec![sender_address],
                vec![sender_acc.nonce],
                vec![(
                    recipient_npk.clone(),
                    recipient_ipk.clone(),
                    eph_holder_recipient.generate_ephemeral_public_key(),
                )],
                output,
            )
            .unwrap();

        let witness_set =
            nssa::privacy_preserving_transaction::witness_set::WitnessSet::for_message(
                &message,
                proof,
                &[sender_priv_key],
            );
        let tx = nssa::PrivacyPreservingTransaction::new(message, witness_set);

        Ok((
            self.sequencer_client.send_tx_private(tx).await?,
            [shared_secret_recipient],
        ))
    }

    pub async fn send_transfer_token_transaction_shielded_foreign_account(
        &self,
        sender_address: Address,
        recipient_npk: NullifierPublicKey,
        recipient_ipk: IncomingViewingPublicKey,
        amount: u128,
    ) -> Result<SendTxResponse, ExecutionFailureKind> {
        let Ok(sender_acc) = self.get_account_public(sender_address).await else {
            return Err(ExecutionFailureKind::KeyNotFoundError);
        };

        let Some(sender_priv_key) = self
            .storage
            .user_data
            .get_pub_account_signing_key(&sender_address)
        else {
            return Err(ExecutionFailureKind::KeyNotFoundError);
        };

        let recipient_acc = nssa_core::account::Account::default();

        let program = Program::token();

        let sender_pre = AccountWithMetadata::new(sender_acc.clone(), true, sender_address);
        let recipient_pre = AccountWithMetadata::new(recipient_acc.clone(), false, &recipient_npk);

        let eph_holder_recipient = EphemeralKeyHolder::new(&recipient_npk);
        let shared_secret_recipient =
            eph_holder_recipient.calculate_shared_secret_sender(&recipient_ipk);

        // Instruction must be: [0x01 || amount (little-endian 16 bytes) || 0x00 || 0x00 || 0x00 || 0x00 || 0x00 || 0x00].
        let mut instruction = [0; 23];
        instruction[0] = 0x01;
        instruction[1..17].copy_from_slice(&amount.to_le_bytes());

        let (output, proof) = circuit::execute_and_prove(
            &[sender_pre, recipient_pre],
            &Program::serialize_instruction(instruction).unwrap(),
            &[0, 2],
            &produce_random_nonces(1),
            &[(recipient_npk.clone(), shared_secret_recipient.clone())],
            &[],
            &program,
        )
        .unwrap();

        let message =
            nssa::privacy_preserving_transaction::message::Message::try_from_circuit_output(
                vec![sender_address],
                vec![sender_acc.nonce],
                vec![(
                    recipient_npk.clone(),
                    recipient_ipk.clone(),
                    eph_holder_recipient.generate_ephemeral_public_key(),
                )],
                output,
            )
            .unwrap();

        let witness_set =
            nssa::privacy_preserving_transaction::witness_set::WitnessSet::for_message(
                &message,
                proof,
                &[sender_priv_key],
            );
        let tx = nssa::PrivacyPreservingTransaction::new(message, witness_set);

        Ok(self.sequencer_client.send_tx_private(tx).await?)
    }
}
