use risc0_zkvm::{guest::env, serde::to_vec};

use nssa_core::{
    account::{Account, AccountWithMetadata, Commitment, Nullifier, NullifierPublicKey},
    program::{validate_execution, ProgramOutput, DEFAULT_PROGRAM_ID},
    verify_membership_proof, EncryptedAccountData, EphemeralPublicKey, EphemeralSecretKey,
    IncomingViewingPublicKey, PrivacyPreservingCircuitInput, PrivacyPreservingCircuitOutput, Tag,
};

fn main() {
    let PrivacyPreservingCircuitInput {
        program_output,
        visibility_mask,
        private_account_nonces,
        private_account_keys,
        private_account_auth,
        program_id,
        commitment_set_digest,
    } = env::read();

    // TODO: Check that `program_execution_proof` is one of the allowed built-in programs
    // assert!(BUILTIN_PROGRAM_IDS.contains(executing_program_id));

    // Check that `program_output` is consistent with the execution of the corresponding program.
    env::verify(program_id, &to_vec(&program_output).unwrap()).unwrap();

    let ProgramOutput {
        pre_states,
        post_states,
    } = program_output;

    // Check that the program is well behaved.
    // See the # Programs section for the definition of the `validate_execution` method.
    validate_execution(&pre_states, &post_states, program_id);

    let n_accounts = pre_states.len();
    if visibility_mask.len() != n_accounts {
        panic!();
    }

    // These lists will be the public outputs of this circuit
    // and will be populated next.
    let mut public_pre_states: Vec<AccountWithMetadata> = Vec::new();
    let mut public_post_states: Vec<Account> = Vec::new();
    let mut encrypted_private_post_states: Vec<EncryptedAccountData> = Vec::new();
    let mut new_commitments: Vec<Commitment> = Vec::new();
    let mut new_nullifiers: Vec<Nullifier> = Vec::new();

    let mut private_nonces_iter = private_account_nonces.iter();
    let mut private_keys_iter = private_account_keys.iter();
    let mut private_auth_iter = private_account_auth.iter();

    for i in 0..n_accounts {
        match visibility_mask[i] {
            0 => {
                // Public account
                public_pre_states.push(pre_states[i].clone());
                public_post_states.push(post_states[i].clone());
            }
            1 | 2 => {
                let new_nonce = private_nonces_iter.next().expect("Missing private nonce");
                let (Npk, Ipk, esk) = private_keys_iter.next().expect("Missing private keys");

                if visibility_mask[i] == 1 {
                    // Private account with authentication
                    let (nsk, membership_proof) =
                        private_auth_iter.next().expect("Missing private auth");

                    // Verify Npk
                    let expected_Npk = NullifierPublicKey::from(nsk);
                    if &expected_Npk != Npk {
                        panic!("Npk mismatch");
                    }

                    // Verify pre-state commitment membership
                    let commitment_pre = Commitment::new(Npk, &pre_states[i].account);
                    if !verify_membership_proof(
                        &commitment_pre,
                        membership_proof,
                        &commitment_set_digest,
                    ) {
                        panic!("Membership proof invalid");
                    }

                    // Check pre_state authorization
                    if !pre_states[i].is_authorized {
                        panic!("Pre-state not authorized");
                    }

                    // Compute nullifier
                    let nullifier = Nullifier::new(&commitment_pre, nsk);
                    new_nullifiers.push(nullifier);
                } else {
                    if pre_states[i].account != Account::default() {
                        panic!("Found new private account with non default values.");
                    }

                    if pre_states[i].is_authorized {
                        panic!("Found new private account marked as authorized.");
                    }
                }

                // Update post-state with new nonce
                let mut post_with_updated_values = post_states[i].clone();
                post_with_updated_values.nonce = *new_nonce;

                if post_with_updated_values.program_owner == DEFAULT_PROGRAM_ID {
                    // Claim account
                    post_with_updated_values.program_owner = program_id;
                }

                // Compute commitment
                let commitment_post = Commitment::new(Npk, &post_with_updated_values);
                new_commitments.push(commitment_post);

                // Encrypt and push post state
                let encrypted_account =
                    EncryptedAccountData::new(&post_with_updated_values, esk, Npk, Ipk);
                encrypted_private_post_states.push(encrypted_account);
            }
            _ => panic!("Invalid visibility mask value"),
        }
    }

    if private_nonces_iter.next().is_some() {
        panic!("Too many nonces.");
    }

    if private_keys_iter.next().is_some() {
        panic!("Too many private accounts keys.");
    }

    if private_auth_iter.next().is_some() {
        panic!("Too many private account authentication keys.");
    }

    let output = PrivacyPreservingCircuitOutput {
        public_pre_states,
        public_post_states,
        encrypted_private_post_states,
        new_commitments,
        new_nullifiers,
        commitment_set_digest,
    };

    env::commit(&output);
}
