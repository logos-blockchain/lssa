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
}

fn main() {
    let amount_to_mint: u128 = env::read();
    let number_of_assets: usize = env::read();
    let owner: AccountAddr = env::read();

    let mut asseted_utxos = vec![];

    for i in 0..number_of_assets {
        let payload = UTXOPayload {
            owner,
            asset: vec![i as u8],
            amount: amount_to_mint,
            privacy_flag: true,
        };

        asseted_utxos.push(payload);
    }

    env::commit(&(asseted_utxos));
}
