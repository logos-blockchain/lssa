use bincode;
use common::merkle_tree_public::merkle_tree::UTXOCommitmentsMerkleTree;
use nssa::Address;
use rand::{thread_rng, RngCore};
use secp256k1_zkp::{CommitmentSecrets, Generator, PedersenCommitment, Tag, Tweak, SECP256K1};
use sha2::{Digest, Sha256};
use utxo::utxo_core::UTXO;

//

use crate::{cryptography::poseidon_hash, public_context::PublicSCContext};

fn hash(input: &[u8]) -> Vec<u8> {
    Sha256::digest(input).to_vec()
}

/// Generate nullifiers
///
/// takes the input_utxo and npk
///
/// returns the nullifiers[i], where the nullifiers[i] = poseidon_hash(in_commitments[i] || npk)
pub fn generate_nullifiers(input_utxo: &UTXO, npk: &[u8]) -> Vec<u8> {
    let commitment = generate_commitment(input_utxo);
    poseidon_hash(&[commitment.as_ref(), npk]).unwrap().to_vec()
}

/// Generate commitment for UTXO
///
///  uses the input_utxo
///
///  returns commitment here commitment is a hash(bincode(input_utxo))
pub fn generate_commitment(input_utxo: &UTXO) -> Vec<u8> {
    let serialized = bincode::serialize(input_utxo).unwrap(); // Serialize UTXO.
    hash(&serialized)
}

/// Generate commitments for UTXO
///
///  uses the input_utxos
///
///  returns commitments
pub fn generate_commitments(input_utxos: &[UTXO]) -> Vec<Vec<u8>> {
    input_utxos
        .iter()
        .map(|utxo| {
            let serialized = bincode::serialize(utxo).unwrap(); // Serialize UTXO.
            hash(&serialized)
        })
        .collect()
}

/// Validate inclusion proof for in_commitments
///
/// ToDo: Solve it in more scalable way
pub fn validate_in_commitments_tree(
    in_commitment: &[u8],
    commitment_tree: &UTXOCommitmentsMerkleTree,
) -> bool {
    let alighned_hash: [u8; 32] = in_commitment.try_into().unwrap();

    commitment_tree.get_proof(alighned_hash).is_some()
}

/// Check, that input utxos balances is equal to out utxo balances
pub fn check_balances_private(in_utxos: &[UTXO], out_utxos: &[UTXO]) -> bool {
    let in_sum = in_utxos.iter().fold(0, |prev, utxo| prev + utxo.amount);
    let out_sum = out_utxos.iter().fold(0, |prev, utxo| prev + utxo.amount);

    in_sum == out_sum
}

pub fn private_circuit(
    input_utxos: &[UTXO],
    output_utxos: &[UTXO],
    public_context: &PublicSCContext,
) -> (Vec<Vec<u8>>, Vec<Vec<u8>>) {
    assert!(check_balances_private(input_utxos, output_utxos));

    let in_commitments = generate_commitments(input_utxos);

    let mut in_nullifiers = vec![];

    for in_utxo in input_utxos {
        let nullifier_public_key = public_context
            .account_masks
            .get(&Address::new(in_utxo.owner))
            .unwrap()
            .nullifier_public_key;

        let key_ser = serde_json::to_vec(&nullifier_public_key).unwrap();

        in_nullifiers.push(generate_nullifiers(in_utxo, &key_ser));
    }

    for in_commitment in in_commitments {
        assert!(validate_in_commitments_tree(
            &in_commitment,
            &public_context.commitments_tree,
        ));
    }

    (in_nullifiers, generate_commitments(output_utxos))
}

/// Check balances DE
///
///  takes the input_utxos[] and output_balance,
///
/// returns the True if the token amount in output_balance matches the sum of all input_utxos[], otherwise return False.
pub fn check_balances_de(input_utxos: &[UTXO], output_balance: u128) -> bool {
    let total_input: u128 = input_utxos.iter().map(|utxo| utxo.amount).sum();
    total_input == output_balance
}

pub fn deshielded_circuit(
    input_utxos: &[UTXO],
    output_balance: u128,
    public_context: &PublicSCContext,
) -> Vec<Vec<u8>> {
    assert!(check_balances_de(input_utxos, output_balance));

    let in_commitments = generate_commitments(input_utxos);

    let mut in_nullifiers = vec![];

    for in_utxo in input_utxos {
        let nullifier_public_key = public_context
            .account_masks
            .get(&Address::new(in_utxo.owner))
            .unwrap()
            .nullifier_public_key;

        let key_ser = serde_json::to_vec(&nullifier_public_key).unwrap();

        in_nullifiers.push(generate_nullifiers(in_utxo, &key_ser));
    }

    for in_commitment in in_commitments {
        assert!(validate_in_commitments_tree(
            &in_commitment,
            &public_context.commitments_tree,
        ));
    }

    in_nullifiers
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

/// new_commitment for a Vec of values
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

/// Verify Pedersen commitment
///
/// takes the public_info, secret_r and pedersen_commitment and
///
/// checks that commitment(public_info,secret_r) is equal pedersen_commitment where the commitment is pedersen commitment.
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

/// Validate inclusion proof for pedersen_commitment
///
/// ToDo: Solve it in more scalable way
pub fn validate_in_commitments_tree_se(
    pedersen_commitment: &PedersenCommitment,
    commitment_tree: &UTXOCommitmentsMerkleTree,
) -> bool {
    let alighned_hash: [u8; 32] = pedersen_commitment.serialize()[0..32].try_into().unwrap();

    commitment_tree.get_proof(alighned_hash).is_some()
}

/// Generate nullifier SE
///
/// takes the pedersen_commitment and npk then
/// returns a nullifier, where the nullifier = poseidon_hash(pedersen_commitment || npk)
pub fn generate_nullifiers_se(pedersen_commitment: &PedersenCommitment, npk: &[u8]) -> Vec<u8> {
    let commitment_ser = pedersen_commitment.serialize().to_vec();

    poseidon_hash(&[&commitment_ser, npk]).unwrap().to_vec()
}

/// Check balances SE
///
///  takes the input_balance and output_utxos[],
///
/// returns the True if the token amount in input_balance matches the sum of all output_utxos[], otherwise return False.
pub fn check_balances_se(input_balance: u128, output_utxos: &[UTXO]) -> bool {
    let total_output: u128 = output_utxos.iter().map(|utxo| utxo.amount).sum();
    total_output == input_balance
}

pub fn shielded_circuit(
    public_info: u64,
    output_utxos: &[UTXO],
    pedersen_commitment: PedersenCommitment,
    secret_r: &[u8],
    public_context: &PublicSCContext,
) -> (Vec<Vec<u8>>, Vec<u8>) {
    assert!(check_balances_se(public_info as u128, output_utxos));

    let out_commitments = generate_commitments(output_utxos);

    let nullifier_public_key = public_context
        .account_masks
        .get(&public_context.caller_address)
        .unwrap()
        .nullifier_public_key;

    let key_ser = serde_json::to_vec(&nullifier_public_key).unwrap();

    let nullifier = generate_nullifiers_se(&pedersen_commitment, &key_ser);

    assert!(validate_in_commitments_tree_se(
        &pedersen_commitment,
        &public_context.commitments_tree,
    ));

    assert!(verify_commitment(
        public_info,
        secret_r,
        &pedersen_commitment
    ));

    (out_commitments, nullifier)
}
