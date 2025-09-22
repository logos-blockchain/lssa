use common::{ExecutionFailureKind, sequencer_client::json::SendTxResponse};
use key_protocol::key_management::ephemeral_key_holder::EphemeralKeyHolder;
use nssa::Address;

use crate::WalletCore;

impl WalletCore {
    pub async fn send_shiedled_native_token_transfer(
        &self,
        from: Address,
        to: Address,
        balance_to_move: u128,
    ) -> Result<SendTxResponse, ExecutionFailureKind> {
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

            let receiver_commitment =
                nssa_core::Commitment::new(&to_keys.nullifer_public_key, to_acc);

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
                &[0, 1],
                &[to_acc.nonce + 1],
                &[(to_npk.clone(), shared_secret)],
                &[(
                    to_keys.private_key_holder.nullifier_secret_key,
                    self.sequencer_client
                        .get_proof_for_commitment(receiver_commitment)
                        .await
                        .unwrap()
                        .unwrap(),
                )],
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

    pub async fn send_shielded_native_token_transfer_maybe_outer_account(
        &self,
        from: Address,
        to_npk: nssa_core::NullifierPublicKey,
        to_ipk: nssa_core::encryption::IncomingViewingPublicKey,
        balance_to_move: u128,
    ) -> Result<SendTxResponse, ExecutionFailureKind> {
        let from_data = self.storage.user_data.get_private_account(&from);

        let Some((from_keys, from_acc)) = from_data else {
            return Err(ExecutionFailureKind::KeyNotFoundError);
        };

        let to_acc = nssa_core::account::Account::default();

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
}
