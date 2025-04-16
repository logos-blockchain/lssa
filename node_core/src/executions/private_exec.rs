use bincode;
use common::{
    commitment::Commitment, commitments_sparse_merkle_tree::CommitmentsSparseMerkleTree,
    nullifier::UTXONullifier, nullifier_sparse_merkle_tree::NullifierSparseMerkleTree,
};
use k256::Scalar;
use monotree::hasher::Blake3;
use monotree::{Hasher, Monotree};
use sha2::{Digest, Sha256};
use utxo::utxo_core::UTXO;

fn hash(input: &[u8]) -> Vec<u8> {
    Sha256::digest(input).to_vec()
}

// Generate nullifiers

// takes the input_utxo and nsk
// returns the nullifiers[i], where the nullifier[i] = hash(in_commitments[i] || nsk) where the hash function
pub fn generate_nullifiers(input_utxo: &UTXO, nsk: &[u8]) -> Vec<u8> {
    let mut input = bincode::serialize(input_utxo).unwrap().to_vec();
    input.extend_from_slice(nsk);
    hash(&input)
}

// Generate commitments for output UTXOs

//  uses the list of input_utxos[]
//  returns in_commitments[] where each in_commitments[i] = Commitment(in_utxos[i]) where the commitment
pub fn generate_commitments(input_utxos: &[UTXO]) -> Vec<Vec<u8>> {
    input_utxos
        .iter()
        .map(|utxo| {
            let serialized = bincode::serialize(utxo).unwrap(); // Serialize UTXO.
            hash(&serialized)
        })
        .collect()
}

// Validate inclusion proof for in_commitments

// takes the in_commitments[i] as a leaf, the root hash root_commitment and the path in_commitments_proofs[i][],
// returns True if the in_commitments[i] is in the tree with root hash root_commitment otherwise returns False, as membership proof.
pub fn validate_in_commitments_proof(
    in_commitment: &Vec<u8>,
    root_commitment: Vec<u8>,
    in_commitments_proof: &[Vec<u8>],
) -> bool {
    // Placeholder implementation.
    // Replace with Merkle proof verification logic.
    // hash(&[pedersen_commitment.serialize().to_vec(), in_commitments_proof.concat()].concat()) == root_commitment

    let mut nsmt = CommitmentsSparseMerkleTree {
        curr_root: Option::Some(root_commitment),
        tree: Monotree::default(),
        hasher: Blake3::new(),
    };

    let commitments: Vec<_> = in_commitments_proof
        .into_iter()
        .map(|n_p| Commitment {
            commitment_hash: n_p.clone(),
        })
        .collect();
    nsmt.insert_items(commitments).unwrap();

    nsmt.get_non_membership_proof(in_commitment.clone())
        .unwrap()
        .1
        .is_some()
}

// Validate non-membership proof for nullifiers

// takes the nullifiers[i], path nullifiers_proof[i][] and the root hash root_nullifier,
// returns True if the nullifiers[i] is not in the tree with root hash root_nullifier otherwise returns False, as non-membership proof.
pub fn validate_nullifiers_proof(
    nullifier: [u8; 32],
    root_nullifier: [u8; 32],
    nullifiers_proof: &[[u8; 32]],
) -> bool {
    let mut nsmt = NullifierSparseMerkleTree {
        curr_root: Option::Some(root_nullifier),
        tree: Monotree::default(),
        hasher: Blake3::new(),
    };

    let nullifiers: Vec<_> = nullifiers_proof
        .into_iter()
        .map(|n_p| UTXONullifier { utxo_hash: *n_p })
        .collect();
    nsmt.insert_items(nullifiers).unwrap();

    nsmt.get_non_membership_proof(nullifier)
        .unwrap()
        .1
        .is_none()
}

#[allow(unused)]
fn private_kernel(
    root_commitment: &[u8],
    root_nullifier: [u8; 32],
    input_utxos: &[UTXO],
    in_commitments_proof: &[Vec<u8>],
    nullifiers_proof: &[[u8; 32]],
    nullifier_secret_key: Scalar,
) -> (Vec<u8>, Vec<Vec<u8>>) {
    let nullifiers: Vec<_> = input_utxos
        .into_iter()
        .map(|utxo| generate_nullifiers(&utxo, &nullifier_secret_key.to_bytes()))
        .collect();

    let in_commitments = generate_commitments(&input_utxos);

    for in_commitment in in_commitments {
        validate_in_commitments_proof(
            &in_commitment,
            root_commitment.to_vec(),
            in_commitments_proof,
        );
    }

    for nullifier in nullifiers.iter() {
        validate_nullifiers_proof(
            nullifier[0..32].try_into().unwrap(),
            root_nullifier,
            nullifiers_proof,
        );
    }

    (vec![], nullifiers)
}
