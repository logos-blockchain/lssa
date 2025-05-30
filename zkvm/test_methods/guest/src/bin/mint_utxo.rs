use risc0_zkvm::{
    guest::env,
};
use serde::{Deserialize, Serialize};

type AccountAddr = [u8; 32];

#[derive(Serialize, Deserialize)]
pub struct UTXOPayload {
    pub owner: AccountAddr,
    pub asset: Vec<u8>,
    // TODO: change to u256
    pub amount: u128,
    pub privacy_flag: bool,
    pub randomness: [u8; 32],
}

fn main() {
    let amount_to_mint: u128 = env::read();
    let owner: AccountAddr = env::read();
    let randomness: [u8; 32] = env::read();

    let payload = UTXOPayload {
        owner,
        asset: vec![],
        amount: amount_to_mint,
        privacy_flag: true,
        randomness,
    };

    env::commit(&(payload));
}
