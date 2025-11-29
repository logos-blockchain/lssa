use common::{error::ExecutionFailureKind, sequencer_client::json::SendTxResponse};
use key_protocol::key_management::ephemeral_key_holder::EphemeralKeyHolder;
use nssa::{
    AccountId, PrivacyPreservingTransaction,
    privacy_preserving_transaction::{circuit, message::Message, witness_set::WitnessSet},
    program::Program,
};
use nssa_core::{
    MembershipProof, NullifierPublicKey, NullifierSecretKey, SharedSecretKey,
    account::AccountWithMetadata, encryption::IncomingViewingPublicKey,
};

use crate::{WalletCore, helperfunctions::produce_random_nonces};

pub(crate) struct AccountPreparedData {
    pub nsk: Option<NullifierSecretKey>,
    pub npk: NullifierPublicKey,
    pub ipk: IncomingViewingPublicKey,
    pub auth_acc: AccountWithMetadata,
    pub proof: Option<MembershipProof>,
}

impl WalletCore {
    pub(crate) async fn private_acc_preparation(
        &self,
        account_id: AccountId,
        is_authorized: bool,
        needs_proof: bool,
    ) -> Result<AccountPreparedData, ExecutionFailureKind> {
        let Some((from_keys, from_acc)) = self
            .storage
            .user_data
            .get_private_account(&account_id)
            .cloned()
        else {
            return Err(ExecutionFailureKind::KeyNotFoundError);
        };

        let mut nsk = None;
        let mut proof = None;

        let from_npk = from_keys.nullifer_public_key;
        let from_ipk = from_keys.incoming_viewing_public_key;

        let sender_pre = AccountWithMetadata::new(from_acc.clone(), is_authorized, &from_npk);

        if is_authorized {
            nsk = Some(from_keys.private_key_holder.nullifier_secret_key);
        }

        if needs_proof {
            // TODO: Remove this unwrap, error types must be compatible
            proof = self
                .check_private_account_initialized(&account_id)
                .await
                .unwrap();
        }

        Ok(AccountPreparedData {
            nsk,
            npk: from_npk,
            ipk: from_ipk,
            auth_acc: sender_pre,
            proof,
        })
    }

    // TODO: Remove
    pub async fn register_account_under_authenticated_transfers_programs_private(
        &self,
        from: AccountId,
    ) -> Result<(SendTxResponse, [SharedSecretKey; 1]), ExecutionFailureKind> {
        let AccountPreparedData {
            nsk: _,
            npk: from_npk,
            ipk: from_ipk,
            auth_acc: sender_pre,
            proof: _,
        } = self.private_acc_preparation(from, false, false).await?;

        let eph_holder_from = EphemeralKeyHolder::new(&from_npk);
        let shared_secret_from = eph_holder_from.calculate_shared_secret_sender(&from_ipk);

        let instruction: u128 = 0;

        let (output, proof) = circuit::execute_and_prove(
            &[sender_pre],
            &Program::serialize_instruction(instruction).unwrap(),
            &[2],
            &produce_random_nonces(1),
            &[(from_npk.clone(), shared_secret_from.clone())],
            &[],
            &Program::authenticated_transfer_program(),
        )
        .unwrap();

        let message = Message::try_from_circuit_output(
            vec![],
            vec![],
            vec![(
                from_npk.clone(),
                from_ipk.clone(),
                eph_holder_from.generate_ephemeral_public_key(),
            )],
            output,
        )
        .unwrap();

        let witness_set = WitnessSet::for_message(&message, proof, &[]);
        let tx = PrivacyPreservingTransaction::new(message, witness_set);

        Ok((
            self.sequencer_client.send_tx_private(tx).await?,
            [shared_secret_from],
        ))
    }
}
