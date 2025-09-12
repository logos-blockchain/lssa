use nssa_core::{
    MembershipProof, NullifierPublicKey, NullifierSecretKey, PrivacyPreservingCircuitInput,
    PrivacyPreservingCircuitOutput, SharedSecretKey,
    account::AccountWithMetadata,
    program::{InstructionData, ProgramOutput},
};
use risc0_zkvm::{ExecutorEnv, InnerReceipt, Receipt, default_prover};

use crate::{error::NssaError, program::Program};

use program_methods::{PRIVACY_PRESERVING_CIRCUIT_ELF, PRIVACY_PRESERVING_CIRCUIT_ID};

/// Proof of the privacy preserving execution circuit
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proof(Vec<u8>);

impl Proof {
    pub(crate) fn is_valid_for(&self, circuit_output: &PrivacyPreservingCircuitOutput) -> bool {
        let inner: InnerReceipt = borsh::from_slice(&self.0).unwrap();
        let receipt = Receipt::new(inner, circuit_output.to_bytes());
        receipt.verify(PRIVACY_PRESERVING_CIRCUIT_ID).is_ok()
    }
}

/// Generates a proof of the execution of a NSSA program inside the privacy preserving execution
/// circuit
pub fn execute_and_prove(
    pre_states: &[AccountWithMetadata],
    instruction_data: &InstructionData,
    visibility_mask: &[u8],
    private_account_nonces: &[u128],
    private_account_keys: &[(NullifierPublicKey, SharedSecretKey)],
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
        Commitment, EncryptionScheme, Nullifier,
        account::{Account, AccountWithMetadata, AccountId},
    };

    use crate::{
        privacy_preserving_transaction::circuit::execute_and_prove,
        program::Program,
        state::{
            CommitmentSet,
            tests::{test_private_account_keys_1, test_private_account_keys_2},
        },
    };

    use super::*;

    #[test]
    fn prove_privacy_preserving_execution_circuit_public_and_private_pre_accounts() {
        let recipient_keys = test_private_account_keys_1();
        let program = Program::authenticated_transfer_program();
        let sender = AccountWithMetadata::new(
            Account {
                balance: 100,
                ..Account::default()
            },
            true,
            AccountId::new([0; 32]),
        );

        let recipient = AccountWithMetadata::new(
            Account::default(),
            false,
            AccountId::from(&recipient_keys.npk()),
        );

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

        let esk = [3; 32];
        let shared_secret = SharedSecretKey::new(&esk, &recipient_keys.ivk());

        let (output, proof) = execute_and_prove(
            &[sender, recipient],
            &Program::serialize_instruction(balance_to_move).unwrap(),
            &[0, 2],
            &[0xdeadbeef],
            &[(recipient_keys.npk(), shared_secret.clone())],
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

        let recipient_post = EncryptionScheme::decrypt(
            &output.ciphertexts[0],
            &shared_secret,
            &output.new_commitments[0],
            0,
        )
        .unwrap();
        assert_eq!(recipient_post, expected_recipient_post);
    }

    #[test]
    fn prove_privacy_preserving_execution_circuit_fully_private() {
        let sender_keys = test_private_account_keys_1();
        let recipient_keys = test_private_account_keys_2();

        let sender_pre = AccountWithMetadata::new(
            Account {
                balance: 100,
                nonce: 0xdeadbeef,
                ..Account::default()
            },
            true,
            AccountId::from(&sender_keys.npk()),
        );
        let commitment_sender = Commitment::new(&sender_keys.npk(), &sender_pre.account);

        let recipient = AccountWithMetadata::new(
            Account::default(),
            false,
            AccountId::from(&recipient_keys.npk()),
        );
        let balance_to_move: u128 = 37;

        let mut commitment_set = CommitmentSet::with_capacity(2);
        commitment_set.extend(std::slice::from_ref(&commitment_sender));

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
        let shared_secret_1 = SharedSecretKey::new(&esk_1, &sender_keys.ivk());

        let esk_2 = [5; 32];
        let shared_secret_2 = SharedSecretKey::new(&esk_2, &recipient_keys.ivk());

        let (output, proof) = execute_and_prove(
            &[sender_pre.clone(), recipient],
            &Program::serialize_instruction(balance_to_move).unwrap(),
            &[1, 2],
            &[0xdeadbeef1, 0xdeadbeef2],
            &[
                (sender_keys.npk(), shared_secret_1.clone()),
                (recipient_keys.npk(), shared_secret_2.clone()),
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

        let sender_post = EncryptionScheme::decrypt(
            &output.ciphertexts[0],
            &shared_secret_1,
            &expected_new_commitments[0],
            0,
        )
        .unwrap();
        assert_eq!(sender_post, expected_private_account_1);

        let recipient_post = EncryptionScheme::decrypt(
            &output.ciphertexts[1],
            &shared_secret_2,
            &expected_new_commitments[1],
            1,
        )
        .unwrap();
        assert_eq!(recipient_post, expected_private_account_2);
    }
}
