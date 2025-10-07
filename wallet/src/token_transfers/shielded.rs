use common::{ExecutionFailureKind, sequencer_client::json::SendTxResponse};
use key_protocol::key_management::ephemeral_key_holder::EphemeralKeyHolder;
use nssa::{
    Account, Address, PrivacyPreservingTransaction,
    privacy_preserving_transaction::{circuit, message::Message, witness_set::WitnessSet},
    program::Program,
};
use nssa_core::{
    Commitment, NullifierPublicKey, SharedSecretKey, account::AccountWithMetadata,
    encryption::IncomingViewingPublicKey,
};

use crate::{WalletCore, helperfunctions::produce_random_nonces};

impl WalletCore {
    pub async fn send_shielded_native_token_transfer(
        &self,
        from: Address,
        to: Address,
        balance_to_move: u128,
    ) -> Result<(SendTxResponse, SharedSecretKey), ExecutionFailureKind> {
        let Ok(from_acc) = self.get_account_public(from).await else {
            return Err(ExecutionFailureKind::KeyNotFoundError);
        };

        let Some((to_keys, to_acc)) = self.storage.user_data.get_private_account(&to).cloned()
        else {
            return Err(ExecutionFailureKind::KeyNotFoundError);
        };

        let to_npk = to_keys.nullifer_public_key.clone();
        let to_ipk = to_keys.incoming_viewing_public_key.clone();

        if from_acc.balance >= balance_to_move {
            let program = Program::authenticated_transfer_program();

            let receiver_commitment = Commitment::new(&to_npk, &to_acc);

            let sender_pre = AccountWithMetadata::new(from_acc.clone(), true, from);
            let recipient_pre = AccountWithMetadata::new(to_acc.clone(), true, &to_npk);

            let eph_holder = EphemeralKeyHolder::new(&to_npk);
            let shared_secret = eph_holder.calculate_shared_secret_sender(&to_ipk);

            let (output, proof) = circuit::execute_and_prove(
                &[sender_pre, recipient_pre],
                &nssa::program::Program::serialize_instruction(balance_to_move).unwrap(),
                &[0, 1],
                &produce_random_nonces(1),
                &[(to_npk.clone(), shared_secret.clone())],
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

            let message = Message::try_from_circuit_output(
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

            let witness_set = WitnessSet::for_message(&message, proof, &[signing_key]);

            let tx = PrivacyPreservingTransaction::new(message, witness_set);

            Ok((
                self.sequencer_client.send_tx_private(tx).await?,
                shared_secret,
            ))
        } else {
            Err(ExecutionFailureKind::InsufficientFundsError)
        }
    }

    pub async fn send_shielded_native_token_transfer_outer_account(
        &self,
        from: Address,
        to_npk: NullifierPublicKey,
        to_ipk: IncomingViewingPublicKey,
        balance_to_move: u128,
    ) -> Result<(SendTxResponse, SharedSecretKey), ExecutionFailureKind> {
        let Ok(from_acc) = self.get_account_public(from).await else {
            return Err(ExecutionFailureKind::KeyNotFoundError);
        };

        let to_acc = Account::default();

        if from_acc.balance >= balance_to_move {
            let program = Program::authenticated_transfer_program();

            let sender_pre = AccountWithMetadata::new(from_acc.clone(), true, from);
            let recipient_pre = AccountWithMetadata::new(to_acc.clone(), false, &to_npk);

            let eph_holder = EphemeralKeyHolder::new(&to_npk);
            let shared_secret = eph_holder.calculate_shared_secret_sender(&to_ipk);

            let (output, proof) = circuit::execute_and_prove(
                &[sender_pre, recipient_pre],
                &Program::serialize_instruction(balance_to_move).unwrap(),
                &[0, 2],
                &produce_random_nonces(1),
                &[(to_npk.clone(), shared_secret.clone())],
                &[],
                &program,
            )
            .unwrap();

            let message = Message::try_from_circuit_output(
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

            let witness_set = WitnessSet::for_message(&message, proof, &[signing_key]);
            let tx = PrivacyPreservingTransaction::new(message, witness_set);

            Ok((
                self.sequencer_client.send_tx_private(tx).await?,
                shared_secret,
            ))
        } else {
            Err(ExecutionFailureKind::InsufficientFundsError)
        }
    }
}
