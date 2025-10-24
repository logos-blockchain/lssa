use common::{error::ExecutionFailureKind, sequencer_client::json::SendTxResponse};
use key_protocol::key_management::ephemeral_key_holder::EphemeralKeyHolder;
use nssa::{
    Account, Address, PrivacyPreservingTransaction,
    privacy_preserving_transaction::{circuit, message::Message, witness_set::WitnessSet},
    program::Program,
};
use nssa_core::{
    Commitment, MembershipProof, NullifierPublicKey, NullifierSecretKey, SharedSecretKey,
    account::AccountWithMetadata, encryption::IncomingViewingPublicKey, program::InstructionData,
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
        addr: Address,
        is_authorized: bool,
        needs_proof: bool,
    ) -> Result<AccountPreparedData, ExecutionFailureKind> {
        let Some((from_keys, from_acc)) =
            self.storage.user_data.get_private_account(&addr).cloned()
        else {
            return Err(ExecutionFailureKind::KeyNotFoundError);
        };

        let mut nsk = None;
        let mut proof = None;

        let from_npk = from_keys.nullifer_public_key;
        let from_ipk = from_keys.incoming_viewing_public_key;

        let sender_commitment = Commitment::new(&from_npk, &from_acc);

        let sender_pre = AccountWithMetadata::new(from_acc.clone(), is_authorized, &from_npk);

        if is_authorized {
            nsk = Some(from_keys.private_key_holder.nullifier_secret_key);
        }

        if needs_proof {
            proof = self
                .sequencer_client
                .get_proof_for_commitment(sender_commitment)
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

    pub(crate) async fn private_tx_two_accs_all_init(
        &self,
        from: Address,
        to: Address,
        instruction_data: InstructionData,
        tx_pre_check: impl FnOnce(&Account, &Account) -> Result<(), ExecutionFailureKind>,
        program: Program,
        to_proof: MembershipProof,
    ) -> Result<(SendTxResponse, [SharedSecretKey; 2]), ExecutionFailureKind> {
        let AccountPreparedData {
            nsk: from_nsk,
            npk: from_npk,
            ipk: from_ipk,
            auth_acc: sender_pre,
            proof: from_proof,
        } = self.private_acc_preparation(from, true, true).await?;

        let AccountPreparedData {
            nsk: to_nsk,
            npk: to_npk,
            ipk: to_ipk,
            auth_acc: recipient_pre,
            proof: _,
        } = self.private_acc_preparation(to, true, false).await?;

        tx_pre_check(&sender_pre.account, &recipient_pre.account)?;

        let eph_holder_from = EphemeralKeyHolder::new(&from_npk);
        let shared_secret_from = eph_holder_from.calculate_shared_secret_sender(&from_ipk);

        let eph_holder_to = EphemeralKeyHolder::new(&to_npk);
        let shared_secret_to = eph_holder_to.calculate_shared_secret_sender(&to_ipk);

        let (output, proof) = circuit::execute_and_prove(
            &[sender_pre, recipient_pre],
            &instruction_data,
            &[1, 1],
            &produce_random_nonces(2),
            &[
                (from_npk.clone(), shared_secret_from.clone()),
                (to_npk.clone(), shared_secret_to.clone()),
            ],
            &[
                (from_nsk.unwrap(), from_proof.unwrap()),
                (to_nsk.unwrap(), to_proof),
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
    }

    pub(crate) async fn private_tx_two_accs_receiver_uninit(
        &self,
        from: Address,
        to: Address,
        instruction_data: InstructionData,
        tx_pre_check: impl FnOnce(&Account, &Account) -> Result<(), ExecutionFailureKind>,
        program: Program,
    ) -> Result<(SendTxResponse, [SharedSecretKey; 2]), ExecutionFailureKind> {
        let AccountPreparedData {
            nsk: from_nsk,
            npk: from_npk,
            ipk: from_ipk,
            auth_acc: sender_pre,
            proof: from_proof,
        } = self.private_acc_preparation(from, true, true).await?;

        let AccountPreparedData {
            nsk: _,
            npk: to_npk,
            ipk: to_ipk,
            auth_acc: recipient_pre,
            proof: _,
        } = self.private_acc_preparation(to, false, false).await?;

        tx_pre_check(&sender_pre.account, &recipient_pre.account)?;

        let eph_holder_from = EphemeralKeyHolder::new(&from_npk);
        let shared_secret_from = eph_holder_from.calculate_shared_secret_sender(&from_ipk);

        let eph_holder_to = EphemeralKeyHolder::new(&to_npk);
        let shared_secret_to = eph_holder_to.calculate_shared_secret_sender(&to_ipk);

        let (output, proof) = circuit::execute_and_prove(
            &[sender_pre, recipient_pre],
            &instruction_data,
            &[1, 2],
            &produce_random_nonces(2),
            &[
                (from_npk.clone(), shared_secret_from.clone()),
                (to_npk.clone(), shared_secret_to.clone()),
            ],
            &[(from_nsk.unwrap(), from_proof.unwrap())],
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
    }

    pub(crate) async fn private_tx_two_accs_receiver_outer(
        &self,
        from: Address,
        to_npk: NullifierPublicKey,
        to_ipk: IncomingViewingPublicKey,
        instruction_data: InstructionData,
        tx_pre_check: impl FnOnce(&Account, &Account) -> Result<(), ExecutionFailureKind>,
        program: Program,
    ) -> Result<(SendTxResponse, [SharedSecretKey; 2]), ExecutionFailureKind> {
        let AccountPreparedData {
            nsk: from_nsk,
            npk: from_npk,
            ipk: from_ipk,
            auth_acc: sender_pre,
            proof: from_proof,
        } = self.private_acc_preparation(from, true, true).await?;

        let to_acc = nssa_core::account::Account::default();

        tx_pre_check(&sender_pre.account, &to_acc)?;

        let recipient_pre = AccountWithMetadata::new(to_acc.clone(), false, &to_npk);

        let eph_holder = EphemeralKeyHolder::new(&to_npk);

        let shared_secret_from = eph_holder.calculate_shared_secret_sender(&from_ipk);
        let shared_secret_to = eph_holder.calculate_shared_secret_sender(&to_ipk);

        let (output, proof) = circuit::execute_and_prove(
            &[sender_pre, recipient_pre],
            &instruction_data,
            &[1, 2],
            &produce_random_nonces(2),
            &[
                (from_npk.clone(), shared_secret_from.clone()),
                (to_npk.clone(), shared_secret_to.clone()),
            ],
            &[(from_nsk.unwrap(), from_proof.unwrap())],
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
    }

    pub(crate) async fn deshielded_tx_two_accs(
        &self,
        from: Address,
        to: Address,
        instruction_data: InstructionData,
        tx_pre_check: impl FnOnce(&Account, &Account) -> Result<(), ExecutionFailureKind>,
        program: Program,
    ) -> Result<(SendTxResponse, [nssa_core::SharedSecretKey; 1]), ExecutionFailureKind> {
        let AccountPreparedData {
            nsk: from_nsk,
            npk: from_npk,
            ipk: from_ipk,
            auth_acc: sender_pre,
            proof: from_proof,
        } = self.private_acc_preparation(from, true, true).await?;

        let Ok(to_acc) = self.get_account_public(to).await else {
            return Err(ExecutionFailureKind::KeyNotFoundError);
        };

        tx_pre_check(&sender_pre.account, &to_acc)?;

        let recipient_pre = AccountWithMetadata::new(to_acc.clone(), false, to);

        let eph_holder = EphemeralKeyHolder::new(&from_npk);
        let shared_secret = eph_holder.calculate_shared_secret_sender(&from_ipk);

        let (output, proof) = circuit::execute_and_prove(
            &[sender_pre, recipient_pre],
            &instruction_data,
            &[1, 0],
            &produce_random_nonces(1),
            &[(from_npk.clone(), shared_secret.clone())],
            &[(from_nsk.unwrap(), from_proof.unwrap())],
            &program,
        )
        .unwrap();

        let message = Message::try_from_circuit_output(
            vec![to],
            vec![],
            vec![(
                from_npk.clone(),
                from_ipk.clone(),
                eph_holder.generate_ephemeral_public_key(),
            )],
            output,
        )
        .unwrap();

        let witness_set = WitnessSet::for_message(&message, proof, &[]);

        let tx = PrivacyPreservingTransaction::new(message, witness_set);

        Ok((
            self.sequencer_client.send_tx_private(tx).await?,
            [shared_secret],
        ))
    }

    pub(crate) async fn shielded_two_accs_all_init(
        &self,
        from: Address,
        to: Address,
        instruction_data: InstructionData,
        tx_pre_check: impl FnOnce(&Account, &Account) -> Result<(), ExecutionFailureKind>,
        program: Program,
        to_proof: MembershipProof,
    ) -> Result<(SendTxResponse, [SharedSecretKey; 1]), ExecutionFailureKind> {
        let Ok(from_acc) = self.get_account_public(from).await else {
            return Err(ExecutionFailureKind::KeyNotFoundError);
        };

        let AccountPreparedData {
            nsk: to_nsk,
            npk: to_npk,
            ipk: to_ipk,
            auth_acc: recipient_pre,
            proof: _,
        } = self.private_acc_preparation(to, true, false).await?;

        tx_pre_check(&from_acc, &recipient_pre.account)?;

        let sender_pre = AccountWithMetadata::new(from_acc.clone(), true, from);

        let eph_holder = EphemeralKeyHolder::new(&to_npk);
        let shared_secret = eph_holder.calculate_shared_secret_sender(&to_ipk);

        let (output, proof) = circuit::execute_and_prove(
            &[sender_pre, recipient_pre],
            &instruction_data,
            &[0, 1],
            &produce_random_nonces(1),
            &[(to_npk.clone(), shared_secret.clone())],
            &[(to_nsk.unwrap(), to_proof)],
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
            [shared_secret],
        ))
    }

    pub(crate) async fn shielded_two_accs_receiver_uninit(
        &self,
        from: Address,
        to: Address,
        instruction_data: InstructionData,
        tx_pre_check: impl FnOnce(&Account, &Account) -> Result<(), ExecutionFailureKind>,
        program: Program,
    ) -> Result<(SendTxResponse, [SharedSecretKey; 1]), ExecutionFailureKind> {
        let Ok(from_acc) = self.get_account_public(from).await else {
            return Err(ExecutionFailureKind::KeyNotFoundError);
        };

        let AccountPreparedData {
            nsk: _,
            npk: to_npk,
            ipk: to_ipk,
            auth_acc: recipient_pre,
            proof: _,
        } = self.private_acc_preparation(to, false, false).await?;

        tx_pre_check(&from_acc, &recipient_pre.account)?;

        let sender_pre = AccountWithMetadata::new(from_acc.clone(), true, from);

        let eph_holder = EphemeralKeyHolder::new(&to_npk);
        let shared_secret = eph_holder.calculate_shared_secret_sender(&to_ipk);

        let (output, proof) = circuit::execute_and_prove(
            &[sender_pre, recipient_pre],
            &instruction_data,
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
            [shared_secret],
        ))
    }

    pub(crate) async fn shielded_two_accs_receiver_outer(
        &self,
        from: Address,
        to_npk: NullifierPublicKey,
        to_ipk: IncomingViewingPublicKey,
        instruction_data: InstructionData,
        tx_pre_check: impl FnOnce(&Account, &Account) -> Result<(), ExecutionFailureKind>,
        program: Program,
    ) -> Result<SendTxResponse, ExecutionFailureKind> {
        let Ok(from_acc) = self.get_account_public(from).await else {
            return Err(ExecutionFailureKind::KeyNotFoundError);
        };

        let to_acc = Account::default();

        tx_pre_check(&from_acc, &to_acc)?;

        let sender_pre = AccountWithMetadata::new(from_acc.clone(), true, from);
        let recipient_pre = AccountWithMetadata::new(to_acc.clone(), false, &to_npk);

        let eph_holder = EphemeralKeyHolder::new(&to_npk);
        let shared_secret = eph_holder.calculate_shared_secret_sender(&to_ipk);

        let (output, proof) = circuit::execute_and_prove(
            &[sender_pre, recipient_pre],
            &instruction_data,
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

        Ok(self.sequencer_client.send_tx_private(tx).await?)
    }
}
