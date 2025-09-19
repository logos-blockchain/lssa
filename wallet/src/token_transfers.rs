use common::{ExecutionFailureKind, sequencer_client::json::SendTxResponse};
use key_protocol::key_management::ephemeral_key_holder::EphemeralKeyHolder;
use nssa::Address;

use crate::WalletCore;

impl WalletCore {
    pub async fn send_public_native_token_transfer(
        &self,
        from: Address,
        to: Address,
        balance_to_move: u128,
    ) -> Result<SendTxResponse, ExecutionFailureKind> {
        let Ok(balance) = self.get_account_balance(from).await else {
            return Err(ExecutionFailureKind::SequencerError);
        };

        if balance >= balance_to_move {
            let Ok(nonces) = self.get_accounts_nonces(vec![from]).await else {
                return Err(ExecutionFailureKind::SequencerError);
            };

            let addresses = vec![from, to];
            let program_id = nssa::program::Program::authenticated_transfer_program().id();
            let message = nssa::public_transaction::Message::try_new(
                program_id,
                addresses,
                nonces,
                balance_to_move,
            )
            .unwrap();

            let signing_key = self.storage.user_data.get_pub_account_signing_key(&from);

            let Some(signing_key) = signing_key else {
                return Err(ExecutionFailureKind::KeyNotFoundError);
            };

            let witness_set =
                nssa::public_transaction::WitnessSet::for_message(&message, &[signing_key]);

            let tx = nssa::PublicTransaction::new(message, witness_set);

            Ok(self.sequencer_client.send_tx_public(tx).await?)
        } else {
            Err(ExecutionFailureKind::InsufficientFundsError)
        }
    }

