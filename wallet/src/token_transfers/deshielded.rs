use common::{ExecutionFailureKind, sequencer_client::json::SendTxResponse};
use key_protocol::key_management::ephemeral_key_holder::EphemeralKeyHolder;
use nssa::{
    Address, PrivacyPreservingTransaction,
    privacy_preserving_transaction::{circuit, message::Message, witness_set::WitnessSet},
    program::Program,
};
use nssa_core::{Commitment, account::AccountWithMetadata};

use crate::{WalletCore, helperfunctions::produce_random_nonces};

impl WalletCore {
    pub async fn send_deshielded_native_token_transfer(
        &self,
        from: Address,
        to: Address,
        balance_to_move: u128,
    ) -> Result<(SendTxResponse, nssa_core::SharedSecretKey), ExecutionFailureKind> {
        let Some((from_keys, from_acc)) =
            self.storage.user_data.get_private_account(&from).cloned()
        else {
            return Err(ExecutionFailureKind::KeyNotFoundError);
        };

        let Ok(to_acc) = self.get_account_public(to).await else {
            return Err(ExecutionFailureKind::KeyNotFoundError);
        };

        if from_acc.balance >= balance_to_move {
            let program = Program::authenticated_transfer_program();

            let npk_from = from_keys.nullifer_public_key;
            let ipk_from = from_keys.incoming_viewing_public_key;

            let sender_commitment = Commitment::new(&npk_from, &from_acc);

            let sender_pre = AccountWithMetadata::new(from_acc.clone(), true, &npk_from);
            let recipient_pre = AccountWithMetadata::new(to_acc.clone(), false, to);

            let eph_holder = EphemeralKeyHolder::new(&npk_from);
            let shared_secret = eph_holder.calculate_shared_secret_sender(&ipk_from);

            let (output, proof) = circuit::execute_and_prove(
                &[sender_pre, recipient_pre],
                &Program::serialize_instruction(balance_to_move).unwrap(),
                &[1, 0],
                &produce_random_nonces(1),
                &[(npk_from.clone(), shared_secret.clone())],
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
                vec![to],
                vec![],
                vec![(
                    npk_from.clone(),
                    ipk_from.clone(),
                    eph_holder.generate_ephemeral_public_key(),
                )],
                output,
            )
            .unwrap();

            let witness_set = WitnessSet::for_message(&message, proof, &[]);

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
