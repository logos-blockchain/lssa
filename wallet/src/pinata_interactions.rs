use common::{ExecutionFailureKind, sequencer_client::json::SendTxResponse};
use key_protocol::key_management::ephemeral_key_holder::EphemeralKeyHolder;
use nssa::{Address, privacy_preserving_transaction::circuit};
use nssa_core::{MembershipProof, SharedSecretKey, account::AccountWithMetadata};

use crate::{WalletCore, helperfunctions::produce_random_nonces};

impl WalletCore {
    pub async fn claim_pinata(
        &self,
        pinata_addr: Address,
        winner_addr: Address,
        solution: u128,
    ) -> Result<SendTxResponse, ExecutionFailureKind> {
        let addresses = vec![pinata_addr, winner_addr];
        let program_id = nssa::program::Program::pinata().id();
        let message =
            nssa::public_transaction::Message::try_new(program_id, addresses, vec![], solution)
                .unwrap();

        let witness_set = nssa::public_transaction::WitnessSet::for_message(&message, &[]);
        let tx = nssa::PublicTransaction::new(message, witness_set);

        Ok(self.sequencer_client.send_tx_public(tx).await?)
    }

    pub async fn claim_pinata_private_owned_account_already_initialized(
        &self,
        pinata_addr: Address,
        winner_addr: Address,
        solution: u128,
        winner_proof: MembershipProof,
    ) -> Result<(SendTxResponse, [SharedSecretKey; 1]), ExecutionFailureKind> {
        let Some((winner_keys, winner_acc)) = self
            .storage
            .user_data
            .get_private_account(&winner_addr)
            .cloned()
        else {
            return Err(ExecutionFailureKind::KeyNotFoundError);
        };

        let pinata_acc = self.get_account_public(pinata_addr).await.unwrap();

        let winner_npk = winner_keys.nullifer_public_key;
        let winner_ipk = winner_keys.incoming_viewing_public_key;

        let program = nssa::program::Program::pinata();

        let pinata_pre = AccountWithMetadata::new(pinata_acc.clone(), false, pinata_addr);
        let winner_pre = AccountWithMetadata::new(winner_acc.clone(), true, &winner_npk);

        let eph_holder_winner = EphemeralKeyHolder::new(&winner_npk);
        let shared_secret_winner = eph_holder_winner.calculate_shared_secret_sender(&winner_ipk);

        let (output, proof) = circuit::execute_and_prove(
            &[pinata_pre, winner_pre],
            &nssa::program::Program::serialize_instruction(solution).unwrap(),
            &[0, 1],
            &produce_random_nonces(1),
            &[(winner_npk.clone(), shared_secret_winner.clone())],
            &[(
                winner_keys.private_key_holder.nullifier_secret_key,
                winner_proof,
            )],
            &program,
        )
        .unwrap();

        let message =
            nssa::privacy_preserving_transaction::message::Message::try_from_circuit_output(
                vec![pinata_addr],
                vec![],
                vec![(
                    winner_npk.clone(),
                    winner_ipk.clone(),
                    eph_holder_winner.generate_ephemeral_public_key(),
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

        Ok((
            self.sequencer_client.send_tx_private(tx).await?,
            [shared_secret_winner],
        ))
    }

    pub async fn claim_pinata_private_owned_account_not_initialized(
        &self,
        pinata_addr: Address,
        winner_addr: Address,
        solution: u128,
    ) -> Result<(SendTxResponse, [SharedSecretKey; 1]), ExecutionFailureKind> {
        let Some((winner_keys, winner_acc)) = self
            .storage
            .user_data
            .get_private_account(&winner_addr)
            .cloned()
        else {
            return Err(ExecutionFailureKind::KeyNotFoundError);
        };

        let pinata_acc = self.get_account_public(pinata_addr).await.unwrap();

        let winner_npk = winner_keys.nullifer_public_key;
        let winner_ipk = winner_keys.incoming_viewing_public_key;

        let program = nssa::program::Program::pinata();

        let pinata_pre = AccountWithMetadata::new(pinata_acc.clone(), false, pinata_addr);
        let winner_pre = AccountWithMetadata::new(winner_acc.clone(), false, &winner_npk);

        let eph_holder_winner = EphemeralKeyHolder::new(&winner_npk);
        let shared_secret_winner = eph_holder_winner.calculate_shared_secret_sender(&winner_ipk);

        let (output, proof) = circuit::execute_and_prove(
            &[pinata_pre, winner_pre],
            &nssa::program::Program::serialize_instruction(solution).unwrap(),
            &[0, 2],
            &produce_random_nonces(1),
            &[(winner_npk.clone(), shared_secret_winner.clone())],
            &[],
            &program,
        )
        .unwrap();

        let message =
            nssa::privacy_preserving_transaction::message::Message::try_from_circuit_output(
                vec![pinata_addr],
                vec![],
                vec![(
                    winner_npk.clone(),
                    winner_ipk.clone(),
                    eph_holder_winner.generate_ephemeral_public_key(),
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

        Ok((
            self.sequencer_client.send_tx_private(tx).await?,
            [shared_secret_winner],
        ))
    }
}
