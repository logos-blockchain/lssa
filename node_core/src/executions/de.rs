use bincode;
use common::{
    commitment::Commitment, commitments_sparse_merkle_tree::CommitmentsSparseMerkleTree,
    nullifier::UTXONullifier, nullifier_sparse_merkle_tree::NullifierSparseMerkleTree,
};
use k256::Scalar;
use monotree::hasher::Blake3;
use monotree::{Hasher, Monotree};
use rand::thread_rng;
use secp256k1_zkp::{CommitmentSecrets, Generator, PedersenCommitment, Tag, Tweak, SECP256K1};
use sha2::{Digest, Sha256};
use utxo::utxo_core::UTXO;

#[allow(unused)]
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

// Check balances

//  takes the public_info and output_utxos[],
// returns the True if the token amount in public_info matches the sum of all output_utxos[], otherwise return False.
pub fn check_balances(public_info: u128, output_utxos: &[UTXO]) -> bool {
    let total_output: u128 = output_utxos.iter().map(|utxo| utxo.amount).sum();
    public_info == total_output
}

// Verify Pedersen commitment

// takes the public_info, secret_r and pedersen_commitment and
// checks that commitment(public_info,secret_r) is equal pedersen_commitment where the commitment is pedersen commitment.
pub fn verify_commitment(
    public_info: u64,
    secret_r: &[u8],
    pedersen_commitment: &PedersenCommitment,
) -> bool {
    let commitment_secrets = CommitmentSecrets {
        value: public_info,
        value_blinding_factor: Tweak::from_slice(secret_r).unwrap(),
        generator_blinding_factor: Tweak::new(&mut thread_rng()),
    };

    let tag = tag_random();
    let commitment = commit(&commitment_secrets, tag);

    commitment == *pedersen_commitment
}

// new_commitment
pub fn new_commitment(public_info: u64, secret_r: &[u8]) -> (Tweak, &[u8], PedersenCommitment) {
    let generator_blinding_factor = Tweak::new(&mut thread_rng());
    let commitment_secrets = CommitmentSecrets {
        value: public_info,
        value_blinding_factor: Tweak::from_slice(secret_r).unwrap(),
        generator_blinding_factor,
    };

    let tag = tag_random();
    let commitment = commit(&commitment_secrets, tag);

    (generator_blinding_factor, secret_r, commitment)
}

// new_commitment for a Vec of values
pub fn new_commitment_vec(
    public_info_vec: Vec<u64>,
    secret_r: &[u8],
) -> (Tweak, &[u8], Vec<PedersenCommitment>) {
    let generator_blinding_factor = Tweak::new(&mut thread_rng());
    let tag = tag_random();

    let vec_commitments = public_info_vec
        .into_iter()
        .map(|public_info| {
            let commitment_secrets = CommitmentSecrets {
                value: public_info,
                value_blinding_factor: Tweak::from_slice(secret_r).unwrap(),
                generator_blinding_factor,
            };

            commit(&commitment_secrets, tag)
        })
        .collect();

    (generator_blinding_factor, secret_r, vec_commitments)
}

#[allow(unused)]
fn de_kernel(
    root_commitment: &[u8],
    root_nullifier: [u8; 32],
    public_info: u64,
    input_utxos: &[UTXO],
    in_commitments_proof: &[Vec<u8>],
    nullifiers_proof: &[[u8; 32]],
    nullifier_secret_key: Scalar,
) -> (Vec<u8>, Vec<Vec<u8>>) {
    check_balances(public_info as u128, input_utxos);

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
