use std::collections::{HashMap, HashSet};

use risc0_zkvm::{guest::env, serde::to_vec};

use nssa_core::{
    Commitment, CommitmentSetDigest, DUMMY_COMMITMENT_HASH, EncryptionScheme, Nullifier,
    NullifierPublicKey, PrivacyPreservingCircuitInput, PrivacyPreservingCircuitOutput,
    account::{Account, AccountId, AccountWithMetadata},
    compute_digest_for_path,
    encryption::Ciphertext,
    program::{DEFAULT_PROGRAM_ID, MAX_NUMBER_CHAINED_CALLS, ProgramOutput, validate_execution},
};

fn main() {
    let PrivacyPreservingCircuitInput {
        program_outputs,
        visibility_mask,
        private_account_nonces,
        private_account_keys,
        private_account_auth,
        mut program_id,
    } = env::read();

    let mut pre_states: Vec<AccountWithMetadata> = Vec::new();
    let mut state_diff: HashMap<AccountId, Account> = HashMap::new();

    let mut program_output = program_outputs[0].clone();

    for _i in 0..MAX_NUMBER_CHAINED_CALLS {
        // Check that `program_output` is consistent with the execution of the corresponding program.
        // TODO: Program output should contain the instruction data to verify the chain of call si
        // performed correctly.
        env::verify(program_id, &to_vec(&program_output).unwrap()).unwrap();

        // Check that the program is well behaved.
        // See the # Programs section for the definition of the `validate_execution` method.
        if !validate_execution(
            &program_output.pre_states,
            &program_output.post_states,
            program_id,
        ) {
            panic!("Bad behaved program");
        }

        // The invoked program claims the accounts with default program id.
        for post in program_output.post_states.iter_mut() {
            if post.program_owner == DEFAULT_PROGRAM_ID {
                post.program_owner = program_id;
            }
        }

        for (pre, post) in program_output
            .pre_states
            .iter()
            .zip(&program_output.post_states)
        {
            if !state_diff.contains_key(&pre.account_id) {
                pre_states.push(pre.clone());
            }
            state_diff.insert(pre.account_id.clone(), post.clone());
        }

        if let Some(next_chained_call) = &program_output.chained_call {
            program_id = next_chained_call.program_id;

            // // Build post states with metadata for next call
            // let mut post_states_with_metadata = Vec::new();
            // for (pre, post) in program_output
            //     .pre_states
            //     .iter()
            //     .zip(program_output.post_states)
            // {
            //     let mut post_with_metadata = pre.clone();
            //     post_with_metadata.account = post.clone();
            //     post_states_with_metadata.push(post_with_metadata);
            // }

            // input_pre_states = next_chained_call
            //     .account_indices
            //     .iter()
            //     .map(|&i| {
            //         post_states_with_metadata
            //             .get(i)
            //             .ok_or_else(|| NssaError::InvalidInput("Invalid account indices".into()))
            //             .cloned()
            //     })
            //     .collect::<Result<Vec<_>, NssaError>>()?;
        } else {
            break;
        };
    }

    let n_accounts = pre_states.len();
    if visibility_mask.len() != n_accounts {
        panic!("Invalid visibility mask length");
    }

    // These lists will be the public outputs of this circuit
    // and will be populated next.
    let mut public_pre_states: Vec<AccountWithMetadata> = Vec::new();
    let mut public_post_states: Vec<Account> = Vec::new();
    let mut ciphertexts: Vec<Ciphertext> = Vec::new();
    let mut new_commitments: Vec<Commitment> = Vec::new();
    let mut new_nullifiers: Vec<(Nullifier, CommitmentSetDigest)> = Vec::new();

    let mut private_nonces_iter = private_account_nonces.iter();
    let mut private_keys_iter = private_account_keys.iter();
    let mut private_auth_iter = private_account_auth.iter();

    let mut output_index = 0;
    for i in 0..n_accounts {
        match visibility_mask[i] {
            0 => {
                // Public account
                public_pre_states.push(pre_states[i].clone());

                let mut post = state_diff.get(&pre_states[i].account_id).unwrap().clone();
                if pre_states[i].is_authorized {
                    post.nonce += 1;
                }
                if post.program_owner == DEFAULT_PROGRAM_ID {
                    // Claim account
                    post.program_owner = program_id;
                }
                public_post_states.push(post);
            }
            1 | 2 => {
                let new_nonce = private_nonces_iter.next().expect("Missing private nonce");
                let (npk, shared_secret) = private_keys_iter.next().expect("Missing keys");

                if AccountId::from(npk) != pre_states[i].account_id {
                    panic!("AccountId mismatch");
                }

                if visibility_mask[i] == 1 {
                    // Private account with authentication
                    let (nsk, membership_proof) =
                        private_auth_iter.next().expect("Missing private auth");

                    // Verify the nullifier public key
                    let expected_npk = NullifierPublicKey::from(nsk);
                    if &expected_npk != npk {
                        panic!("Nullifier public key mismatch");
                    }

                    // Compute commitment set digest associated with provided auth path
                    let commitment_pre = Commitment::new(npk, &pre_states[i].account);
                    let set_digest = compute_digest_for_path(&commitment_pre, membership_proof);

                    // Check pre_state authorization
                    if !pre_states[i].is_authorized {
                        panic!("Pre-state not authorized");
                    }

                    // Compute update nullifier
                    let nullifier = Nullifier::for_account_update(&commitment_pre, nsk);
                    new_nullifiers.push((nullifier, set_digest));
                } else {
                    if pre_states[i].account != Account::default() {
                        panic!("Found new private account with non default values.");
                    }

                    if pre_states[i].is_authorized {
                        panic!("Found new private account marked as authorized.");
                    }

                    // Compute initialization nullifier
                    let nullifier = Nullifier::for_account_initialization(npk);
                    new_nullifiers.push((nullifier, DUMMY_COMMITMENT_HASH));
                }

                // Update post-state with new nonce
                let mut post_with_updated_values =
                    state_diff.get(&pre_states[i].account_id).unwrap().clone();
                post_with_updated_values.nonce = *new_nonce;

                if post_with_updated_values.program_owner == DEFAULT_PROGRAM_ID {
                    // Claim account
                    post_with_updated_values.program_owner = program_id;
                }

                // Compute commitment
                let commitment_post = Commitment::new(npk, &post_with_updated_values);

                // Encrypt and push post state
                let encrypted_account = EncryptionScheme::encrypt(
                    &post_with_updated_values,
                    shared_secret,
                    &commitment_post,
                    output_index,
                );

                new_commitments.push(commitment_post);
                ciphertexts.push(encrypted_account);
                output_index += 1;
            }
            _ => panic!("Invalid visibility mask value"),
        }
    }

    if private_nonces_iter.next().is_some() {
        panic!("Too many nonces.");
    }

    if private_keys_iter.next().is_some() {
        panic!("Too many private account keys.");
    }

    if private_auth_iter.next().is_some() {
        panic!("Too many private account authentication keys.");
    }

    let output = PrivacyPreservingCircuitOutput {
        public_pre_states,
        public_post_states,
        ciphertexts,
        new_commitments,
        new_nullifiers,
    };

    env::commit(&output);
}
