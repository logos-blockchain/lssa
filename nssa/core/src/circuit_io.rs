use serde::{Deserialize, Serialize};

use crate::{
    Commitment, CommitmentSetDigest, MembershipProof, Nullifier, NullifierPublicKey,
    NullifierSecretKey, SharedSecretKey,
    account::{Account, AccountWithMetadata, Nonce},
    encryption::Ciphertext,
    program::{ProgramId, ProgramOutput},
};

#[derive(Serialize, Deserialize)]
pub struct PrivacyPreservingCircuitInput {
    pub program_output: ProgramOutput,
    pub visibility_mask: Vec<u8>,
    pub private_account_nonces: Vec<Nonce>,
    pub private_account_keys: Vec<(NullifierPublicKey, SharedSecretKey)>,
    pub private_account_auth: Vec<(NullifierSecretKey, MembershipProof)>,
    pub program_id: ProgramId,
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(any(feature = "host", test), derive(Debug, PartialEq, Eq))]
pub struct PrivacyPreservingCircuitOutput {
    pub public_pre_states: Vec<AccountWithMetadata>,
    pub public_post_states: Vec<Account>,
    pub ciphertexts: Vec<Ciphertext>,
    pub new_commitments: Vec<Commitment>,
    pub new_nullifiers: Vec<(Nullifier, CommitmentSetDigest)>,
}

#[cfg(feature = "host")]
impl PrivacyPreservingCircuitOutput {
    pub fn to_bytes(&self) -> Vec<u8> {
        bytemuck::cast_slice(&risc0_zkvm::serde::to_vec(&self).unwrap()).to_vec()
    }
}

#[cfg(feature = "host")]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Commitment, Nullifier, NullifierPublicKey,
        account::{Account, AccountWithMetadata},
    };
    use risc0_zkvm::serde::from_slice;

    #[test]
    fn test_privacy_preserving_circuit_output_to_bytes_is_compatible_with_from_slice() {
        let output = PrivacyPreservingCircuitOutput {
            public_pre_states: vec![
                AccountWithMetadata {
                    account: Account {
                        program_owner: [1, 2, 3, 4, 5, 6, 7, 8],
                        balance: 12345678901234567890,
                        data: b"test data".to_vec(),
                        nonce: 18446744073709551614,
                    },
                    is_authorized: true,
                },
                AccountWithMetadata {
                    account: Account {
                        program_owner: [9, 9, 9, 8, 8, 8, 7, 7],
                        balance: 123123123456456567112,
                        data: b"test data".to_vec(),
                        nonce: 9999999999999999999999,
                    },
                    is_authorized: false,
                },
            ],
            public_post_states: vec![Account {
                program_owner: [1, 2, 3, 4, 5, 6, 7, 8],
                balance: 100,
                data: b"post state data".to_vec(),
                nonce: 18446744073709551615,
            }],
            ciphertexts: vec![Ciphertext(vec![255, 255, 1, 1, 2, 2])],
            new_commitments: vec![Commitment::new(
                &NullifierPublicKey::from(&[1; 32]),
                &Account::default(),
            )],
            new_nullifiers: vec![(
                Nullifier::new(
                    &Commitment::new(&NullifierPublicKey::from(&[2; 32]), &Account::default()),
                    &[1; 32],
                ),
                [0xab; 32],
            )],
        };
        let bytes = output.to_bytes();
        let output_from_slice: PrivacyPreservingCircuitOutput = from_slice(&bytes).unwrap();
        assert_eq!(output, output_from_slice);
    }
}
