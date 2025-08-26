use nssa_core::{
    CommitmentSetDigest, MembershipProof, PrivacyPreservingCircuitInput,
    PrivacyPreservingCircuitOutput, SharedSecretKey,
    account::{Account, AccountWithMetadata, Nonce, NullifierPublicKey, NullifierSecretKey},
    program::{InstructionData, ProgramId, ProgramOutput},
};
use risc0_zkvm::{ExecutorEnv, InnerReceipt, Receipt, default_prover};

use crate::{error::NssaError, program::Program};

use program_methods::{PRIVACY_PRESERVING_CIRCUIT_ELF, PRIVACY_PRESERVING_CIRCUIT_ID};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proof(Vec<u8>);

impl Proof {
    pub(crate) fn is_valid_for(&self, circuit_output: &PrivacyPreservingCircuitOutput) -> bool {
        let inner: InnerReceipt = borsh::from_slice(&self.0).unwrap();
        let receipt = Receipt::new(inner, circuit_output.to_bytes());
        receipt.verify(PRIVACY_PRESERVING_CIRCUIT_ID).is_ok()
    }
}

pub fn execute_and_prove(
    pre_states: &[AccountWithMetadata],
    instruction_data: &InstructionData,
    visibility_mask: &[u8],
    private_account_nonces: &[u128],
    private_account_keys: &[(
        NullifierPublicKey,
        SharedSecretKey,
        // IncomingViewingPublicKey,
        // EphemeralSecretKey,
    )],
    private_account_auth: &[(NullifierSecretKey, MembershipProof)],
    program: &Program,
) -> Result<(PrivacyPreservingCircuitOutput, Proof), NssaError> {
    let inner_receipt = execute_and_prove_program(program, pre_states, instruction_data)?;

    let program_output: ProgramOutput = inner_receipt
        .journal
        .decode()
        .map_err(|e| NssaError::ProgramOutputDeserializationError(e.to_string()))?;

    let circuit_input = PrivacyPreservingCircuitInput {
        program_output,
        visibility_mask: visibility_mask.to_vec(),
        private_account_nonces: private_account_nonces.to_vec(),
        private_account_keys: private_account_keys.to_vec(),
        private_account_auth: private_account_auth.to_vec(),
        program_id: program.id(),
    };

    // Prove circuit.
    let mut env_builder = ExecutorEnv::builder();
    env_builder.add_assumption(inner_receipt);
    env_builder.write(&circuit_input).unwrap();
    let env = env_builder.build().unwrap();
    let prover = default_prover();
    let prove_info = prover.prove(env, PRIVACY_PRESERVING_CIRCUIT_ELF).unwrap();

    let proof = Proof(borsh::to_vec(&prove_info.receipt.inner)?);

    let circuit_output: PrivacyPreservingCircuitOutput = prove_info
        .receipt
        .journal
        .decode()
        .map_err(|e| NssaError::CircuitOutputDeserializationError(e.to_string()))?;

    Ok((circuit_output, proof))
}

fn execute_and_prove_program(
    program: &Program,
    pre_states: &[AccountWithMetadata],
    instruction_data: &InstructionData,
) -> Result<Receipt, NssaError> {
    // Write inputs to the program
    let mut env_builder = ExecutorEnv::builder();
    Program::write_inputs(pre_states, instruction_data, &mut env_builder)?;
    let env = env_builder.build().unwrap();

    // Prove the program
    let prover = default_prover();
    Ok(prover
        .prove(env, program.elf())
        .map_err(|e| NssaError::ProgramProveFailed(e.to_string()))?
        .receipt)
}

#[cfg(test)]
mod tests {
    use nssa_core::{
        Ciphertext,
        account::{
            Account, AccountWithMetadata, Commitment, Nullifier, NullifierPublicKey,
            NullifierSecretKey,
        },
    };
    use risc0_zkvm::{InnerReceipt, Journal, Receipt};

    use crate::{
        Address, V01State,
        merkle_tree::MerkleTree,
        privacy_preserving_transaction::{
            circuit::{Proof, execute_and_prove},
            message::{EncryptedAccountData, EphemeralPublicKey},
        },
        program::Program,
        state::{
            CommitmentSet,
            tests::{test_private_account_keys_1, test_private_account_keys_2},
        },
    };

    use rand::{Rng, RngCore, rngs::OsRng};

    use super::*;

