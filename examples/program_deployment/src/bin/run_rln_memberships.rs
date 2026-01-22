use clap::{Parser, ValueEnum};
use nssa::{
    AccountId, PublicTransaction,
    program::Program,
    public_transaction::{Message, WitnessSet},
};
use rln::protocol::seeded_keygen;
use rln::utils::fr_to_bytes_le;
use wallet::WalletCore;

// Before running this example, compile the `rln_memberships.rs` guest program with:
//
//   cargo risczero build --manifest-path examples/program_deployment/methods/guest/Cargo.toml
//
// Note: you must run the above command from the root of the `lssa` repository.
// Note: The compiled binary file is stored in
// target/riscv32im-risc0-zkvm-elf/docker/rln_memberships.bin
//
//
// Usage:
//   cargo run --bin run_rln_memberships -- \
//       <program_binary> \
//       [--account-id <account_id>] \
//       [--hash <zerokit|light|jellyfish>] \
//       [--iterations <count>] \
//       [--user-limit <hex>]
//
// Note: if account_id is not provided, a new public account will be created
// Note: the provided account_id needs to be of a public self owned account
// Note: the identity commitment is derived from the account's signing key using seeded_keygen
//
// Example (with existing account):
//   cargo run --bin run_rln_memberships -- \
//      target/riscv32im-risc0-zkvm-elf/docker/rln_memberships.bin \
//      --account-id Ds8q5PjLcKwwV97Zi7duhRVF9uwA2PuYMoLL7FwCzsXE \
//      --hash jellyfish --iterations 50
//
// Example (create new account):
//   cargo run --bin run_rln_memberships -- \
//      target/riscv32im-risc0-zkvm-elf/docker/rln_memberships.bin \
//      --hash light --iterations 20

/// Hash implementation selector (must match guest program's HashImpl enum)
#[derive(Debug, Clone, Copy, ValueEnum)]
enum HashImplArg {
    /// Zerokit's poseidon hash (BN254 curve)
    Zerokit,
    /// Light-poseidon hash (BN254 curve)
    Light,
    /// Jellyfish poseidon2 hash (BN254 curve)
    Jellyfish,
}

impl HashImplArg {
    fn to_u8(self) -> u8 {
        match self {
            HashImplArg::Zerokit => 0,
            HashImplArg::Light => 1,
            HashImplArg::Jellyfish => 2,
        }
    }
}

#[derive(Parser, Debug)]
#[command(name = "run_rln_memberships")]
#[command(about = "Run the RLN memberships program with configurable hash function")]
struct Args {
    /// Path to the compiled guest program binary
    program_binary: String,

    /// Account ID (must be a self-owned public account). If not provided, a new account will be created.
    #[arg(long)]
    account_id: Option<String>,

    /// Hash implementation to use
    #[arg(long, default_value = "light")]
    hash: HashImplArg,

    /// Number of hash iterations
    #[arg(long, default_value = "20")]
    iterations: u16,

    /// User message limit as hex (32 bytes), defaults to 300
    #[arg(long)]
    user_limit: Option<String>,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    // Initialize wallet (mutable for potential account creation)
    let mut wallet_core = WalletCore::from_env().unwrap();

    // Get or create account_id
    let account_id: AccountId = match args.account_id {
        Some(id_str) => {
            let id: AccountId = id_str.parse().expect("Invalid account ID format");
            println!("Using existing account: {}", id);
            id
        }
        None => {
            // Create a new public account
            let (new_account_id, chain_index) = wallet_core.create_new_account_public(None);
            println!("Created new public account: {} at path {}", new_account_id, chain_index);

            // Persist the new account
            wallet_core
                .store_persistent_data()
                .await
                .expect("Failed to store wallet data");

            new_account_id
        }
    };

    // Load the program
    let bytecode: Vec<u8> = std::fs::read(&args.program_binary).unwrap();
    let program = Program::new(bytecode).unwrap();

    // Load signing keys to provide authorization
    let signing_key = wallet_core
        .storage()
        .user_data
        .get_pub_account_signing_key(&account_id)
        .expect("Input account should be a self owned public account");

    // Generate identity commitment from the signing key using seeded_keygen
    let seed = signing_key.value();
    let (_identity_secret, id_commitment) = seeded_keygen(seed)
        .expect("seeded_keygen should succeed with valid seed");
    let identity_commitment = fr_to_bytes_le(&id_commitment);

    // Parse user message limit (default to 300 if not provided)
    let user_message_limit: Vec<u8> = match args.user_limit {
        Some(hex_str) => {
            let bytes = hex::decode(&hex_str).expect("User message limit should be valid hex");
            assert_eq!(bytes.len(), 32, "User message limit must be 32 bytes");
            bytes
        }
        None => {
            // Default to 300, encoded as 32-byte little-endian
            let mut bytes = [0u8; 32];
            bytes[..8].copy_from_slice(&300u64.to_le_bytes());
            bytes.to_vec()
        }
    };

    // Construct the instruction data
    // - First byte: instruction type (0 = ValidateAndStoreIdentityCommitment)
    // - Byte 1: hash_impl
    // - Bytes 2-3: hash_iterations (u16 LE)
    // - Bytes 4-35: identity commitment (32 bytes)
    // - Bytes 36-67: user message limit (32 bytes)
    let mut instruction: Vec<u8> = Vec::with_capacity(1 + 1 + 2 + 32 + 32);
    instruction.push(0); // InstructionType::ValidateAndStoreIdentityCommitment
    instruction.push(args.hash.to_u8());
    instruction.extend_from_slice(&args.iterations.to_le_bytes());
    instruction.extend_from_slice(&identity_commitment);
    instruction.extend_from_slice(&user_message_limit);

    println!("Using hash implementation: {:?}", args.hash);
    println!("Hash iterations: {}", args.iterations);

    // Construct the public transaction
    // Query the current nonce from the node
    let nonces = wallet_core
        .get_accounts_nonces(vec![account_id])
        .await
        .expect("Node should be reachable to query account data");
    let signing_keys = [signing_key];
    let message = Message::try_new(program.id(), vec![account_id], nonces, instruction).unwrap();
    // Pass the signing key to sign the message. This will be used by the node
    // to flag the pre_state as `is_authorized` when executing the program
    let witness_set = WitnessSet::for_message(&message, &signing_keys);
    let tx = PublicTransaction::new(message, witness_set);

    // Submit the transaction
    let response = wallet_core
        .sequencer_client
        .send_tx_public(tx)
        .await
        .unwrap();

    println!("Transaction submitted: {:?}", response);
}
