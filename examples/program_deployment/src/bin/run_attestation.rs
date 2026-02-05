use clap::{Parser, Subcommand};
use nssa::{
    AccountId, PublicTransaction,
    program::Program,
    public_transaction::{Message, WitnessSet},
};
use wallet::WalletCore;

// Before running this example, compile the `attestation.rs` guest program with:
//
//   cargo risczero build --manifest-path examples/program_deployment/methods/guest/Cargo.toml
//
// Note: you must run the above command from the root of the repository.
// Note: The compiled binary file is stored in
// methods/guest/target/riscv32im-risc0-zkvm-elf/docker/attestation.bin
//
// Usage:
//   cargo run --bin run_attestation -- <program_binary_path> <subcommand>
//
// Examples:
//   cargo run --bin run_attestation -- \
//     methods/guest/target/riscv32im-risc0-zkvm-elf/docker/attestation.bin \
//     attest <creator_id> <attestation_id> <subject_id> <key_hex> <value_string>
//
//   cargo run --bin run_attestation -- \
//     methods/guest/target/riscv32im-risc0-zkvm-elf/docker/attestation.bin \
//     revoke <creator_id> <attestation_id>

#[derive(Parser, Debug)]
struct Cli {
    /// Path to the attestation program binary
    program_path: String,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Create or update an attestation
    Attest {
        /// Account ID of the creator (must be a self-owned public account)
        creator_id: String,
        /// Account ID for the attestation record
        attestation_id: String,
        /// Account ID of the subject being attested about
        subject_id: String,
        /// 32-byte attestation key as a hex string (64 hex chars)
        key_hex: String,
        /// Attestation value as a UTF-8 string
        value_string: String,
    },
    /// Revoke an existing attestation
    Revoke {
        /// Account ID of the creator (must be a self-owned public account)
        creator_id: String,
        /// Account ID of the attestation record to revoke
        attestation_id: String,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // Load the program
    let bytecode: Vec<u8> = std::fs::read(cli.program_path).unwrap();
    let program = Program::new(bytecode).unwrap();

    // Initialize wallet
    let wallet_core = WalletCore::from_env().unwrap();

    match cli.command {
        Command::Attest {
            creator_id,
            attestation_id,
            subject_id,
            key_hex,
            value_string,
        } => {
            let creator_id: AccountId = creator_id.parse().unwrap();
            let attestation_id: AccountId = attestation_id.parse().unwrap();
            let subject_id: AccountId = subject_id.parse().unwrap();
            let key_bytes: [u8; 32] = hex::decode(&key_hex)
                .expect("key_hex must be valid hex")
                .try_into()
                .expect("key_hex must decode to exactly 32 bytes");

            let signing_key = wallet_core
                .storage()
                .user_data
                .get_pub_account_signing_key(&creator_id)
                .expect("Creator account should be a self-owned public account");

            let nonces = wallet_core
                .get_accounts_nonces(vec![creator_id])
                .await
                .expect("Node should be reachable to query account data");

            // Build instruction: [0x00 || subject (32) || key (32) || value (var)]
            let mut instruction: Vec<u8> = Vec::new();
            instruction.push(0x00);
            instruction.extend_from_slice(subject_id.value());
            instruction.extend_from_slice(&key_bytes);
            instruction.extend_from_slice(value_string.as_bytes());

            let message = Message::try_new(
                program.id(),
                vec![creator_id, attestation_id],
                nonces,
                instruction,
            )
            .unwrap();
            let witness_set = WitnessSet::for_message(&message, &[signing_key]);
            let tx = PublicTransaction::new(message, witness_set);

            let _response = wallet_core
                .sequencer_client
                .send_tx_public(tx)
                .await
                .unwrap();
        }
        Command::Revoke {
            creator_id,
            attestation_id,
        } => {
            let creator_id: AccountId = creator_id.parse().unwrap();
            let attestation_id: AccountId = attestation_id.parse().unwrap();

            let signing_key = wallet_core
                .storage()
                .user_data
                .get_pub_account_signing_key(&creator_id)
                .expect("Creator account should be a self-owned public account");

            let nonces = wallet_core
                .get_accounts_nonces(vec![creator_id])
                .await
                .expect("Node should be reachable to query account data");

            // Build instruction: [0x01]
            let instruction: Vec<u8> = vec![0x01];

            let message = Message::try_new(
                program.id(),
                vec![creator_id, attestation_id],
                nonces,
                instruction,
            )
            .unwrap();
            let witness_set = WitnessSet::for_message(&message, &[signing_key]);
            let tx = PublicTransaction::new(message, witness_set);

            let _response = wallet_core
                .sequencer_client
                .send_tx_public(tx)
                .await
                .unwrap();
        }
    }
}