    pub async fn send_private_native_token_transfer_maybe_outer_account(
        &self,
        from: Address,
        to_npk: nssa_core::NullifierPublicKey,
        to_ipk: nssa_core::encryption::IncomingViewingPublicKey,
        balance_to_move: u128,
        to_acc: Option<nssa_core::account::Account>,
    ) -> Result<SendTxResponse, ExecutionFailureKind> {
        let from_data = self.storage.user_data.get_private_account(&from);

        let Some((from_keys, from_acc)) = from_data else {
            return Err(ExecutionFailureKind::KeyNotFoundError);
        };

        let to_acc = to_acc.unwrap_or_default();

        if from_acc.balance >= balance_to_move {
            let program = nssa::program::Program::authenticated_transfer_program();
            let sender_commitment =
                nssa_core::Commitment::new(&from_keys.nullifer_public_key, from_acc);

            let sender_pre = nssa_core::account::AccountWithMetadata {
                account: from_acc.clone(),
                is_authorized: true,
            };
            let recipient_pre = nssa_core::account::AccountWithMetadata {
                account: to_acc.clone(),
                is_authorized: false,
            };

            let eph_holder = EphemeralKeyHolder::new(
                to_npk.clone(),
                from_keys.private_key_holder.outgoing_viewing_secret_key,
                from_acc.nonce.try_into().unwrap(),
            );

            let shared_secret = eph_holder.calculate_shared_secret_sender(to_ipk.clone());

            let (output, proof) = nssa::privacy_preserving_transaction::circuit::execute_and_prove(
                &[sender_pre, recipient_pre],
                &nssa::program::Program::serialize_instruction(balance_to_move).unwrap(),
                &[1, 2],
                &[from_acc.nonce + 1, to_acc.nonce + 1],
                &[
                    (from_keys.nullifer_public_key.clone(), shared_secret.clone()),
                    (to_npk.clone(), shared_secret),
                ],
                &[(
                    from_keys.private_key_holder.nullifier_secret_key,
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
                            from_keys.nullifer_public_key.clone(),
                            from_keys.incoming_viewing_public_key.clone(),
                            eph_holder.generate_ephemeral_public_key(),
                        ),
                        (
                            to_npk.clone(),
                            to_ipk.clone(),
                            eph_holder.generate_ephemeral_public_key(),
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

            let tx = nssa::privacy_preserving_transaction::PrivacyPreservingTransaction::new(
                message,
                witness_set,
            );

            Ok(self.sequencer_client.send_tx_private(tx).await?)
        } else {
            Err(ExecutionFailureKind::InsufficientFundsError)
        }
    }

    pub async fn send_shielded_native_token_transfer_maybe_outer_account(
        &self,
        from: Address,
        to_npk: nssa_core::NullifierPublicKey,
        to_ipk: nssa_core::encryption::IncomingViewingPublicKey,
        balance_to_move: u128,
        to_acc: Option<nssa_core::account::Account>,
    ) -> Result<SendTxResponse, ExecutionFailureKind> {
        let from_data = self.storage.user_data.get_private_account(&from);

        let Some((from_keys, from_acc)) = from_data else {
            return Err(ExecutionFailureKind::KeyNotFoundError);
        };

        let to_acc = to_acc.unwrap_or_default();

        if from_acc.balance >= balance_to_move {
            let program = nssa::program::Program::authenticated_transfer_program();

            let sender_pre = nssa_core::account::AccountWithMetadata {
                account: from_acc.clone(),
                is_authorized: true,
            };
            let recipient_pre = nssa_core::account::AccountWithMetadata {
                account: to_acc.clone(),
                is_authorized: false,
            };

            let eph_holder = EphemeralKeyHolder::new(
                to_npk.clone(),
                from_keys.private_key_holder.outgoing_viewing_secret_key,
                from_acc.nonce.try_into().unwrap(),
            );

            let shared_secret = eph_holder.calculate_shared_secret_sender(to_ipk.clone());

            let (output, proof) = nssa::privacy_preserving_transaction::circuit::execute_and_prove(
                &[sender_pre, recipient_pre],
                &nssa::program::Program::serialize_instruction(balance_to_move).unwrap(),
                &[0, 2],
                &[to_acc.nonce + 1],
                &[(to_npk.clone(), shared_secret)],
                &[],
                &program,
            )
            .unwrap();

            let message =
                nssa::privacy_preserving_transaction::message::Message::try_from_circuit_output(
                    vec![from],
                    vec![from_acc.nonce],
                    vec![(
                        to_npk.clone(),
                        to_ipk.clone(),
                        eph_holder.generate_ephemeral_public_key(),
                    )],
                    output,
                )
                .unwrap();

            let signing_key = self.storage.user_data.get_pub_account_signing_key(&from);

            let Some(signing_key) = signing_key else {
                return Err(ExecutionFailureKind::KeyNotFoundError);
            };

            let witness_set =
                nssa::privacy_preserving_transaction::witness_set::WitnessSet::for_message(
                    &message,
                    proof,
                    &[signing_key],
                );

            let tx = nssa::privacy_preserving_transaction::PrivacyPreservingTransaction::new(
                message,
                witness_set,
            );

            Ok(self.sequencer_client.send_tx_private(tx).await?)
        } else {
            Err(ExecutionFailureKind::InsufficientFundsError)
        }
    }

    pub async fn send_deshielded_native_token_transfer_maybe_outer_account(
        &self,
        from: Address,
        to_addr: Address,
        to_npk: nssa_core::NullifierPublicKey,
        to_ipk: nssa_core::encryption::IncomingViewingPublicKey,
        balance_to_move: u128,
        to_acc: Option<nssa_core::account::Account>,
    ) -> Result<SendTxResponse, ExecutionFailureKind> {
        let from_data = self.storage.user_data.get_private_account(&from);

        let Some((from_keys, from_acc)) = from_data else {
            return Err(ExecutionFailureKind::KeyNotFoundError);
        };

        let to_acc = to_acc.unwrap_or_default();

        if from_acc.balance >= balance_to_move {
            let program = nssa::program::Program::authenticated_transfer_program();
            let sender_commitment =
                nssa_core::Commitment::new(&from_keys.nullifer_public_key, from_acc);

            let sender_pre = nssa_core::account::AccountWithMetadata {
                account: from_acc.clone(),
                is_authorized: true,
            };
            let recipient_pre = nssa_core::account::AccountWithMetadata {
                account: to_acc.clone(),
                is_authorized: false,
            };

            let eph_holder = EphemeralKeyHolder::new(
                to_npk.clone(),
                from_keys.private_key_holder.outgoing_viewing_secret_key,
                from_acc.nonce.try_into().unwrap(),
            );

            let shared_secret = eph_holder.calculate_shared_secret_sender(to_ipk.clone());

            let (output, proof) = nssa::privacy_preserving_transaction::circuit::execute_and_prove(
                &[sender_pre, recipient_pre],
                &nssa::program::Program::serialize_instruction(balance_to_move).unwrap(),
                &[1, 0],
                &[from_acc.nonce + 1],
                &[(from_keys.nullifer_public_key.clone(), shared_secret.clone())],
                &[(
                    from_keys.private_key_holder.nullifier_secret_key,
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
                    vec![to_addr],
                    vec![],
                    vec![(
                        from_keys.nullifer_public_key.clone(),
                        from_keys.incoming_viewing_public_key.clone(),
                        eph_holder.generate_ephemeral_public_key(),
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

            let tx = nssa::privacy_preserving_transaction::PrivacyPreservingTransaction::new(
                message,
                witness_set,
            );

            Ok(self.sequencer_client.send_tx_private(tx).await?)
        } else {
            Err(ExecutionFailureKind::InsufficientFundsError)
        }
    }

    pub async fn send_private_native_token_transfer(
        &self,
        from: Address,
        to: Address,
        balance_to_move: u128,
    ) -> Result<(SendTxResponse, nssa_core::SharedSecretKey), ExecutionFailureKind> {
        let from_data = self.storage.user_data.get_private_account(&from);
        let to_data = self.storage.user_data.get_private_account(&to);

        let Some((from_keys, from_acc)) = from_data else {
            return Err(ExecutionFailureKind::KeyNotFoundError);
        };

        let Some((to_keys, to_acc)) = to_data else {
            return Err(ExecutionFailureKind::KeyNotFoundError);
        };

        let to_npk = to_keys.nullifer_public_key.clone();
        let to_ipk = to_keys.incoming_viewing_public_key.clone();

        if from_acc.balance >= balance_to_move {
            let program = nssa::program::Program::authenticated_transfer_program();
            let sender_commitment =
                nssa_core::Commitment::new(&from_keys.nullifer_public_key, from_acc);
            let receiver_commitment =
                nssa_core::Commitment::new(&to_keys.nullifer_public_key, to_acc);

            let sender_pre = nssa_core::account::AccountWithMetadata {
                account: from_acc.clone(),
                is_authorized: true,
            };
            let recipient_pre = nssa_core::account::AccountWithMetadata {
                account: to_acc.clone(),
                is_authorized: true,
            };

            let eph_holder = EphemeralKeyHolder::new(
                to_npk.clone(),
                from_keys.private_key_holder.outgoing_viewing_secret_key,
                from_acc.nonce.try_into().unwrap(),
            );

            let shared_secret = eph_holder.calculate_shared_secret_sender(to_ipk.clone());

            let (output, proof) = nssa::privacy_preserving_transaction::circuit::execute_and_prove(
                &[sender_pre, recipient_pre],
                &nssa::program::Program::serialize_instruction(balance_to_move).unwrap(),
                &[1, 1],
                &[from_acc.nonce + 1, to_acc.nonce + 1],
                &[
                    (from_keys.nullifer_public_key.clone(), shared_secret.clone()),
                    (to_npk.clone(), shared_secret.clone()),
                ],
                &[
                    (
                        from_keys.private_key_holder.nullifier_secret_key,
                        self.sequencer_client
                            .get_proof_for_commitment(sender_commitment)
                            .await
                            .unwrap()
                            .unwrap(),
                    ),
                    (
                        to_keys.private_key_holder.nullifier_secret_key,
                        self.sequencer_client
                            .get_proof_for_commitment(receiver_commitment)
                            .await
                            .unwrap()
                            .unwrap(),
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
                            from_keys.nullifer_public_key.clone(),
                            from_keys.incoming_viewing_public_key.clone(),
                            eph_holder.generate_ephemeral_public_key(),
                        ),
                        (
                            to_npk.clone(),
                            to_ipk.clone(),
                            eph_holder.generate_ephemeral_public_key(),
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

            let tx = nssa::privacy_preserving_transaction::PrivacyPreservingTransaction::new(
                message,
                witness_set,
            );

            Ok((
                self.sequencer_client.send_tx_private(tx).await?,
                shared_secret,
            ))
        } else {
            Err(ExecutionFailureKind::InsufficientFundsError)
        }
    }

    pub async fn send_shiedled_native_token_transfer(
        &self,
        from: Address,
        to: Address,
        balance_to_move: u128,
    ) -> Result<SendTxResponse, ExecutionFailureKind> {
        let to_data = self.storage.user_data.get_private_account(&to);

        let Some((to_keys, to_acc)) = to_data else {
            return Err(ExecutionFailureKind::KeyNotFoundError);
        };

        self.send_shielded_native_token_transfer_maybe_outer_account(
            from,
            to_keys.nullifer_public_key.clone(),
            to_keys.incoming_viewing_public_key.clone(),
            balance_to_move,
            Some(to_acc.clone()),
        )
        .await
    }

    pub async fn send_deshielded_native_token_transfer(
        &self,
        from: Address,
        to: Address,
        balance_to_move: u128,
    ) -> Result<SendTxResponse, ExecutionFailureKind> {
        let to_data = self.storage.user_data.get_private_account(&to);

        let Some((to_keys, to_acc)) = to_data else {
            return Err(ExecutionFailureKind::KeyNotFoundError);
        };

        self.send_deshielded_native_token_transfer_maybe_outer_account(
            from,
            to,
            to_keys.nullifer_public_key.clone(),
            to_keys.incoming_viewing_public_key.clone(),
            balance_to_move,
            Some(to_acc.clone()),
        )
        .await
    }
}