    #[test]
    fn prove_privacy_preserving_execution_circuit_public_and_private_pre_accounts() {
        let program = Program::authenticated_transfer_program();
        let sender = AccountWithMetadata {
            account: Account {
                balance: 100,
                ..Account::default()
            },
            is_authorized: true,
        };

        let recipient = AccountWithMetadata {
            account: Account::default(),
            is_authorized: false,
        };

        let balance_to_move: u128 = 37;

        let expected_sender_post = Account {
            program_owner: program.id(),
            balance: 100 - balance_to_move,
            nonce: 1,
            data: vec![],
        };

        let expected_recipient_post = Account {
            program_owner: program.id(),
            balance: balance_to_move,
            nonce: 0xdeadbeef,
            data: vec![],
        };

        let expected_sender_pre = sender.clone();
        let recipient_keys = test_private_account_keys_1();

        let esk = [3; 32];
        let shared_secret =
            EncryptedAccountData::compute_shared_secret(&esk, &recipient_keys.ivk());
        let epk = EphemeralPublicKey::from_scalar(esk);

        let (output, proof) = execute_and_prove(
            &[sender, recipient],
            &Program::serialize_instruction(balance_to_move).unwrap(),
            &[0, 2],
            &[0xdeadbeef],
            &[(recipient_keys.npk(), shared_secret)],
            &[],
            &Program::authenticated_transfer_program(),
        )
        .unwrap();

        assert!(proof.is_valid_for(&output));

        let [sender_pre] = output.public_pre_states.try_into().unwrap();
        let [sender_post] = output.public_post_states.try_into().unwrap();
        assert_eq!(sender_pre, expected_sender_pre);
        assert_eq!(sender_post, expected_sender_post);
        assert_eq!(output.new_commitments.len(), 1);
        assert_eq!(output.new_nullifiers.len(), 0);
        assert_eq!(output.ciphertexts.len(), 1);

        let recipient_post = output.ciphertexts[0]
            .clone()
            .decrypt(&shared_secret, &recipient_keys.npk(), 0)
            .unwrap();
        assert_eq!(recipient_post, expected_recipient_post);
    }

    #[test]
    fn prove_privacy_preserving_execution_circuit_fully_private() {
        let sender_pre = AccountWithMetadata {
            account: Account {
                balance: 100,
                nonce: 0xdeadbeef,
                ..Account::default()
            },
            is_authorized: true,
        };
        let sender_keys = test_private_account_keys_1();
        let recipient_keys = test_private_account_keys_2();
        let commitment_sender = Commitment::new(&sender_keys.npk(), &sender_pre.account);

        let recipient = AccountWithMetadata {
            account: Account::default(),
            is_authorized: false,
        };
        let balance_to_move: u128 = 37;

        let mut commitment_set = CommitmentSet::with_capacity(2);
        commitment_set.extend(&[commitment_sender.clone()]);

        let expected_new_nullifiers = vec![(
            Nullifier::new(&commitment_sender, &sender_keys.nsk),
            commitment_set.digest(),
        )];

        let program = Program::authenticated_transfer_program();

        let expected_private_account_1 = Account {
            program_owner: program.id(),
            balance: 100 - balance_to_move,
            nonce: 0xdeadbeef1,
            ..Default::default()
        };
        let expected_private_account_2 = Account {
            program_owner: program.id(),
            balance: balance_to_move,
            nonce: 0xdeadbeef2,
            ..Default::default()
        };
        let expected_new_commitments = vec![
            Commitment::new(&sender_keys.npk(), &expected_private_account_1),
            Commitment::new(&recipient_keys.npk(), &expected_private_account_2),
        ];

        let esk_1 = [3; 32];
        let shared_secret_1 =
            EncryptedAccountData::compute_shared_secret(&esk_1, &sender_keys.ivk());
        let epk_1 = EphemeralPublicKey::from_scalar(esk_1);

        let esk_2 = [5; 32];
        let shared_secret_2 =
            EncryptedAccountData::compute_shared_secret(&esk_2, &recipient_keys.ivk());
        let epk_2 = EphemeralPublicKey::from_scalar(esk_2);

        let (output, proof) = execute_and_prove(
            &[sender_pre.clone(), recipient],
            &Program::serialize_instruction(balance_to_move).unwrap(),
            &[1, 2],
            &[0xdeadbeef1, 0xdeadbeef2],
            &[
                (sender_keys.npk(), shared_secret_1),
                (recipient_keys.npk(), shared_secret_2),
            ],
            &[(
                sender_keys.nsk,
                commitment_set.get_proof_for(&commitment_sender).unwrap(),
            )],
            &program,
        )
        .unwrap();

        assert!(proof.is_valid_for(&output));
        assert!(output.public_pre_states.is_empty());
        assert!(output.public_post_states.is_empty());
        assert_eq!(output.new_commitments, expected_new_commitments);
        assert_eq!(output.new_nullifiers, expected_new_nullifiers);
        assert_eq!(output.ciphertexts.len(), 2);

        let sender_post = output.ciphertexts[0]
            .clone()
            .decrypt(&shared_secret_1, &sender_keys.npk(), 0)
            .unwrap();
        assert_eq!(sender_post, expected_private_account_1);

        let recipient_post = output.ciphertexts[1]
            .clone()
            .decrypt(&shared_secret_2, &recipient_keys.npk(), 1)
            .unwrap();
        assert_eq!(recipient_post, expected_private_account_2);
    }
}
