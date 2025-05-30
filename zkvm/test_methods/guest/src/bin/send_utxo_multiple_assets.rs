use risc0_zkvm::{
    guest::env,
};
use serde::{Deserialize, Serialize};

type AccountAddr = [u8; 32];

#[derive(Clone, Serialize, Deserialize)]
pub struct UTXOPayload {
    pub owner: AccountAddr,
    pub asset: Vec<u8>,
    // TODO: change to u256
    pub amount: u128,
    pub privacy_flag: bool,
    pub randomness: [u8; 32],
}

fn main() {
    let utxo_spent: Vec<UTXOPayload> = env::read();
    let number_to_send = env::read();
    let receiver: AccountAddr = env::read();

    let mut utxo_received = vec![];
    let mut utxo_not_spent = vec![];

    for i in 0..utxo_spent.len() {
        let mut utxo_payload = utxo_spent[i].clone();

        if i < number_to_send {
            utxo_payload.owner = receiver;

            utxo_received.push(utxo_payload);
        } else {
            utxo_payload.asset.push(0);

            utxo_not_spent.push(utxo_payload);
        }
    }

    env::commit(&(utxo_received, utxo_not_spent));
}
