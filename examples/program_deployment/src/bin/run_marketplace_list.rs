use nssa::{
    AccountId, PublicTransaction,
    program::Program,
    public_transaction::{Message, WitnessSet},
};
use wallet::WalletCore;

// Before running this example, compile the `marketplace.rs` guest program with:
//
//   cargo risczero build --manifest-path examples/program_deployment/methods/guest/Cargo.toml
//
// Note: you must run the above command from the root of the `lssa` repository.
// Note: The compiled binary file is stored in
// methods/guest/target/riscv32im-risc0-zkvm-elf/docker/marketplace.bin
//
//
// Usage:
//   ./run_marketplace_list /path/to/guest/binary <account_id>
//
// Note: the provided account_id needs to be of a public self owned account
//
// Example:
//   cargo run --bin run_marketplace_list \
//      methods/guest/target/riscv32im-risc0-zkvm-elf/docker/marketplace.bin \
//      Ds8q5PjLcKwwV97Zi7duhRVF9uwA2PuYMoLL7FwCzsXE

type Instruction = (u8, Vec<u8>);
const LIST_FUNC_ID: u8 = 0;
const BUY_FUNC_ID: u8 = 1;
const WITHDRAW_FUNC_ID: u8 = 2;

fn serialize_list_instruction(price: u128, unique_string: [u8; 16]) -> Instruction {
    let mut instr_bytes = Vec::with_capacity(32); // 16 + 16
    instr_bytes.extend_from_slice(&price.to_le_bytes()); // 16 bytes
    instr_bytes.extend_from_slice(&unique_string); // 16 bytes
    (LIST_FUNC_ID, instr_bytes) // 0 = WRITE_FUNCTION_ID / list
}

#[tokio::main]
async fn main() {
    // Initialize wallet
    let wallet_core = WalletCore::from_env().unwrap();

    // Parse arguments
    // First argument is the path to the program binary
    let program_path = std::env::args_os().nth(1).unwrap().into_string().unwrap();
    // Second argument is the SIGNER account_id
    let account_id_item: AccountId = std::env::args_os()
        .nth(2)
        .unwrap()
        .into_string()
        .unwrap()
        .parse()
        .unwrap();

    let account_id_seller: AccountId = std::env::args_os()
        .nth(3)
        .unwrap()
        .into_string()
        .unwrap()
        .parse()
        .unwrap();

    // Load the program
    let bytecode: Vec<u8> = std::fs::read(program_path).unwrap();
    let program = Program::new(bytecode).unwrap();

    // Load signing keys to provide authorization
    let signing_key: &nssa::PrivateKey = wallet_core
        .storage()
        .user_data
        .get_pub_account_signing_key(&account_id_seller)
        .expect("Input account should be a self owned public account");

    // hardcoding item value and price for ease of use
    let unique_string: [u8; 16] = *b"UNIQUE_ITEM_1234";
    let price: u128 = 500;
    let instruction: Instruction = serialize_list_instruction(price, unique_string);

    let nonces = wallet_core
        .get_accounts_nonces(vec![account_id_seller])
        .await
        .expect("Node should be reachable to query account data");

    let message: Message = Message::try_new(
        program.id(),
        vec![account_id_item, account_id_seller],
        nonces,
        instruction,
    )
    .unwrap();
    let witness_set = WitnessSet::for_message(&message, &[signing_key]);
    let tx = PublicTransaction::new(message, witness_set);

    // Submit the transaction
    let _response = wallet_core
        .sequencer_client
        .send_tx_public(tx)
        .await
        .unwrap();
    // Pretty-print the response for debugging
    println!("Transaction response: {:#?}", _response);
}
