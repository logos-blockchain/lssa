use k256::Scalar;
use secp256k1_zkp::{compute_adaptive_blinding_factor, verify_commitments_sum_to_equal, CommitmentSecrets, Generator, PedersenCommitment, Tag, Tweak, SECP256K1};
use rand::thread_rng;
use sha2::{Digest, Sha256};
use serde::{Serialize, Deserialize};
use storage::{commitment::Commitment, commitments_sparse_merkle_tree::CommitmentsSparseMerkleTree, nullifier::UTXONullifier, nullifier_sparse_merkle_tree::NullifierSparseMerkleTree};
use utxo::{
    utxo_core::{UTXOPayload, UTXO},
    utxo_tree::UTXOSparseMerkleTree,
};
use monotree::hasher::Blake3;
use monotree::{Hasher, Monotree, Proof};
use bincode;

fn commitment_secrets_random(value: u64) -> CommitmentSecrets {
    CommitmentSecrets {
        value,
        value_blinding_factor: Tweak::new(&mut thread_rng()),
        generator_blinding_factor: Tweak::new(&mut thread_rng()),
    }
}

pub fn tag_random() -> Tag {
    use rand::thread_rng;
    use rand::RngCore;

    let mut bytes = [0u8; 32];
    thread_rng().fill_bytes(&mut bytes);

    Tag::from(bytes)
}

pub fn commit(comm: &CommitmentSecrets, tag: Tag) -> PedersenCommitment {
    let generator = Generator::new_blinded(SECP256K1, tag, comm.generator_blinding_factor);

    PedersenCommitment::new(SECP256K1, comm.value, comm.value_blinding_factor, generator)
}

// Hash function placeholder (replace with your cryptographic library's hash).
fn hash(input: &[u8]) -> Vec<u8> {
    Sha256::digest(input).to_vec()
}

// Generate nullifiers

// takes the pedersen_commitment and nsk then 
// returns a list of nullifiers, where the nullifier = hash(pedersen_commitment || nsk) where the hash function will be determined 

pub fn generate_nullifiers(pedersen_commitment: &PedersenCommitment, nsk: &[u8]) -> Vec<u8> {
    let mut input = pedersen_commitment.serialize().to_vec();
    input.extend_from_slice(nsk);
    hash(&input)
}

// Generate commitments for output UTXOs

// uses the list of output_utxos[] and 
// returns out_commitments[] where each out_commitments[i] = Commitment(output_utxos[i]) 
// where the commitment will be determined
pub fn generate_commitments(output_utxos: &[UTXO]) -> Vec<Vec<u8>> {
    output_utxos
        .iter()
        .map(|utxo| {
            let serialized = bincode::serialize(utxo).unwrap(); // Serialize UTXO.
            hash(&serialized)
        })
        .collect()
}

