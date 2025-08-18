mod encoding;
mod message;
mod transaction;
mod witness_set;

pub use transaction::PrivacyPreservingTransaction;

pub mod circuit {
    use nssa_core::{
        CommitmentSetDigest, EphemeralSecretKey, IncomingViewingPublicKey, MembershipProof,
        PrivacyPreservingCircuitInput, PrivacyPreservingCircuitOutput,
        account::{Account, AccountWithMetadata, Nonce, NullifierPublicKey, NullifierSecretKey},
        program::{InstructionData, ProgramOutput},
    };
    use risc0_zkvm::{ExecutorEnv, Receipt, default_prover};

    use crate::{error::NssaError, program::Program};

    use program_methods::PRIVACY_PRESERVING_CIRCUIT_ELF;

    pub type Proof = Vec<u8>;

    /// Executes and proves the program `P`.
    /// Returns the proof
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

    pub fn prove_privacy_preserving_execution_circuit(
        pre_states: &[AccountWithMetadata],
        instruction_data: &InstructionData,
        private_account_keys: &[(
            NullifierPublicKey,
            IncomingViewingPublicKey,
            EphemeralSecretKey,
        )],
        private_account_auth: Vec<(NullifierSecretKey, MembershipProof)>,
        visibility_mask: &[u8],
        commitment_set_digest: CommitmentSetDigest,
        program: &Program,
    ) -> Result<(Proof, PrivacyPreservingCircuitOutput), NssaError> {
        let inner_receipt = execute_and_prove_program(program, pre_states, instruction_data)?;

        let program_output: ProgramOutput = inner_receipt
            .journal
            .decode()
            .map_err(|e| NssaError::ProgramOutputDeserializationError(e.to_string()))?;

        let private_account_nonces: Vec<_> = (0..private_account_keys.len())
            .map(|_| new_random_nonce())
            .collect();

        let circuit_input = PrivacyPreservingCircuitInput {
            program_output,
            visibility_mask: visibility_mask.to_vec(),
            private_account_nonces: private_account_nonces.to_vec(),
            private_account_keys: private_account_keys.to_vec(),
            private_account_auth: private_account_auth.to_vec(),
            program_id: program.id(),
            commitment_set_digest,
        };

        // Prove circuit.
        let mut env_builder = ExecutorEnv::builder();
        env_builder.add_assumption(inner_receipt);
        env_builder.write(&circuit_input).unwrap();
        let env = env_builder.build().unwrap();
        let prover = default_prover();
        let prove_info = prover.prove(env, PRIVACY_PRESERVING_CIRCUIT_ELF).unwrap();

        let proof = borsh::to_vec(&prove_info.receipt.inner)?;

        let circuit_output: PrivacyPreservingCircuitOutput = prove_info
            .receipt
            .journal
            .decode()
            .map_err(|e| NssaError::CircuitOutputDeserializationError(e.to_string()))?;

        Ok((proof, circuit_output))
    }

    fn new_random_nonce() -> Nonce {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use crate::program::Program;

}
