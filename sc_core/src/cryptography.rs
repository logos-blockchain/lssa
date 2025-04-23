use ark_bn254::Fr;
use light_poseidon::{Poseidon, PoseidonBytesHasher};

#[allow(unused)]
fn poseidon_hash(inputs: &[&[u8]]) -> anyhow::Result<[u8; 32]> {
    let mut poseidon = Poseidon::<Fr>::new_circom(2).unwrap();

    let hash = poseidon.hash_bytes_be(inputs)?;

    Ok(hash)
}
