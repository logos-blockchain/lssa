use common::{ExecutionFailureKind, sequencer_client::json::SendTxResponse};
use key_protocol::key_management::ephemeral_key_holder::EphemeralKeyHolder;
use nssa::{
    Address, PrivacyPreservingTransaction,
    privacy_preserving_transaction::{circuit, message::Message, witness_set::WitnessSet},
    program::Program,
};
use nssa_core::{
    Commitment, NullifierPublicKey, SharedSecretKey, account::AccountWithMetadata,
    encryption::IncomingViewingPublicKey,
};

use crate::{WalletCore, helperfunctions::produce_random_nonces};

impl WalletCore {
    pub async fn send_private_native_token_transfer_outer_account(
        &self,
        from: Address,
        to_npk: NullifierPublicKey,
        to_ipk: IncomingViewingPublicKey,
        balance_to_move: u128,
    ) -> Result<(SendTxResponse, [SharedSecretKey; 2]), ExecutionFailureKind> {
        let Some((from_keys, from_acc)) =
            self.storage.user_data.get_private_account(&from).cloned()
        else {
            return Err(ExecutionFailureKind::KeyNotFoundError);
        };

        let to_acc = nssa_core::account::Account::default();

        if from_acc.balance >= balance_to_move {
            let program = Program::authenticated_transfer_program();

            let from_npk = from_keys.nullifer_public_key;
            let from_ipk = from_keys.incoming_viewing_public_key;

            let sender_commitment = Commitment::new(&from_npk, &from_acc);

            let sender_pre = AccountWithMetadata::new(from_acc.clone(), true, &from_npk);

            let recipient_pre = AccountWithMetadata::new(to_acc.clone(), false, &to_npk);

            let eph_holder = EphemeralKeyHolder::new(&to_npk);

            let shared_secret_from = eph_holder.calculate_shared_secret_sender(&from_ipk);
            let shared_secret_to = eph_holder.calculate_shared_secret_sender(&to_ipk);

            let (output, proof) = circuit::execute_and_prove(
                &[sender_pre, recipient_pre],
                &Program::serialize_instruction(balance_to_move).unwrap(),
                &[1, 2],
                &produce_random_nonces(2),
                &[
                    (from_npk.clone(), shared_secret_from.clone()),
                    (to_npk.clone(), shared_secret_to.clone()),
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

            let message = Message::try_from_circuit_output(
                vec![],
                vec![],
                vec![
                    (
                        from_npk.clone(),
                        from_ipk.clone(),
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

            let witness_set = WitnessSet::for_message(&message, proof, &[]);

            let tx = PrivacyPreservingTransaction::new(message, witness_set);

            Ok((
                self.sequencer_client.send_tx_private(tx).await?,
                [shared_secret_from, shared_secret_to],
            ))
        } else {
            Err(ExecutionFailureKind::InsufficientFundsError)
        }
    }

    pub async fn send_private_native_token_transfer_owned_account(
        &self,
        from: Address,
        to: Address,
        balance_to_move: u128,
    ) -> Result<(SendTxResponse, [SharedSecretKey; 2]), ExecutionFailureKind> {
        let Some((from_keys, from_acc)) =
            self.storage.user_data.get_private_account(&from).cloned()
        else {
            return Err(ExecutionFailureKind::KeyNotFoundError);
        };

        let Some((to_keys, to_acc)) = self.storage.user_data.get_private_account(&to).cloned()
        else {
            return Err(ExecutionFailureKind::KeyNotFoundError);
        };

        let from_npk = from_keys.nullifer_public_key;
        let from_ipk = from_keys.incoming_viewing_public_key;
        let to_npk = to_keys.nullifer_public_key.clone();
        let to_ipk = to_keys.incoming_viewing_public_key.clone();

        if from_acc.balance >= balance_to_move {
            let program = Program::authenticated_transfer_program();

            let sender_commitment = Commitment::new(&from_npk, &from_acc);
            let receiver_commitment = Commitment::new(&to_npk, &to_acc);

            let sender_pre = AccountWithMetadata::new(from_acc.clone(), true, &from_npk);
            let recipient_pre = AccountWithMetadata::new(to_acc.clone(), true, &to_npk);

            let eph_holder_from = EphemeralKeyHolder::new(&from_npk);
            let shared_secret_from = eph_holder_from.calculate_shared_secret_sender(&from_ipk);

            let eph_holder_to = EphemeralKeyHolder::new(&to_npk);
            let shared_secret_to = eph_holder_to.calculate_shared_secret_sender(&to_ipk);

            let (output, proof) = circuit::execute_and_prove(
                &[sender_pre, recipient_pre],
                &Program::serialize_instruction(balance_to_move).unwrap(),
                &[1, 1],
                &produce_random_nonces(2),
                &[
                    (from_npk.clone(), shared_secret_from.clone()),
                    (to_npk.clone(), shared_secret_to.clone()),
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

            let message = Message::try_from_circuit_output(
                vec![],
                vec![],
                vec![
                    (
                        from_npk.clone(),
                        from_ipk.clone(),
                        eph_holder_from.generate_ephemeral_public_key(),
                    ),
                    (
                        to_npk.clone(),
                        to_ipk.clone(),
                        eph_holder_to.generate_ephemeral_public_key(),
                    ),
                ],
                output,
            )
            .unwrap();

            let witness_set = WitnessSet::for_message(&message, proof, &[]);
            let tx = PrivacyPreservingTransaction::new(message, witness_set);

            Ok((
                self.sequencer_client.send_tx_private(tx).await?,
                [shared_secret_from, shared_secret_to],
            ))
        } else {
            Err(ExecutionFailureKind::InsufficientFundsError)
        }
    }
}
