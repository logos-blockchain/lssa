//! C-compatible type definitions for the FFI layer.

use std::ffi::c_char;

/// Opaque pointer to the Wallet instance.
///
/// This type is never instantiated directly - it's used as an opaque handle
/// to hide the internal wallet structure from C code.
#[repr(C)]
pub struct WalletHandle {
    _private: [u8; 0],
}

/// 32-byte array type for AccountId, keys, hashes, etc.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct FfiBytes32 {
    pub data: [u8; 32],
}

/// Program ID - 8 u32 values (32 bytes total).
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct FfiProgramId {
    pub data: [u32; 8],
}

/// Account data structure - C-compatible version of nssa Account.
///
/// Note: `balance` and `nonce` are u128 values represented as little-endian
/// byte arrays since C doesn't have native u128 support.
#[repr(C)]
pub struct FfiAccount {
    pub program_owner: FfiProgramId,
    /// Balance as little-endian [u8; 16]
    pub balance: [u8; 16],
    /// Pointer to account data bytes
    pub data: *const u8,
    /// Length of account data
    pub data_len: usize,
    /// Nonce as little-endian [u8; 16]
    pub nonce: [u8; 16],
}

impl Default for FfiAccount {
    fn default() -> Self {
        Self {
            program_owner: FfiProgramId::default(),
            balance: [0u8; 16],
            data: std::ptr::null(),
            data_len: 0,
            nonce: [0u8; 16],
        }
    }
}

/// Public keys for a private account (safe to expose).
#[repr(C)]
pub struct FfiPrivateAccountKeys {
    /// Nullifier public key (32 bytes)
    pub nullifier_public_key: FfiBytes32,
    /// Incoming viewing public key (compressed secp256k1 point)
    pub incoming_viewing_public_key: *const u8,
    /// Length of incoming viewing public key (typically 33 bytes)
    pub incoming_viewing_public_key_len: usize,
}

impl Default for FfiPrivateAccountKeys {
    fn default() -> Self {
        Self {
            nullifier_public_key: FfiBytes32::default(),
            incoming_viewing_public_key: std::ptr::null(),
            incoming_viewing_public_key_len: 0,
        }
    }
}

/// Public key info for a public account.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct FfiPublicAccountKey {
    pub public_key: FfiBytes32,
}

/// Single entry in the account list.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct FfiAccountListEntry {
    pub account_id: FfiBytes32,
    pub is_public: bool,
}

/// List of accounts returned by wallet_ffi_list_accounts.
#[repr(C)]
pub struct FfiAccountList {
    pub entries: *mut FfiAccountListEntry,
    pub count: usize,
}

impl Default for FfiAccountList {
    fn default() -> Self {
        Self {
            entries: std::ptr::null_mut(),
            count: 0,
        }
    }
}

/// Result of a transfer operation.
#[repr(C)]
pub struct FfiTransferResult {
    // TODO: Replace with HashType FFI representation
    /// Transaction hash (null-terminated string, or null on failure)
    pub tx_hash: *mut c_char,
    /// Whether the transfer succeeded
    pub success: bool,
}

impl Default for FfiTransferResult {
    fn default() -> Self {
        Self {
            tx_hash: std::ptr::null_mut(),
            success: false,
        }
    }
}

// Helper functions to convert between Rust and FFI types

impl FfiBytes32 {
    /// Create from a 32-byte array.
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self { data: bytes }
    }

    /// Create from an AccountId.
    pub fn from_account_id(id: &nssa::AccountId) -> Self {
        Self { data: *id.value() }
    }
}

impl From<&nssa::AccountId> for FfiBytes32 {
    fn from(id: &nssa::AccountId) -> Self {
        Self::from_account_id(id)
    }
}

impl From<FfiBytes32> for nssa::AccountId {
    fn from(bytes: FfiBytes32) -> Self {
        nssa::AccountId::new(bytes.data)
    }
}
