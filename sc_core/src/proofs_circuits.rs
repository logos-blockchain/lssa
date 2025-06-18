use bincode;
use k256::Scalar;
use rand::{thread_rng, RngCore};
use secp256k1_zkp::{CommitmentSecrets, Generator, PedersenCommitment, Tag, Tweak, SECP256K1};
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
    _in_commitment: &Vec<u8>,
    _root_commitment: Vec<u8>,
    _in_commitments_proof: &[Vec<u8>],
) -> bool {
    // ToDo: Implement correct check

    todo!()
}

// Validate that `nullifier` has not been present in set items before
pub fn validate_nullifier_not_present_in_set_items(
    nullifier: [u8; 32],
    nullifiers_items: &[[u8; 32]],
) -> bool {
    !nullifiers_items.contains(&nullifier)
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
        validate_nullifier_not_present_in_set_items(
            nullifier[0..32].try_into().unwrap(),
            nullifiers_proof,
        );
    }

    (vec![], nullifiers)
}

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

// Check balances

//  takes the public_info and output_utxos[],
// returns the True if the token amount in public_info matches the sum of all output_utxos[], otherwise return False.
pub fn check_balances(public_info: u128, output_utxos: &[UTXO]) -> bool {
    let total_output: u128 = output_utxos.iter().map(|utxo| utxo.amount).sum();
    public_info == total_output
}

// new_commitment for a Vec of values
pub fn pedersen_commitment_vec(
    public_info_vec: Vec<u64>,
) -> (Tweak, [u8; 32], Vec<PedersenCommitment>) {
    let mut random_val: [u8; 32] = [0; 32];
    thread_rng().fill_bytes(&mut random_val);

    let generator_blinding_factor = Tweak::new(&mut thread_rng());
    let tag = tag_random();

    let vec_commitments = public_info_vec
        .into_iter()
        .map(|public_info| {
            let commitment_secrets = CommitmentSecrets {
                value: public_info,
                value_blinding_factor: Tweak::from_slice(&random_val).unwrap(),
                generator_blinding_factor,
            };

            commit(&commitment_secrets, tag)
        })
        .collect();

    (generator_blinding_factor, random_val, vec_commitments)
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
        validate_nullifier_not_present_in_set_items(
            nullifier[0..32].try_into().unwrap(),
            nullifiers_proof,
        );
    }

    (vec![], nullifiers)
}

// Validate inclusion proof for in_commitments

// takes the pedersen_commitment as a leaf, the root hash root_commitment and the path in_commitments_proof[],
// returns True if the pedersen_commitment is in the tree with root hash root_commitment
// otherwise
// returns False, as membership proof.
pub fn validate_in_commitments_proof_se(
    _pedersen_commitment: &PedersenCommitment,
    _root_commitment: Vec<u8>,
    _in_commitments_proof: &[Vec<u8>],
) -> bool {
    // ToDo: Implement correct check

    todo!()
}

// Generate nullifiers SE

// takes the pedersen_commitment and nsk then
// returns a list of nullifiers, where the nullifier = hash(pedersen_commitment || nsk) where the hash function will be determined

pub fn generate_nullifiers_se(pedersen_commitment: &PedersenCommitment, nsk: &[u8]) -> Vec<u8> {
    let mut input = pedersen_commitment.serialize().to_vec();
    input.extend_from_slice(nsk);
    hash(&input)
}

#[allow(unused)]
fn se_kernel(
    root_commitment: &[u8],
    root_nullifier: [u8; 32],
    public_info: u64,
    pedersen_commitment: PedersenCommitment,
    secret_r: &[u8],
    output_utxos: &[UTXO],
    in_commitments_proof: &[Vec<u8>],
    nullifiers_proof: &[[u8; 32]],
    nullifier_secret_key: Scalar,
) -> (Vec<u8>, Vec<Vec<u8>>, Vec<u8>) {
    check_balances(public_info as u128, output_utxos);

    let out_commitments = generate_commitments(output_utxos);

    let nullifier = generate_nullifiers_se(&pedersen_commitment, &nullifier_secret_key.to_bytes());

    validate_in_commitments_proof_se(
        &pedersen_commitment,
        root_commitment.to_vec(),
        in_commitments_proof,
    );

    verify_commitment(public_info, secret_r, &pedersen_commitment);

    (vec![], out_commitments, nullifier)
}
