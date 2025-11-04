pub mod block;
pub mod error;
pub mod rpc_primitives;
pub mod sequencer_client;
pub mod transaction;

//Module for tests utility functions
//TODO: Compile only for tests
pub mod test_utils;
pub type HashType = [u8; 32];

pub const PINATA_BASE58: &str = "EfQhKQAkX2FJiwNii2WFQsGndjvF1Mzd7RuVe7QdPLw7";
