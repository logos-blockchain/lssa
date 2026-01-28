//! Formatting utilities for the explorer

use indexer_service_protocol::{AccountId, ProgramId};

/// Format timestamp to human-readable string
pub fn format_timestamp(timestamp: u64) -> String {
    let seconds = timestamp / 1000;
    let datetime = chrono::DateTime::from_timestamp(seconds as i64, 0)
        .unwrap_or_else(|| chrono::DateTime::from_timestamp(0, 0).unwrap());
    datetime.format("%Y-%m-%d %H:%M:%S UTC").to_string()
}

/// Format hash (32 bytes) to hex string
pub fn format_hash(hash: &[u8; 32]) -> String {
    hex::encode(hash)
}

/// Format account ID to hex string
pub fn format_account_id(account_id: &AccountId) -> String {
    hex::encode(account_id.value)
}

/// Format program ID to hex string
pub fn format_program_id(program_id: &ProgramId) -> String {
    let bytes: Vec<u8> = program_id.iter().flat_map(|n| n.to_be_bytes()).collect();
    hex::encode(bytes)
}

/// Parse hex string to bytes
pub fn parse_hex(s: &str) -> Option<Vec<u8>> {
    let s = s.trim().trim_start_matches("0x");
    hex::decode(s).ok()
}
